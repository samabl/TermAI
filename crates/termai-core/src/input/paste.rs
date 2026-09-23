//! Paste barrier and sanitisation (AR-29.5 / OQ-INP-12).
//!
//! Non-negotiable: sanitisation can never be disabled, and a multi-line paste is
//! never silently allowed. Policy may only raise friction (ask -> always), never lower it.

use super::PastePayload;

/// Bracketed paste start/end (xterm).
pub const BRACKET_START: &[u8] = b"\x1b[200~";
pub const BRACKET_END: &[u8] = b"\x1b[201~";

/// Friction policy. Only strengthening is allowed (OQ-INP-12 item 3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PastePolicy {
    /// Ask when the paste is multiline or carries control bytes.
    #[default]
    Ask,
    /// Always ask, even for a single short line.
    Always,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PasteCtx {
    pub bracketed_mode: bool,
    pub policy: PastePolicy,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PasteDecision {
    pub allow: bool,
    pub needs_confirm: bool,
    pub reason: &'static str,
    pub sanitized: Vec<u8>,
}

/// Remove embedded bracketed-paste terminators and stray C1 introducers.
///
/// This is the paste barrier: it must never be configurable (AR-29.5).
#[must_use]
pub fn sanitize_paste(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0usize;
    while i < data.len() {
        if data[i..].starts_with(BRACKET_END) || data[i..].starts_with(BRACKET_START) {
            i += BRACKET_END.len();
            continue;
        }
        // Strip a lone ESC (0x1B) that begins a control introducer but is not part
        // of a printable sequence: keep it only when followed by a normal byte.
        if data[i] == 0x1B && i + 1 < data.len() && (data[i + 1] == b'[' || data[i + 1] == b']') {
            i += 1;
            continue;
        }
        out.push(data[i]);
        i += 1;
    }
    out
}

/// Decide friction for a paste. Never silently allows multiline pastes.
#[must_use]
pub fn paste_gate(p: &PastePayload, ctx: &PasteCtx) -> PasteDecision {
    let sanitized = sanitize_paste(&p.data);
    let bracketed = ctx.bracketed_mode || p.bracketed;
    let multiline = sanitized.iter().any(|b| *b == b'\n' || *b == b'\r');
    let has_control = sanitized
        .iter()
        .any(|b| (*b < 0x20 && *b != b'\t' && *b != b'\n' && *b != b'\r') || *b == 0x7F);

    if ctx.policy == PastePolicy::Always {
        return PasteDecision {
            allow: false,
            needs_confirm: true,
            reason: "policy_always_ask",
            sanitized,
        };
    }
    if multiline && !bracketed {
        return PasteDecision {
            allow: false,
            needs_confirm: true,
            reason: "multiline_unbracketed",
            sanitized,
        };
    }
    if has_control {
        return PasteDecision {
            allow: false,
            needs_confirm: true,
            reason: "control_bytes_present",
            sanitized,
        };
    }
    if sanitized.len() > 1024 * 1024 {
        return PasteDecision {
            allow: false,
            needs_confirm: true,
            reason: "paste_too_large",
            sanitized,
        };
    }
    PasteDecision {
        allow: true,
        needs_confirm: false,
        reason: "single_line_clean",
        sanitized,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(data: &[u8], bracketed: bool) -> PastePayload {
        PastePayload {
            data: data.to_vec(),
            origin: PasteOrigin::LocalClipboard,
            bracketed,
        }
    }

    use crate::input::PasteOrigin;

    #[test]
    fn sanitizer_removes_embedded_bracket_end() {
        let dirty = b"safe\x1b[201~rm -rf /\x1b[201~tail";
        let clean = sanitize_paste(dirty);
        assert!(!clean.windows(6).any(|w| w == BRACKET_END));
        assert_eq!(String::from_utf8_lossy(&clean), "saferm -rf /tail");
    }

    #[test]
    fn single_line_clean_is_allowed_without_confirm() {
        let d = paste_gate(&payload(b"ls -la", false), &PasteCtx::default());
        assert!(d.allow);
        assert!(!d.needs_confirm);
    }

    #[test]
    fn multiline_unbracketed_requires_confirm() {
        let d = paste_gate(&payload(b"a\nb", false), &PasteCtx::default());
        assert!(!d.allow);
        assert!(d.needs_confirm);
        assert_eq!(d.reason, "multiline_unbracketed");
    }

    #[test]
    fn bracketed_multiline_is_allowed_but_policy_can_strengthen() {
        let ok = paste_gate(&payload(b"a\nb", true), &PasteCtx::default());
        assert!(ok.allow);
        let strict = paste_gate(
            &payload(b"a\nb", true),
            &PasteCtx {
                bracketed_mode: true,
                policy: PastePolicy::Always,
            },
        );
        assert!(!strict.allow);
        assert_eq!(strict.reason, "policy_always_ask");
    }

    #[test]
    fn control_bytes_require_confirm() {
        let d = paste_gate(&payload(b"a\x01b", true), &PasteCtx::default());
        assert!(d.needs_confirm);
        assert_eq!(d.reason, "control_bytes_present");
    }
}
