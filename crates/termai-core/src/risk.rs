//! Risk tiers L0/L1/L2/L3/U (AR-06 / DC-26; CR-01: single enum, R0-R3 forbidden).
//!
//! AR-06: U (undecidable) starts at effective L2. Pure, deterministic, no I/O.
//! DC-27: parse failure is treated conservatively at a higher risk.

use crate::SessionId;

/// Risk tier (AR-06).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum RiskTier {
    /// L0 read only.
    ReadOnly,
    /// L1 idempotent write.
    IdempotentWrite,
    /// L2 destructive: dry-run + confirm every time.
    Destructive,
    /// L3 irreversible or outbound: confirm + second input + no-rollback badge.
    IrreversibleOrExternal,
    /// U undecidable (effective tier starts at L2).
    Undetermined,
}

impl RiskTier {
    /// Effective tier: U folds to L2 (AR-06).
    #[must_use]
    pub const fn effective(self) -> RiskTier {
        match self {
            RiskTier::Undetermined => RiskTier::Destructive,
            other => other,
        }
    }

    /// Raise by one step (AR-22 item 5: cross-session writes raise one tier).
    #[must_use]
    pub const fn raise(self) -> RiskTier {
        match self.effective() {
            RiskTier::ReadOnly => RiskTier::IdempotentWrite,
            RiskTier::IdempotentWrite => RiskTier::Destructive,
            RiskTier::Destructive => RiskTier::IrreversibleOrExternal,
            RiskTier::IrreversibleOrExternal | RiskTier::Undetermined => {
                RiskTier::IrreversibleOrExternal
            }
        }
    }

    /// L0/L1 may be fixed by a once/session/rule grant (AR-06).
    #[must_use]
    pub const fn rule_fixable(self) -> bool {
        matches!(self, RiskTier::ReadOnly | RiskTier::IdempotentWrite)
    }

    /// AR-06: L2 and above always require dry-run.
    #[must_use]
    pub const fn requires_dry_run(self) -> bool {
        matches!(
            self.effective(),
            RiskTier::Destructive | RiskTier::IrreversibleOrExternal
        )
    }

    /// AR-06: L3 requires a second input of the target phrase.
    #[must_use]
    pub const fn requires_second_input(self) -> bool {
        matches!(self.effective(), RiskTier::IrreversibleOrExternal)
    }

    /// AR-06: confirmation can never be disabled by configuration.
    #[must_use]
    pub const fn confirmation_is_mandatory(self) -> bool {
        self.requires_dry_run()
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            RiskTier::ReadOnly => "L0",
            RiskTier::IdempotentWrite => "L1",
            RiskTier::Destructive => "L2",
            RiskTier::IrreversibleOrExternal => "L3",
            RiskTier::Undetermined => "U",
        }
    }
}

/// Effects shown on the dry-run card (AR-06 / golden rule 3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Effects {
    pub read: bool,
    pub write: bool,
    pub network: bool,
    pub privilege: bool,
    pub cross_host: bool,
}

/// Full classification result with explainable reasons (no command text stored).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RiskAssessment {
    pub tier: RiskTier,
    /// Stable reason keys. Never contains the command text (AR-12 / section 6.3).
    pub reasons: Vec<&'static str>,
    pub effects: Effects,
    /// Target phrase required for L3 second input.
    pub target_phrase: Option<String>,
}

/// Classification input: parsed argv, never a joined shell string (golden rule 2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CommandIntent {
    pub argv: Vec<String>,
    pub session: SessionId,
    pub untrusted_source: bool,
}

