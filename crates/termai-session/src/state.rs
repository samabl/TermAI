//! Session state machine (kernel/04 section 3.1).
//!
//! The transition table is the single authority: a transition not in the table is
//! rejected (None), never silently allowed. Invariants: Dead has no out-edges; only
//! Running/Detached may enter Recovering; every transition is logged BEFORE it is
//! broadcast (persist then publish).

use termai_core::SessionId;

/// Session state (kernel/04 section 3.1). Do not confuse with AgentSession (DC-03).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SessionState {
    /// Record and resources allocated; the PTY has not spawned yet.
    Created,
    /// PTY master readable; an Interactive attach or a headless driver exists.
    Running,
    /// No Interactive attach (lease free); PTY and children keep running.
    Detached,
    /// Child tree reaped, exit code captured; PTY handle pending release.
    Exited,
    /// After a sessiond restart: checkpoint + tail replay. Writes are refused.
    Recovering,
    /// Terminal state: handles released, Log kept read-only.
    Dead,
}

impl SessionState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            SessionState::Created => "created",
            SessionState::Running => "running",
            SessionState::Detached => "detached",
            SessionState::Exited => "exited",
            SessionState::Recovering => "recovering",
            SessionState::Dead => "dead",
        }
    }

    /// Dead is terminal: no out-edges.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, SessionState::Dead)
    }

    /// The screen (not the process) can be restored from Running/Detached.
    #[must_use]
    pub const fn allows_screen_recovery(self) -> bool {
        matches!(self, SessionState::Running | SessionState::Detached)
    }

    /// Writes are refused in Recovering (kernel/04 section 3.1).
    #[must_use]
    pub const fn accepts_writes(self) -> bool {
        matches!(self, SessionState::Running)
    }
}

/// Attach mode (kernel/04 section 3.4).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AttachMode {
    ReadOnly,
    Interactive,
}

/// Stable client identity (uid/SID-verified at the transport boundary).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct ClientId(pub u128);

/// Who performed an action (audit field, DC-34).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ActorKind {
    Human,
    Agent,
    Plugin,
    Policy,
    System,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Actor {
    pub kind: ActorKind,
    pub id: u64,
}

impl Actor {
    #[must_use]
    pub const fn system() -> Self {
        Self {
            kind: ActorKind::System,
            id: 0,
        }
    }
}

/// Close reason. These four strings are the Log contract (kernel/04 section 3.8).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CloseReason {
    /// spawn failed or 5s timeout.
    SpawnFailed,
    /// checkpoint and tail both unverifiable.
    Unrecoverable,
    /// reaper reclaimed an exited session.
    Reaped,
    /// explicit user close (kill included). L2+ requires confirmation (AR-06).
    Closed,
}

impl CloseReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            CloseReason::SpawnFailed => "spawn_failed",
            CloseReason::Unrecoverable => "unrecoverable",
            CloseReason::Reaped => "reaped",
            CloseReason::Closed => "closed",
        }
    }
}

/// Every trigger in the transition table.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Trigger {
    /// PtyBackend::spawn succeeded.
    SpawnOk,
    /// spawn failed or timed out (5s).
    SpawnFail,
    /// Last Interactive attach detached, or the lease expired.
    LastDetach,
    /// A new Interactive attach acquired the lease.
    InteractiveAttach,
    /// PTY reader EOF and the process tree was reaped.
    EofReaped,
    /// sessiond restart with a non-closed session.
    SessiondRestart,
    /// Rebuild succeeded and grid_digest verified.
    RebuildOk,
    /// Rebuild failed.
    RebuildFail,
    /// Reaper reclaimed the PTY / Job Object.
    Reaped,
    /// Explicit user close.
    UserClose,
}

/// Result of a transition request.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Transition {
    pub from: SessionState,
    pub to: SessionState,
    pub reason: Option<CloseReason>,
}

/// The one authority for legal transitions (kernel/04 section 3.1 table).
#[must_use]
pub fn transition(from: SessionState, trigger: Trigger) -> Option<Transition> {
    use CloseReason as R;
    use SessionState as S;
    use Trigger as T;

    let (to, reason) = match (from, trigger) {
        (S::Created, T::SpawnOk) => (S::Running, None),
        (S::Created, T::SpawnFail) => (S::Dead, Some(R::SpawnFailed)),
        (S::Running, T::LastDetach) => (S::Detached, None),
        (S::Detached, T::InteractiveAttach) => (S::Running, None),
        (S::Running | S::Detached, T::EofReaped) => (S::Exited, None),
        (S::Running | S::Detached, T::SessiondRestart) => (S::Recovering, None),
        (S::Recovering, T::RebuildOk) => (S::Detached, None),
        (S::Recovering, T::RebuildFail) => (S::Dead, Some(R::Unrecoverable)),
        (S::Exited, T::Reaped) => (S::Dead, Some(R::Reaped)),
        (_, T::UserClose) => (S::Dead, Some(R::Closed)),
        _ => return None,
    };
    Some(Transition { from, to, reason })
}

