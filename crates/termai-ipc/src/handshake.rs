//! Handshake: version negotiation + capability bitmap (kernel/07 section 3.3, DC-22).
//!
//! SPEC DEFECT SD-02: the negotiation rule says "a capability the client declares but
//! the server does not know and which is REQUIRED must be explicitly refused", but the
//! Hello struct only carries one undifferentiated caps list. Without a required/optional
//! split the rule is not implementable. This module adds Hello::required and documents
//! the deviation; it is additive (minor) and therefore within the N-2 compat promise.

use termai_core::capability::CapId;
use termai_core::error::handshake as reason_code;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClientKind {
    Ui,
    Agent,
    PluginHost,
    Cli,
    Ide,
    Ci,
}

impl ClientKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ClientKind::Ui => "ui",
            ClientKind::Agent => "agent",
            ClientKind::PluginHost => "plugin_host",
            ClientKind::Cli => "cli",
            ClientKind::Ide => "ide",
            ClientKind::Ci => "ci",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuthState {
    Anonymous,
    LocalPeer,
    Entitled,
    Degraded,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ShmOffer {
    pub slot_shift: u8,
    pub slot_count_shift: u8,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Limits {
    pub max_frame: u32,
    pub credits: u32,
    pub max_chunks: u16,
    pub shm: Option<ShmOffer>,
    pub audit_queue: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_frame: crate::frame::MAX_FRAME_LEN as u32,
            credits: 64,
            max_chunks: 1024,
            shm: None,
            audit_queue: 256,
        }
    }
}

/// Client hello. required is the additive field described in SD-02.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Hello {
    pub proto_min: u16,
    pub proto_max: u16,
    pub client_kind: ClientKind,
    pub caps: Vec<CapId>,
    /// Capabilities the client cannot work without.
    pub required: Vec<CapId>,
    pub session_claim: termai_core::SessionId,
    pub feature_bits: u128,
    pub nonce: [u8; 16],
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HelloAck {
    pub chosen_ver: u16,
    pub caps_inter: Vec<CapId>,
    pub limits: Limits,
    pub auth_state: AuthState,
    pub server_feature_bits: u128,
    /// Set when a declared optional capability was unknown to the server.
    pub degraded: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RefusedReason {
    VerUnsupported,
    CapUnknown,
    PeerDenied,
    HandshakeReplay,
    HandshakeTimeout,
}

impl RefusedReason {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            RefusedReason::VerUnsupported => reason_code::VER_UNSUPPORTED,
            RefusedReason::CapUnknown => reason_code::CAP_UNKNOWN,
            RefusedReason::PeerDenied => reason_code::PEER_DENIED,
            RefusedReason::HandshakeReplay => reason_code::HANDSHAKE_REPLAY,
            RefusedReason::HandshakeTimeout => reason_code::HANDSHAKE_TIMEOUT,
        }
    }

    /// CLI exit code (kernel/07 section 3.3 table).
    #[must_use]
    pub const fn cli_exit(self) -> i32 {
        match self {
            RefusedReason::HandshakeTimeout => 2,
            RefusedReason::PeerDenied | RefusedReason::HandshakeReplay => 3,
            RefusedReason::VerUnsupported | RefusedReason::CapUnknown => 4,
        }
    }

    #[must_use]
    pub const fn retryable(self) -> bool {
        matches!(self, RefusedReason::HandshakeTimeout)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Outcome {
    Negotiated {
        ack: HelloAck,
        /// Optional capabilities the client declared that the server does not know.
        unknown_optional: Vec<CapId>,
    },
    Refused(RefusedReason),
}

/// Server side policy.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ServerPolicy {
    pub proto_min: u16,
    pub proto_max: u16,
    pub caps: Vec<CapId>,
    /// Capabilities the server requires the client to understand.
    pub required: Vec<CapId>,
    pub limits: Limits,
    pub auth_state: AuthState,
    pub feature_bits: u128,
    /// nonces already seen (replay defence).
    pub seen_nonces: Vec<[u8; 16]>,
}

fn sorted_unique(mut caps: Vec<CapId>) -> Vec<CapId> {
    caps.sort_by_key(|c| c.0);
    caps.dedup();
    caps
}

