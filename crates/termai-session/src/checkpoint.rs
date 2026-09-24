//! Checkpoint and crash recovery (kernel/04 section 3.3, AR-13 / AR-26).
//!
//! Honest boundary: the SCREEN is recoverable, the PROCESS is not. Recovery never
//! spawns and never executes PtyIn (audit index only). A test asserts that statically.
//!
//! Digest semantics: the checkpoint's grid_digest describes the grid AT the checkpoint.
//! It is therefore verified against the state loaded from CAS BEFORE the tail is
//! replayed, not against the final grid.

use std::path::Path;

use termai_core::SessionId;

use crate::log::{list_segments, read_segment, LogError, LoggedRecord, Record};
use crate::state::{transition, SessionState, Trigger};

/// The grid side of recovery. Implemented by apps/sessiond over termai-vt, so that
/// this crate never depends on the VT implementation (DC-21).
pub trait GridReplay {
    /// Load the checkpoint's grid state from CAS. False means the checkpoint content
    /// was unavailable, which is a rebuild failure (kernel/04 section 3.1).
    fn restore(&mut self, ckpt: &Checkpoint) -> bool;
    /// Replay raw VT bytes into the grid. This is the only mutation recovery performs.
    fn feed_raw(&mut self, bytes: &[u8]);
    /// Apply a geometry change (`Record::Resize`, kernel/04 section 3.2.3, retention class P0)
    /// **at the position the record occupies in the Log**, i.e. between two
    /// [`GridReplay::feed_raw`] calls. Bytes logged after a resize must replay at the geometry
    /// the live session held when it wrote them: ignoring this call - or applying only the last
    /// `Resize` - silently rebuilds a resized session at the wrong size (debt-p0 A24 item 2).
    ///
    /// The default does nothing, which is only correct for a sink that owns no grid geometry
    /// (an audit sink, or a unit-test double). A sink whose `digest()` is compared against a
    /// live screen **must** override this; the daemon's rebuild path does, in
    /// `apps/sessiond/src/restore.rs`.
    fn resize(&mut self, cols: u16, rows: u16) {
        let _ = (cols, rows);
    }
    /// Digest of the current grid (blake3 of GridSnapshot canonical bytes).
    fn digest(&self) -> [u8; 32];
}

/// Serialised checkpoint (stored in CAS; the Log only carries a CheckpointRef).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Checkpoint {
    pub ckpt_id: u64,
    pub seq: u64,
    pub segment_id: u32,
    pub offset: u32,
    pub grid_digest: [u8; 32],
    pub modes: [u8; 64],
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub scrollback_head: u64,
    pub cwd: String,
    pub created_at_ns: u64,
}

impl Checkpoint {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(160);
        out.extend_from_slice(&self.ckpt_id.to_le_bytes());
        out.extend_from_slice(&self.seq.to_le_bytes());
        out.extend_from_slice(&self.segment_id.to_le_bytes());
        out.extend_from_slice(&self.offset.to_le_bytes());
        out.extend_from_slice(&self.grid_digest);
        out.extend_from_slice(&self.modes);
        out.extend_from_slice(&self.cursor_row.to_le_bytes());
        out.extend_from_slice(&self.cursor_col.to_le_bytes());
        out.extend_from_slice(&self.scrollback_head.to_le_bytes());
        out.extend_from_slice(&(self.cwd.len() as u32).to_le_bytes());
        out.extend_from_slice(self.cwd.as_bytes());
        out.extend_from_slice(&self.created_at_ns.to_le_bytes());
        out
    }

    pub fn decode(b: &[u8]) -> Result<Self, LogError> {
        if b.len() < 136 + 8 {
            return Err(LogError::Malformed("checkpoint too short"));
        }
        let u64at = |o: usize| {
            let mut a = [0u8; 8];
            a.copy_from_slice(&b[o..o + 8]);
            u64::from_le_bytes(a)
        };
        let u32at = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        let u16at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
        let mut digest = [0u8; 32];
        digest.copy_from_slice(&b[24..56]);
        let mut modes = [0u8; 64];
        modes.copy_from_slice(&b[56..120]);
        let cwd_len = u32at(132) as usize;
        if b.len() < 136 + cwd_len + 8 {
            return Err(LogError::Malformed("checkpoint cwd truncated"));
        }
        let cwd = String::from_utf8(b[136..136 + cwd_len].to_vec())
            .map_err(|_| LogError::Malformed("checkpoint cwd utf8"))?;
        Ok(Checkpoint {
            ckpt_id: u64at(0),
            seq: u64at(8),
            segment_id: u32at(16),
            offset: u32at(20),
            grid_digest: digest,
            modes,
            cursor_row: u16at(120),
            cursor_col: u16at(122),
            scrollback_head: u64at(124),
            cwd,
            created_at_ns: u64at(136 + cwd_len),
        })
    }
}

