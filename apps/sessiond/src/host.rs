//! PTY-owning session host: the daemon's side of the PTY lifecycle
//! (kernel/02 section 3.3, DC-16/DC-27, AR-30 item 2 / PTY-ORPHAN-1).
//!
//! The registry stays PTY-free on purpose: it owns session state, the Session Log and a
//! [`TerminalEngine`](crate::registry::TerminalEngine), which is what keeps the whole wire
//! protocol testable with an engine double. This module is the other half of the session
//! lifecycle - sessiond's ownership of the real child process:
//!
//! - [`SessionHost::open`] spawns the child on the platform backend (ConPTY + Job Object on
//!   Windows, forkpty on unix) and keys its process tree by session id;
//! - [`SessionHost::close`] is the one teardown path. It kills the **whole tree** with
//!   [`KillMode::Force`] and waits for the reap, bounded by [`REAP_BUDGET`]. The kill cannot
//!   be delegated to `PtyBackend::close`: that face releases handles and explicitly does NOT
//!   kill processes (kernel/02 section 3.1), so orphan cleanup is the owner's job - which is
//!   exactly the debt this module closes;
//! - a closed session leaves nothing behind: the entry is dropped, and closing the same id
//!   again is a no-op reporting `already_closed` - never an error, never a panic (kernel/02
//!   section 3.1 invariant 5).
//!
//! Sessions are handed out as `Arc<PtySession>` and every operation takes `&self`, so a pump
//! loop can read the pty in another thread while the host remains free to close the session.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use termai_core::SessionId;
use termai_pty::{
    Command, KillMode, ProcEntry, ProcessTree, PtyBackend, PtyError, PtyHandle, ResizeEffect,
    SpawnOpts, WaitTimeout, WinSize,
};

/// The bound the close path waits for a reaped tree.
///
/// AR-30 item 2 allows 2 s from session close to a clean process tree; the budget sits below
/// that so the kill, the wait and the handle release all fit inside the acceptance bound.
pub const REAP_BUDGET: WaitTimeout = WaitTimeout::Millis(1_500);

/// Why a host operation failed. Structured - no panic crosses this boundary
/// (AGENTS section 6).
#[derive(Debug)]
pub enum HostError {
    /// A live session already owns this id: one process tree per session id.
    AlreadyOpen(SessionId),
    /// The platform layer failed (spawn, job attach, kill, wait, ...).
    Pty(PtyError),
    /// The tree was still not empty when the reap budget elapsed. Reported as a failure
    /// rather than as a success: AR-30 item 2 asks for a 100% clean tree, and claiming
    /// otherwise would hide an orphan (AR-20).
    NotReaped { live: u32 },
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::AlreadyOpen(id) => write!(f, "session {id:?} already owns a process tree"),
            HostError::Pty(err) => write!(f, "pty error: {err}"),
            HostError::NotReaped { live } => {
                write!(
                    f,
                    "process tree still has {live} live process(es) after kill(Force)"
                )
            }
        }
    }
}

impl std::error::Error for HostError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            HostError::Pty(err) => Some(err),
            _ => None,
        }
    }
}

impl From<PtyError> for HostError {
    fn from(err: PtyError) -> Self {
        HostError::Pty(err)
    }
}

/// What a close did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CloseOutcome {
    /// True when nothing had to be done because the session was already closed (or is not
    /// open at all). This is the idempotent repeat-close answer.
    pub already_closed: bool,
    /// True when the bounded wait returned with the tree reaped (kernel/02 section 3.1
    /// invariant 2: `wait` returning means reaped).
    pub reaped: bool,
    /// Processes still live in the tree when the close finished. PTY-ORPHAN-1 / AR-30 item 2
    /// require 0.
    pub live_children: u32,
    /// Exit code of the root process, when the platform reported one; a forced kill normally
    /// reports a signal instead.
    pub exit_code: Option<i32>,
    /// Wall time the kill + wait took. AR-30 item 2 gives 2 s.
    pub wall: Duration,
}

impl CloseOutcome {
    /// The outcome of closing something that was not open: the requested end state (no child
    /// of this daemon for that session) already holds.
    #[must_use]
    pub const fn already_closed() -> Self {
        Self {
            already_closed: true,
            reaped: false,
            live_children: 0,
            exit_code: None,
            wall: Duration::ZERO,
        }
    }

