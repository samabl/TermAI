//! Capability gate: the pure decision function behind kernel/07 CAP-1 / CAP-2 / CAP-6.
//!
//! Contract (kernel/07 section 3.4): pure, no I/O, no allocation on the hot path,
//! no clock acquisition. The caller injects now. Fail-closed (K-09): any anomaly
//! denies rather than degrades.

use crate::error::authz;
use crate::risk::RiskTier;
use crate::time::MonoTime;
use crate::SessionId;

/// Stable capability id (CapId = u32 stable enum, kernel/07 section 3.3).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct CapId(pub u32);

pub const CAP_SESSION_READ: CapId = CapId(0x0001);
pub const CAP_STDIN_WRITE: CapId = CapId(0x0002);
pub const CAP_PTY_SIGNAL: CapId = CapId(0x0003);
pub const CAP_PTY_RESIZE: CapId = CapId(0x0004);
pub const CAP_AUDIT_READ: CapId = CapId(0x0005);
pub const CAP_CONFIG_WRITE: CapId = CapId(0x0006);
pub const CAP_CLIPBOARD_READ: CapId = CapId(0x0007);
pub const CAP_CLIPBOARD_WRITE: CapId = CapId(0x0008);
pub const CAP_PLUGIN_GRANT: CapId = CapId(0x0009);
pub const CAP_IPC_EXPERIMENTAL: CapId = CapId(0x000A);

impl CapId {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self.0 {
            0x0001 => "session.read",
            0x0002 => "stdin.write",
            0x0003 => "pty.signal",
            0x0004 => "pty.resize",
            0x0005 => "audit.read",
            0x0006 => "config.write",
            0x0007 => "clipboard.read",
            0x0008 => "clipboard.write",
            0x0009 => "plugin.grant",
            0x000A => "ipc.experimental",
            _ => "unknown",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<CapId> {
        const ALL: [CapId; 10] = [
            CAP_SESSION_READ,
            CAP_STDIN_WRITE,
            CAP_PTY_SIGNAL,
            CAP_PTY_RESIZE,
            CAP_AUDIT_READ,
            CAP_CONFIG_WRITE,
            CAP_CLIPBOARD_READ,
            CAP_CLIPBOARD_WRITE,
            CAP_PLUGIN_GRANT,
            CAP_IPC_EXPERIMENTAL,
        ];
        ALL.into_iter().find(|c| c.name() == name)
    }
}

/// Token scope. Global covers all sessions of one device-bound principal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CapScope {
    Global,
    Session(SessionId),
    Workspace(u64),
    AgentSession(u64),
}

/// Taint of the values flowing into the action (golden rule 5).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Taint {
    #[default]
    Clean,
    Untrusted,
}

/// Approval receipt reference. L2/L3 approval can never be pre-fixed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ApprovalRef {
    pub receipt_hash: u64,
    pub approver: u64,
    pub issued_at: MonoTime,
}

/// A capability token. The body never crosses a process boundary as plaintext:
/// frames carry cap_ref + seal_hash only (kernel/07 section 3.4).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CapToken {
    pub cap_ref: u32,
    pub caps: Vec<CapId>,
    pub scope: CapScope,
    pub issued_at: MonoTime,
    pub expires_at: MonoTime,
    pub revoked_at: Option<MonoTime>,
    pub taint: Taint,
    pub approval: Option<ApprovalRef>,
    pub policy_id: String,
}

/// Effective policy. Enterprise policy can only strengthen, never weaken (AR-06 / DC-35).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EffectivePolicy {
    /// Lower bound applied to every request tier (enterprise floor).
    pub tier_floor: RiskTier,
    /// Whether untrusted-tainted writes are permitted at all. Default false.
    pub allow_tainted_write: bool,
    /// Whether cross-host takeover may be approved by a user.
    pub allow_takeover: bool,
}

impl Default for EffectivePolicy {
    fn default() -> Self {
        Self {
            tier_floor: RiskTier::ReadOnly,
            allow_tainted_write: false,
            allow_takeover: false,
        }
    }
}

impl EffectivePolicy {
    /// Enterprise floor can only raise the tier, never lower it (AR-06).
    #[must_use]
    pub fn floor_of(&self, tier: RiskTier) -> RiskTier {
        tier.max(self.tier_floor)
    }
}

/// One authorization request.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CapRequest {
    pub act: CapId,
    pub session: SessionId,
    pub risk: RiskTier,
    pub taint: Taint,
    pub approval: Option<ApprovalRef>,
    /// Second input supplied by the human for L3 (the target phrase).
    pub second_input: Option<String>,
    /// Target phrase the human must type for L3.
    pub target_phrase: Option<String>,
}

