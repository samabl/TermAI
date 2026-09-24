//! Session restore: the daemon's rebuild path (kernel/04 section 3.3, AR-26 item 4).
//!
//! A rebuild is the only way a session's SCREEN comes back once sessiond (or the client
//! connection) is gone: the daemon re-reads the Session Log, restores the newest checkpoint
//! when the Log carries one, replays the tail into a fresh terminal engine and answers the
//! next attach with a `GRID_SNAPSHOT` plus the digest its `ATTACH_ACK` references. The
//! PROCESS is never recovered - kernel/04's honest boundary, asserted in
//! [`termai_session::checkpoint`]: the screen is recoverable, the process is not.
//!
//! This module is the in-process entry point that HARNESS section 8.2's reliability row
//! ("sessiond 重建 P95 <=2s / P99 <=5s", AR-26 item 4) is timed against. The measurement
//! definition itself is **not** here and **not** in HARNESS section 5's nineteen rows: it is
//! kernel/06 section 3.10, registered separately in `tools/bench/reliability.mjs` under
//! `RELIABILITY_MAPPING` (ADR-0029 D-4).
//!
//! Honest boundary (AR-20), stated here because the acceptance depends on it:
//!
//! * the path measured today is a **full-Log replay** (no checkpoint in the Log). The
//!   checkpoint branch needs the CAS content store, and [`crate::registry::EngineReplay`]
//!   is still the M0 adapter that reports a restore attempt rather than loading bytes from
//!   CAS - so a Log carrying a `CheckpointRef` is *not* faithfully rebuildable yet. The
//!   fixture below asserts it writes no checkpoint, and the bench driver refuses to publish
//!   timings for a Log whose recovery resumed from one. **This gap is still open** (debt-p0
//!   A24 item 1);
//! * `Resize` records **are** replayed, in Log order: recovery hands every `Record::Resize`
//!   to the replay sink at the point it occupies in the Log
//!   ([`termai_session::checkpoint::GridReplay::resize`], which [`ResizeAwareReplay`] below
//!   overrides), so a session resized mid-Log rebuilds at the geometry it actually ended with
//!   and the bytes logged after a resize replay at the size the live session held when it
//!   wrote them (debt-p0 A24 item 2). Two limits remain on that, and both are asserted rather
//!   than papered over: the geometry a session *spawned* with is not itself a Log record, so
//!   the caller still supplies it ([`rebuild_session`]'s `cols`/`rows`), and a `Resize` only
//!   drives the grid through its `cols`/`rows` - `px_w`/`px_h` carry no cell state. The
//!   mechanism test (`tests/session_rebuild.rs`) covers a mid-Log resize, a trailing one and
//!   the no-resize control; the *timing* fixture deliberately keeps one size, so no timing
//!   number claims to cover a resize.

use std::path::Path;

use termai_session::checkpoint::{recover_session, Checkpoint, GridReplay, RecoverOutcome};
use termai_session::log::LogError;

use crate::engine::VtEngine;
use crate::registry::{EngineReplay, TerminalEngine};

/// A session whose screen was rebuilt from its Session Log.
pub struct Rebuilt {
    /// The fresh terminal engine holding the rebuilt screen. Its [`VtEngine::snapshot`] is
    /// the payload of the `GRID_SNAPSHOT` a reconnecting client receives, and its
    /// [`VtEngine::digest`] is the digest the `ATTACH_ACK` references.
    pub engine: VtEngine,
    /// What recovery reported: the checkpoint it resumed from (if any), whether a checkpoint
    /// digest was verified, tail damage, and the reconstructed metadata index.
    pub outcome: RecoverOutcome,
}

impl Rebuilt {
    /// The rebuilt screen - the `GRID_SNAPSHOT` payload, ready to chunk and send.
    #[must_use]
    pub fn snapshot(&self) -> termai_core::grid::GridSnapshot {
        self.engine.snapshot()
    }
}