const READ_ONLY: &[&str] = &[
    "ls",
    "cat",
    "head",
    "tail",
    "grep",
    "rg",
    "find",
    "pwd",
    "which",
    "whoami",
    "id",
    "ps",
    "df",
    "du",
    "free",
    "env",
    "printenv",
    "less",
    "more",
    "stat",
    "file",
    "wc",
    "sort",
    "uniq",
    "cut",
    "awk",
    "sed",
    "jq",
    "tree",
    "date",
    "uptime",
    "uname",
    "hostname",
    "echo",
    "printf",
    "true",
    "false",
    "test",
    "type",
    "command",
    "hash",
    "lsof",
    "netstat",
    "ss",
    "dig",
    "nslookup",
    "ping",
    "curl",
    "wget",
    "git",
    "cargo",
    "npm",
    "pnpm",
    "python",
    "python3",
    "node",
    "rustc",
    "make",
    "docker",
    "kubectl",
    "systemctl",
    "journalctl",
    "tar",
    "unzip",
];

const IDEMPOTENT_WRITE: &[&str] = &[
    "mkdir", "touch", "cp", "install", "ln", "tee", "patch", "chmod", "chown", "rustup",
];

const DESTRUCTIVE: &[&str] = &[
    "rm", "rmdir", "shred", "mkfs", "fdisk", "parted", "dd", "wipefs", "truncate", "kill",
    "killall", "pkill", "reboot", "shutdown", "halt", "poweroff", "init", "dropdb", "userdel",
    "groupdel", "crontab", "iptables", "nft",
];

const EXTERNAL: &[&str] = &[
    "ssh",
    "scp",
    "sftp",
    "rsync",
    "telnet",
    "nc",
    "ncat",
    "nmap",
    "terraform",
    "ansible",
    "ansible-playbook",
    "helm",
    "aws",
    "gcloud",
    "az",
    "gh",
    "flyctl",
    "heroku",
    "push",
];

/// (program, subcommand, tier, reason key)
const SUBCOMMAND_RULES: &[(&str, &str, RiskTier, &str)] = &[
    (
        "git",
        "push",
        RiskTier::IrreversibleOrExternal,
        "git_push_external",
    ),
    ("git", "reset", RiskTier::Destructive, "git_reset_hard"),
    ("git", "clean", RiskTier::Destructive, "git_clean_untracked"),
    ("git", "checkout", RiskTier::IdempotentWrite, "git_checkout"),
    ("git", "commit", RiskTier::IdempotentWrite, "git_commit"),
    ("git", "status", RiskTier::ReadOnly, "git_status"),
    ("git", "diff", RiskTier::ReadOnly, "git_diff"),
    ("git", "log", RiskTier::ReadOnly, "git_log"),
    (
        "docker",
        "push",
        RiskTier::IrreversibleOrExternal,
        "docker_push_external",
    ),
    ("docker", "rmi", RiskTier::Destructive, "docker_rmi"),
    ("docker", "rm", RiskTier::Destructive, "docker_rm"),
    ("docker", "ps", RiskTier::ReadOnly, "docker_ps"),
    ("docker", "images", RiskTier::ReadOnly, "docker_images"),
    ("kubectl", "delete", RiskTier::Destructive, "kubectl_delete"),
    (
        "kubectl",
        "apply",
        RiskTier::IdempotentWrite,
        "kubectl_apply",
    ),
    ("kubectl", "get", RiskTier::ReadOnly, "kubectl_get"),
    ("cargo", "build", RiskTier::IdempotentWrite, "cargo_build"),
    ("cargo", "test", RiskTier::IdempotentWrite, "cargo_test"),
    ("cargo", "run", RiskTier::IdempotentWrite, "cargo_run"),
    (
        "cargo",
        "publish",
        RiskTier::IrreversibleOrExternal,
        "cargo_publish",
    ),
    (
        "npm",
        "publish",
        RiskTier::IrreversibleOrExternal,
        "npm_publish",
    ),
    ("npm", "install", RiskTier::IdempotentWrite, "npm_install"),
    ("npm", "test", RiskTier::IdempotentWrite, "npm_test"),
    ("systemctl", "stop", RiskTier::Destructive, "systemctl_stop"),
    (
        "systemctl",
        "disable",
        RiskTier::Destructive,
        "systemctl_disable",
    ),
    (
        "systemctl",
        "status",
        RiskTier::ReadOnly,
        "systemctl_status",
    ),
];