    /// The same outcome as seen by a repeated close.
    #[must_use]
    fn repeated(&self) -> Self {
        Self {
            already_closed: true,
            reaped: self.reaped,
            live_children: self.live_children,
            exit_code: self.exit_code,
            wall: self.wall,
        }
    }
}

/// Close state of one session. The pty handle is released exactly once; `done` records a
/// completed close so a repeat close is answered without touching the platform again.
#[derive(Default)]
struct CloseState {
    handle: Option<PtyHandle>,
    done: Option<CloseOutcome>,
}

/// One daemon-owned child process and its tree.
pub struct PtySession {
    backend: Arc<dyn PtyBackend>,
    state: Mutex<CloseState>,
    tree: ProcessTree,
    size: WinSize,
}

impl PtySession {
    /// Spawn the real child process for a session.
    ///
    /// The nine-face backend contract is untouched: `SpawnOpts::detach_policy` stays
    /// `Deny` by default, so descendants cannot leave the Job Object / process group and
    /// the close path below can reap the whole tree.
    pub fn spawn(
        backend: Arc<dyn PtyBackend>,
        cmd: &Command,
        size: WinSize,
        opts: SpawnOpts,
    ) -> Result<Self, PtyError> {
        let handle = backend.spawn(cmd, size, opts)?;
        let tree = match backend.process_tree(&handle) {
            Ok(tree) => tree,
            Err(err) => {
                // No handle leak on an error path (kernel/02 section 3.1 invariant 5).
                let _ = backend.close(handle);
                return Err(err);
            }
        };
        Ok(Self {
            backend,
            state: Mutex::new(CloseState {
                handle: Some(handle),
                done: None,
            }),
            tree,
            size,
        })
    }

    /// The process tree of this session; cheap to clone and usable from a monitor thread.
    #[must_use]
    pub fn tree(&self) -> &ProcessTree {
        &self.tree
    }

    /// The size the pty was created with.
    #[must_use]
    pub const fn size(&self) -> WinSize {
        self.size
    }

    /// True once the pty handle has been released by [`PtySession::close`].
    #[must_use]
    pub fn is_closed(&self) -> bool {
        lock(&self.state).handle.is_none()
    }

    /// Current tree contents (pid/ppid/name/start_time/cpu_ms).
    pub fn snapshot(&self) -> Result<Vec<ProcEntry>, PtyError> {
        self.backend.tree_snapshot(&self.tree)
    }

    /// Number of processes the tree still holds.
    #[must_use]
    pub fn live_children(&self) -> u32 {
        self.tree.live_children()
    }

    /// Pid of the root process, when the platform still reports it.
    #[must_use]
    pub fn root_pid(&self) -> Option<u32> {
        self.snapshot().ok()?.first().map(|e| e.pid)
    }

    /// Write to the pty input. Partial writes are the caller's loop (kernel/02 section 3.1).
    pub fn write(&self, data: &[u8]) -> Result<usize, PtyError> {
        self.backend.write(&self.open_handle()?, data)
    }

    /// Read from the pty output; 0 means EOF.
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, PtyError> {
        self.backend.read(&self.open_handle()?, buf)
    }

    /// Resize the terminal; the returned effect says whether it was lossless (K-07/C-W3).
    pub fn resize(&self, cols: u16, rows: u16) -> Result<ResizeEffect, PtyError> {
        self.backend
            .resize(&self.open_handle()?, WinSize::new(cols, rows))
    }