/// The rebuild's replay sink: [`EngineReplay`] plus the `Resize` half of the contract.
///
/// [`crate::registry::EngineReplay`] leaves [`GridReplay::resize`] at its default (a sink that
/// owns no geometry), which is not enough here: a session that was resized mid-Log can only be
/// rebuilt by a sink that applies `Resize` where it sits in the Log (debt-p0 A24 item 2). This
/// wrapper forwards the geometry to the same `TerminalEngine::resize` the daemon's own resize
/// path calls (`broker::on_resize`), so the rebuild repeats the session's operation exactly;
/// everything else delegates, so there is one definition of restore/feed/digest semantics.
struct ResizeAwareReplay<'a>(EngineReplay<'a>);

impl GridReplay for ResizeAwareReplay<'_> {
    fn restore(&mut self, ckpt: &Checkpoint) -> bool {
        self.0.restore(ckpt)
    }

    fn feed_raw(&mut self, bytes: &[u8]) {
        self.0.feed_raw(bytes);
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        self.0 .0.resize(cols, rows);
    }

    fn digest(&self) -> [u8; 32] {
        self.0.digest()
    }
}

/// Rebuild one session's screen from the Session Log in `dir`.
///
/// The window AR-26 item 4 times is one call of this function plus the snapshot/digest the
/// caller takes from the result: the daemon is asked to reconstruct a session whose live
/// engine is gone (reconnect / attach / restart), and the window ends when a usable
/// `GRID_SNAPSHOT` payload and its digest exist. Everything in that window is a read of the
/// Log plus VT replay, so the call is repeatable on the same directory - which is what makes
/// the timing measurement possible at all.
///
/// `cols`/`rows` are the geometry the live engine was **created** with (the size the daemon
/// spawned the session at). The Log's `Resize` records then carry the rebuild from there to
/// the geometry the session actually ended with, in Log order; a session that was never
/// resized rebuilds at exactly `cols`x`rows`. Passing a size other than the spawn geometry
/// cannot be repaired by this function - see the module docs on `Resize` replay.
///
/// # Errors
/// Returns the Log layer's error when the directory holds no segment
/// ([`LogError::NoSegments`]) or a segment cannot be read/decoded.
pub fn rebuild_session(dir: &Path, cols: u16, rows: u16) -> Result<Rebuilt, LogError> {
    let mut engine = VtEngine::new(cols, rows);
    let outcome = {
        let mut replay = ResizeAwareReplay(EngineReplay(&mut engine, [0u8; 32]));
        recover_session(dir, &mut replay)?
    };
    Ok(Rebuilt { engine, outcome })
}

/// Deterministic Session Log fixture.
///
/// **Acceptance / bench support - never called by the daemon.** This module exists so the
/// mechanism test (`tests/session_rebuild.rs`) and the timing driver
/// (`examples/rebuild_bench.rs`) build the *same* Log through the *same* sessiond code path
/// the daemon uses, instead of each inventing one:
///
/// * the Log is written by [`Registry`] itself (`feed_pty_out` -> append `PtyOut` + feed the
///   engine), so the "before" screen is literally the live session's screen, not a
///   reconstruction of the test's own;
/// * every byte and every timestamp is derived from the line counter, so two fixture runs
///   produce byte-identical segments (the D0 property kernel/06 section 3.6 requires: same
///   scene -> same hash);
/// * [`write_session_log_with_resizes`] splices `Resize` records in at chosen points and
///   resizes the *live* engine at exactly those points - the operation order the daemon's
///   resize path uses (`broker::on_resize`: resize the engine, then append the record) - so a
///   resized fixture's "before" screen is still the live session's own screen;
/// * it asserts nothing about timing. Only equality.
pub mod fixture {
    use std::path::{Path, PathBuf};

    use termai_core::grid::GridSnapshot;
    use termai_core::time::MonoTime;
    use termai_core::SessionId;
    use termai_session::log::{self, LogError, Record, SegmentWriter, Source};
    use termai_session::state::{ClientId, Trigger};