/// Negotiation algorithm (kernel/07 section 3.3).
#[must_use]
pub fn negotiate(client: &Hello, server: &ServerPolicy) -> Outcome {
    if server.seen_nonces.contains(&client.nonce) {
        return Outcome::Refused(RefusedReason::HandshakeReplay);
    }
    let lo = client.proto_min.max(server.proto_min);
    let hi = client.proto_max.min(server.proto_max);
    if lo > hi {
        return Outcome::Refused(RefusedReason::VerUnsupported);
    }

    // Required capabilities: both directions must be understood, else refuse.
    for cap in &client.required {
        if !server.caps.contains(cap) {
            return Outcome::Refused(RefusedReason::CapUnknown);
        }
    }
    for cap in &server.required {
        if !client.caps.contains(cap) {
            return Outcome::Refused(RefusedReason::CapUnknown);
        }
    }

    let caps_inter = sorted_unique(
        client
            .caps
            .iter()
            .copied()
            .filter(|c| server.caps.contains(c))
            .collect(),
    );
    let unknown_optional = sorted_unique(
        client
            .caps
            .iter()
            .copied()
            .filter(|c| !server.caps.contains(c) && !client.required.contains(c))
            .collect(),
    );
    let degraded = !unknown_optional.is_empty();

    let ack = HelloAck {
        chosen_ver: hi,
        caps_inter,
        limits: server.limits.clone(),
        auth_state: if degraded {
            AuthState::Degraded
        } else {
            server.auth_state
        },
        server_feature_bits: server.feature_bits,
        degraded,
    };
    Outcome::Negotiated {
        ack,
        unknown_optional,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use termai_core::capability::{
        CAP_AUDIT_READ, CAP_IPC_EXPERIMENTAL, CAP_SESSION_READ, CAP_STDIN_WRITE,
    };

    fn client(min: u16, max: u16, caps: Vec<CapId>, required: Vec<CapId>) -> Hello {
        Hello {
            proto_min: min,
            proto_max: max,
            client_kind: ClientKind::Cli,
            caps,
            required,
            session_claim: termai_core::SessionId(1),
            feature_bits: 0,
            nonce: [7u8; 16],
        }
    }

    fn server() -> ServerPolicy {
        ServerPolicy {
            proto_min: 0x0001,
            proto_max: 0x0003,
            caps: vec![CAP_SESSION_READ, CAP_STDIN_WRITE, CAP_AUDIT_READ],
            required: vec![CAP_SESSION_READ],
            limits: Limits::default(),
            auth_state: AuthState::LocalPeer,
            feature_bits: 0,
            seen_nonces: Vec::new(),
        }
    }

    #[test]
    fn chooses_highest_common_version() {
        let out = negotiate(
            &client(
                0x0001,
                0x0002,
                vec![CAP_SESSION_READ],
                vec![CAP_SESSION_READ],
            ),
            &server(),
        );
        match out {
            Outcome::Negotiated { ack, .. } => assert_eq!(ack.chosen_ver, 0x0002),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn disjoint_version_ranges_are_refused_with_exit_code_4() {
        let out = negotiate(&client(0x0009, 0x000A, vec![], vec![]), &server());
        assert_eq!(out, Outcome::Refused(RefusedReason::VerUnsupported));
        assert_eq!(RefusedReason::VerUnsupported.cli_exit(), 4);
        assert_eq!(RefusedReason::VerUnsupported.code(), "VerUnsupported");
        assert!(!RefusedReason::VerUnsupported.retryable());
    }

    #[test]
    fn unknown_required_capability_is_refused() {
        let out = negotiate(
            &client(
                0x0001,
                0x0002,
                vec![CAP_IPC_EXPERIMENTAL],
                vec![CAP_IPC_EXPERIMENTAL],
            ),
            &server(),
        );
        assert_eq!(out, Outcome::Refused(RefusedReason::CapUnknown));
    }

    #[test]
    fn server_required_capability_must_be_declared_by_client() {
        let out = negotiate(
            &client(0x0001, 0x0002, vec![CAP_STDIN_WRITE], vec![]),
            &server(),
        );
        assert_eq!(out, Outcome::Refused(RefusedReason::CapUnknown));
    }

    #[test]
    fn unknown_optional_capability_degrades_but_negotiates() {
        let out = negotiate(
            &client(
                0x0001,
                0x0002,
                vec![CAP_SESSION_READ, CAP_IPC_EXPERIMENTAL],
                vec![CAP_SESSION_READ],
            ),
            &server(),
        );
        match out {
            Outcome::Negotiated {
                ack,
                unknown_optional,
            } => {
                assert!(ack.degraded);
                assert_eq!(ack.auth_state, AuthState::Degraded);
                assert_eq!(unknown_optional, vec![CAP_IPC_EXPERIMENTAL]);
                assert!(!ack.caps_inter.contains(&CAP_IPC_EXPERIMENTAL));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn capability_intersection_is_sorted_and_deduped() {
        let out = negotiate(
            &client(
                0x0001,
                0x0002,
                vec![CAP_STDIN_WRITE, CAP_SESSION_READ, CAP_STDIN_WRITE],
                vec![CAP_SESSION_READ],
            ),
            &server(),
        );
        match out {
            Outcome::Negotiated { ack, .. } => {
                assert_eq!(ack.caps_inter, vec![CAP_SESSION_READ, CAP_STDIN_WRITE]);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn replayed_nonce_is_refused_with_exit_code_3() {
        let mut s = server();
        s.seen_nonces.push([7u8; 16]);
        let out = negotiate(
            &client(
                0x0001,
                0x0002,
                vec![CAP_SESSION_READ],
                vec![CAP_SESSION_READ],
            ),
            &s,
        );
        assert_eq!(out, Outcome::Refused(RefusedReason::HandshakeReplay));
        assert_eq!(RefusedReason::HandshakeReplay.cli_exit(), 3);
    }
}
