use termai_pty::{
    bundled_rules, is_gate_failure, load_rules, RewriteRule, RuleError, RuleScope, Severity,
};

#[test]
fn bundled_registry_parses_and_is_not_a_gate_failure() {
    let rules = load_rules(bundled_rules()).expect("bundled conpty-rules.toml must parse");
    assert!(rules.len() >= 6, "at least C-W1..C-W6 must be registered");
    for rule in &rules {
        assert!(!rule.owner.is_empty(), "{} needs an owner", rule.id);
        assert!(!rule.expires.is_empty(), "{} needs an expiry", rule.id);
        assert!(
            !is_gate_failure(rule),
            "{} must not be a gate failure",
            rule.id
        );
    }
    for id in ["C-W1", "C-W2", "C-W3", "C-W4", "C-W5", "C-W6"] {
        assert!(
            rules.iter().any(|rule| rule.id == id),
            "missing bundled rule {id}"
        );
    }
}

#[test]
fn scope_data_and_severity_block_are_gate_failures() {
    let mut rule = RewriteRule {
        id: "X".to_string(),
        phenomenon: "p".to_string(),
        detection: "d".to_string(),
        severity: Severity::Warn,
        scope: RuleScope::Data,
        owner: "o".to_string(),
        expires: "e".to_string(),
    };
    assert!(
        is_gate_failure(&rule),
        "scope = data must always be a gate failure"
    );
    rule.scope = RuleScope::Meta;
    assert!(!is_gate_failure(&rule));
    rule.severity = Severity::Block;
    assert!(
        is_gate_failure(&rule),
        "severity = block must be a gate failure"
    );
}

#[test]
fn malformed_registry_is_rejected() {
    assert_eq!(load_rules(""), Err(RuleError::Empty));
    assert!(load_rules("id = \"x\"\n").is_err());
    assert!(load_rules("[[rule]]\nid = \"a\"\n").is_err());
    assert!(load_rules("[[rule]]\nbogus = \"1\"\n").is_err());
    assert!(load_rules("[[rule]]\nid = \"a\"\nid = \"b\"\n").is_err());
    let duplicate = "[[rule]]\nid = \"C-D\"\nphenomenon = \"p\"\ndetection = \"d\"\nseverity = \"info\"\nscope = \"meta\"\nowner = \"o\"\nexpires = \"e\"\n[[rule]]\nid = \"C-D\"\nphenomenon = \"p\"\ndetection = \"d\"\nseverity = \"info\"\nscope = \"meta\"\nowner = \"o\"\nexpires = \"e\"\n";
    assert!(matches!(
        load_rules(duplicate),
        Err(RuleError::DuplicateId { .. })
    ));
}
