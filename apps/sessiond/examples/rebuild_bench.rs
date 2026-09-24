//! AR-26 item 4, timing half: run the sessiond rebuild path N times and report every
//! rebuild's wall time.
//!
//! Measurement definition: `docs/spec/kernel/06-performance-methodology.md` section 3.10
//! (registered separately from HARNESS section 5 as `RELIABILITY_MAPPING` R1/R2 in
//! `tools/bench/reliability.mjs`, ADR-0029 D-4). This binary is a **driver, not a judge**: it
//! emits raw per-rebuild wall times plus the mechanism equality verdict, and it decides no
//! threshold. The Node producer `tools/bench/sessiond-rebuild.mjs` computes the percentile
//! statistic, aggregates over Runs and emits the `bench-report.json`; the machine binding that
//! decides whether the numbers may ever be a gate number (RM-A, T0) lives there too.
//!
//! The timed window is one rebuild plus the response artefacts the daemon needs to answer a
//! reconnecting client: `rebuild_session` (Log read + checkpoint window + VT tail replay) and
//! the rebuilt `GRID_SNAPSHOT` plus its digest (what `GRID_SNAPSHOT` / `ATTACH_ACK` carry).
//! The equality comparison happens **outside** the timed window, so the test's own hashing
//! never inflates the number.
//!
//! Usage: `rebuild_bench [--samples N] [--warmup N] [--lines L] [--cols C] [--rows R] [--out FILE]`
//!
//! Stdout gets one human summary line; `--out` gets the JSON document the producer reads
//! (written to a file rather than piped so the driver needs no pipe and no stdout parsing).
//! Exit codes: 0 = every rebuild reproduced the screen, 2 = usage/IO error, 3 = the rebuild
//! changed the screen (a mechanism defect: the driver then publishes no timing at all).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sessiond::registry::TerminalEngine;
use sessiond::restore::{fixture, rebuild_session};

/// What one rebuild is asked to reproduce.
struct Args {
    samples: usize,
    warmup: usize,
    lines: usize,
    cols: u16,
    rows: u16,
    out: Option<PathBuf>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            samples: 1_000,
            warmup: 2,
            lines: 2_000,
            cols: 120,
            rows: 40,
            out: None,
        }
    }
}

