//! Machine-readable ConPTY rewrite registry (kernel/02 section 3.2, AR-25 third clause).
//!
//! conpty-rules.toml is the single source of truth for registered ConPTY rewrites.
//! severity = block OR scope = data is a gate failure (AR-25.3); the bundled file
//! must therefore contain no such rule. A hand-rolled TOML subset parser is used
//! on purpose: no toml crate enters the link boundary (ADR-0015).

use std::collections::HashSet;
use std::fmt;

/// Gate grade of a registered rewrite.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    /// Blocking rewrite: a gate failure by construction (AR-25.3).
    Block,
    /// Accepted degradation, tracked but non-blocking.
    Warn,
    /// Informational, accepted rewrite.
    Info,
}

/// Which layer a registered rewrite touches.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RuleScope {
    /// User data is rewritten: gate failure, no exception (AR-25.3).
    Data,
    /// Metadata / conhost-generated rewrite.
    Meta,
    /// Render-layer rewrite.
    Render,
}

/// One registered ConPTY rewrite. Field names are the kernel/02 section 3.2 contract.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RewriteRule {
    pub id: String,
    pub phenomenon: String,
    pub detection: String,
    pub severity: Severity,
    pub scope: RuleScope,
    pub owner: String,
    pub expires: String,
}

/// Parse failure of the hand-rolled TOML subset.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RuleError {
    /// The document contains no rule at all.
    Empty,
    /// Generic syntax problem at a line.
    Parse { line: usize, message: String },
    /// A required key is missing on a rule.
    MissingField { line: usize, field: &'static str },
    /// A key outside the kernel/02 section 3.2 schema.
    UnknownKey { line: usize, key: String },
    /// Two rules share the same stable id.
    DuplicateId { line: usize, id: String },
    /// A key carries a value outside its allowed vocabulary.
    BadValue {
        line: usize,
        field: &'static str,
        value: String,
    },
}

impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleError::Empty => f.write_str("no [[rule]] entry found"),
            RuleError::Parse { line, message } => write!(f, "line {line}: {message}"),
            RuleError::MissingField { line, field } => {
                write!(f, "line {line}: missing required field {field}")
            }
            RuleError::UnknownKey { line, key } => write!(f, "line {line}: unknown key {key}"),
            RuleError::DuplicateId { line, id } => write!(f, "line {line}: duplicate rule id {id}"),
            RuleError::BadValue { line, field, value } => {
                write!(f, "line {line}: bad value for {field}: {value}")
            }
        }
    }
}

impl std::error::Error for RuleError {}

/// Parse the kernel/02 section 3.2 rule schema from a TOML subset.
///
/// Supported syntax: blank lines, # comments, [[rule]] tables, and
/// key = "basic string" / key = 'literal string'. Unknown keys, duplicate ids and
/// missing required fields are rejected (fail closed).
pub fn load_rules(text: &str) -> Result<Vec<RewriteRule>, RuleError> {
    let mut rules: Vec<RewriteRule> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut cur: Option<PartialRule> = None;
    let mut saw_rule = false;

    for (idx, raw) in text.lines().enumerate() {
        let line = idx + 1;
        let stripped = strip_comment(raw);
        let trimmed = stripped.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "[[rule]]" {
            if let Some(partial) = cur.take() {
                push_rule(partial.finish()?, &mut rules, &mut seen, line)?;
            }
            cur = Some(PartialRule {
                line,
                ..PartialRule::default()
            });
            saw_rule = true;
            continue;
        }
        if trimmed.starts_with('[') {
            return Err(RuleError::Parse {
                line,
                message: format!("unsupported table header: {trimmed}"),
            });
        }
        let (key, value) = split_key_value(trimmed).ok_or_else(|| RuleError::Parse {
            line,
            message: format!("expected key = value, got: {trimmed}"),
        })?;
        let value = parse_string(value, line)?;
        let partial = cur.as_mut().ok_or(RuleError::Parse {
            line,
            message: "key = value appears before any [[rule]]".to_string(),
        })?;
        partial.set(line, key, value)?;
    }

    if let Some(partial) = cur.take() {
        push_rule(partial.finish()?, &mut rules, &mut seen, 0)?;
    }
    if !saw_rule {
        return Err(RuleError::Empty);
    }
    Ok(rules)
}

