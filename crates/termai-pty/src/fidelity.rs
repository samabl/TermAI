//! Byte-fidelity hop labels (kernel/02 section 3.6, AR-25).
//!
//! Every hop on the keyboard -> application -> pixel chain must declare exactly one
//! of F0 / F1 / F2 (AR-25 second clause). The byte buffer layer is never writable;
//! F1 means the metadata layer may rewrite, F2 means the render layer may rewrite.
//! The F0 identity guarantee is a per-byte differential, never sampling.

use crate::Fidelity;

/// Stable rule-set identifier carried by an F1 / F2 hop.
pub type RuleSetId = u32;

/// Registered rule set for ConPTY rewrites (conpty-rules.toml).
pub const RULESET_CONPTY: RuleSetId = 1;
/// Container daemon-side TTY handling (F2 on the daemon side, F0 in the channel).
pub const RULESET_CONTAINER: RuleSetId = 2;
/// SSH protocol annotations / keepalive (F1 in the channel, F0 in the data path).
pub const RULESET_SSH: RuleSetId = 3;
/// UI visual layer (soft wrap / clipping / decoration), never a data rewrite.
pub const RULESET_UI_VISUAL: RuleSetId = 4;
/// AI / plugin read-only mirror.
pub const RULESET_AI_PLUGIN: RuleSetId = 5;

/// A hop on the byte path. The set is exactly kernel/02 section 3.6.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Hop {
    /// Application <-> VT parser (the F0 identity guarantee of K-01).
    PtyToVt,
    /// sessiond <-> UI over termai-ipc.
    SessiondToUi,
    /// Local / WSL local jump.
    LocalJump,
    /// Container exec / attach (F0 channel, F2 daemon side).
    Container,
    /// SSH (F0 data, F1 protocol annotations).
    Ssh,
    /// UI visual layer (L0 rendering).
    UiVisual,
    /// AI / plugin read-only mirror.
    AiPlugin,
}

/// Fidelity label for a hop, exactly the kernel/02 section 3.6 table.
///
/// Container and SSH are multi-part hops: the table gives an F0 channel with an
/// F2 (container daemon) / F1 (SSH protocol annotations) side. We surface the
/// stricter side as the hop label and record the split in hop_note; a hop
/// carrying F2 must never be placed on a link that needs F0 (section 3.6).
#[must_use]
pub const fn fidelity_for_hop(hop: Hop) -> Fidelity {
    match hop {
        Hop::PtyToVt | Hop::SessiondToUi | Hop::LocalJump => Fidelity::F0,
        Hop::Container => Fidelity::F2(RULESET_CONTAINER),
        Hop::Ssh => Fidelity::F1(RULESET_SSH),
        Hop::UiVisual => Fidelity::F2(RULESET_UI_VISUAL),
        Hop::AiPlugin => Fidelity::F2(RULESET_AI_PLUGIN),
    }
}

/// Human-readable note explaining the multi-part hops.
#[must_use]
pub const fn hop_note(hop: Hop) -> &'static str {
    match hop {
        Hop::PtyToVt => "F0: bytes read from the PTY are handed to the VT parser byte-for-byte.",
        Hop::SessiondToUi => "F0: binary IPC frames, no text round-trip (DC-22).",
        Hop::LocalJump => "F0: local / WSL channel passes bytes through unchanged.",
        Hop::Container => {
            "F0 channel x F2 daemon side: the channel is byte-identical; docker/podman exec -it TTY handling is an external implementation and must be declared as F2."
        }
        Hop::Ssh => {
            "F0 data x F1 protocol annotations: keepalive / banner may be inserted at the protocol layer; every byte entering the data channel is passed through."
        }
        Hop::UiVisual => "F2: soft wrap / clipping / decoration only; character content and grid columns are unchanged.",
        Hop::AiPlugin => "F2 read-only mirror: plugins never write the PTY; stdin writes need explicit authorisation (AR-07 / AR-03).",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_kernel_02_3_6() {
        assert_eq!(fidelity_for_hop(Hop::PtyToVt), Fidelity::F0);
        assert_eq!(fidelity_for_hop(Hop::SessiondToUi), Fidelity::F0);
        assert_eq!(fidelity_for_hop(Hop::LocalJump), Fidelity::F0);
        assert_eq!(
            fidelity_for_hop(Hop::Container),
            Fidelity::F2(RULESET_CONTAINER)
        );
        assert_eq!(fidelity_for_hop(Hop::Ssh), Fidelity::F1(RULESET_SSH));
        assert_eq!(
            fidelity_for_hop(Hop::UiVisual),
            Fidelity::F2(RULESET_UI_VISUAL)
        );
        assert_eq!(
            fidelity_for_hop(Hop::AiPlugin),
            Fidelity::F2(RULESET_AI_PLUGIN)
        );
    }

    #[test]
    fn notes_are_non_empty() {
        for hop in [
            Hop::PtyToVt,
            Hop::SessiondToUi,
            Hop::LocalJump,
            Hop::Container,
            Hop::Ssh,
            Hop::UiVisual,
            Hop::AiPlugin,
        ] {
            assert!(!hop_note(hop).is_empty());
        }
    }
}