/// A command block recovered from the Log.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BlockMeta {
    pub cmd_id: u64,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub cwd: String,
    pub confidence: u8,
}

/// Structured metadata reconstructed from P0 records.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct MetaIndex {
    pub state: Option<(u8, u8)>,
    pub cwd: Option<String>,
    pub title: Option<String>,
    pub blocks: Vec<BlockMeta>,
    pub checkpoints: usize,
    pub raw_bytes: u64,
    pub input_events: u64,
}

impl MetaIndex {
    fn apply(&mut self, rec: &Record) {
        match rec {
            Record::PtyOut { bytes, .. } => self.raw_bytes += bytes.len() as u64,
            // PtyIn is indexed for audit only: it is never executed.
            Record::PtyIn { .. } => self.input_events += 1,
            Record::CmdStart { cmd_id, .. } => self.blocks.push(BlockMeta {
                cmd_id: *cmd_id,
                exit_code: None,
                duration_ms: 0,
                cwd: self.cwd.clone().unwrap_or_default(),
                confidence: 0,
            }),
            Record::CmdEnd {
                cmd_id,
                exit_code,
                duration_ms,
                cwd,
                confidence,
                ..
            } => {
                if let Some(b) = self.blocks.iter_mut().rev().find(|b| b.cmd_id == *cmd_id) {
                    b.exit_code = Some(*exit_code);
                    b.duration_ms = *duration_ms;
                    b.cwd = cwd.clone();
                    b.confidence = *confidence;
                } else {
                    self.blocks.push(BlockMeta {
                        cmd_id: *cmd_id,
                        exit_code: Some(*exit_code),
                        duration_ms: *duration_ms,
                        cwd: cwd.clone(),
                        confidence: *confidence,
                    });
                }
                self.cwd = Some(cwd.clone());
            }
            Record::CwdChange { cwd, .. } => self.cwd = Some(cwd.clone()),
            Record::TitleChange { title, .. } => self.title = Some(title.clone()),
            Record::StateChange { from, to, .. } => self.state = Some((*from, *to)),
            Record::CheckpointRef { .. } => self.checkpoints += 1,
            Record::ContextEvent { .. }
            | Record::Resize { .. }
            | Record::LeaseEvent { .. }
            | Record::SubscriptionDrop { .. }
            | Record::AuditRef { .. } => {}
        }
    }
}

/// Recovery outcome. digest_verified == false means there was no checkpoint (or its
/// content was unavailable); the caller must surface that honestly (AR-20).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RecoverOutcome {
    pub state: SessionState,
    pub resumed_from: Option<u64>,
    pub digest_verified: bool,
    pub tail_truncated: bool,
    pub crc_mismatch_at: Option<u32>,
    pub last_valid_seq: u64,
    /// Digest of the reconstructed (post tail-replay) grid.
    pub grid_digest: [u8; 32],
    pub meta: MetaIndex,
    pub raw_replayed: usize,
    pub truncated_label: Option<String>,
}