fn push_rule(
    rule: RewriteRule,
    rules: &mut Vec<RewriteRule>,
    seen: &mut HashSet<String>,
    line: usize,
) -> Result<(), RuleError> {
    if !seen.insert(rule.id.clone()) {
        return Err(RuleError::DuplicateId { line, id: rule.id });
    }
    rules.push(rule);
    Ok(())
}

/// AR-25.3: severity == Block OR scope == Data is a gate failure.
#[must_use]
pub fn is_gate_failure(rule: &RewriteRule) -> bool {
    matches!(rule.severity, Severity::Block) || matches!(rule.scope, RuleScope::Data)
}

/// The bundled, machine-readable registry (AR-25.3, OQ-PTY-01 -> T1 kernel).
#[must_use]
pub fn bundled_rules() -> &'static str {
    include_str!("../conpty-rules.toml")
}

#[derive(Default)]
struct PartialRule {
    line: usize,
    id: Option<String>,
    phenomenon: Option<String>,
    detection: Option<String>,
    severity: Option<Severity>,
    scope: Option<RuleScope>,
    owner: Option<String>,
    expires: Option<String>,
}

impl PartialRule {
    fn set(&mut self, line: usize, key: &str, value: String) -> Result<(), RuleError> {
        if self.is_set(key) {
            return Err(RuleError::Parse {
                line,
                message: format!("duplicate key {key}"),
            });
        }
        match key {
            "id" => self.id = Some(non_empty(line, "id", value)?),
            "phenomenon" => self.phenomenon = Some(non_empty(line, "phenomenon", value)?),
            "detection" => self.detection = Some(non_empty(line, "detection", value)?),
            "severity" => {
                let parsed = match value.as_str() {
                    "block" => Severity::Block,
                    "warn" => Severity::Warn,
                    "info" => Severity::Info,
                    _ => {
                        return Err(RuleError::BadValue {
                            line,
                            field: "severity",
                            value,
                        });
                    }
                };
                self.severity = Some(parsed);
            }
            "scope" => {
                let parsed = match value.as_str() {
                    "data" => RuleScope::Data,
                    "meta" => RuleScope::Meta,
                    "render" => RuleScope::Render,
                    _ => {
                        return Err(RuleError::BadValue {
                            line,
                            field: "scope",
                            value,
                        })
                    }
                };
                self.scope = Some(parsed);
            }
            "owner" => self.owner = Some(non_empty(line, "owner", value)?),
            "expires" => self.expires = Some(non_empty(line, "expires", value)?),
            _ => {
                return Err(RuleError::UnknownKey {
                    line,
                    key: key.to_string(),
                })
            }
        }
        Ok(())
    }

    fn is_set(&self, key: &str) -> bool {
        match key {
            "id" => self.id.is_some(),
            "phenomenon" => self.phenomenon.is_some(),
            "detection" => self.detection.is_some(),
            "severity" => self.severity.is_some(),
            "scope" => self.scope.is_some(),
            "owner" => self.owner.is_some(),
            "expires" => self.expires.is_some(),
            _ => false,
        }
    }

    fn finish(self) -> Result<RewriteRule, RuleError> {
        let line = self.line;
        Ok(RewriteRule {
            id: self
                .id
                .ok_or(RuleError::MissingField { line, field: "id" })?,
            phenomenon: self.phenomenon.ok_or(RuleError::MissingField {
                line,
                field: "phenomenon",
            })?,
            detection: self.detection.ok_or(RuleError::MissingField {
                line,
                field: "detection",
            })?,
            severity: self.severity.ok_or(RuleError::MissingField {
                line,
                field: "severity",
            })?,
            scope: self.scope.ok_or(RuleError::MissingField {
                line,
                field: "scope",
            })?,
            owner: self.owner.ok_or(RuleError::MissingField {
                line,
                field: "owner",
            })?,
            expires: self.expires.ok_or(RuleError::MissingField {
                line,
                field: "expires",
            })?,
        })
    }
}