    use crate::engine::VtEngine;
    use crate::registry::{Registry, RegistryError};

    /// A deterministic Session Log plus the screen the live session held before the rebuild.
    #[derive(Debug)]
    pub struct LogFixture {
        /// Directory holding the segment(s).
        pub dir: PathBuf,
        /// Terminal width the live session was **created** with (what the rebuild is passed).
        pub cols: u16,
        /// Terminal height the live session was **created** with.
        pub rows: u16,
        /// The screen the live session held (what the rebuild has to reproduce). Once `Resize`
        /// records were replayed this is the geometry the session *ended* with, which is why it
        /// - not `cols`/`rows` - is the comparison's authority.
        pub state: GridSnapshot,
        /// blake3 digest of `state.canonical_bytes()`.
        pub digest: [u8; 32],
        /// Log bytes on disk.
        pub bytes: u64,
        /// Records in the Log.
        pub records: usize,
        /// `PtyOut` records (the records recovery replays).
        pub pty_out_records: usize,
        /// Bytes carried by the `PtyOut` records.
        pub pty_out_bytes: u64,
        /// `Resize` records (recovery replays these too, in Log order).
        pub resize_records: usize,
        /// `PtyIn` records (indexed for audit, never executed by recovery).
        pub input_events: usize,
        /// Segment files on disk.
        pub segments: usize,
        /// `CheckpointRef` records. Must be 0 for a measurable rebuild (see the module docs:
        /// CAS restore is unimplemented, so a checkpoint window is not faithfully replayable).
        pub checkpoints: usize,
        /// sha256 over the segment bytes (the D0 scene hash).
        pub log_sha256: [u8; 32],
    }

    /// One deterministic geometry change to splice into the fixture's Log.
    ///
    /// Recovery replays `Resize` where it sits in the Log (debt-p0 A24 item 2), so a fixture
    /// that is supposed to exercise that path has to place the record - and resize the live
    /// engine - at a chosen point of the byte script.
    #[derive(Clone, Copy, Debug)]
    pub struct ResizeAt {
        /// Feed this many script bytes first, then apply the resize. [`ResizeAt::at_end`] uses
        /// a sentinel that sorts after every real offset.
        after_bytes: usize,
        /// New width (`cols` of the `Resize` record).
        pub cols: u16,
        /// New height (`rows` of the `Resize` record).
        pub rows: u16,
    }

    impl ResizeAt {
        /// A resize that lands once `after_bytes` of the script have been written. It lands at
        /// the first `PtyOut` chunk boundary at or after that offset, never inside a record.
        #[must_use]
        pub const fn after_bytes(after_bytes: usize, cols: u16, rows: u16) -> Self {
            Self {
                after_bytes,
                cols,
                rows,
            }
        }

        /// A resize that lands after the last byte of the script, i.e. at the tail of the Log.
        #[must_use]
        pub const fn at_end(cols: u16, rows: u16) -> Self {
            Self {
                after_bytes: usize::MAX,
                cols,
                rows,
            }
        }
    }

    /// Map a registry failure onto the Log error type. A registry rejection here means the
    /// fixture itself is wrong, so it is reported as malformed Log input rather than hidden.
    fn log_err(err: RegistryError) -> LogError {
        match err {
            RegistryError::Log(e) => e,
            _ => LogError::Malformed("deterministic fixture rejected by the session registry"),
        }
    }

    /// The CAP-1 gate is pure logic; a refusal here also means the fixture is wrong.
    fn lease_err(_: termai_session::lease::LeaseError) -> LogError {
        LogError::Malformed("deterministic fixture refused by the CAP-1 lease gate")
    }

