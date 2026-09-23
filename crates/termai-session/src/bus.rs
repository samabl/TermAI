//! Subscription contract, backpressure and AI-off zero overhead (kernel/04 section 3.7).
//!
//! Non-negotiable: every drop is counted and visible (no silent drops), and a slow
//! subscriber must never block the write path. When there are no subscribers, publish
//! takes the ZeroOverhead branch and performs no work (AR-03 / AC-S9).

use termai_core::SessionId;

/// Event classes (kernel/04 section 3.7 table).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum EventClass {
    /// C0 control: StateChange / LeaseEvent / Resize. Must never be lost.
    C0Control,
    /// C1 metadata: CmdEnd / CwdChange / TitleChange / ContextEvent.
    C1Metadata,
    /// C2 raw bytes: PtyOut increments.
    C2RawBytes,
    /// C3 large objects: CAS references.
    C3LargeObject,
}

impl EventClass {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            EventClass::C0Control => 0,
            EventClass::C1Metadata => 1,
            EventClass::C2RawBytes => 2,
            EventClass::C3LargeObject => 3,
        }
    }

    /// Default overflow policy. C0 disconnects rather than losing an event.
    #[must_use]
    pub const fn default_policy(self) -> DropPolicy {
        match self {
            EventClass::C0Control => DropPolicy::Disconnect,
            EventClass::C1Metadata => DropPolicy::DropOldest,
            EventClass::C2RawBytes => DropPolicy::Coalesce { ms: 16 },
            EventClass::C3LargeObject => DropPolicy::CasRef,
        }
    }

    /// Per-subscription credit default (256 KiB, cap 1 MiB).
    #[must_use]
    pub const fn default_credit(self) -> u32 {
        match self {
            EventClass::C0Control => 64 * 1024,
            EventClass::C1Metadata => 256 * 1024,
            EventClass::C2RawBytes => 256 * 1024,
            EventClass::C3LargeObject => 64 * 1024,
        }
    }
}

/// Maximum credit any subscription may request.
pub const MAX_CREDIT_BYTES: u32 = 1 << 20;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DropPolicy {
    /// C0: never drop; disconnect the subscriber instead.
    Disconnect,
    DropOldest,
    /// C2: coalesce within the window, then drop oldest.
    Coalesce {
        ms: u16,
    },
    /// C3: the reference is never dropped; only content is collected.
    CasRef,
}

impl DropPolicy {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            DropPolicy::Disconnect => 0,
            DropPolicy::DropOldest => 1,
            DropPolicy::Coalesce { .. } => 2,
            DropPolicy::CasRef => 3,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SubscribeSpec {
    pub sub_id: u64,
    pub session: SessionId,
    pub classes: Vec<EventClass>,
    pub credit_bytes: u32,
    pub policies: Vec<(EventClass, DropPolicy)>,
}

impl SubscribeSpec {
    #[must_use]
    pub fn for_classes(sub_id: u64, session: SessionId, classes: Vec<EventClass>) -> Self {
        let credit = classes
            .iter()
            .map(|c| c.default_credit())
            .min()
            .unwrap_or(EventClass::C1Metadata.default_credit());
        Self {
            sub_id,
            session,
            classes,
            credit_bytes: credit,
            policies: Vec::new(),
        }
    }

