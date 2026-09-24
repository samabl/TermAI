//! Session registry: state machine + lease + Session Log + terminal engine per session.

use std::collections::BTreeMap;
use std::path::PathBuf;

use termai_core::grid::GridSnapshot;
use termai_core::time::MonoTime;
use termai_core::SessionId;
use termai_session::checkpoint::{Checkpoint, GridReplay};
use termai_session::lease::{LeaseError, LeaseId, LeaseManager};
use termai_session::log::{FlushMode, Record, SegmentWriter, Source};
use termai_session::state::{ClientId, Machine, SessionState, Transition, Trigger};

/// The terminal engine a session drives. Implemented over termai-vt by the daemon.
pub trait TerminalEngine: Send {
    fn feed(&mut self, bytes: &[u8]);
    fn resize(&mut self, cols: u16, rows: u16);
    fn snapshot(&self) -> GridSnapshot;
    fn digest(&self) -> [u8; 32];
    /// Drain the bytes the emulator owes the application (DSR/CPR today). A component that
    /// owns a real pty must write these back or the application blocks on its own query
    /// (kernel/02 section 3.1), which is what [`crate::daemon`] does; an engine double owes
    /// nothing, so the default is empty and the protocol tests need no pty.
    fn take_responses(&mut self) -> Vec<Vec<u8>> {
        Vec::new()
    }
}

/// Adapter that exposes a TerminalEngine as a recovery replay target.
pub struct EngineReplay<'a>(pub &'a mut dyn TerminalEngine, pub [u8; 32]);

impl GridReplay for EngineReplay<'_> {
    fn restore(&mut self, ckpt: &Checkpoint) -> bool {
        self.1 = ckpt.grid_digest;
        true
    }
    fn feed_raw(&mut self, bytes: &[u8]) {
        self.0.feed(bytes);
    }
    fn digest(&self) -> [u8; 32] {
        self.0.digest()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RegistryError {
    NoSuchSession,
    AlreadyExists,
    Log(termai_session::log::LogError),
}

impl From<termai_session::log::LogError> for RegistryError {
    fn from(e: termai_session::log::LogError) -> Self {
        RegistryError::Log(e)
    }
}

/// One live session.
pub struct SessionEntry {
    pub id: SessionId,
    pub machine: Machine,
    pub lease: LeaseManager,
    pub engine: Box<dyn TerminalEngine>,
    pub writer: SegmentWriter,
    pub created_ns: u64,
    pub last_state_ns: u64,
}

/// Owns every live session. The registry is the only writer of session state.
#[derive(Default)]
pub struct Registry {
    sessions: BTreeMap<u128, SessionEntry>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("sessions", &self.sessions.len())
            .finish()
    }
}

