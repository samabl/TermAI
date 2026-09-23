//! .trec replay and minimal reproduction runner (kernel/01 section 3.8).
//!
//! Line based, deterministic and clock-free. Supported directives:
//!   FEED <hex> / PTY_OUT <hex> / KEY "<escaped>"
//!   RESIZE <cols>x<rows>
//!   ASSERT ROW <n> = <text>
//!   ASSERT COUNTER <key> = <value>
//!   ASSERT TITLE <text>
//!   ASSERT CURSOR <row> <col>
//! An optional leading +virtual.time token and a TERMAI-REPLAY 1 header / meta line
//! are accepted and ignored.

use std::fmt;

use crate::terminal::Terminal;

/// One replay step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Feed raw bytes.
    Feed(Vec<u8>),
    /// Resize the grid.
    Resize {
        /// New column count.
        cols: u16,
        /// New row count.
        rows: u16,
    },
    /// Assert one row's logical text.
    AssertRow {
        /// Row index.
        row: u16,
        /// Expected text.
        text: String,
    },
    /// Assert one counter value.
    AssertCounter {
        /// Counter key.
        key: String,
        /// Expected value.
        value: u64,
    },
    /// Assert the window title.
    AssertTitle(String),
    /// Assert the cursor position.
    AssertCursor {
        /// Row index.
        row: u16,
        /// Column index.
        col: u16,
    },
}

/// A parsed replay script.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayScript {
    /// Steps in order.
    pub steps: Vec<Step>,
}

/// Outcome of running a script.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayReport {
    /// Number of steps that succeeded.
    pub passed: usize,
    /// Human readable failures.
    pub failed: Vec<String>,
}

/// Parse error with the source line number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayError {
    /// 1-based line number.
    pub line: usize,
    /// What went wrong.
    pub message: String,
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ReplayError {}

/// Parse .trec text.
pub fn parse_trec(text: &str) -> Result<ReplayScript, ReplayError> {
    let mut steps = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line_no = index + 1;
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "TERMAI-REPLAY 1" || line.starts_with("meta ") {
            continue;
        }
        if let Some(rest) = line.strip_prefix('+') {
            match rest.split_once(char::is_whitespace) {
                Some((_, tail)) => line = tail.trim(),
                None => continue,
            }
        }
        let (keyword, rest) = match line.split_once(char::is_whitespace) {
            Some((keyword, rest)) => (keyword, rest.trim()),
            None => (line, ""),
        };
        let step = match keyword {
            "FEED" | "PTY_OUT" => Step::Feed(parse_hex(rest).map_err(|message| ReplayError {
                line: line_no,
                message,
            })?),
            "KEY" => Step::Feed(parse_quoted(rest).map_err(|message| ReplayError {
                line: line_no,
                message,
            })?),
            "RESIZE" => {
                let (cols, rows) = parse_resize(rest).map_err(|message| ReplayError {
                    line: line_no,
                    message,
                })?;
                Step::Resize { cols, rows }
            }
            "ASSERT" => parse_assert(rest).map_err(|message| ReplayError {
                line: line_no,
                message,
            })?,
            _ => {
                return Err(ReplayError {
                    line: line_no,
                    message: format!("unknown directive: {keyword}"),
                });
            }
        };
        steps.push(step);
    }
    Ok(ReplayScript { steps })
}

/// Run a script against a terminal.
#[must_use]
pub fn run(script: &ReplayScript, term: &mut Terminal) -> ReplayReport {
    let mut report = ReplayReport::default();
    for (index, step) in script.steps.iter().enumerate() {
        match step {
            Step::Feed(bytes) => {
                term.feed(bytes);
                report.passed += 1;
            }
            Step::Resize { cols, rows } => {
                term.resize(*cols, *rows);
                report.passed += 1;
            }
            Step::AssertRow { row, text } => {
                let actual = term.grid().row_text(*row);
                if actual == *text {
                    report.passed += 1;
                } else {
                    report.failed.push(format!(
                        "step {index}: ASSERT ROW {row} expected {text:?} got {actual:?}"
                    ));
                }
            }
            Step::AssertCounter { key, value } => match term.counters().get_key(key) {
                Some(actual) if actual == *value => report.passed += 1,
                Some(actual) => report.failed.push(format!(
                    "step {index}: ASSERT COUNTER {key} expected {value} got {actual}"
                )),
                None => report
                    .failed
                    .push(format!("step {index}: unknown counter key {key}")),
            },
            Step::AssertTitle(expected) => {
                let actual = term.grid().title();
                if actual == expected {
                    report.passed += 1;
                } else {
                    report.failed.push(format!(
                        "step {index}: ASSERT TITLE expected {expected:?} got {actual:?}"
                    ));
                }
            }
            Step::AssertCursor { row, col } => {
                let actual = term.grid().cursor();
                if actual == (*row, *col) {
                    report.passed += 1;
                } else {
                    report.failed.push(format!(
                        "step {index}: ASSERT CURSOR expected ({row},{col}) got {actual:?}"
                    ));
                }
            }
        }
    }
    report
}