/// The single recovery entry point. Contains NO spawn/exec (asserted by test).
pub fn recover_session(
    dir: &Path,
    replay: &mut dyn GridReplay,
) -> Result<RecoverOutcome, LogError> {
    let segments = list_segments(dir)?;
    if segments.is_empty() {
        return Err(LogError::NoSegments);
    }

    let mut all: Vec<LoggedRecord> = Vec::new();
    let mut tail_truncated = false;
    let mut crc_mismatch_at = None;
    for (_, path) in &segments {
        let read = read_segment(path)?;
        let damaged = read.tail_truncated;
        let crc = read.crc_mismatch_at;
        all.extend(read.records);
        if damaged {
            tail_truncated = true;
            crc_mismatch_at = crc;
            break;
        }
    }

    // Latest CheckpointRef decides the replay window.
    let mut ckpt_index = None;
    let mut ckpt: Option<Checkpoint> = None;
    for (i, r) in all.iter().enumerate() {
        if let Record::CheckpointRef {
            ckpt_id,
            segment_id,
            offset,
            grid_digest,
        } = r.record
        {
            ckpt_index = Some(i);
            ckpt = Some(Checkpoint {
                ckpt_id,
                seq: r.id.seq,
                segment_id,
                offset,
                grid_digest,
                modes: [0u8; 64],
                cursor_row: 0,
                cursor_col: 0,
                scrollback_head: 0,
                cwd: String::new(),
                created_at_ns: r.ts_ns,
            });
        }
    }

    let start = ckpt_index.map_or(0, |i| i + 1);

    // Verify the checkpoint content BEFORE replaying the tail.
    let (digest_verified, rebuild_ok) = match &ckpt {
        Some(c) => {
            let restored = replay.restore(c);
            let matches = replay.digest() == c.grid_digest;
            (true, restored && matches)
        }
        None => (false, !all.is_empty()),
    };

    let mut meta = MetaIndex::default();
    if ckpt.is_some() {
        meta.checkpoints = 1;
    }
    if let Some(Record::CwdChange { cwd, .. }) = all
        .iter()
        .take(start)
        .rev()
        .map(|r| &r.record)
        .find(|r| matches!(r, Record::CwdChange { .. }))
    {
        meta.cwd = Some(cwd.clone());
    }

    let mut raw_replayed = 0usize;
    for r in all.iter().skip(start) {
        match &r.record {
            Record::PtyOut { bytes, .. } => {
                replay.feed_raw(bytes);
                raw_replayed += 1;
            }
            // A `Resize` is applied where it sits in the Log - not once at the end and not
            // only the last one: the records after it must replay at the new geometry, so
            // that a session resized mid-Log rebuilds to the screen it actually ended with
            // (debt-p0 A24 item 2). `pane` is ignored like everywhere else in recovery, and
            // `px_w`/`px_h` carry no cell state, so only `cols`/`rows` drive the grid.
            Record::Resize { cols, rows, .. } => replay.resize(*cols, *rows),
            _ => {}
        }
        meta.apply(&r.record);
    }

    let actual = replay.digest();
    let t = transition(
        SessionState::Recovering,
        if rebuild_ok {
            Trigger::RebuildOk
        } else {
            Trigger::RebuildFail
        },
    )
    .ok_or(LogError::Malformed("recovery transition"))?;

    let last_valid_seq = all.last().map_or(0, |r| r.id.seq);
    let truncated_label = if tail_truncated {
        Some(match crc_mismatch_at {
            Some(at) => format!("gap: crc mismatch at offset {at}"),
            None => "gap: truncated tail".to_string(),
        })
    } else {
        None
    };

    Ok(RecoverOutcome {
        state: t.to,
        resumed_from: ckpt.as_ref().map(|c| c.ckpt_id),
        digest_verified,
        tail_truncated,
        crc_mismatch_at,
        last_valid_seq,
        grid_digest: actual,
        meta,
        raw_replayed,
        truncated_label,
    })
}