/// Live state machine with a guarded apply step.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Machine {
    state: SessionState,
    resumed_from: Option<u64>,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: SessionState::Created,
            resumed_from: None,
        }
    }

    #[must_use]
    pub const fn state(&self) -> SessionState {
        self.state
    }

    #[must_use]
    pub const fn resumed_from(&self) -> Option<u64> {
        self.resumed_from
    }

    /// Apply a trigger. Rejected transitions leave the state untouched.
    pub fn apply(&mut self, trigger: Trigger) -> Option<Transition> {
        let t = transition(self.state, trigger)?;
        self.state = t.to;
        Some(t)
    }

    /// Record the checkpoint a recovery resumed from (AR-13 honest reporting).
    pub fn set_resumed_from(&mut self, ckpt_id: u64) {
        self.resumed_from = Some(ckpt_id);
    }

    /// Bind the session id, used for Log correlation.
    #[must_use]
    pub fn session(&self, id: SessionId) -> (SessionId, SessionState) {
        (id, self.state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SessionState as S;
    use Trigger as T;

    #[test]
    fn happy_path_reaches_dead() {
        let mut m = Machine::new();
        assert_eq!(m.apply(T::SpawnOk).unwrap().to, S::Running);
        assert_eq!(m.apply(T::LastDetach).unwrap().to, S::Detached);
        assert_eq!(m.apply(T::InteractiveAttach).unwrap().to, S::Running);
        assert_eq!(m.apply(T::EofReaped).unwrap().to, S::Exited);
        assert_eq!(m.apply(T::Reaped).unwrap().to, S::Dead);
        assert!(m.state().is_terminal());
    }

    #[test]
    fn dead_has_no_out_edges() {
        for t in [
            T::SpawnOk,
            T::SpawnFail,
            T::LastDetach,
            T::InteractiveAttach,
            T::EofReaped,
            T::SessiondRestart,
            T::RebuildOk,
            T::RebuildFail,
            T::Reaped,
        ] {
            assert_eq!(transition(S::Dead, t), None, "dead out-edge via {t:?}");
        }
    }

    #[test]
    fn only_running_or_detached_can_recover() {
        assert!(transition(S::Running, T::SessiondRestart).is_some());
        assert!(transition(S::Detached, T::SessiondRestart).is_some());
        assert_eq!(transition(S::Created, T::SessiondRestart), None);
        assert_eq!(transition(S::Exited, T::SessiondRestart), None);
        assert_eq!(transition(S::Recovering, T::SessiondRestart), None);
    }

    #[test]
    fn spawn_failure_goes_dead_not_running() {
        let t = transition(S::Created, T::SpawnFail).unwrap();
        assert_eq!(t.to, S::Dead);
        assert_eq!(t.reason, Some(CloseReason::SpawnFailed));
    }

    #[test]
    fn recovery_failure_is_unrecoverable() {
        let t = transition(S::Recovering, T::RebuildFail).unwrap();
        assert_eq!(t.to, S::Dead);
        assert_eq!(t.reason.unwrap().as_str(), "unrecoverable");
    }

    #[test]
    fn user_close_works_from_every_state_except_dead() {
        for s in [
            S::Created,
            S::Running,
            S::Detached,
            S::Exited,
            S::Recovering,
        ] {
            let t = transition(s, T::UserClose).unwrap();
            assert_eq!(t.to, S::Dead);
            assert_eq!(t.reason.unwrap().as_str(), "closed");
        }
    }

    #[test]
    fn writes_only_in_running() {
        assert!(S::Running.accepts_writes());
        for s in [S::Created, S::Detached, S::Exited, S::Recovering, S::Dead] {
            assert!(!s.accepts_writes(), "{s:?} must refuse writes");
        }
    }

    #[test]
    fn rejected_transition_leaves_state_untouched() {
        let mut m = Machine::new();
        assert_eq!(m.apply(T::EofReaped), None);
        assert_eq!(m.state(), S::Created);
    }
}