fn usage() -> String {
    "usage: rebuild_bench [--samples N] [--warmup N] [--lines L] [--cols C] [--rows R] [--out FILE]"
        .to_string()
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut args = Args::default();
    let mut i = 0;
    while i < argv.len() {
        let flag = argv[i].as_str();
        let value = argv
            .get(i + 1)
            .ok_or_else(|| format!("{flag} needs a value\n{}", usage()))?;
        match flag {
            "--samples" => args.samples = value.parse().map_err(|_| "bad --samples")?,
            "--warmup" => args.warmup = value.parse().map_err(|_| "bad --warmup")?,
            "--lines" => args.lines = value.parse().map_err(|_| "bad --lines")?,
            "--cols" => args.cols = value.parse().map_err(|_| "bad --cols")?,
            "--rows" => args.rows = value.parse().map_err(|_| "bad --rows")?,
            "--out" => args.out = Some(PathBuf::from(value)),
            other => return Err(format!("unknown argument {other}\n{}", usage())),
        }
        i += 2;
    }
    if args.samples == 0 {
        return Err("--samples must be at least 1".to_string());
    }
    Ok(args)
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Per-rebuild equality verdicts. `all` is the one that gates the run.
#[derive(Default)]
struct Equality {
    snapshot: bool,
    canonical_bytes: bool,
    digest: bool,
    rows: bool,
    replayed_all: bool,
    checked: usize,
    first_mismatch: Option<String>,
}

impl Equality {
    fn all(&self) -> bool {
        self.checked > 0
            && self.snapshot
            && self.canonical_bytes
            && self.digest
            && self.rows
            && self.replayed_all
    }

    fn note(&mut self, ok: bool, sample: usize, what: &str) {
        if !ok && self.first_mismatch.is_none() {
            self.first_mismatch = Some(format!("sample {sample}: {what}"));
        }
    }
}

fn observe(
    eq: &mut Equality,
    sample: usize,
    after: &termai_core::grid::GridSnapshot,
    after_digest: [u8; 32],
    fx: &fixture::LogFixture,
    raw_replayed: usize,
) {
    let snapshot = after == &fx.state;
    let canonical = after.canonical_bytes() == fx.state.canonical_bytes();
    let digest = after_digest == fx.digest;
    let mut rows = true;
    for row in 0..fx.rows {
        if after.row_text(row) != fx.state.row_text(row) {
            rows = false;
            break;
        }
    }
    let replayed = raw_replayed == fx.pty_out_records;
    eq.snapshot &= snapshot;
    eq.canonical_bytes &= canonical;
    eq.digest &= digest;
    eq.rows &= rows;
    eq.replayed_all &= replayed;
    eq.checked += 1;
    eq.note(
        snapshot,
        sample,
        "the rebuilt GridSnapshot differs from the live screen",
    );
    eq.note(
        canonical,
        sample,
        "canonical_bytes differ after the rebuild",
    );
    eq.note(digest, sample, "the rebuilt grid digest differs");
    eq.note(rows, sample, "a rebuilt row's text differs");
    eq.note(
        replayed,
        sample,
        "recovery did not replay every PtyOut record",
    );
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn main() {
    if let Err(err) = run() {
        eprintln!("rebuild_bench: {err}");
        std::process::exit(2);
    }
}

#[allow(clippy::too_many_lines)]
fn run() -> Result<(), String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&argv)?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "termai-rebuild-bench-{}-{stamp}",
        std::process::id()
    ));

    let fx = fixture::write_session_log(&dir, args.cols, args.rows, args.lines)
        .map_err(|e| format!("cannot build the Log fixture: {e}"))?;
    if fx.checkpoints != 0 {
        return Err(
            "the Log carries a checkpoint and CAS restore is unimplemented; a checkpoint window \
             is not faithfully replayable, so no timing is published (see apps/sessiond/src/restore.rs)"
                .to_string(),
        );
    }

    let total = args.warmup + args.samples;
    let mut samples_ms: Vec<f64> = Vec::with_capacity(args.samples);
    let mut eq = Equality {
        snapshot: true,
        canonical_bytes: true,
        digest: true,
        rows: true,
        replayed_all: true,
        checked: 0,
        first_mismatch: None,
    };
    let started = Instant::now();
    for sample in 0..total {
        // Warmup rebuilds are discarded from the sample set (kernel/06 section 4,
        // `Measurement::warmup`: "丢弃前 2 次") but still checked for equality.
        let t0 = Instant::now();
        let rebuilt = rebuild_session(&dir, args.cols, args.rows)
            .map_err(|e| format!("rebuild {sample} failed: {e}"))?;
        let snapshot = rebuilt.snapshot();
        let digest = rebuilt.engine.digest();
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1_000.0;
        observe(
            &mut eq,
            sample,
            &snapshot,
            digest,
            &fx,
            rebuilt.outcome.raw_replayed,
        );
        if sample >= args.warmup {
            samples_ms.push(elapsed_ms);
        }
    }
    let wall_ms = started.elapsed().as_secs_f64() * 1_000.0;
    std::fs::remove_dir_all(&dir).ok();

    let mut sorted = samples_ms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let min = sorted.first().copied().unwrap_or(f64::NAN);
    let max = sorted.last().copied().unwrap_or(f64::NAN);
    let median = sorted.get(sorted.len() / 2).copied().unwrap_or(f64::NAN);

    let mut json = String::with_capacity(samples_ms.len() * 12 + 1_024);
    let _ = write!(
        json,
        "{{\"cols\":{},\"rows\":{},\"lines\":{},\"warmup\":{},\"samples_per_run\":{},",
        args.cols, args.rows, args.lines, args.warmup, args.samples
    );
    let _ = write!(
        json,
        "\"fixture\":{{\"bytes\":{},\"records\":{},\"pty_out_records\":{},\"pty_out_bytes\":{},\
         \"input_events\":{},\"segments\":{},\"checkpoints\":{},\"log_sha256\":\"{}\",\
         \"state_digest\":\"{}\"}},",
        fx.bytes,
        fx.records,
        fx.pty_out_records,
        fx.pty_out_bytes,
        fx.input_events,
        fx.segments,
        fx.checkpoints,
        hex(&fx.log_sha256),
        hex(&fx.digest)
    );
    let _ = write!(
        json,
        "\"equality\":{{\"snapshot\":{},\"canonical_bytes\":{},\"digest\":{},\"rows\":{},\
         \"replayed_all\":{},\"all_equal\":{},\"samples_checked\":{},\"first_mismatch\":{}}},",
        eq.snapshot,
        eq.canonical_bytes,
        eq.digest,
        eq.rows,
        eq.replayed_all,
        eq.all(),
        eq.checked,
        match &eq.first_mismatch {
            Some(m) => format!("\"{}\"", json_escape(m)),
            None => "null".to_string(),
        }
    );
    json.push_str("\"samples_ms\":[");
    for (i, v) in samples_ms.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        let _ = write!(json, "{v:.6}");
    }
    let _ = write!(json, "],\"driver_wall_ms\":{wall_ms:.3}}}");

    if let Some(path) = &args.out {
        std::fs::write(path, &json).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    } else {
        println!("{json}");
    }

    println!(
        "rebuild_bench: samples={} warmup={} fixture_bytes={} records={} equality={} \
         min={min:.3}ms median={median:.3}ms max={max:.3}ms log_sha256={} state_digest={}",
        samples_ms.len(),
        args.warmup,
        fx.bytes,
        fx.records,
        if eq.all() { "ok" } else { "MISMATCH" },
        hex(&fx.log_sha256),
        hex(&fx.digest)
    );
    if let Some(m) = &eq.first_mismatch {
        eprintln!("rebuild_bench: the rebuild changed the screen -- {m}");
        eprintln!(
            "rebuild_bench: refusing to publish timings for a broken rebuild (AR-26 item 4's \
             timing row is only meaningful if the rebuild reproduces the screen)"
        );
        std::process::exit(3);
    }
    Ok(())
}