    /// Tear this session down: `KillMode::Force` on the whole tree, then a wait bounded by
    /// [`REAP_BUDGET`], then release the pty exactly once.
    ///
    /// Idempotent: a session that was already closed returns `already_closed` and touches no
    /// platform state. A failed reap is retryable - the tree handle is kept, so calling
    /// again re-attempts the kill instead of leaking an orphan.
    pub fn close(&self) -> Result<CloseOutcome, HostError> {
        {
            let state = lock(&self.state);
            if let Some(done) = state.done {
                return Ok(done.repeated());
            }
        }
        // Fixed order: force the tree down, then wait for the reap. Never the other way
        // round, and never only PtyBackend::close (which does not kill - kernel/02 3.1).
        let started = Instant::now();
        let kill = self.backend.kill(&self.tree, KillMode::Force);
        let wait = self.backend.wait(&self.tree, REAP_BUDGET);
        let live = self.tree.live_children();
        let release = {
            let handle = lock(&self.state).handle.take();
            match handle {
                Some(handle) => self.backend.close(handle),
                None => Ok(()),
            }
        };
        let outcome = CloseOutcome {
            already_closed: false,
            reaped: wait.as_ref().is_ok_and(|info| info.reaped),
            live_children: live,
            exit_code: wait.as_ref().ok().and_then(|info| info.code),
            wall: started.elapsed(),
        };
        let primary = match (kill, wait) {
            (_, Err(PtyError::Timeout { .. })) | (Err(PtyError::TreeNotEmpty { .. }), _) => {
                Err(HostError::NotReaped { live })
            }
            (Err(err), _) | (_, Err(err)) => Err(HostError::Pty(err)),
            (Ok(_), Ok(_)) => Ok(outcome),
        };
        match (primary, release) {
            (Err(err), _) => Err(err),
            (Ok(_), Err(err)) => Err(HostError::Pty(err)),
            (Ok(done), Ok(())) => {
                lock(&self.state).done = Some(done);
                Ok(done)
            }
        }
    }

    /// The pty handle for byte operations, refusing after close.
    fn open_handle(&self) -> Result<PtyHandle, PtyError> {
        lock(&self.state).handle.clone().ok_or(PtyError::NoSuchPty)
    }
}

impl std::fmt::Debug for PtySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = lock(&self.state);
        f.debug_struct("PtySession")
            .field("size", &self.size)
            .field("closed", &state.handle.is_none())
            .field("live_children", &self.tree.live_children())
            .finish()
    }
}

/// Owns every daemon-created child process, keyed by session id.
///
/// This is the daemon-level answer to AR-30 item 2: sessiond holds the PTY, so a session
/// created here has a real child process and closing it reaps the whole tree inside 2 s.
pub struct SessionHost {
    backend: Arc<dyn PtyBackend>,
    sessions: BTreeMap<u128, Arc<PtySession>>,
}

impl SessionHost {
    #[must_use]
    pub fn new(backend: Arc<dyn PtyBackend>) -> Self {
        Self {
            backend,
            sessions: BTreeMap::new(),
        }
    }

    /// Host on the best backend this machine has: the native ConPTY/forkpty path, or the
    /// declared PipeFallback when the native path is unavailable (kernel/02 section 6).
    #[must_use]
    pub fn probed() -> Self {
        Self::new(Arc::from(termai_pty::probe_backend()))
    }

    #[must_use]
    pub fn backend(&self) -> &Arc<dyn PtyBackend> {
        &self.backend
    }

    /// Create the real child process for a session. One tree per session id.
    pub fn open(
        &mut self,
        id: SessionId,
        cmd: &Command,
        size: WinSize,
        opts: SpawnOpts,
    ) -> Result<(), HostError> {
        if self.sessions.contains_key(&id.0) {
            return Err(HostError::AlreadyOpen(id));
        }
        let session = PtySession::spawn(Arc::clone(&self.backend), cmd, size, opts)?;
        self.sessions.insert(id.0, Arc::new(session));
        Ok(())
    }

    /// The session for `id`, or `None` when it was never opened or is already closed.
    #[must_use]
    pub fn get(&self, id: SessionId) -> Option<Arc<PtySession>> {
        self.sessions.get(&id.0).map(Arc::clone)
    }

    #[must_use]
    pub fn ids(&self) -> Vec<SessionId> {
        self.sessions.keys().map(|k| SessionId(*k)).collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// The one close path (AR-30 item 2): force the whole tree down, wait for the reap
    /// inside the 2 s bound, then forget the session.
    ///
    /// Closing an id that is not open is not an error - the wanted end state (no child of
    /// this daemon for that session) already holds, which is what makes a repeated close
    /// idempotent. A failed reap keeps the entry so the caller can retry instead of
    /// abandoning a live tree.
    pub fn close(&mut self, id: SessionId) -> Result<CloseOutcome, HostError> {
        let Some(session) = self.sessions.remove(&id.0) else {
            return Ok(CloseOutcome::already_closed());
        };
        match session.close() {
            Ok(outcome) => Ok(outcome),
            Err(err) => {
                self.sessions.insert(id.0, session);
                Err(err)
            }
        }
    }

    /// Close every hosted session - the daemon shutdown path (orphan cleanup = 100%).
    pub fn close_all(&mut self) -> Vec<(SessionId, Result<CloseOutcome, HostError>)> {
        self.ids()
            .into_iter()
            .map(|id| (id, self.close(id)))
            .collect()
    }
}

impl std::fmt::Debug for SessionHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHost")
            .field("sessions", &self.sessions.len())
            .finish()
    }
}