/// Session id correlation helper (kept for API symmetry with the daemon).
#[must_use]
pub const fn session_of(id: SessionId) -> SessionId {
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::{FlushMode, Record, SegmentWriter, Source};
    use std::path::PathBuf;

    struct FakeGrid {
        text: String,
        restore_text: String,
        restore_ok: bool,
        /// Geometry applied by `resize`, in call order (debt-p0 A24: order is the point).
        sizes: Vec<(u16, u16)>,
    }

    impl Default for FakeGrid {
        fn default() -> Self {
            Self {
                text: String::new(),
                restore_text: String::new(),
                restore_ok: true,
                sizes: Vec::new(),
            }
        }
    }

    impl FakeGrid {
        fn digest_of(text: &str) -> [u8; 32] {
            *blake3::hash(text.as_bytes()).as_bytes()
        }
    }

    impl GridReplay for FakeGrid {
        fn restore(&mut self, _ckpt: &Checkpoint) -> bool {
            if self.restore_ok {
                self.text = self.restore_text.clone();
            }
            self.restore_ok
        }
        fn feed_raw(&mut self, bytes: &[u8]) {
            self.text.push_str(&String::from_utf8_lossy(bytes));
        }
        fn resize(&mut self, cols: u16, rows: u16) {
            self.sizes.push((cols, rows));
        }
        fn digest(&self) -> [u8; 32] {
            Self::digest_of(&self.text)
        }
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "termai-recover-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn checkpoint_round_trips() {
        let c = Checkpoint {
            ckpt_id: 3,
            seq: 9,
            segment_id: 1,
            offset: 4096,
            grid_digest: [7u8; 32],
            modes: [1u8; 64],
            cursor_row: 12,
            cursor_col: 40,
            scrollback_head: 55,
            cwd: "/work".into(),
            created_at_ns: 12345,
        };
        assert_eq!(Checkpoint::decode(&c.encode()).unwrap(), c);
        assert!(Checkpoint::decode(&[0u8; 10]).is_err());
    }

    #[test]
    fn recovery_replays_only_after_the_checkpoint_and_never_executes_input() {
        let dir = tmp_dir("window");
        let mut w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, 0).unwrap();
        w.append(
            &Record::PtyOut {
                pane: 0,
                bytes: b"BEFORE".to_vec(),
            },
            1,
        )
        .unwrap();
        // Resize inside the checkpointed window: its effect is part of the state the
        // checkpoint restores, so recovery must NOT replay it.
        w.append(
            &Record::Resize {
                pane: 0,
                cols: 132,
                rows: 43,
                px_w: 0,
                px_h: 0,
            },
            2,
        )
        .unwrap();
        w.append(
            &Record::CheckpointRef {
                ckpt_id: 42,
                segment_id: 0,
                offset: 100,
                grid_digest: FakeGrid::digest_of("BEFORE"),
            },
            3,
        )
        .unwrap();
        w.append(
            &Record::PtyIn {
                pane: 0,
                sha256: [0u8; 32],
                len: 3,
                source: Source::Agent,
                lease_id: 1,
            },
            4,
        )
        .unwrap();
        w.append(
            &Record::PtyOut {
                pane: 0,
                bytes: b"AFTER".to_vec(),
            },
            5,
        )
        .unwrap();
        // Two resizes inside the replay window: both must be applied, in Log order.
        w.append(
            &Record::Resize {
                pane: 0,
                cols: 100,
                rows: 30,
                px_w: 0,
                px_h: 0,
            },
            6,
        )
        .unwrap();
        w.append(
            &Record::Resize {
                pane: 0,
                cols: 60,
                rows: 20,
                px_w: 0,
                px_h: 0,
            },
            7,
        )
        .unwrap();
        w.flush(FlushMode::FsyncFull).unwrap();
        drop(w);

        let mut grid = FakeGrid {
            restore_text: "BEFORE".to_string(),
            ..Default::default()
        };
        let out = recover_session(&dir, &mut grid).unwrap();
        assert_eq!(out.resumed_from, Some(42));
        assert_eq!(out.state, SessionState::Detached);
        assert_eq!(
            out.raw_replayed, 1,
            "only records after the checkpoint replay"
        );
        assert_eq!(grid.text, "BEFOREAFTER");
        assert_eq!(
            grid.sizes,
            vec![(100, 30), (60, 20)],
            "only the replay window's resizes are applied, and in Log order"
        );
        assert_eq!(out.meta.input_events, 1, "PtyIn is indexed, never executed");
        assert!(!out.tail_truncated);
        assert!(out.digest_verified);
    }

    #[test]
    fn resize_records_replay_in_log_order_at_the_point_they_occur() {
        // The failure this pins down (debt-p0 A24 item 2): recovery used to replay only
        // `PtyOut`, so a resized session rebuilt at its spawn geometry. Applying only the
        // last `Resize` would also pass a "final size" check while putting every byte before
        // it at the wrong width, so the assertion is on the ordered call log, not the size.
        let dir = tmp_dir("resize-order");
        let mut w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, 0).unwrap();
        w.append(
            &Record::PtyOut {
                pane: 0,
                bytes: b"head".to_vec(),
            },
            1,
        )
        .unwrap();
        w.append(
            &Record::Resize {
                pane: 0,
                cols: 100,
                rows: 30,
                px_w: 800,
                px_h: 600,
            },
            2,
        )
        .unwrap();
        w.append(
            &Record::PtyOut {
                pane: 0,
                bytes: b"-middle".to_vec(),
            },
            3,
        )
        .unwrap();
        w.append(
            &Record::Resize {
                pane: 0,
                cols: 61,
                rows: 17,
                px_w: 0,
                px_h: 0,
            },
            4,
        )
        .unwrap();
        w.append(
            &Record::PtyOut {
                pane: 0,
                bytes: b"-tail".to_vec(),
            },
            5,
        )
        .unwrap();
        w.append(
            &Record::Resize {
                pane: 0,
                cols: 80,
                rows: 24,
                px_w: 0,
                px_h: 0,
            },
            6,
        )
        .unwrap();
        w.flush(FlushMode::FsyncFull).unwrap();
        drop(w);

        let mut grid = FakeGrid::default();
        let out = recover_session(&dir, &mut grid).unwrap();
        assert_eq!(grid.text, "head-middle-tail", "PtyOut order is unchanged");
        assert_eq!(
            grid.sizes,
            vec![(100, 30), (61, 17), (80, 24)],
            "every Resize replays once, in Log order, including the trailing one"
        );
        assert_eq!(out.raw_replayed, 3);
    }

    #[test]
    fn digest_mismatch_goes_dead_with_unrecoverable() {
        let dir = tmp_dir("mismatch");
        let mut w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, 0).unwrap();
        w.append(
            &Record::CheckpointRef {
                ckpt_id: 1,
                segment_id: 0,
                offset: 64,
                grid_digest: FakeGrid::digest_of("EXPECTED"),
            },
            1,
        )
        .unwrap();
        w.append(
            &Record::PtyOut {
                pane: 0,
                bytes: b"x".to_vec(),
            },
            2,
        )
        .unwrap();
        w.flush(FlushMode::FsyncFull).unwrap();
        drop(w);
        let mut grid = FakeGrid {
            restore_text: "SOMETHING ELSE".to_string(),
            ..Default::default()
        };
        let out = recover_session(&dir, &mut grid).unwrap();
        assert_eq!(out.state, SessionState::Dead);
        assert!(out.digest_verified);
    }

    #[test]
    fn unavailable_checkpoint_content_is_a_rebuild_failure() {
        let dir = tmp_dir("nocas");
        let mut w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, 0).unwrap();
        w.append(
            &Record::CheckpointRef {
                ckpt_id: 1,
                segment_id: 0,
                offset: 64,
                grid_digest: FakeGrid::digest_of(""),
            },
            1,
        )
        .unwrap();
        w.flush(FlushMode::FsyncFull).unwrap();
        drop(w);
        let mut grid = FakeGrid {
            restore_ok: false,
            ..Default::default()
        };
        let out = recover_session(&dir, &mut grid).unwrap();
        assert_eq!(out.state, SessionState::Dead);
    }

    #[test]
    fn recovery_without_checkpoint_is_honest_about_unverified_digest() {
        let dir = tmp_dir("nockpt");
        let mut w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, 0).unwrap();
        w.append(
            &Record::PtyOut {
                pane: 0,
                bytes: b"only".to_vec(),
            },
            1,
        )
        .unwrap();
        w.flush(FlushMode::FsyncFull).unwrap();
        drop(w);
        let mut grid = FakeGrid::default();
        let out = recover_session(&dir, &mut grid).unwrap();
        assert!(
            !out.digest_verified,
            "no checkpoint means no digest verification"
        );
        assert_eq!(out.state, SessionState::Detached);
        assert_eq!(out.resumed_from, None);
    }

    #[test]
    fn empty_directory_is_an_error_not_a_silent_success() {
        let dir = tmp_dir("empty");
        let mut grid = FakeGrid::default();
        assert_eq!(recover_session(&dir, &mut grid), Err(LogError::NoSegments));
    }

    #[test]
    fn recovery_module_contains_no_spawn_or_exec() {
        // AC-S4: recovery must contain no spawn/exec. Static assertion on our own source.
        // The needles are built at runtime so this test does not match itself.
        let src = include_str!("checkpoint.rs");
        // Note: std::process::id() is allowed (benign); only spawning APIs are banned.
        let needles = [
            ["Command", "::new"].concat(),
            ["process", "::Command"].concat(),
            [".spawn", "("].concat(),
            ["libc", "::fork"].concat(),
            ["posix", "_spawn"].concat(),
            ["fork", "pty"].concat(),
        ];
        for n in needles {
            assert!(!src.contains(&n), "recovery must not contain {n}");
        }
    }
}