    #[must_use]
    pub fn policy_for(&self, class: EventClass) -> DropPolicy {
        self.policies
            .iter()
            .find(|(c, _)| *c == class)
            .map_or_else(|| class.default_policy(), |(_, p)| *p)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SubscriptionHandle {
    pub sub_id: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SubscribeError {
    DuplicateSub(u64),
    NoClasses,
    CreditTooLarge(u32),
}

/// A published event. The payload is opaque to the bus; classification is explicit.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SessionEvent {
    pub session: SessionId,
    pub class: EventClass,
    pub kind: u16,
    pub payload: Vec<u8>,
}

/// One subscription's accounting.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Subscription {
    pub handle: SubscriptionHandle,
    pub session: SessionId,
    pub classes: Vec<EventClass>,
    pub credit_bytes: u32,
    pub used_bytes: u64,
    pub policies: Vec<(EventClass, DropPolicy)>,
    pub dropped_bytes: u64,
    pub dropped_events: u64,
    pub connected: bool,
}

impl Subscription {
    #[must_use]
    pub fn policy_for(&self, class: EventClass) -> DropPolicy {
        self.policies
            .iter()
            .find(|(c, _)| *c == class)
            .map_or_else(|| class.default_policy(), |(_, p)| *p)
    }
}

/// A visible drop notice. Every drop produces exactly one of these.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DropNotice {
    pub sub_id: u64,
    pub class: EventClass,
    pub policy: DropPolicy,
    pub dropped_bytes: u64,
    pub dropped_events: u64,
    /// True when the subscriber was disconnected instead of dropping.
    pub disconnected: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PublishOutcome {
    /// No subscribers: nothing was serialised, allocated or locked.
    ZeroOverhead,
    Delivered {
        n: usize,
    },
    Dropped {
        n: usize,
        bytes: u64,
    },
    Disconnected {
        sub_id: u64,
    },
}

/// Publisher-side accounting, exposed for the subscription-drop gate.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct PublisherStats {
    pub zero_overhead_publishes: u64,
    pub delivered: u64,
    pub dropped_events: u64,
    pub dropped_bytes: u64,
}

/// The event bus.
#[derive(Clone, Default, Debug)]
pub struct EventBus {
    subs: Vec<Subscription>,
    notices: Vec<DropNotice>,
    stats: PublisherStats,
}

impl EventBus {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            subs: Vec::new(),
            notices: Vec::new(),
            stats: PublisherStats {
                zero_overhead_publishes: 0,
                delivered: 0,
                dropped_events: 0,
                dropped_bytes: 0,
            },
        }
    }

    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.subs.iter().filter(|s| s.connected).count()
    }

    #[must_use]
    pub fn drop_notices(&self) -> &[DropNotice] {
        &self.notices
    }

    #[must_use]
    pub const fn stats(&self) -> &PublisherStats {
        &self.stats
    }

    /// True when publish would take the zero-overhead branch (AI off by default).
    #[must_use]
    pub fn is_zero_overhead(&self) -> bool {
        self.subscriber_count() == 0
    }

    pub fn subscribe(&mut self, spec: SubscribeSpec) -> Result<SubscriptionHandle, SubscribeError> {
        if self.subs.iter().any(|s| s.handle.sub_id == spec.sub_id) {
            return Err(SubscribeError::DuplicateSub(spec.sub_id));
        }
        if spec.classes.is_empty() {
            return Err(SubscribeError::NoClasses);
        }
        if spec.credit_bytes > MAX_CREDIT_BYTES {
            return Err(SubscribeError::CreditTooLarge(spec.credit_bytes));
        }
        let h = SubscriptionHandle {
            sub_id: spec.sub_id,
        };
        self.subs.push(Subscription {
            handle: h,
            session: spec.session,
            classes: spec.classes,
            credit_bytes: spec.credit_bytes,
            used_bytes: 0,
            policies: spec.policies,
            dropped_bytes: 0,
            dropped_events: 0,
            connected: true,
        });
        Ok(h)
    }

    pub fn unsubscribe(&mut self, h: SubscriptionHandle) {
        self.subs.retain(|s| s.handle != h);
    }

    /// Publish an event. Zero work when nobody is subscribed.
    pub fn publish(&mut self, ev: &SessionEvent) -> PublishOutcome {
        if self.is_zero_overhead() {
            self.stats.zero_overhead_publishes += 1;
            return PublishOutcome::ZeroOverhead;
        }
        let len = ev.payload.len() as u64;
        let mut delivered = 0usize;
        let mut dropped = 0usize;
        let mut dropped_bytes = 0u64;
        let mut disconnected = None;

        for sub in &mut self.subs {
            if !sub.connected || !sub.classes.contains(&ev.class) {
                continue;
            }
            let policy = sub.policy_for(ev.class);
            if sub.used_bytes + len <= u64::from(sub.credit_bytes) {
                sub.used_bytes += len;
                delivered += 1;
                continue;
            }
            match policy {
                DropPolicy::Disconnect => {
                    sub.connected = false;
                    sub.dropped_events += 1;
                    sub.dropped_bytes += len;
                    self.notices.push(DropNotice {
                        sub_id: sub.handle.sub_id,
                        class: ev.class,
                        policy,
                        dropped_bytes: sub.dropped_bytes,
                        dropped_events: sub.dropped_events,
                        disconnected: true,
                    });
                    disconnected = Some(sub.handle.sub_id);
                }
                DropPolicy::DropOldest | DropPolicy::Coalesce { .. } | DropPolicy::CasRef => {
                    // Coalescing reclaims the oldest bytes; the notice is never silent.
                    let credit = u64::from(sub.credit_bytes);
                    let reclaim = (sub.used_bytes + len).saturating_sub(credit);
                    sub.used_bytes = sub.used_bytes.saturating_sub(reclaim).min(credit);
                    sub.dropped_events += 1;
                    sub.dropped_bytes += reclaim.max(len);
                    self.notices.push(DropNotice {
                        sub_id: sub.handle.sub_id,
                        class: ev.class,
                        policy,
                        dropped_bytes: sub.dropped_bytes,
                        dropped_events: sub.dropped_events,
                        disconnected: false,
                    });
                    dropped += 1;
                    dropped_bytes += reclaim.max(len);
                }
            }
        }

        self.stats.delivered += delivered as u64;
        self.stats.dropped_events += dropped as u64;
        self.stats.dropped_bytes += dropped_bytes;

        if let Some(sub_id) = disconnected {
            return PublishOutcome::Disconnected { sub_id };
        }
        if dropped > 0 && delivered == 0 {
            return PublishOutcome::Dropped {
                n: dropped,
                bytes: dropped_bytes,
            };
        }
        PublishOutcome::Delivered { n: delivered }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(class: EventClass, len: usize) -> SessionEvent {
        SessionEvent {
            session: SessionId(1),
            class,
            kind: 1,
            payload: vec![0u8; len],
        }
    }

    #[test]
    fn publish_with_no_subscribers_is_zero_overhead() {
        let mut bus = EventBus::new();
        assert!(bus.is_zero_overhead());
        let out = bus.publish(&ev(EventClass::C2RawBytes, 4096));
        assert_eq!(out, PublishOutcome::ZeroOverhead);
        assert_eq!(bus.stats().zero_overhead_publishes, 1);
        assert_eq!(bus.stats().delivered, 0);
        assert!(bus.drop_notices().is_empty());
    }

    #[test]
    fn subscription_rejects_duplicates_and_oversized_credit() {
        let mut bus = EventBus::new();
        let spec = SubscribeSpec::for_classes(1, SessionId(1), vec![EventClass::C1Metadata]);
        bus.subscribe(spec.clone()).unwrap();
        assert_eq!(bus.subscribe(spec), Err(SubscribeError::DuplicateSub(1)));
        let mut big = SubscribeSpec::for_classes(2, SessionId(1), vec![EventClass::C2RawBytes]);
        big.credit_bytes = MAX_CREDIT_BYTES + 1;
        assert_eq!(
            bus.subscribe(big),
            Err(SubscribeError::CreditTooLarge(MAX_CREDIT_BYTES + 1))
        );
        let empty = SubscribeSpec::for_classes(3, SessionId(1), vec![]);
        assert_eq!(bus.subscribe(empty), Err(SubscribeError::NoClasses));
    }

    #[test]
    fn c0_overflow_disconnects_rather_than_dropping() {
        let mut bus = EventBus::new();
        let mut spec = SubscribeSpec::for_classes(7, SessionId(1), vec![EventClass::C0Control]);
        spec.credit_bytes = 64;
        bus.subscribe(spec).unwrap();
        assert_eq!(
            bus.publish(&ev(EventClass::C0Control, 64)),
            PublishOutcome::Delivered { n: 1 }
        );
        let out = bus.publish(&ev(EventClass::C0Control, 64));
        assert_eq!(out, PublishOutcome::Disconnected { sub_id: 7 });
        assert_eq!(bus.subscriber_count(), 0);
        let n = bus.drop_notices();
        assert_eq!(n.len(), 1);
        assert!(n[0].disconnected);
        assert_eq!(n[0].class, EventClass::C0Control);
    }

    #[test]
    fn c1_overflow_drops_oldest_and_is_always_visible() {
        let mut bus = EventBus::new();
        let mut spec = SubscribeSpec::for_classes(9, SessionId(1), vec![EventClass::C1Metadata]);
        spec.credit_bytes = 128;
        bus.subscribe(spec).unwrap();
        bus.publish(&ev(EventClass::C1Metadata, 128));
        let out = bus.publish(&ev(EventClass::C1Metadata, 64));
        assert!(matches!(out, PublishOutcome::Dropped { .. }));
        let n = bus.drop_notices();
        assert_eq!(n.len(), 1);
        assert!(!n[0].disconnected);
        assert_eq!(n[0].policy, DropPolicy::DropOldest);
        assert!(
            bus.subscriber_count() == 1,
            "drop must not disconnect a C1 subscriber"
        );
    }

    #[test]
    fn unsubscribed_classes_are_not_counted() {
        let mut bus = EventBus::new();
        bus.subscribe(SubscribeSpec::for_classes(
            3,
            SessionId(1),
            vec![EventClass::C1Metadata],
        ))
        .unwrap();
        let out = bus.publish(&ev(EventClass::C2RawBytes, 16));
        assert_eq!(out, PublishOutcome::Delivered { n: 0 });
        assert!(bus.drop_notices().is_empty());
    }

    #[test]
    fn default_policies_match_the_spec_table() {
        assert_eq!(
            EventClass::C0Control.default_policy(),
            DropPolicy::Disconnect
        );
        assert_eq!(
            EventClass::C1Metadata.default_policy(),
            DropPolicy::DropOldest
        );
        assert_eq!(
            EventClass::C2RawBytes.default_policy(),
            DropPolicy::Coalesce { ms: 16 }
        );
        assert_eq!(
            EventClass::C3LargeObject.default_policy(),
            DropPolicy::CasRef
        );
    }

    #[test]
    fn unsubscribe_removes_the_subscriber() {
        let mut bus = EventBus::new();
        let h = bus
            .subscribe(SubscribeSpec::for_classes(
                5,
                SessionId(1),
                vec![EventClass::C1Metadata],
            ))
            .unwrap();
        assert_eq!(bus.subscriber_count(), 1);
        bus.unsubscribe(h);
        assert_eq!(bus.subscriber_count(), 0);
        assert_eq!(
            bus.publish(&ev(EventClass::C1Metadata, 1)),
            PublishOutcome::ZeroOverhead
        );
    }
}
