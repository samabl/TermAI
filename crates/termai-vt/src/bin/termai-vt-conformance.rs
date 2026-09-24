//! Headless conformance harness entry point (W1-B / kernel/01 section 3.9).
//!
//! This binary is *test tooling*, not part of the VT contract. It exists so that
//! tools/conformance/run.mjs can drive byte-level corpora through termai-vt
//! offline and headless (AR-03: no PTY, no renderer, single machine) without adding
//! a Rust dependency and without a Cargo.toml entry (auto-discovered bin target).
//!
//! It calls only the public API (parse_trec, run, write_golden, Terminal), so it adds
//! no VT semantics. The replay loop below is a copy of termai_vt::run that additionally
//! records AdvanceReport.consumed; every case reports a PARITY field that the Node
//! runner asserts, so the copy cannot silently drift from the library.
//!
//! Modes
//! -----
//!   (default)   read 'id<TAB>path<TAB>cols<TAB>rows' lines from stdin, print one
//!               deterministic result block per case.
//!   --server    line protocol used by the upstream-suite adapter
//!               (tools/conformance/upstream/): FEED <hex> / RESIZE c r / ROW n /
//!               CURSOR / TITLE / DIMS / HASH / COUNTERS / RESET / QUIT.
//!
//! Determinism: no timestamps, no absolute paths, no environment hashing. The same
//! input always produces the same stdout bytes.

#![forbid(unsafe_code)]

use std::io::{self, BufRead, Write};

use termai_vt::{
    lane_verdict, parse_trec, run, write_golden, Lane, ReplayScript, Step, Terminal, Verdict,
};

const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = if args.iter().any(|arg| arg == "--server") {
        run_server()
    } else {
        run_batch()
    };
    if let Err(err) = result {
        eprintln!("termai-vt-conformance: {err}");
        std::process::exit(2);
    }
}

// ------------------------------------------------------------------ batch mode

fn run_batch() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let line = line?;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t');
        let id = fields.next().unwrap_or("").to_string();
        let path = fields.next().unwrap_or("").to_string();
        let cols = fields
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_COLS);
        let rows = fields
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_ROWS);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                write_block(&mut out, &id, &text, cols, rows)?;
            }
            Err(err) => {
                // Deterministic text only: the OS message is locale dependent and the
                // absolute path is machine dependent, so neither may reach the report.
                writeln!(out, "CASE {id}")?;
                writeln!(out, "VERDICT ERROR")?;
                writeln!(out, "PARITY n/a")?;
                writeln!(
                    out,
                    "FAIL {}",
                    hex_encode(format!("case file unreadable: {:?}", err.kind()).as_bytes())
                )?;
                writeln!(out, "END")?;
            }
        }
        out.flush()?;
    }
    Ok(())
}

struct Execution {
    passed: usize,
    failed: Vec<String>,
    consumed: usize,
    input: usize,
    all_consumed: bool,
    stream: Vec<u8>,
}

fn execute(script: &ReplayScript, term: &mut Terminal) -> Execution {
    let mut out = Execution {
        passed: 0,
        failed: Vec::new(),
        consumed: 0,
        input: 0,
        all_consumed: true,
        stream: Vec::new(),
    };
    for (index, step) in script.steps.iter().enumerate() {
        match step {
            Step::Feed(bytes) => {
                let report = term.feed(bytes);
                out.consumed += report.consumed;
                out.input += bytes.len();
                out.stream.extend_from_slice(bytes);
                if report.consumed != bytes.len() {
                    out.all_consumed = false;
                }
                out.passed += 1;
            }
            Step::Resize { cols, rows } => {
                term.resize(*cols, *rows);
                out.passed += 1;
            }
            Step::AssertRow { row, text } => {
                let actual = term.grid().row_text(*row);
                if actual == *text {
                    out.passed += 1;
                } else {
                    out.failed.push(format!(
                        "step {index}: ASSERT ROW {row} expected {text:?} got {actual:?}"
                    ));
                }
            }
            Step::AssertCounter { key, value } => match term.counters().get_key(key) {
                Some(actual) if actual == *value => out.passed += 1,
                Some(actual) => out.failed.push(format!(
                    "step {index}: ASSERT COUNTER {key} expected {value} got {actual}"
                )),
                None => out
                    .failed
                    .push(format!("step {index}: unknown counter key {key}")),
            },
            Step::AssertTitle(expected) => {
                let actual = term.grid().title();
                if actual == expected {
                    out.passed += 1;
                } else {
                    out.failed.push(format!(
                        "step {index}: ASSERT TITLE expected {expected:?} got {actual:?}"
                    ));
                }
            }
            Step::AssertCursor { row, col } => {
                let actual = term.grid().cursor();
                if actual == (*row, *col) {
                    out.passed += 1;
                } else {
                    out.failed.push(format!(
                        "step {index}: ASSERT CURSOR expected ({row},{col}) got {actual:?}"
                    ));
                }
            }
        }
    }
    out
}

