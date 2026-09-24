#!/usr/bin/env node
// tools/bench/sessiond-rebuild.mjs
//
// Producer for HARNESS section 8.2's reliability row "sessiond 重建 P95 <=2s / P99 <=5s"
// (AR-26 item 4). Measurement definition: docs/spec/kernel/06-performance-methodology.md
// section 3.10, registered as a SEPARATE mapping from HARNESS section 5 -- RELIABILITY_MAPPING
// rows R1 (P95) / R2 (P99) in tools/bench/reliability.mjs (ADR-0029 D-4). This producer never
// touches SECTION5_MAPPING and never puts a reliability row in it.
//
//   node tools/bench/sessiond-rebuild.mjs [--runs 10] [--samples 10000] [--warmup 2]
//        [--lines 500] [--cols 120] [--rows 40] [--out <path>] [--json] [--no-build]
//
// What it drives: apps/sessiond/examples/rebuild_bench.rs, which performs the real rebuild path
// (`sessiond::restore::rebuild_session` = Log read + checkpoint window + VT tail replay into a
// fresh engine, then the GRID_SNAPSHOT + digest a reconnecting client receives) N times per Run
// and reports every rebuild's wall time. The binary asserts mechanism equality on every single
// rebuild; if a rebuild ever reproduces a different screen, this producer refuses to publish a
// report at all (a timing claim about a broken rebuild is worthless).
//
// Statistic (kernel/06 section 3.10 + section 3.2's frame row + K-02's three layers):
//   samples (per rebuild) -> Run statistic (P95 / P99 inside one Run)
//                         -> report value = MEDIAN over N>=10 Runs' statistics
// The document fixes that aggregation; it does not name an interpolation rule for the
// percentile itself, so this producer uses nearest-rank (the ceil(p*n)-th smallest) and says so
// in the report's `method` field rather than leaving the choice implicit.
//
// Honesty boundary (AR-20 / ADR-0014 iron law 5): this host has no registered reference machine
// fingerprint, so both rows come out INCONCLUSIVE (REFERENCE_MACHINE_UNAVAILABLE) with
// gating=false. Nothing here can produce a PASS, and the report is written where the bench gate
// only reads it when asked (`--report`), so no gate summary changes silently.

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

import * as B from './lib.mjs';
import * as REL from './reliability.mjs';
import * as V from './values.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..', '..');
const EXAMPLE = 'rebuild_bench';
const DEFAULT_OUT = path.join(ROOT, 'target', 'bench-reports', 'sessiond-rebuild-report.json');

// kernel/06 section 3.10「Run 定义」: a Run is one sessiond rebuild... with >= 1e4 samples in it,
// otherwise P99 must be INVALID(INSUFFICIENT_SAMPLES). spec 07 3.8.3-1 / kernel/06 3.1: N >= 10
// Runs, and the report value is the median over the Runs' statistics.
const RUNS_FLOOR = 10;
const SAMPLES_FLOOR = 10000;

// --------------------------------------------------- small helpers

// kernel/06 fixes the aggregation, not the estimator. Nearest-rank, declared here and in the
// report's `method`: P95 of n samples is the ceil(0.95*n)-th smallest value.
export function nearestRank(sortedAscending, p) {
  if (!sortedAscending.length) throw new Error('nearestRank: empty sample set');
  const rank = Math.ceil(p * sortedAscending.length);
  const index = Math.min(Math.max(rank, 1), sortedAscending.length) - 1;
  return sortedAscending[index];
}

export function runStatistic(samplesMs, p) {
  const sorted = samplesMs.slice().sort(function (a, b) { return a - b; });
  return nearestRank(sorted, p);
}

function medianOf(values) {
  return B.median(values);
}

function madOverMedianOf(values) {
  const m = medianOf(values);
  if (m === 0) return 0;
  return B.medianAbsoluteDeviation(values) / m;
}