/// A poisoned lock is recovered instead of panicking across this boundary
/// (AGENTS section 6: library code must not panic across an interface).
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A long-lived command with no descendants, so the pipe fallback's single-process
    /// kill is complete (its Force path kills the root only - the Job Object tree kill is
    /// the native backend's, and the integration tests cover that with real descendants).
    fn idle_command() -> Command {
        #[cfg(windows)]
        {
            Command::new("cmd.exe")
        }
        #[cfg(unix)]
        {
            Command::with_args("/bin/sleep", vec!["30".to_string()])
        }
    }

    fn size() -> WinSize {
        WinSize::new(80, 24)
    }

    fn opts() -> SpawnOpts {
        SpawnOpts::default()
    }

    fn pipes() -> Arc<dyn PtyBackend> {
        Arc::from(termai_pty::pipe_backend())
    }

    #[test]
    fn a_hosted_session_owns_a_live_child_process() {
        let mut host = SessionHost::new(pipes());
        let id = SessionId(1);
        host.open(id, &idle_command(), size(), opts())
            .expect("open session");
        assert_eq!(host.len(), 1);
        let session = host.get(id).expect("session");
        assert!(!session.is_closed(), "a fresh session owns its pty");
        assert!(
            session.root_pid().is_some(),
            "a daemon-created session must have a real child process"
        );
        assert!(session.live_children() >= 1);
        host.close(id).expect("close");
    }

    #[test]
    fn closing_a_session_releases_it_and_a_second_close_is_a_no_op() {
        let backend = pipes();
        let session = PtySession::spawn(Arc::clone(&backend), &idle_command(), size(), opts())
            .expect("spawn");
        let first = session.close().expect("first close");
        assert!(!first.already_closed);
        assert!(first.reaped, "close must wait for the reap");
        assert_eq!(first.live_children, 0, "PTY-ORPHAN-1: no live process left");
        assert!(session.is_closed());
        let again = session.close().expect("a second close must not fail");
        assert!(again.already_closed, "close must be idempotent: {again:?}");
        assert_eq!(again.live_children, 0);
    }

    #[test]
    fn closing_a_session_that_is_not_open_is_a_no_op() {
        let mut host = SessionHost::new(pipes());
        let outcome = host
            .close(SessionId(9))
            .expect("unknown id is not an error");
        assert!(outcome.already_closed);
        assert_eq!(outcome.live_children, 0);
        assert!(host.is_empty());
    }

    #[test]
    fn a_second_open_for_the_same_id_is_refused() {
        let mut host = SessionHost::new(pipes());
        let id = SessionId(3);
        host.open(id, &idle_command(), size(), opts())
            .expect("first open");
        assert!(matches!(
            host.open(id, &idle_command(), size(), opts()),
            Err(HostError::AlreadyOpen(_))
        ));
        assert_eq!(host.ids(), vec![id]);
        host.close(id).expect("close");
        assert!(host.is_empty(), "a closed session leaves no entry behind");
    }

    #[test]
    fn the_probed_backend_can_open_and_close_a_session() {
        let mut host = SessionHost::probed();
        let id = SessionId(5);
        host.open(id, &idle_command(), size(), opts())
            .expect("open on the probed backend");
        let outcome = host.close(id).expect("close on the probed backend");
        assert!(!outcome.already_closed);
        assert_eq!(outcome.live_children, 0);
    }

    #[test]
    fn close_all_leaves_no_hosted_session_behind() {
        let mut host = SessionHost::new(pipes());
        let (a, b) = (SessionId(1), SessionId(2));
        host.open(a, &idle_command(), size(), opts())
            .expect("open a");
        host.open(b, &idle_command(), size(), opts())
            .expect("open b");
        let results = host.close_all();
        assert_eq!(results.len(), 2);
        for (id, outcome) in results {
            let outcome = outcome.unwrap_or_else(|err| panic!("close {id:?} failed: {err}"));
            assert_eq!(outcome.live_children, 0);
        }
        assert!(host.is_empty());
    }
}