impl Registry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn insert(
        &mut self,
        id: SessionId,
        engine: Box<dyn TerminalEngine>,
        writer: SegmentWriter,
        now_ns: u64,
    ) -> Result<(), RegistryError> {
        if self.sessions.contains_key(&id.0) {
            return Err(RegistryError::AlreadyExists);
        }
        self.sessions.insert(
            id.0,
            SessionEntry {
                id,
                machine: Machine::new(),
                lease: LeaseManager::new(id),
                engine,
                writer,
                created_ns: now_ns,
                last_state_ns: now_ns,
            },
        );
        Ok(())
    }

    #[must_use]
    pub fn get(&self, id: SessionId) -> Option<&SessionEntry> {
        self.sessions.get(&id.0)
    }

    pub fn get_mut(&mut self, id: SessionId) -> Option<&mut SessionEntry> {
        self.sessions.get_mut(&id.0)
    }

    #[must_use]
    pub fn ids(&self) -> Vec<SessionId> {
        self.sessions.keys().map(|k| SessionId(*k)).collect()
    }

    /// Apply a lifecycle trigger, persist the StateChange FIRST, then report.
    /// This is kernel/04 invariant 4: persist, then publish.
    pub fn apply(
        &mut self,
        id: SessionId,
        trigger: Trigger,
        ts_ns: u64,
    ) -> Result<Transition, RegistryError> {
        let e = self
            .sessions
            .get_mut(&id.0)
            .ok_or(RegistryError::NoSuchSession)?;
        let t = e
            .machine
            .apply(trigger)
            .ok_or(RegistryError::NoSuchSession)?;
        let reason = t.reason.map_or(0u8, |r| match r {
            termai_session::state::CloseReason::SpawnFailed => 1,
            termai_session::state::CloseReason::Unrecoverable => 2,
            termai_session::state::CloseReason::Reaped => 3,
            termai_session::state::CloseReason::Closed => 4,
        });
        let from = state_code(t.from);
        let to = state_code(t.to);
        e.writer.append(
            &Record::StateChange {
                from,
                to,
                reason,
                exit_code: None,
            },
            ts_ns,
        )?;
        // Crash-relevant transitions must be durable before they become visible.
        e.writer.flush(FlushMode::FsyncData)?;
        e.last_state_ns = ts_ns;
        Ok(t)
    }

    /// Read PTY output: append the raw ring record, then feed the engine.
    pub fn feed_pty_out(
        &mut self,
        id: SessionId,
        bytes: &[u8],
        ts_ns: u64,
    ) -> Result<(), RegistryError> {
        let e = self
            .sessions
            .get_mut(&id.0)
            .ok_or(RegistryError::NoSuchSession)?;
        e.writer.append(
            &Record::PtyOut {
                pane: 0,
                bytes: bytes.to_vec(),
            },
            ts_ns,
        )?;
        e.engine.feed(bytes);
        Ok(())
    }

    /// Write stdin through the CAP-1 gate. Returns the recorded hash length.
    pub fn write_stdin(
        &mut self,
        id: SessionId,
        client: ClientId,
        data: &[u8],
        source: Source,
        ts_ns: u64,
        now: MonoTime,
    ) -> Result<u64, LeaseError> {
        let e = self.sessions.get_mut(&id.0).ok_or(LeaseError::NoLease)?;
        e.lease.authorize_write(client, now)?;
        // The Log records a digest, never the bytes (kernel/04 section 3.2.3).
        let digest = termai_session::log::sha256_of(data);
        let _ = e.writer.append(
            &Record::PtyIn {
                pane: 0,
                sha256: digest,
                len: data.len() as u64,
                source,
                lease_id: e.lease.holder().map_or(0, |c| c.0 as u64),
            },
            ts_ns,
        );
        Ok(digest.len() as u64)
    }

    /// CAP-1 lease check with NO side effects (never appends a record).
    pub fn authorize_stdin(
        &self,
        id: SessionId,
        client: ClientId,
        now: MonoTime,
    ) -> Result<(), LeaseError> {
        let e = self.sessions.get(&id.0).ok_or(LeaseError::NoLease)?;
        e.lease.authorize_write(client, now)
    }

    pub fn snapshot(&self, id: SessionId) -> Result<GridSnapshot, RegistryError> {
        self.sessions
            .get(&id.0)
            .map(|e| e.engine.snapshot())
            .ok_or(RegistryError::NoSuchSession)
    }

    pub fn digest(&self, id: SessionId) -> Result<[u8; 32], RegistryError> {
        self.sessions
            .get(&id.0)
            .map(|e| e.engine.digest())
            .ok_or(RegistryError::NoSuchSession)
    }

    /// Drain the bytes the engine owes the application (DSR/CPR). Unknown session -> nothing
    /// owed, not an error: the only caller is the pty pump, which may race a closed session.
    pub fn take_responses(&mut self, id: SessionId) -> Vec<Vec<u8>> {
        self.sessions
            .get_mut(&id.0)
            .map_or_else(Vec::new, |e| e.engine.take_responses())
    }

    /// Current Log position as (segment_id, next seq). `seq` is the exclusive upper
    /// bound of appended records, i.e. the watermark an attach snapshot is taken at:
    /// a client whose `resume_from <= seq` already holds everything before it.
    #[must_use]
    pub fn log_position(&self, id: SessionId) -> Option<(u32, u64)> {
        self.sessions
            .get(&id.0)
            .map(|e| (e.writer.segment_id(), e.writer.seq()))
    }

    pub fn acquire_lease(
        &mut self,
        id: SessionId,
        client: ClientId,
        now: MonoTime,
    ) -> Result<LeaseId, LeaseError> {
        let e = self.sessions.get_mut(&id.0).ok_or(LeaseError::NoLease)?;
        e.lease.acquire(client, now)
    }

    /// Release a lease through the explicit revoke path (never a silent state edit).
    /// Only the current holder may release, so the CAP-1 invariant holds for
    /// DETACH_NOTICE just as it does for stdin writes. Works even after the deadline
    /// passed, because the holder is still the one asking to let go.
    pub fn release_lease(
        &mut self,
        id: SessionId,
        lease: LeaseId,
        client: ClientId,
        now: MonoTime,
    ) -> Result<(), LeaseError> {
        let e = self.sessions.get_mut(&id.0).ok_or(LeaseError::NoLease)?;
        if e.lease.holder() != Some(client) {
            return Err(LeaseError::NotHolder);
        }
        e.lease
            .revoke(lease, termai_session::state::Actor::system(), now)
    }

    pub fn flush(&mut self, id: SessionId) -> Result<(), RegistryError> {
        let e = self
            .sessions
            .get_mut(&id.0)
            .ok_or(RegistryError::NoSuchSession)?;
        e.writer.flush(FlushMode::FsyncData)?;
        Ok(())
    }

    #[must_use]
    pub fn state(&self, id: SessionId) -> Option<SessionState> {
        self.sessions.get(&id.0).map(|e| e.machine.state())
    }

    #[must_use]
    pub fn log_path(&self, id: SessionId) -> Option<PathBuf> {
        self.sessions
            .get(&id.0)
            .map(|e| e.writer.path().to_path_buf())
    }
}