function tryExec(file, args) {
  try {
    return execFileSync(file, args, { cwd: ROOT, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
  } catch (e) {
    return null;
  }
}

function parseArgs(argv) {
  const out = {
    runs: RUNS_FLOOR,
    samples: SAMPLES_FLOOR,
    warmup: 2,
    lines: 500,
    cols: 120,
    rows: 40,
    out: DEFAULT_OUT,
    json: false,
    build: true,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const next = function () { return argv[++i]; };
    if (a === '--runs') out.runs = parseInt(next(), 10);
    else if (a === '--samples') out.samples = parseInt(next(), 10);
    else if (a === '--warmup') out.warmup = parseInt(next(), 10);
    else if (a === '--lines') out.lines = parseInt(next(), 10);
    else if (a === '--cols') out.cols = parseInt(next(), 10);
    else if (a === '--rows') out.rows = parseInt(next(), 10);
    else if (a === '--out') out.out = path.resolve(next());
    else if (a === '--json') out.json = true;
    else if (a === '--no-build') out.build = false;
    else if (a === '--help' || a === '-h') out.help = true;
    else throw new Error('unknown argument ' + a);
  }
  for (const k of ['runs', 'samples', 'lines', 'cols', 'rows', 'warmup']) {
    if (!Number.isInteger(out[k]) || out[k] < (k === 'warmup' ? 0 : 1)) {
      throw new Error('--' + k + ' must be a positive integer');
    }
  }
  if (out.cols > 65535 || out.rows > 65535) throw new Error('--cols/--rows must fit u16');
  return out;
}

// --------------------------------------------------- driver

function driverPath() {
  return path.join(ROOT, 'target', 'release', 'examples', EXAMPLE + (process.platform === 'win32' ? '.exe' : ''));
}

function buildDriver() {
  execFileSync('cargo', ['build', '--release', '--example', EXAMPLE, '-p', 'sessiond'], { cwd: ROOT, stdio: 'inherit' });
  const exe = driverPath();
  if (!fs.existsSync(exe)) throw new Error('the rebuild driver was not produced at ' + exe);
  return exe;
}

// One Run = one driver invocation = `samples` rebuilds of the SAME deterministic Log fixture.
// The driver writes its JSON to a file (no stdout parsing, no pipe dependency); it exits
// non-zero when a rebuild changed the screen, which is a mechanism defect, not a measurement.
function runOnce(exe, index, opts) {
  const file = path.join(os.tmpdir(), 'termai-rebuild-run-' + process.pid + '-' + index + '.json');
  try {
    execFileSync(exe, [
      '--samples', String(opts.samples),
      '--warmup', String(opts.warmup),
      '--lines', String(opts.lines),
      '--cols', String(opts.cols),
      '--rows', String(opts.rows),
      '--out', file,
    ], { cwd: ROOT, stdio: ['ignore', 'inherit', 'inherit'] });
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } finally {
    fs.rmSync(file, { force: true });
  }
}

// --------------------------------------------------- report assembly

function commitInfo() {
  const sha = tryExec('git', ['rev-parse', 'HEAD']);
  const status = tryExec('git', ['status', '--porcelain']);
  return { sha: sha || 'unknown', dirty: status === null ? null : status.length > 0 };
}

function toolchainInfo() {
  const rustc = tryExec('rustc', ['--version']);
  let channel = 'unknown';
  try {
    const text = fs.readFileSync(path.join(ROOT, 'rust-toolchain.toml'), 'utf8');
    const m = /channel\s*=\s*"([^"]+)"/.exec(text);
    if (m) channel = m[1];
  } catch (e) { /* the pin is documentation here, not a control */ }
  return {
    rustc: rustc || 'unknown',
    channel: rustc ? (channel + ' (pinned; rustc reports ' + rustc.replace(/^rustc\s*/, '') + ')') : channel,
  };
}