fn parse_hex(rest: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = rest.chars().filter(|c| !c.is_whitespace()).collect();
    let digits = if let Some(stripped) = cleaned.strip_prefix("0x") {
        stripped
    } else {
        cleaned.as_str()
    };
    if digits.len() % 2 != 0 {
        return Err(format!("hex payload has odd length: {digits}"));
    }
    let bytes = digits.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i + 1 < bytes.len() {
        let hi = hex_value(bytes[i]).ok_or_else(|| format!("bad hex: {}", digits))?;
        let lo = hex_value(bytes[i + 1]).ok_or_else(|| format!("bad hex: {}", digits))?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn parse_quoted(rest: &str) -> Result<Vec<u8>, String> {
    let inner = rest
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .ok_or_else(|| format!("expected quoted string: {rest}"))?;
    let bytes = inner.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let escape = *bytes.get(i + 1).ok_or("trailing backslash")?;
        i += 2;
        match escape {
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'\\' => out.push(b'\\'),
            b'"' => out.push(b'"'),
            b'x' => {
                let hi = *bytes.get(i).ok_or("bad \\x")?;
                let lo = *bytes.get(i + 1).ok_or("bad \\x")?;
                let value =
                    (hex_value(hi).ok_or("bad \\x")? << 4) | hex_value(lo).ok_or("bad \\x")?;
                out.push(value);
                i += 2;
            }
            other => return Err(format!("unknown escape \\{}", char::from(other))),
        }
    }
    Ok(out)
}

fn parse_resize(rest: &str) -> Result<(u16, u16), String> {
    let (cols, rows) = rest
        .split_once('x')
        .ok_or_else(|| format!("expected WxH: {rest}"))?;
    let cols = cols
        .trim()
        .parse::<u16>()
        .map_err(|_| format!("bad cols: {cols}"))?;
    let rows = rows
        .trim()
        .parse::<u16>()
        .map_err(|_| format!("bad rows: {rows}"))?;
    Ok((cols, rows))
}

fn parse_assert(rest: &str) -> Result<Step, String> {
    if let Some(body) = rest.strip_prefix("ROW ") {
        let (index, text) = body
            .split_once('=')
            .ok_or_else(|| format!("expected ROW n = text: {rest}"))?;
        let row = index
            .trim()
            .parse::<u16>()
            .map_err(|_| format!("bad row index: {index}"))?;
        let text = text.strip_prefix(' ').unwrap_or(text);
        return Ok(Step::AssertRow {
            row,
            text: text.to_string(),
        });
    }
    if let Some(body) = rest.strip_prefix("COUNTER ") {
        let (key, value) = body
            .split_once('=')
            .ok_or_else(|| format!("expected COUNTER key = value: {rest}"))?;
        let value = value
            .trim()
            .parse::<u64>()
            .map_err(|_| format!("bad counter value: {value}"))?;
        return Ok(Step::AssertCounter {
            key: key.trim().to_string(),
            value,
        });
    }
    if let Some(body) = rest.strip_prefix("TITLE") {
        let text = body
            .trim_start()
            .strip_prefix('=')
            .unwrap_or(body)
            .trim_start();
        return Ok(Step::AssertTitle(text.to_string()));
    }
    if let Some(body) = rest.strip_prefix("CURSOR") {
        let normalized = body.replace(',', " ");
        let mut parts = normalized.split_whitespace();
        let row = parts
            .next()
            .ok_or_else(|| format!("expected CURSOR r c: {rest}"))?
            .parse::<u16>()
            .map_err(|_| format!("bad cursor row: {rest}"))?;
        let col = parts
            .next()
            .ok_or_else(|| format!("expected CURSOR r c: {rest}"))?
            .parse::<u16>()
            .map_err(|_| format!("bad cursor col: {rest}"))?;
        return Ok(Step::AssertCursor { row, col });
    }
    Err(format!("unknown ASSERT form: {rest}"))
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