fn non_empty(line: usize, field: &'static str, value: String) -> Result<String, RuleError> {
    if value.is_empty() {
        return Err(RuleError::BadValue { line, field, value });
    }
    Ok(value)
}

fn strip_comment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in raw.chars() {
        match quote {
            Some(q) => {
                out.push(ch);
                if escaped {
                    escaped = false;
                } else if ch == '\\' && q == '"' {
                    escaped = true;
                } else if ch == q {
                    quote = None;
                }
            }
            None => {
                if ch == '#' {
                    break;
                }
                if ch == '"' || ch == '\'' {
                    quote = Some(ch);
                }
                out.push(ch);
            }
        }
    }
    out
}

fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if ch == '\\' && q == '"' {
                    escaped = true;
                } else if ch == q {
                    quote = None;
                }
            }
            None => {
                if ch == '"' || ch == '\'' {
                    quote = Some(ch);
                } else if ch == '=' {
                    return Some((line[..idx].trim(), line[idx + 1..].trim()));
                }
            }
        }
    }
    None
}

fn parse_string(value: &str, line: usize) -> Result<String, RuleError> {
    if let Some(rest) = value.strip_prefix('\'') {
        let inner = rest.strip_suffix('\'').ok_or_else(|| RuleError::Parse {
            line,
            message: "unterminated literal string".to_string(),
        })?;
        return Ok(inner.to_string());
    }
    let inner = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .ok_or_else(|| RuleError::Parse {
            line,
            message: "expected a quoted string".to_string(),
        })?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some(other) => {
                return Err(RuleError::Parse {
                    line,
                    message: format!("unsupported escape sequence: backslash {other}"),
                });
            }
            None => {
                return Err(RuleError::Parse {
                    line,
                    message: "trailing backslash".to_string(),
                });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_file_parses_and_is_non_gate() {
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
                rules.iter().any(|r| r.id == id),
                "missing bundled rule {id}"
            );
        }
    }

    #[test]
    fn data_scope_or_block_is_a_gate_failure() {
        let mut rule = RewriteRule {
            id: "X".to_string(),
            phenomenon: "p".to_string(),
            detection: "d".to_string(),
            severity: Severity::Warn,
            scope: RuleScope::Data,
            owner: "o".to_string(),
            expires: "e".to_string(),
        };
        assert!(is_gate_failure(&rule));
        rule.scope = RuleScope::Meta;
        assert!(!is_gate_failure(&rule));
        rule.severity = Severity::Block;
        assert!(is_gate_failure(&rule));
    }

    #[test]
    fn parser_handles_literal_strings_and_comments() {
        let text = "[[rule]]\nid = 'C-T1' # trailing comment\nphenomenon = \"a # b\"\ndetection = \"d\"\nseverity = \"info\"\nscope = \"meta\"\nowner = \"o\"\nexpires = \"2 minors\"\n";
        let rules = load_rules(text).expect("parse");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "C-T1");
        assert_eq!(rules[0].phenomenon, "a # b");
    }

    #[test]
    fn malformed_documents_are_rejected() {
        assert_eq!(load_rules(""), Err(RuleError::Empty));
        assert!(load_rules("id = \"x\"\n").is_err());
        assert!(load_rules("[[rule]]\nid = \"a\"\n").is_err());
        assert!(load_rules("[[rule]]\nid = \"a\"\nid = \"b\"\n").is_err());
        let duplicate = "[[rule]]\nid = \"C-D\"\nphenomenon = \"p\"\ndetection = \"d\"\nseverity = \"info\"\nscope = \"meta\"\nowner = \"o\"\nexpires = \"e\"\n[[rule]]\nid = \"C-D\"\nphenomenon = \"p\"\ndetection = \"d\"\nseverity = \"info\"\nscope = \"meta\"\nowner = \"o\"\nexpires = \"e\"\n";
        assert!(matches!(
            load_rules(duplicate),
            Err(RuleError::DuplicateId { .. })
        ));
        assert!(load_rules("[[rule]]\nbogus = \"1\"\n").is_err());
        assert!(load_rules("[[rule]]\nseverity = \"nope\"\n").is_err());
    }
}