fn write_block(out: &mut impl Write, id: &str, text: &str, cols: u16, rows: u16) -> io::Result<()> {
    writeln!(out, "CASE {id}")?;
    let script = match parse_trec(text) {
        Ok(script) => script,
        Err(err) => {
            writeln!(out, "VERDICT ERROR")?;
            writeln!(out, "PARITY n/a")?;
            writeln!(out, "FAIL {}", hex_encode(err.to_string().as_bytes()))?;
            writeln!(out, "END")?;
            return Ok(());
        }
    };
    // 1. Authoritative verdict: the library replay entry point.
    let mut reference = Terminal::new(cols, rows);
    let expected = run(&script, &mut reference);
    // 2. Traced replay: identical semantics, plus AdvanceReport.consumed.
    let mut term = Terminal::new(cols, rows);
    let traced = execute(&script, &mut term);

    let parity = traced.passed == expected.passed && traced.failed.len() == expected.failed.len();
    let verdict = if traced.failed.is_empty() {
        "PASS"
    } else {
        "FAIL"
    };
    writeln!(out, "VERDICT {verdict}")?;
    writeln!(out, "PARITY {}", if parity { "ok" } else { "mismatch" })?;
    writeln!(out, "STEPS_PASSED {}", traced.passed)?;
    writeln!(out, "STEPS_FAILED {}", traced.failed.len())?;
    writeln!(out, "INPUT_BYTES {}", traced.input)?;
    writeln!(out, "CONSUMED_BYTES {}", traced.consumed)?;
    writeln!(out, "ALL_CONSUMED {}", traced.all_consumed)?;
    let cursor = term.grid().cursor();
    writeln!(out, "DIMS {} {}", term.grid().cols(), term.grid().rows())?;
    writeln!(out, "CURSOR {} {}", cursor.0, cursor.1)?;
    writeln!(out, "GRID_HASH {}", grid_hash(&term))?;
    writeln!(out, "GRID_CONTROL_BYTES {}", grid_control_bytes(&term))?;
    let responses = term.take_responses();
    writeln!(out, "RESPONSES {}", responses.len())?;
    for response in &responses {
        writeln!(out, "RESPONSE {}", hex_encode(response))?;
    }
    for (key, value) in term.counters().as_pairs() {
        if value != 0 {
            writeln!(out, "COUNTER {key} {value}")?;
        }
    }
    for failure in &traced.failed {
        writeln!(out, "FAIL {}", hex_encode(failure.as_bytes()))?;
    }
    if !traced.failed.is_empty() {
        writeln!(out, "STREAM {}", hex_encode(&traced.stream))?;
        let golden = write_golden(&term.snapshot());
        for line in golden.lines() {
            writeln!(out, "GOLDEN {}", hex_encode(line.as_bytes()))?;
        }
    }
    writeln!(out, "END")?;
    Ok(())
}

/// Count grid cells holding a C0/C1 control character. kernel/01 K-06 forbids echoing
/// any part of an unrecognised sequence to the screen; a non-zero count is that bug.
fn grid_control_bytes(term: &Terminal) -> u32 {
    let grid = term.grid();
    let mut count = 0u32;
    for row in 0..grid.rows() {
        for col in 0..grid.cols() {
            if let Some(cell) = grid.cell(row, col) {
                // The right half of a wide char is Cell::WIDE_CONTINUATION (U+0000);
                // it is a placeholder, not a control byte that reached the grid.
                if cell.is_wide_continuation() {
                    continue;
                }
                let code = cell.ch as u32;
                if code < 0x20 || (0x7f..=0x9f).contains(&code) {
                    count += 1;
                }
            }
        }
    }
    count
}

fn grid_hash(term: &Terminal) -> String {
    let golden = write_golden(&term.snapshot());
    golden
        .lines()
        .last()
        .and_then(|line| line.strip_prefix("hash "))
        .unwrap_or("blake3:unknown")
        .to_string()
}

// ----------------------------------------------------------------- server mode