    /// The deterministic terminal byte script. Non-trivial on purpose: truecolor SGR, CJK
    /// double-width text and a combining sequence, a periodic status line written with CUP
    /// (cursor positioning, i.e. overwrite rather than append), wrapped long lines (which set
    /// the per-row `LINE_WRAPPED` flag ADR-0025 added to `canonical_bytes`), and an OSC 0
    /// title, so the rebuild has to reproduce attributes, cursor, modes, title and row flags -
    /// not merely some text.
    fn script(cols: u16, rows: u16, lines: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(lines * 96);
        let width = usize::from(cols.max(1));
        let height = usize::from(rows.max(1));
        for i in 0..lines {
            if i % 97 == 0 {
                out.extend_from_slice(b"\x1b]0;termai sessiond rebuild fixture\x07");
            }
            if i % 41 == 0 {
                let row = 1 + (i / 41) % height;
                out.extend_from_slice(format!("\x1b[{row};1H").as_bytes());
            }
            out.extend_from_slice(format!("\x1b[3{}m[{i:05}] ", i % 8).as_bytes());
            out.extend_from_slice(b"sessiond rebuild fixture: parse + grid + damage");
            if i % 3 == 0 {
                out.extend_from_slice("中文宽字符与组合符 e\u{301} ".as_bytes());
            }
            if i % 13 == 0 {
                // A line longer than the terminal width, so the replay has to reproduce a
                // wrapped row chain (the per-row LINE_WRAPPED flag, ADR-0025).
                out.resize(out.len() + width + 10, b'y');
            }
            out.extend_from_slice(b"\x1b[0m\r\n");
        }
        out
    }

    /// Length in bytes of the deterministic script [`write_session_log`] feeds, so a caller
    /// can place a [`ResizeAt`] at a derived fraction of the Log instead of a magic offset.
    #[must_use]
    pub fn script_len(cols: u16, rows: u16, lines: usize) -> usize {
        script(cols, rows, lines).len()
    }

    /// Write one deterministic session to `dir` and return its Log metadata plus the screen
    /// the live session held.
    ///
    /// Chunk boundaries are part of the fixture: the live engine is fed the same chunks the
    /// Log stores, and recovery replays exactly those records, so the comparison tests the
    /// rebuild rather than a re-chunking difference.
    ///
    /// # Errors
    /// Propagates Log I/O errors, and reports a malformed fixture if the registry refuses a
    /// deterministic record.
    pub fn write_session_log(
        dir: &Path,
        cols: u16,
        rows: u16,
        lines: usize,
    ) -> Result<LogFixture, LogError> {
        write_session_log_with_resizes(dir, cols, rows, lines, &[])
    }

