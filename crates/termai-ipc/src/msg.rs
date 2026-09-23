//! Message type table (kernel/07 section 3.2). Numeric values are contract:
//! published values are never reused.

pub const HELLO: u16 = 0x0001;
pub const HELLO_ACK: u16 = 0x0002;
pub const HELLO_NACK: u16 = 0x0003;
pub const PING: u16 = 0x0004;
pub const PONG: u16 = 0x0005;
pub const GO_AWAY: u16 = 0x0006;
pub const ERROR: u16 = 0x0007;
pub const CREDIT_UPDATE: u16 = 0x0008;
pub const RESYNC_REQUEST: u16 = 0x0009;
pub const RESYNC_BEGIN: u16 = 0x000A;

/// Input (kernel/05 section 4.3 owns the name and the value).
pub const INPUT: u16 = 0x0100;
/// Paste (large payload, chunked, PasteGate semantics).
pub const PASTE: u16 = 0x0101;
pub const RESIZE: u16 = 0x0102;
pub const SIGNAL: u16 = 0x0103;

pub const GRID_SNAPSHOT: u16 = 0x0180;
pub const GRID_DELTA: u16 = 0x0181;
pub const PTY_BYTES: u16 = 0x0182;
pub const SCROLLBACK_CHUNK: u16 = 0x0183;

pub const CONTEXT_SUBSCRIBE: u16 = 0x0200;
pub const CONTEXT_UNSUBSCRIBE: u16 = 0x0201;
pub const CONTEXT_EVENT: u16 = 0x0202;
pub const CONTEXT_DROP_NOTICE: u16 = 0x0203;

pub const CAP_REF_PRESENT: u16 = 0x0300;
pub const CAP_REF_SEALED: u16 = 0x0301;
pub const CAP_DENIED: u16 = 0x0302;
pub const LEASE_ACQUIRE: u16 = 0x0304;
pub const LEASE_GRANT: u16 = 0x0305;
pub const LEASE_REVOKE: u16 = 0x0306;
pub const LEASE_TRANSFER: u16 = 0x0307;

pub const AUDIT_RECORD: u16 = 0x0400;
pub const AUDIT_BACKPRESSURE: u16 = 0x0401;

/// Session attach family (0x05xx). Allocated and frozen by ADR-0023 D1;
/// 0x0504..=0x05FF stays reserved, and assigning anything there needs a new ADR.
pub const ATTACH_REQUEST: u16 = 0x0500;
pub const ATTACH_ACK: u16 = 0x0501;
pub const TAIL_REPLAY: u16 = 0x0502;
pub const DETACH_NOTICE: u16 = 0x0503;

pub const EXPERIMENTAL_BASE: u16 = 0xF000;

/// How an unknown msg_type on a known section is handled (section 3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MsgClass {
    Known,
    /// Unknown type inside a known section: reply Error{UnsupportedMsg}, keep the link.
    KnownSection,
    /// Reserved section: reply Error{ReservedMsgType}.
    ReservedSection,
    /// Experimental: only with capability ipc.experimental.
    Experimental,
}

const KNOWN: &[u16] = &[
    HELLO,
    HELLO_ACK,
    HELLO_NACK,
    PING,
    PONG,
    GO_AWAY,
    ERROR,
    CREDIT_UPDATE,
    RESYNC_REQUEST,
    RESYNC_BEGIN,
    INPUT,
    PASTE,
    RESIZE,
    SIGNAL,
    GRID_SNAPSHOT,
    GRID_DELTA,
    PTY_BYTES,
    SCROLLBACK_CHUNK,
    CONTEXT_SUBSCRIBE,
    CONTEXT_UNSUBSCRIBE,
    CONTEXT_EVENT,
    CONTEXT_DROP_NOTICE,
    CAP_REF_PRESENT,
    CAP_REF_SEALED,
    CAP_DENIED,
    LEASE_ACQUIRE,
    LEASE_GRANT,
    LEASE_REVOKE,
    LEASE_TRANSFER,
    AUDIT_RECORD,
    AUDIT_BACKPRESSURE,
    ATTACH_REQUEST,
    ATTACH_ACK,
    TAIL_REPLAY,
    DETACH_NOTICE,
];

#[must_use]
pub fn classify(msg_type: u16) -> MsgClass {
    if KNOWN.contains(&msg_type) {
        return MsgClass::Known;
    }
    let section = msg_type & 0xFF00;
    match section {
        0x0000 | 0x0100 | 0x0200 | 0x0300 | 0x0400 | 0x0500 => MsgClass::KnownSection,
        0xF000 => MsgClass::Experimental,
        _ => MsgClass::ReservedSection,
    }
}

/// True when a frame of this type must produce an audit record when it acts (flag bit 8).
#[must_use]
pub const fn is_auditable_action(msg_type: u16) -> bool {
    matches!(
        msg_type,
        INPUT | PASTE | RESIZE | SIGNAL | CAP_REF_PRESENT | LEASE_ACQUIRE | LEASE_TRANSFER
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_values_are_stable() {
        assert_eq!(HELLO, 0x0001);
        assert_eq!(INPUT, 0x0100);
        assert_eq!(PASTE, 0x0101);
        assert_eq!(GRID_SNAPSHOT, 0x0180);
        assert_eq!(LEASE_ACQUIRE, 0x0304);
        assert_eq!(AUDIT_RECORD, 0x0400);
        assert_eq!(ATTACH_REQUEST, 0x0500);
        assert_eq!(ATTACH_ACK, 0x0501);
        assert_eq!(TAIL_REPLAY, 0x0502);
        assert_eq!(DETACH_NOTICE, 0x0503);
    }

    #[test]
    fn unknown_types_are_classified_without_disconnect() {
        assert_eq!(classify(PING), MsgClass::Known);
        assert_eq!(classify(0x00FF), MsgClass::KnownSection);
        assert_eq!(classify(0x01FF), MsgClass::KnownSection);
        assert_eq!(classify(ATTACH_REQUEST), MsgClass::Known);
        // The 0x05xx section is known (ADR-0023 D1); unassigned values inside it must
        // yield UnsupportedMsg rather than ReservedMsgType.
        assert_eq!(classify(0x0504), MsgClass::KnownSection);
        assert_eq!(classify(0x05FF), MsgClass::KnownSection);
        assert_eq!(classify(0x0600), MsgClass::ReservedSection);
        assert_eq!(classify(0xF001), MsgClass::Experimental);
    }
}