fn basename(p: &str) -> &str {
    p.rsplit(['/', '\\']).next().unwrap_or(p)
}

fn strip_privilege_prefix(argv: &[String]) -> (&[String], bool) {
    let mut i = 0usize;
    let mut privilege = false;
    while i < argv.len() {
        match basename(&argv[i]) {
            "sudo" | "doas" | "runas" | "su" => {
                privilege = true;
                i += 1;
                while i < argv.len() && argv[i].starts_with('-') {
                    i += 1;
                }
            }
            "env" | "nohup" | "time" | "command" | "exec" => i += 1,
            _ => break,
        }
    }
    let cut = i.min(argv.len());
    (&argv[cut..], privilege)
}

/// Deterministic risk classification (pure function, no I/O).
#[must_use]
pub fn classify(intent: &CommandIntent) -> RiskAssessment {
    let mut effects = Effects {
        read: true,
        ..Default::default()
    };
    let mut reasons: Vec<&'static str> = Vec::new();
    let mut tier = RiskTier::ReadOnly;

    let (argv, privilege) = strip_privilege_prefix(&intent.argv);
    if argv.is_empty() {
        return RiskAssessment {
            tier: RiskTier::Undetermined,
            reasons: vec!["empty_argv"],
            effects: Effects::default(),
            target_phrase: None,
        };
    }
    if privilege {
        effects.privilege = true;
        tier = tier.max(RiskTier::Destructive);
        reasons.push("privilege_escalation");
    }

    let prog = basename(&argv[0]).to_ascii_lowercase();
    let lower: Vec<String> = argv.iter().map(|s| s.to_ascii_lowercase()).collect();

    if lower.iter().any(|a| a.contains('|')) {
        tier = RiskTier::IrreversibleOrExternal;
        reasons.push("pipe_to_shell");
    }
    if lower.iter().any(|a| a.contains('>')) {
        effects.write = true;
        tier = tier.max(RiskTier::IdempotentWrite);
        reasons.push("output_redirection");
    }

    if let Some(sub) = lower.iter().skip(1).find(|s| !s.starts_with('-')) {
        if let Some((_, _, t, key)) = SUBCOMMAND_RULES
            .iter()
            .find(|(p, s, _, _)| *p == prog && s == sub)
        {
            tier = tier.max(*t);
            reasons.push(key);
            if *t == RiskTier::IrreversibleOrExternal {
                effects.network = true;
                effects.cross_host = true;
            }
            if *t != RiskTier::ReadOnly {
                effects.write = true;
            }
        }
    }

    if DESTRUCTIVE.contains(&prog.as_str()) {
        tier = tier.max(RiskTier::Destructive);
        effects.write = true;
        reasons.push("destructive_program");
    }
    if EXTERNAL.contains(&prog.as_str()) {
        effects.network = true;
        tier = tier.max(RiskTier::IrreversibleOrExternal);
        reasons.push("external_egress");
        if matches!(prog.as_str(), "ssh" | "scp" | "sftp" | "rsync") {
            effects.cross_host = true;
            reasons.push("cross_host");
        }
    }
    if IDEMPOTENT_WRITE.contains(&prog.as_str()) {
        tier = tier.max(RiskTier::IdempotentWrite);
        effects.write = true;
        reasons.push("idempotent_write_program");
    }
    let known = READ_ONLY.contains(&prog.as_str())
        || IDEMPOTENT_WRITE.contains(&prog.as_str())
        || DESTRUCTIVE.contains(&prog.as_str())
        || EXTERNAL.contains(&prog.as_str());
    if READ_ONLY.contains(&prog.as_str()) {
        reasons.push("known_read_only_program");
    } else if !known {
        tier = RiskTier::Undetermined;
        reasons.push("unknown_program");
    }

    let joined = lower.join(" ");
    if joined.contains("--force") || joined.contains("-rf") || joined.contains("-fr") {
        tier = tier.max(RiskTier::Destructive);
        reasons.push("force_flag");
    }
    if prog == "dd" || joined.contains(" of=") {
        tier = RiskTier::IrreversibleOrExternal;
        reasons.push("block_device_write");
    }
    if prog == "mkfs" || prog.starts_with("mkfs.") {
        tier = RiskTier::IrreversibleOrExternal;
        reasons.push("filesystem_format");
    }
    if tier >= RiskTier::Destructive && argv.iter().any(|a| a == "/" || a == "/*" || a == "~") {
        tier = RiskTier::IrreversibleOrExternal;
        reasons.push("root_or_home_target");
    }
    if intent.untrusted_source {
        reasons.push("untrusted_source");
    }

    let target_phrase = if tier >= RiskTier::IrreversibleOrExternal {
        effects.cross_host = effects.cross_host || effects.network;
        Some(prog)
    } else {
        None
    };

    RiskAssessment {
        tier,
        reasons,
        effects,
        target_phrase,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(args: &[&str]) -> CommandIntent {
        CommandIntent {
            argv: args.iter().map(|s| (*s).to_string()).collect(),
            session: SessionId(1),
            untrusted_source: false,
        }
    }

    #[test]
    fn read_only_commands_are_l0() {
        assert_eq!(classify(&intent(&["ls", "-la"])).tier, RiskTier::ReadOnly);
        assert_eq!(
            classify(&intent(&["git", "status"])).tier,
            RiskTier::ReadOnly
        );
        assert_eq!(
            classify(&intent(&["cat", "a.txt"])).tier,
            RiskTier::ReadOnly
        );
    }

    #[test]
    fn destructive_commands_are_l2_and_require_dry_run() {
        let a = classify(&intent(&["rm", "-rf", "build"]));
        assert!(a.tier >= RiskTier::Destructive);
        assert!(a.effects.write);
        assert!(a.tier.requires_dry_run());
        assert!(a.tier.confirmation_is_mandatory());
        assert!(a.reasons.contains(&"force_flag"));
    }

    #[test]
    fn external_and_privileged_are_escalated() {
        assert_eq!(
            classify(&intent(&["ssh", "host"])).tier,
            RiskTier::IrreversibleOrExternal
        );
        assert!(classify(&intent(&["sudo", "ls"])).effects.privilege);
        assert_eq!(
            classify(&intent(&["git", "push", "origin", "main"])).tier,
            RiskTier::IrreversibleOrExternal
        );
    }

    #[test]
    fn unknown_program_is_undetermined_but_effective_l2() {
        let a = classify(&intent(&["frobnicate", "--now"]));
        assert_eq!(a.tier, RiskTier::Undetermined);
        assert_eq!(a.tier.effective(), RiskTier::Destructive);
        assert!(a.reasons.contains(&"unknown_program"));
    }

    #[test]
    fn empty_argv_is_undetermined_and_not_read() {
        let a = classify(&intent(&[]));
        assert_eq!(a.tier, RiskTier::Undetermined);
        assert!(!a.effects.read);
    }

    #[test]
    fn raise_and_rule_fixable_follow_ar06() {
        assert_eq!(RiskTier::ReadOnly.raise(), RiskTier::IdempotentWrite);
        assert_eq!(
            RiskTier::Destructive.raise(),
            RiskTier::IrreversibleOrExternal
        );
        assert!(RiskTier::IdempotentWrite.rule_fixable());
        assert!(!RiskTier::Destructive.rule_fixable());
    }

    #[test]
    fn classify_is_deterministic() {
        let a = classify(&intent(&["kubectl", "delete", "pod", "x"]));
        let b = classify(&intent(&["kubectl", "delete", "pod", "x"]));
        assert_eq!(a, b);
        assert_eq!(a.tier, RiskTier::Destructive);
    }

    #[test]
    fn reasons_never_contain_command_text() {
        let a = classify(&intent(&["rm", "-rf", "/"]));
        assert!(a
            .reasons
            .iter()
            .all(|r| !r.contains('/') && !r.contains(' ')));
    }
}