fn run_server() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut term = Terminal::new(DEFAULT_COLS, DEFAULT_ROWS);
    for line in stdin.lock().lines() {
        let line = line?;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }
        let (cmd, rest) = match line.split_once(' ') {
            Some((cmd, rest)) => (cmd, rest.trim()),
            None => (line, ""),
        };
        match cmd {
            "FEED" => match hex_decode(rest) {
                Ok(bytes) => {
                    term.feed(&bytes);
                    for response in term.take_responses() {
                        writeln!(out, "RESP {}", hex_encode(&response))?;
                    }
                    writeln!(out, "OK")?;
                }
                Err(err) => writeln!(out, "ERR {}", hex_encode(err.as_bytes()))?,
            },
            "RESIZE" => {
                let mut parts = rest.split_whitespace();
                let cols = parts.next().and_then(|value| value.parse().ok());
                let rows = parts.next().and_then(|value| value.parse().ok());
                match (cols, rows) {
                    (Some(cols), Some(rows)) => {
                        term.resize(cols, rows);
                        writeln!(out, "OK")?;
                    }
                    _ => writeln!(out, "ERR {}", hex_encode(b"RESIZE needs cols rows"))?,
                }
            }
            "ROW" => {
                let row = rest.parse::<u16>().unwrap_or(0);
                writeln!(
                    out,
                    "ROW {}",
                    hex_encode(term.grid().row_text(row).as_bytes())
                )?;
                writeln!(out, "OK")?;
            }
            "CURSOR" => {
                let cursor = term.grid().cursor();
                writeln!(out, "CURSOR {} {}", cursor.0, cursor.1)?;
                writeln!(out, "OK")?;
            }
            "TITLE" => {
                writeln!(out, "TITLE {}", hex_encode(term.grid().title().as_bytes()))?;
                writeln!(out, "OK")?;
            }
            "DIMS" => {
                writeln!(out, "DIMS {} {}", term.grid().cols(), term.grid().rows())?;
                writeln!(out, "OK")?;
            }
            "HASH" => {
                writeln!(out, "HASH {}", grid_hash(&term))?;
                writeln!(out, "OK")?;
            }
            "LANE" => {
                let mut parts = rest.split_whitespace();
                let lane = match parts.next() {
                    Some("l0") => Some(Lane::L0Parser),
                    Some("l1") => Some(Lane::L1Transport),
                    Some("l2") => Some(Lane::L2EndToEnd),
                    _ => None,
                };
                let passed = parts.next().and_then(|value| value.parse().ok());
                let total = parts.next().and_then(|value| value.parse().ok());
                match (lane, passed, total) {
                    (Some(lane), Some(passed), Some(total)) => {
                        writeln!(
                            out,
                            "LANE {}",
                            verdict_key(lane_verdict(lane, passed, total))
                        )?;
                        writeln!(out, "OK")?;
                    }
                    _ => writeln!(
                        out,
                        "ERR {}",
                        hex_encode(b"LANE needs l0|l1|l2 passed total")
                    )?,
                }
            }
            "CHECKSUM" => {
                // CHECKSUM <top> <left> <bottom> <right>, 1-based inclusive. The
                // upstream esctest suite asks the terminal for a DECRQCRA checksum;
                // answering it here keeps the adapter from inventing screen content.
                let nums: Vec<i64> = rest
                    .split_whitespace()
                    .filter_map(|value| value.parse().ok())
                    .collect();
                if nums.len() == 4 {
                    writeln!(
                        out,
                        "CHECKSUM {}",
                        grid_checksum(&term, nums[0], nums[1], nums[2], nums[3])
                    )?;
                    writeln!(out, "OK")?;
                } else {
                    writeln!(out, "ERR {}", hex_encode(b"CHECKSUM needs 4 numbers"))?;
                }
            }
            "RESET" => {
                term.reset();
                writeln!(out, "OK")?;
            }
            "QUIT" => {
                writeln!(out, "BYE")?;
                out.flush()?;
                return Ok(());
            }
            other => writeln!(
                out,
                "ERR {}",
                hex_encode(format!("unknown command {other}").as_bytes())
            )?,
        }
        out.flush()?;
    }
    Ok(())
}

/// 16-bit DEC-style checksum over a 1-based inclusive rectangle. Blanks count as
/// 0x20 and the right half of a wide char contributes 0x20, matching the "treat all
/// blanks equally" behaviour esctest selects with --xterm-checksum >= 334.
fn grid_checksum(term: &Terminal, top: i64, left: i64, bottom: i64, right: i64) -> u32 {
    let grid = term.grid();
    let rows = i64::from(grid.rows());
    let cols = i64::from(grid.cols());
    let mut sum: u32 = 0;
    let mut row = top.max(1);
    while row <= bottom.min(rows) {
        let mut col = left.max(1);
        while col <= right.min(cols) {
            if let Some(cell) = grid.cell((row - 1) as u16, (col - 1) as u16) {
                let code = if cell.is_wide_continuation() {
                    0x20
                } else {
                    (cell.ch as u32) & 0xffff
                };
                sum = sum.wrapping_add(code);
            }
            col += 1;
        }
        row += 1;
    }
    sum & 0xffff
}

fn verdict_key(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Pass => "pass",
        Verdict::Fail { .. } => "fail",
        Verdict::Registered { .. } => "registered",
        Verdict::NotApplicable { .. } => "not_applicable",
    }
}

// ---------------------------------------------------------------------- hex

fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

fn hex_decode(text: &str) -> Result<Vec<u8>, String> {
    let digits = text.trim();
    if digits.len() % 2 != 0 {
        return Err(format!("hex payload has odd length: {digits}"));
    }
    let bytes = digits.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut index = 0;
    while index + 1 < bytes.len() {
        let hi = hex_value(bytes[index]).ok_or_else(|| format!("bad hex: {digits}"))?;
        let lo = hex_value(bytes[index + 1]).ok_or_else(|| format!("bad hex: {digits}"))?;
        out.push((hi << 4) | lo);
        index += 2;
    }
    Ok(out)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