// The window this producer times, in the words of the report itself: kernel/06 3.10's Run
// definition is "client sends reconnect / attach -> a usable GRID_SNAPSHOT (or a covering
// TAIL_REPLAY) exists and the grid is interactive", server-observable segment only. For a
// rebuild that segment is: Log read + checkpoint window + VT tail replay into a fresh engine +
// the snapshot and digest the daemon answers with. The producer states the fixture parameters
// (the "scene") because they are part of the number, and it states the estimator it used.
function methodText(row, opts, fixture, host, sampleFloorMet) {
  return [
    'window = kernel/06 3.10「Run 定义」server-observable segment: rebuild_session (Log read + checkpoint window + VT tail replay into a fresh engine) + the GRID_SNAPSHOT snapshot and its digest; equality of the rebuilt screen is asserted on every rebuild by apps/sessiond/examples/rebuild_bench.rs',
    'statistic = kernel/06 3.10 + 3.2 frame row + K-02 three layers: ' + row.statistic + ' inside one Run, report value = median over ' + opts.runs + ' Run(s); estimator = nearest-rank (ceil(p*n)-th smallest), declared because section 3.10 fixes the aggregation and not the interpolation',
    'scene = deterministic Session Log fixture built through sessiond::restore::fixture (Registry::feed_pty_out) - lines=' + opts.lines + ' bytes=' + fixture.bytes + ' records=' + fixture.records + ' pty_out_records=' + fixture.pty_out_records + ' cols=' + opts.cols + ' rows=' + opts.rows + ' checkpoints=' + fixture.checkpoints + ' sha256=' + fixture.log_sha256 + ' state_digest=' + fixture.state_digest,
    'sample floor: ' + opts.samples + ' rebuilds per Run (kernel/06 3.10 requires >= ' + SAMPLES_FLOOR + ' to report P99) -> ' + (sampleFloorMet ? 'met' : 'NOT MET: on a reference machine this Run would be INVALID(INSUFFICIENT_SAMPLES)'),
    'machine binding: no registered reference-machine fingerprint on this host, so the row is ' + host.state + '(' + host.reason + ') and gating=false (kernel/06 3.10「本机状态」+ ADR-0014 iron law 5); kernel/06 3.1 step 1 would also refuse a verdict (INVALID(FP_MISSING)), so no PASS is claimable either way',
    'warmup: the first ' + opts.warmup + ' rebuild(s) of each Run were discarded from the sample set (§4 Measurement::warmup) but still equality-checked',
  ].join(' | ');
}

function buildReport(opts, runs, host, commit, toolchain, startedAt) {
  const now = new Date().toISOString();
  const r1 = REL.findReliability('R1');
  const r2 = REL.findReliability('R2');
  const p95s = runs.map(function (r) { return r.p95; });
  const p99s = runs.map(function (r) { return r.p99; });
  const fixture = runs[0].fixture;
  const sceneHash = B.sha256Hex(fixture.log_sha256 + ':' + fixture.state_digest);
  const sceneStable = runs.every(function (r) { return r.fixture.log_sha256 === fixture.log_sha256 && r.fixture.state_digest === fixture.state_digest; });
  const d1 = runs.length >= 2
    ? Math.max(Math.abs(runs[1].p95 - runs[0].p95) / (runs[0].p95 || 1), Math.abs(runs[1].p99 - runs[0].p99) / (runs[0].p99 || 1))
    : null;
  const epsSelf = B.epsSelfFor(r1.family); // family "frame": 5% (kernel/06 3.6 / AR-31 #9)
  const sampleFloorMet = opts.samples >= SAMPLES_FLOOR;
  const samplesChecked = runs.reduce(function (a, r) { return a + r.n; }, 0);
  const minutes = (Date.now() - startedAt) / 60000;

  const metric = function (row, values) {
    return {
      metric: row.metric,
      value: medianOf(values),
      unit: row.unit,
      samples: opts.samples,
      runner: 'no-fingerprint-registered',
      commit: commit.sha,
      toolchain: toolchain.rustc,
      ts: now,
      statistic: row.statistic,
      runs: runs.length,
      runStat: 'median',
      madOverMedian: madOverMedianOf(values),
      gate: row.gate,
      target: null,
      gating: false,
      verdict: host.state,
      method: methodText(row, opts, fixture, host, sampleFloorMet),
      dCalibrationMs: null,
      artifacts: [path.relative(ROOT, opts.out), path.relative(ROOT, runsArtifactPath(opts))],
    };
  };

  return {
    schemaVersion: '1.0.0',
    reportId: 'sessiond-rebuild-' + commit.sha.slice(0, 8) + '-' + startedAt,
    generatedAt: now,
    commit: { sha: commit.sha, dirty: commit.dirty === true },
    toolchain: { rustc: toolchain.rustc, channel: toolchain.channel },
    runner: { class: 'self-hosted', machineId: 'no-fingerprint-registered', role: 'none', backend: 't0' },
    // kernel/06 3.7's schema requires a 64-hex value; this host has no registered fingerprint,
    // so the honest value is the all-zero sentinel (there is nothing to hash) and every row
    // carries gating=false + the INCONCLUSIVE machine binding.
    fingerprintSha256: '0'.repeat(64),
    environment: {
      powerPlan: 'unknown (not controlled by this producer)',
      exclusive: false,
      warmed: opts.warmup,
      valid: false,
    },
    selfcheck: {
      scene: { verdict: sceneStable ? 'PASS' : 'INVALID', frames: samplesChecked, hash: sceneHash },
      doubleRun: { verdict: d1 === null ? 'INVALID' : (d1 <= epsSelf ? 'PASS' : 'INVALID'), deltaPct: d1 === null ? 1 : d1 },
      verdict: (sceneStable && d1 !== null && d1 <= epsSelf) ? 'PASS' : 'INVALID',
    },
    corpus: { manifestSha256: fixture.log_sha256, shardsOk: fixture.segments, bytes: fixture.bytes },
    metrics: [metric(r1, p95s), metric(r2, p99s)],
    verdict: host.state,
    cost: {
      minutes: Number(minutes.toFixed(3)),
      runnerClass: 'self-hosted',
      // ADR-0014 decision 6 magnitude for self-hosted machines (0.04-0.08 USD/core-hour; RM-A is
      // 8 cores). The low end is used, and no CI runner was launched for a local run.
      estUsd: Number((minutes / 60 * 8 * 0.04).toFixed(4)),
    },
  };
}