/// Decision. DryRun / NeedsApproval / NeedsSecondInput are Ok: they are the
/// mandated interaction, not authorization failures (AR-06).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Grant {
    Allow,
    DryRun { reason: &'static str },
    NeedsApproval { reason: &'static str },
    NeedsSecondInput { phrase: String },
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AuthzError {
    Missing,
    Expired,
    Revoked { since: MonoTime },
    Scope,
    Taint,
    Approval,
    Policy,
    Malformed,
}

impl AuthzError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            AuthzError::Missing => authz::MISSING,
            AuthzError::Expired => authz::EXPIRED,
            AuthzError::Revoked { .. } => authz::REVOKED,
            AuthzError::Scope => authz::SCOPE,
            AuthzError::Taint => authz::TAINT,
            AuthzError::Approval => authz::APPROVAL,
            AuthzError::Policy => authz::POLICY,
            AuthzError::Malformed => authz::MALFORMED,
        }
    }
}

/// The single pure gate (kernel/07 CAP-1..CAP-7 all funnel through this shape).
pub fn require(
    req: &CapRequest,
    tok: &CapToken,
    now: MonoTime,
    pol: &EffectivePolicy,
) -> Result<Grant, AuthzError> {
    // Malformed first: a token with an inverted lifetime can never be trusted.
    if tok.expires_at < tok.issued_at || tok.cap_ref == 0 {
        return Err(AuthzError::Malformed);
    }
    if !tok.caps.contains(&req.act) {
        return Err(AuthzError::Missing);
    }
    match tok.scope {
        CapScope::Global => {}
        CapScope::Session(s) if s == req.session => {}
        CapScope::Workspace(_) | CapScope::AgentSession(_) | CapScope::Session(_) => {
            return Err(AuthzError::Scope)
        }
    }
    if tok.expires_at.has_passed(now) {
        return Err(AuthzError::Expired);
    }
    if let Some(since) = tok.revoked_at {
        return Err(AuthzError::Revoked { since });
    }

    let tier = pol.floor_of(req.risk.effective());

    // Golden rule 5: untrusted values must not become command args or paths.
    if req.taint == Taint::Untrusted
        && tier >= RiskTier::IdempotentWrite
        && !pol.allow_tainted_write
    {
        return Err(AuthzError::Taint);
    }

    // AR-06: L2 and above always dry-run first; approval cannot be pre-fixed by rule.
    if tier.requires_dry_run() && req.approval.is_none() {
        return Ok(if tier >= RiskTier::IrreversibleOrExternal {
            Grant::NeedsApproval {
                reason: "l3_approval_mandatory",
            }
        } else {
            Grant::DryRun {
                reason: "l2_dry_run_mandatory",
            }
        });
    }

    if tier >= RiskTier::IrreversibleOrExternal {
        let Some(phrase) = req.target_phrase.as_deref() else {
            return Err(AuthzError::Malformed);
        };
        match req.second_input.as_deref() {
            Some(s) if s == phrase => {}
            Some(_) => return Err(AuthzError::Approval),
            None => {
                return Ok(Grant::NeedsSecondInput {
                    phrase: phrase.to_string(),
                })
            }
        }
    }

    if pol.tier_floor > RiskTier::ReadOnly && req.risk < pol.tier_floor && req.approval.is_none() {
        // The floor raised the tier; the raised tier still needs its confirmation.
        return Ok(Grant::NeedsApproval {
            reason: "policy_floor",
        });
    }

    Ok(Grant::Allow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(caps: Vec<CapId>) -> CapToken {
        CapToken {
            cap_ref: 1,
            caps,
            scope: CapScope::Session(SessionId(7)),
            issued_at: MonoTime::from_secs(0),
            expires_at: MonoTime::from_secs(60),
            revoked_at: None,
            taint: Taint::Clean,
            approval: None,
            policy_id: "default".to_string(),
        }
    }

    fn req(risk: RiskTier) -> CapRequest {
        CapRequest {
            act: CAP_STDIN_WRITE,
            session: SessionId(7),
            risk,
            taint: Taint::Clean,
            approval: None,
            second_input: None,
            target_phrase: None,
        }
    }

    fn now() -> MonoTime {
        MonoTime::from_secs(10)
    }

    #[test]
    fn missing_capability_is_denied() {
        let e = require(
            &req(RiskTier::ReadOnly),
            &token(vec![]),
            now(),
            &EffectivePolicy::default(),
        );
        assert_eq!(e, Err(AuthzError::Missing));
        assert_eq!(AuthzError::Missing.code(), "TERMAI-E-AUTHZ-MISSING");
    }

    #[test]
    fn scope_mismatch_is_denied() {
        let mut r = req(RiskTier::ReadOnly);
        r.session = SessionId(99);
        assert_eq!(
            require(
                &r,
                &token(vec![CAP_STDIN_WRITE]),
                now(),
                &EffectivePolicy::default()
            ),
            Err(AuthzError::Scope)
        );
    }

    #[test]
    fn expired_and_revoked_are_denied() {
        let t = token(vec![CAP_STDIN_WRITE]);
        let late = MonoTime::from_secs(61);
        assert_eq!(
            require(
                &req(RiskTier::ReadOnly),
                &t,
                late,
                &EffectivePolicy::default()
            ),
            Err(AuthzError::Expired)
        );
        let mut revoked = t.clone();
        revoked.revoked_at = Some(MonoTime::from_secs(5));
        assert_eq!(
            require(
                &req(RiskTier::ReadOnly),
                &revoked,
                now(),
                &EffectivePolicy::default()
            ),
            Err(AuthzError::Revoked {
                since: MonoTime::from_secs(5)
            })
        );
    }

    #[test]
    fn malformed_token_is_rejected_first() {
        let mut t = token(vec![CAP_STDIN_WRITE]);
        t.expires_at = MonoTime::from_secs(0);
        t.issued_at = MonoTime::from_secs(10);
        assert_eq!(
            require(
                &req(RiskTier::ReadOnly),
                &t,
                now(),
                &EffectivePolicy::default()
            ),
            Err(AuthzError::Malformed)
        );
    }

    #[test]
    fn l2_always_dry_runs_even_with_session_rule() {
        let g = require(
            &req(RiskTier::Destructive),
            &token(vec![CAP_STDIN_WRITE]),
            now(),
            &EffectivePolicy::default(),
        );
        assert_eq!(
            g,
            Ok(Grant::DryRun {
                reason: "l2_dry_run_mandatory"
            })
        );
    }

    #[test]
    fn l3_needs_approval_then_second_input_then_allows() {
        let mut r = req(RiskTier::IrreversibleOrExternal);
        r.target_phrase = Some("prod-db".to_string());
        let t = token(vec![CAP_STDIN_WRITE]);
        assert_eq!(
            require(&r, &t, now(), &EffectivePolicy::default()),
            Ok(Grant::NeedsApproval {
                reason: "l3_approval_mandatory"
            })
        );
        r.approval = Some(ApprovalRef {
            receipt_hash: 1,
            approver: 2,
            issued_at: now(),
        });
        assert_eq!(
            require(&r, &t, now(), &EffectivePolicy::default()),
            Ok(Grant::NeedsSecondInput {
                phrase: "prod-db".to_string()
            })
        );
        r.second_input = Some("wrong".to_string());
        assert_eq!(
            require(&r, &t, now(), &EffectivePolicy::default()),
            Err(AuthzError::Approval)
        );
        r.second_input = Some("prod-db".to_string());
        assert_eq!(
            require(&r, &t, now(), &EffectivePolicy::default()),
            Ok(Grant::Allow)
        );
    }

    #[test]
    fn untrusted_taint_blocks_writes_by_default() {
        let mut r = req(RiskTier::IdempotentWrite);
        r.taint = Taint::Untrusted;
        assert_eq!(
            require(
                &r,
                &token(vec![CAP_STDIN_WRITE]),
                now(),
                &EffectivePolicy::default()
            ),
            Err(AuthzError::Taint)
        );
    }

    #[test]
    fn enterprise_floor_can_only_strengthen() {
        let pol = EffectivePolicy {
            tier_floor: RiskTier::Destructive,
            ..Default::default()
        };
        assert_eq!(pol.floor_of(RiskTier::ReadOnly), RiskTier::Destructive);
        assert_eq!(
            pol.floor_of(RiskTier::IrreversibleOrExternal),
            RiskTier::IrreversibleOrExternal
        );
        assert_eq!(pol.floor_of(RiskTier::Destructive), RiskTier::Destructive);
    }

    #[test]
    fn cap_names_round_trip() {
        for c in [CAP_STDIN_WRITE, CAP_AUDIT_READ, CAP_IPC_EXPERIMENTAL] {
            assert_eq!(CapId::from_name(c.name()), Some(c));
        }
        assert_eq!(CapId::from_name("nope"), None);
    }
}