#[must_use]
pub const fn state_code(s: SessionState) -> u8 {
    match s {
        SessionState::Created => 0,
        SessionState::Running => 1,
        SessionState::Detached => 2,
        SessionState::Exited => 3,
        SessionState::Recovering => 4,
        SessionState::Dead => 5,
    }
}

/// Deterministic engine doubles. Available outside cfg(test) so integration tests
/// (which compile the library without cfg(test)) can drive the same registry.
pub mod testing {
    use super::*;

    /// Deterministic engine double: records bytes as text. Used by broker tests so
    /// the protocol can be tested without a real PTY or VT implementation.
    pub struct TextEngine {
        pub text: String,
        cols: u16,
        rows: u16,
    }

    impl TextEngine {
        #[must_use]
        pub fn new(cols: u16, rows: u16) -> Self {
            Self {
                text: String::new(),
                cols,
                rows,
            }
        }
    }

    impl TerminalEngine for TextEngine {
        fn feed(&mut self, bytes: &[u8]) {
            self.text.push_str(&String::from_utf8_lossy(bytes));
        }
        fn resize(&mut self, cols: u16, rows: u16) {
            self.cols = cols;
            self.rows = rows;
        }
        fn snapshot(&self) -> GridSnapshot {
            let mut g = GridSnapshot::new(self.cols, self.rows);
            g.backend = "text-double".into();
            for (i, ch) in self.text.chars().take(g.cells.len()).enumerate() {
                g.cells[i].ch = ch;
            }
            g
        }
        fn digest(&self) -> [u8; 32] {
            *blake3::hash(self.text.as_bytes()).as_bytes()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::TextEngine;
    use super::*;
    use std::path::PathBuf;

    fn tmp_dir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "termai-registry-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn reg(tag: &str) -> (Registry, SessionId) {
        let dir = tmp_dir(tag);
        let id = SessionId(1);
        let mut r = Registry::new();
        let w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, 0).unwrap();
        r.insert(id, Box::new(TextEngine::new(80, 24)), w, 1)
            .unwrap();
        (r, id)
    }

