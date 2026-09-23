//! stdin single-writer lease (kernel/04 section 3.4, OQ-30).
//!
//! Default TTL 30s, renew 10s. Two consecutive missed renews release the lease.
//! Takeover is denied by default: only explicit expiry or a user-confirmed takeover
//! (holder detached or silent for 15s) may move the writer.

use core::time::Duration;

use termai_core::time::MonoTime;

use termai_core::SessionId;

use crate::state::{Actor, ClientId};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LeaseId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LeaseState {
    Free,
    Held {
        holder: ClientId,
        acquired_at: MonoTime,
        expires_at: MonoTime,
        missed_renews: u8,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LeaseAction {
    Grant,
    Renew,
    Transfer,
    Revoke,
    Expire,
    Takeover,
    Deny,
}

impl LeaseAction {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            LeaseAction::Grant => 0,
            LeaseAction::Renew => 1,
            LeaseAction::Transfer => 2,
            LeaseAction::Revoke => 3,
            LeaseAction::Expire => 4,
            LeaseAction::Takeover => 5,
            LeaseAction::Deny => 6,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LeaseEvent {
    pub session: SessionId,
    pub action: LeaseAction,
    pub from: u64,
    pub to: u64,
    pub approver: u64,
    pub at: MonoTime,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LeaseError {
    HeldByOther { holder: ClientId },
    NoLease,
    Expired,
    NotHolder,
    TakeoverDisabled,
    NotConfirmed,
}

/// Default TTL / renew interval (kernel/04 section 3.4).
pub const DEFAULT_TTL: Duration = Duration::from_secs(30);
pub const DEFAULT_RENEW: Duration = Duration::from_secs(10);
/// A holder silent this long may be taken over, with user confirmation.
pub const TAKEOVER_SILENCE: Duration = Duration::from_secs(15);

/// The single lease authority.
#[derive(Clone, Debug)]
pub struct LeaseManager {
    session: SessionId,
    state: LeaseState,
    next_id: u64,
    holder_id: Option<LeaseId>,
    ttl: Duration,
    renew: Duration,
    takeover_enabled: bool,
    events: Vec<LeaseEvent>,
}

impl LeaseManager {
    #[must_use]
    pub const fn new(session: SessionId) -> Self {
        Self {
            session,
            state: LeaseState::Free,
            next_id: 1,
            holder_id: None,
            ttl: DEFAULT_TTL,
            renew: DEFAULT_RENEW,
            takeover_enabled: false,
            events: Vec::new(),
        }
    }

    #[must_use]
    pub const fn state(&self) -> LeaseState {
        self.state
    }

    #[must_use]
    pub const fn holder(&self) -> Option<ClientId> {
        match self.state {
            LeaseState::Held { holder, .. } => Some(holder),
            LeaseState::Free => None,
        }
    }

    #[must_use]
    pub const fn ttl(&self) -> Duration {
        self.ttl
    }

    #[must_use]
    pub const fn renew_interval(&self) -> Duration {
        self.renew
    }

    #[must_use]
    pub fn events(&self) -> &[LeaseEvent] {
        &self.events
    }

    pub fn set_takeover_enabled(&mut self, enabled: bool) {
        self.takeover_enabled = enabled;
    }

    fn push(&mut self, action: LeaseAction, from: u64, to: u64, approver: u64, at: MonoTime) {
        self.events.push(LeaseEvent {
            session: self.session,
            action,
            from,
            to,
            approver,
            at,
        });
    }

    /// First Interactive attach while Free grants automatically (writes a LeaseEvent).
    pub fn acquire(&mut self, client: ClientId, now: MonoTime) -> Result<LeaseId, LeaseError> {
        match self.state {
            LeaseState::Held {
                holder, expires_at, ..
            } if !expires_at.has_passed(now) => {
                let h = holder.0 as u64;
                self.push(LeaseAction::Deny, client.0 as u64, h, 0, now);
                Err(LeaseError::HeldByOther { holder })
            }
            _ => {
                let id = LeaseId(self.next_id);
                self.next_id += 1;
                self.state = LeaseState::Held {
                    holder: client,
                    acquired_at: now,
                    expires_at: now.saturating_add(self.ttl),
                    missed_renews: 0,
                };
                self.holder_id = Some(id);
                self.push(LeaseAction::Grant, 0, client.0 as u64, 0, now);
                Ok(id)
            }
        }
    }

    pub fn renew(&mut self, id: LeaseId, now: MonoTime) -> Result<(), LeaseError> {
        if self.holder_id != Some(id) {
            return Err(LeaseError::NotHolder);
        }
        match self.state {
            LeaseState::Held {
                holder,
                acquired_at,
                ..
            } => {
                if self.expired_at(now) {
                    return Err(LeaseError::Expired);
                }
                self.state = LeaseState::Held {
                    holder,
                    acquired_at,
                    expires_at: now.saturating_add(self.ttl),
                    missed_renews: 0,
                };
                self.push(LeaseAction::Renew, holder.0 as u64, holder.0 as u64, 0, now);
                Ok(())
            }
            LeaseState::Free => Err(LeaseError::NoLease),
        }
    }

    /// A missed renew is only counted when the deadline passed; two in a row release.
    pub fn miss_renew(&mut self, now: MonoTime) -> Vec<LeaseEvent> {
        let before = self.events.len();
        if let LeaseState::Held {
            holder,
            acquired_at,
            expires_at,
            missed_renews,
        } = self.state
        {
            if expires_at.has_passed(now) {
                if missed_renews + 1 >= 2 {
                    self.state = LeaseState::Free;
                    self.holder_id = None;
                    self.push(LeaseAction::Expire, holder.0 as u64, 0, 0, now);
                } else {
                    self.state = LeaseState::Held {
                        holder,
                        acquired_at,
                        expires_at: now.saturating_add(self.renew),
                        missed_renews: missed_renews + 1,
                    };
                }
            }
        }
        self.events[before..].to_vec()
    }

    fn expired_at(&self, now: MonoTime) -> bool {
        match self.state {
            LeaseState::Held { expires_at, .. } => expires_at.has_passed(now),
            LeaseState::Free => true,
        }
    }

    /// Transfer requires the receiver to ACK; both sides get a LeaseEvent.
    pub fn transfer(
        &mut self,
        id: LeaseId,
        to: ClientId,
        approver: Actor,
        now: MonoTime,
    ) -> Result<LeaseId, LeaseError> {
        if self.holder_id != Some(id) {
            return Err(LeaseError::NotHolder);
        }
        let from = self.holder().ok_or(LeaseError::NoLease)?;
        let new_id = LeaseId(self.next_id);
        self.next_id += 1;
        self.state = LeaseState::Held {
            holder: to,
            acquired_at: now,
            expires_at: now.saturating_add(self.ttl),
            missed_renews: 0,
        };
        self.holder_id = Some(new_id);
        self.push(
            LeaseAction::Transfer,
            from.0 as u64,
            to.0 as u64,
            approver.id,
            now,
        );
        Ok(new_id)
    }

    /// Explicit revoke only (never implicit).
    pub fn revoke(&mut self, id: LeaseId, by: Actor, now: MonoTime) -> Result<(), LeaseError> {
        if self.holder_id != Some(id) {
            return Err(LeaseError::NotHolder);
        }
        let from = self.holder().ok_or(LeaseError::NoLease)?;
        self.state = LeaseState::Free;
        self.holder_id = None;
        self.push(LeaseAction::Revoke, from.0 as u64, 0, by.id, now);
        Ok(())
    }

    /// Takeover. Default DENIED. Requires policy enabled, human confirmation, and the
    /// holder either detached or silent for 15s. Never "last one wins".
    pub fn takeover(
        &mut self,
        to: ClientId,
        confirmed_by_human: bool,
        holder_detached: bool,
        now: MonoTime,
    ) -> Result<LeaseId, LeaseError> {
        if !self.takeover_enabled {
            return Err(LeaseError::TakeoverDisabled);
        }
        if !confirmed_by_human {
            return Err(LeaseError::NotConfirmed);
        }
        let acquired = match self.state {
            LeaseState::Held { acquired_at, .. } => acquired_at,
            LeaseState::Free => {
                return Err(LeaseError::NoLease);
            }
        };
        let silent_long_enough = acquired.saturating_add(TAKEOVER_SILENCE).has_passed(now);
        if !holder_detached && !silent_long_enough {
            return Err(LeaseError::HeldByOther {
                holder: self.holder().unwrap_or(ClientId(0)),
            });
        }
        let from = self.holder().unwrap_or(ClientId(0));
        let new_id = LeaseId(self.next_id);
        self.next_id += 1;
        self.state = LeaseState::Held {
            holder: to,
            acquired_at: now,
            expires_at: now.saturating_add(self.ttl),
            missed_renews: 0,
        };
        self.holder_id = Some(new_id);
        self.push(LeaseAction::Takeover, from.0 as u64, to.0 as u64, 0, now);
        Ok(new_id)
    }

    /// CAP-1 gate: only the lease holder may write stdin.
    pub fn authorize_write(&self, client: ClientId, now: MonoTime) -> Result<(), LeaseError> {
        match self.holder() {
            Some(h) if h == client => {
                if self.expired_at(now) {
                    Err(LeaseError::Expired)
                } else {
                    Ok(())
                }
            }
            Some(holder) => Err(LeaseError::HeldByOther { holder }),
            None => Err(LeaseError::NoLease),
        }
    }

    /// Expire the lease if its deadline passed.
    pub fn tick(&mut self, now: MonoTime) -> Vec<LeaseEvent> {
        let before = self.events.len();
        if let LeaseState::Held {
            holder, expires_at, ..
        } = self.state
        {
            if expires_at.has_passed(now) {
                self.state = LeaseState::Free;
                self.holder_id = None;
                self.push(LeaseAction::Expire, holder.0 as u64, 0, 0, now);
            }
        }
        self.events[before..].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(ms: u64) -> MonoTime {
        MonoTime::from_millis(ms)
    }

    #[test]
    fn only_one_writer_survives_ten_thousand_contentions() {
        let mut m = LeaseManager::new(SessionId(1));
        let mut granted = 0usize;
        for i in 0..10_000u128 {
            if m.acquire(ClientId(i), t(0)).is_ok() {
                granted += 1;
            }
        }
        assert_eq!(granted, 1, "exactly one writer may be granted (AC-S7)");
        assert_eq!(m.holder(), Some(ClientId(0)));
    }

    #[test]
    fn non_holder_write_is_denied_by_cap1_gate() {
        let mut m = LeaseManager::new(SessionId(1));
        m.acquire(ClientId(1), t(0)).unwrap();
        assert!(m.authorize_write(ClientId(1), t(1)).is_ok());
        assert_eq!(
            m.authorize_write(ClientId(2), t(1)),
            Err(LeaseError::HeldByOther {
                holder: ClientId(1)
            })
        );
    }

    #[test]
    fn two_missed_renews_release_the_lease() {
        let mut m = LeaseManager::new(SessionId(1));
        m.acquire(ClientId(1), t(0)).unwrap();
        assert!(m.miss_renew(t(31_000)).is_empty());
        assert_eq!(m.holder(), Some(ClientId(1)));
        let ev = m.miss_renew(t(42_000));
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].action, LeaseAction::Expire);
        assert_eq!(m.holder(), None);
    }

    #[test]
    fn renew_clears_missed_renew_count() {
        let mut m = LeaseManager::new(SessionId(1));
        let id = m.acquire(ClientId(1), t(0)).unwrap();
        m.miss_renew(t(31_000));
        m.renew(id, t(31_500)).unwrap();
        assert_eq!(m.holder(), Some(ClientId(1)));
        assert!(m.miss_renew(t(62_000)).is_empty());
        assert_eq!(m.holder(), Some(ClientId(1)));
    }

    #[test]
    fn tick_expires_and_emits_an_event() {
        let mut m = LeaseManager::new(SessionId(1));
        m.acquire(ClientId(1), t(0)).unwrap();
        assert!(m.tick(t(1_000)).is_empty());
        let ev = m.tick(t(30_001));
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].action, LeaseAction::Expire);
        assert_eq!(m.holder(), None);
    }

    #[test]
    fn transfer_requires_the_current_holder() {
        let mut m = LeaseManager::new(SessionId(1));
        let id = m.acquire(ClientId(1), t(0)).unwrap();
        assert_eq!(
            m.transfer(LeaseId(999), ClientId(2), Actor::system(), t(1)),
            Err(LeaseError::NotHolder)
        );
        let new_id = m.transfer(id, ClientId(2), Actor::system(), t(1)).unwrap();
        assert_ne!(new_id, id);
        assert_eq!(m.holder(), Some(ClientId(2)));
        assert_eq!(m.events().last().unwrap().action, LeaseAction::Transfer);
    }

    #[test]
    fn takeover_is_denied_by_default_and_needs_confirmation() {
        let mut m = LeaseManager::new(SessionId(1));
        m.acquire(ClientId(1), t(0)).unwrap();
        assert_eq!(
            m.takeover(ClientId(2), true, true, t(1)),
            Err(LeaseError::TakeoverDisabled)
        );
        m.set_takeover_enabled(true);
        assert_eq!(
            m.takeover(ClientId(2), false, true, t(1)),
            Err(LeaseError::NotConfirmed)
        );
        assert_eq!(
            m.takeover(ClientId(2), true, false, t(1)),
            Err(LeaseError::HeldByOther {
                holder: ClientId(1)
            })
        );
        let id = m.takeover(ClientId(2), true, true, t(1)).unwrap();
        assert_eq!(m.holder(), Some(ClientId(2)));
        assert_eq!(m.events().last().unwrap().action, LeaseAction::Takeover);
        assert!(m.renew(id, t(2)).is_ok());
    }

    #[test]
    fn defaults_match_the_spec() {
        let m = LeaseManager::new(SessionId(1));
        assert_eq!(m.ttl(), Duration::from_secs(30));
        assert_eq!(m.renew_interval(), Duration::from_secs(10));
    }
}