function runsArtifactPath(opts) {
  return opts.out.replace(/\.json$/, '') + '.runs.json';
}

// --------------------------------------------------- main

function main(argv) {
  const opts = parseArgs(argv);
  if (opts.help) {
    console.log('usage: node tools/bench/sessiond-rebuild.mjs [--runs 10] [--samples 10000] [--warmup 2] [--lines 500] [--cols 120] [--rows 40] [--out <path>] [--json] [--no-build]');
    return 0;
  }
  const startedAt = Date.now();
  const host = B.classifyMachine({ fingerprintRecorded: false, class: 'self-hosted', backend: 't0' });
  const commit = commitInfo();
  const toolchain = toolchainInfo();

  const exe = opts.build ? buildDriver() : driverPath();
  if (!fs.existsSync(exe)) throw new Error('the rebuild driver is missing at ' + exe + '; run without --no-build');

  console.log('sessiond rebuild producer (AR-26 item 4 / HARNESS §8.2 reliability; measurement definition kernel/06 §3.10)');
  console.log('  driver: ' + path.relative(ROOT, exe) + '  runs=' + opts.runs + ' samples/run=' + opts.samples + ' warmup=' + opts.warmup + ' scene: lines=' + opts.lines + ' cols=' + opts.cols + ' rows=' + opts.rows);

  const runs = [];
  for (let i = 0; i < opts.runs; i++) {
    const raw = runOnce(exe, i, opts);
    const p95 = runStatistic(raw.samples_ms, 0.95);
    const p99 = runStatistic(raw.samples_ms, 0.99);
    const entry = {
      run: i + 1,
      n: raw.samples_ms.length,
      p95: p95,
      p99: p99,
      median: medianOf(raw.samples_ms),
      min: Math.min.apply(null, raw.samples_ms),
      max: Math.max.apply(null, raw.samples_ms),
      wallMs: raw.driver_wall_ms,
      equality: raw.equality,
      fixture: raw.fixture,
    };
    runs.push(entry);
    console.log('  run ' + String(entry.run).padStart(2) + ': n=' + entry.n + ' p95=' + p95.toFixed(4) + 'ms p99=' + p99.toFixed(4) + 'ms median=' + entry.median.toFixed(4) + 'ms min=' + entry.min.toFixed(4) + 'ms max=' + entry.max.toFixed(4) + 'ms equality=' + (entry.equality.all_equal ? 'ok (' + entry.equality.samples_checked + ' rebuilds checked)' : 'MISMATCH ' + entry.equality.first_mismatch));
  }

  const report = buildReport(opts, runs, host, commit, toolchain, startedAt);

  // Self-check against the reader the bench gate uses: a producer that emits a report the gate
  // then rejects is broken, so the binding is verified here too (the real judgement is
  // `node tools/bench/check.mjs --report <path>`).
  const readAs = [{ rel: path.relative(ROOT, opts.out), json: report, parseError: null }];
  const binding = V.collectReliabilityValues(readAs, host);
  const schema = B.validateBenchReport(report);
  if (!schema.ok) {
    for (const e of schema.errors.slice(0, 6)) console.error('  schema: ' + e.code + ' ' + e.path + ': ' + e.message);
    throw new Error('the produced report does not satisfy the kernel/06 §3.7 schema');
  }
  if (binding.problems.length) {
    for (const p of binding.problems) console.error('  binding: ' + p);
    throw new Error('the produced report does not bind to RELIABILITY_MAPPING');
  }
  if (binding.reported.join(',') !== 'R1,R2') {
    throw new Error('the report must carry both rows (kernel/06 §3.10: 两行都要报，不得只报 P95)');
  }

  fs.mkdirSync(path.dirname(opts.out), { recursive: true });
  fs.writeFileSync(opts.out, JSON.stringify(report, null, 2) + '\n');
  const runDetail = {
    reportId: report.reportId,
    generatedAt: report.generatedAt,
    runner: report.runner,
    sampleFloor: SAMPLES_FLOOR,
    runs: runs.map(function (r) {
      return {
        run: r.run,
        n: r.n,
        p95: r.p95,
        p99: r.p99,
        median: r.median,
        min: r.min,
        max: r.max,
        wallMs: r.wallMs,
        scene: r.fixture.log_sha256 + ':' + r.fixture.state_digest,
      };
    }),
  };
  fs.writeFileSync(runsArtifactPath(opts), JSON.stringify(runDetail, null, 2) + '\n');

  const sampleFloorMet = opts.samples >= SAMPLES_FLOOR;
  console.log('  selfcheck: scene ' + report.selfcheck.scene.verdict + ' (' + opts.runs + ' run(s) on scene ' + report.selfcheck.scene.hash.slice(0, 16) + '…), doubleRun ' + report.selfcheck.doubleRun.verdict + ' (delta ' + (report.selfcheck.doubleRun.deltaPct * 100).toFixed(3) + '% <= eps_self ' + (B.epsSelfFor('frame') * 100).toFixed(0) + '% for family frame)');
  if (report.selfcheck.verdict !== 'PASS') {
    console.log('  selfcheck NOT PASS: kernel/06 §3.6 D1 / §3.5 say this run must not be compared with a baseline and must not gate anything even on a reference machine (K-04: fix the environment first). It does not change the verdict here: the rows are INCONCLUSIVE because this host has no registered reference machine.');
  }
  console.log('  samples/run: ' + opts.samples + ' vs kernel/06 §3.10 floor ' + SAMPLES_FLOOR + ' -> ' + (sampleFloorMet ? 'met' : 'NOT MET (on a reference machine the P99 row would be INVALID(INSUFFICIENT_SAMPLES))'));
  for (const m of report.metrics) {
    const row = m.metric.indexOf('.p95') > 0 ? REL.findReliability('R1') : REL.findReliability('R2');
    const limit = B.madLimitFor(row.family);
    // Never print "8.9% <= 5%": the grading is stated as a comparison that can fail.
    const madNote = (typeof limit === 'number' && m.madOverMedian > limit)
      ? 'MAD/median ' + (m.madOverMedian * 100).toFixed(3) + '% EXCEEDS the family ' + row.family + ' grading ' + (limit * 100).toFixed(0) + '% -> on a reference machine this run would be INCONCLUSIVE(NOISE_FLOOR), and kernel/06 §3.1 step 8 forbids judging it until the environment is fixed (K-04)'
      : 'MAD/median ' + (m.madOverMedian * 100).toFixed(3) + '% within the family ' + row.family + ' grading ' + (limit * 100).toFixed(0) + '%';
    console.log('  ' + row.id + ' ' + m.metric + ' = ' + m.value.toFixed(4) + ' ' + m.unit + '  [gate ' + row.gateOp + ' ' + m.gate + ' ' + m.unit + '; statistic ' + m.statistic + '; runs ' + m.runs + '; runStat ' + m.runStat + '; ' + madNote + '; gating ' + m.gating + ']');
    console.log('      verdict ' + m.verdict + ' (' + host.reason + ') -- no registered RM-A on this host: this is a NON_GATING number, never a gate result (ADR-0014 iron law 5 / kernel/06 §3.10)');
  }
  console.log('  report: ' + path.relative(ROOT, opts.out) + '   run detail: ' + path.relative(ROOT, runsArtifactPath(opts)));
  console.log('  gate read: node tools/bench/check.mjs --report ' + path.relative(ROOT, opts.out));
  if (opts.json) console.log(JSON.stringify(report, null, 2));
  return 0;
}

const invokedDirectly = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) {
  try {
    process.exit(main(process.argv.slice(2)));
  } catch (e) {
    console.error('sessiond-rebuild: ' + (e && e.message ? e.message : String(e)));
    process.exit(2);
  }
}