    #[test]
    fn duplicate_session_is_rejected() {
        let (mut r, id) = reg("dup");
        let dir = tmp_dir("dup2");
        let w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, 0).unwrap();
        assert_eq!(
            r.insert(id, Box::new(TextEngine::new(1, 1)), w, 2),
            Err(RegistryError::AlreadyExists)
        );
    }

    #[test]
    fn feed_persists_before_it_is_visible() {
        let (mut r, id) = reg("feed");
        r.feed_pty_out(id, b"hello", 5).unwrap();
        assert_eq!(r.snapshot(id).unwrap().row_text(0), "hello");
        let path = r.log_path(id).unwrap();
        drop(r);
        let read = termai_session::log::read_segment(&path).unwrap();
        assert_eq!(read.records.len(), 1);
        assert_eq!(read.crc_mismatch_at, None);
    }

    #[test]
    fn stdin_without_lease_is_denied_and_logs_nothing() {
        let (mut r, id) = reg("nolease");
        let e = r.write_stdin(
            id,
            ClientId(1),
            b"ls",
            Source::Human,
            1,
            MonoTime::from_secs(0),
        );
        assert_eq!(e, Err(LeaseError::NoLease));
        let path = r.log_path(id).unwrap();
        drop(r);
        assert!(termai_session::log::read_segment(&path)
            .unwrap()
            .records
            .is_empty());
    }

    #[test]
    fn stdin_with_lease_records_a_digest_not_the_bytes() {
        let (mut r, id) = reg("lease");
        let now = MonoTime::from_secs(0);
        r.acquire_lease(id, ClientId(1), now).unwrap();
        r.write_stdin(id, ClientId(1), b"secret-command", Source::Human, 1, now)
            .unwrap();
        let path = r.log_path(id).unwrap();
        drop(r);
        let read = termai_session::log::read_segment(&path).unwrap();
        assert_eq!(read.records.len(), 1);
        match &read.records[0].record {
            Record::PtyIn { len, sha256, .. } => {
                assert_eq!(*len, 14);
                assert_ne!(sha256, &[0u8; 32]);
            }
            other => panic!("expected PtyIn, got {other:?}"),
        }
        // The plaintext must not appear anywhere in the segment bytes.
        let raw = std::fs::read(&path).unwrap();
        assert!(
            !raw.windows(6).any(|w| w == b"secret"),
            "plaintext must never be written to the Log"
        );
    }

    #[test]
    fn state_changes_are_persisted_then_visible() {
        let (mut r, id) = reg("state");
        let t = r.apply(id, Trigger::SpawnOk, 10).unwrap();
        assert_eq!(t.to, SessionState::Running);
        assert_eq!(r.state(id), Some(SessionState::Running));
        let path = r.log_path(id).unwrap();
        drop(r);
        let read = termai_session::log::read_segment(&path).unwrap();
        match &read.records[0].record {
            Record::StateChange { from, to, .. } => {
                assert_eq!((*from, *to), (0, 1));
            }
            other => panic!("expected StateChange, got {other:?}"),
        }
    }

    #[test]
    fn illegal_transition_is_rejected_and_logs_nothing() {
        let (mut r, id) = reg("illegal");
        assert_eq!(
            r.apply(id, Trigger::EofReaped, 1),
            Err(RegistryError::NoSuchSession)
        );
        assert_eq!(r.state(id), Some(SessionState::Created));
    }

    #[test]
    fn unknown_session_is_an_error_not_a_panic() {
        let mut r = Registry::new();
        let missing = SessionId(999);
        assert_eq!(r.snapshot(missing), Err(RegistryError::NoSuchSession));
        assert_eq!(
            r.feed_pty_out(missing, b"x", 1),
            Err(RegistryError::NoSuchSession)
        );
        assert!(r.get(missing).is_none());
        assert!(r.is_empty());
    }
}