    /// [`write_session_log`] with deterministic `Resize` records spliced into the Log.
    ///
    /// Each [`ResizeAt`] lands at a `PtyOut` chunk boundary, and the **live** engine is
    /// resized at exactly that point (`ResizeAwareReplay` replays the record at the same
    /// point), so `LogFixture::state` is the screen of a session that really was resized
    /// there. The record is appended after the engine resize, which is the order the daemon's
    /// resize path uses (`broker::on_resize`); pixel geometry is written as `0` because the
    /// rebuild reads only `cols`/`rows`.
    ///
    /// # Errors
    /// As [`write_session_log`].
    pub fn write_session_log_with_resizes(
        dir: &Path,
        cols: u16,
        rows: u16,
        lines: usize,
        resizes: &[ResizeAt],
    ) -> Result<LogFixture, LogError> {
        std::fs::create_dir_all(dir)?;
        // created_at_unix_ns = 0 and a zero chain prefix keep the segment bytes a pure
        // function of the script (D0: same scene -> same hash).
        let writer = SegmentWriter::create(dir, 0, 0, [0u8; 8], 0, 0)?;
        let id = SessionId(1);
        let mut registry = Registry::new();
        registry
            .insert(id, Box::new(VtEngine::new(cols, rows)), writer, 0)
            .map_err(log_err)?;
        registry.apply(id, Trigger::SpawnOk, 0).map_err(log_err)?;

        let script = script(cols, rows, lines);
        let mut ts = 1_000_000_u64;
        let mut pty_out_records = 0_usize;
        let mut pty_out_bytes = 0_u64;

        // Sorted by offset (stable), so resizes with the same offset keep the caller's order
        // and `at_end` (usize::MAX) sorts after every real offset.
        let mut pending: Vec<ResizeAt> = resizes.to_vec();
        pending.sort_by_key(|r| r.after_bytes);

        let mut at = 0_usize;
        while at < script.len() {
            while let Some(r) = pending.first() {
                if at < r.after_bytes {
                    break;
                }
                let r = pending.remove(0);
                apply_resize(&mut registry, id, r, &mut ts)?;
            }
            let step = 1024 + (at * 37) % 3072;
            let end = (at + step).min(script.len());
            registry
                .feed_pty_out(id, &script[at..end], ts)
                .map_err(log_err)?;
            pty_out_records += 1;
            pty_out_bytes += (end - at) as u64;
            ts += 1_000_000;
            at = end;
        }
        // Whatever is left is `at_end` (or an offset the last chunk step overshot): apply it
        // after the final `PtyOut`, i.e. at the tail of the Log.
        for r in pending {
            apply_resize(&mut registry, id, r, &mut ts)?;
        }

        // One CAP-1 gated stdin event: the Log carries a digest, never the bytes. Recovery
        // indexes it and must never execute it (kernel/04 AC-S4).
        let now = MonoTime::from_secs(0);
        registry
            .acquire_lease(id, ClientId(1), now)
            .map_err(lease_err)?;
        registry
            .write_stdin(
                id,
                ClientId(1),
                b"termai-rebuild-fixture\r",
                Source::Human,
                ts,
                now,
            )
            .map_err(lease_err)?;

        registry.flush(id).map_err(log_err)?;
        let state = registry.snapshot(id).map_err(log_err)?;
        let digest = registry.digest(id).map_err(log_err)?;
        let path = registry.log_path(id).ok_or(LogError::NoSegments)?;
        drop(registry);

        let raw = std::fs::read(&path)?;
        let read = log::read_segment(&path)?;
        let checkpoints = read
            .records
            .iter()
            .filter(|r| matches!(r.record, Record::CheckpointRef { .. }))
            .count();
        let resize_records = read
            .records
            .iter()
            .filter(|r| matches!(r.record, Record::Resize { .. }))
            .count();
        let input_events = read
            .records
            .iter()
            .filter(|r| matches!(r.record, Record::PtyIn { .. }))
            .count();

        Ok(LogFixture {
            dir: dir.to_path_buf(),
            cols,
            rows,
            state,
            digest,
            bytes: raw.len() as u64,
            records: read.records.len(),
            pty_out_records,
            pty_out_bytes,
            resize_records,
            input_events,
            segments: 1,
            checkpoints,
            log_sha256: log::sha256_of(&raw),
        })
    }

    /// Append one `Resize` record and resize the live engine, in that order - the daemon's own
    /// resize path order (`broker::on_resize`: `engine.resize`, then `writer.append`). A
    /// missing session here means the fixture is wrong, which is reported as malformed input.
    fn apply_resize(
        registry: &mut Registry,
        id: SessionId,
        r: ResizeAt,
        ts: &mut u64,
    ) -> Result<(), LogError> {
        let entry = registry.get_mut(id).ok_or(LogError::Malformed(
            "deterministic fixture session vanished",
        ))?;
        entry.engine.resize(r.cols, r.rows);
        entry.writer.append(
            &Record::Resize {
                pane: 0,
                cols: r.cols,
                rows: r.rows,
                // No pixel geometry: `px_w`/`px_h` carry no cell state, and the rebuild only
                // reads `cols`/`rows` (see the module docs).
                px_w: 0,
                px_h: 0,
            },
            *ts,
        )?;
        *ts += 1_000_000;
        Ok(())
    }
}
