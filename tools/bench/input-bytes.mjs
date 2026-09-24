#!/usr/bin/env node
// tools/bench/input-bytes.mjs
//
// Producer for HARNESS section 5 row H12 "输入字节等价（键盘 / 粘贴 / IME commit 全语料回放，
// 逐字节比对）= 100%". Unlike H1-H11 and the section 8.2 reliability rows, H12 is registered with
// `machine: 'none'` (tools/bench/registry.mjs): no reference machine produces this number, because
// byte equality is machine independent. That is why this producer can be the first carrier that
// actually produces a section 5 value on an ordinary host.
//
//   node tools/bench/input-bytes.mjs [--corpus <path>] [--out <path>] [--json] [--no-build]
//                                   [--inject-mismatch <caseId>]
//
// What it drives: the REAL encoder, `termai_core::input::StandardEncoder` -- the single S3 encoder
// of AR-29 item 3 / kernel/05 K-01. It does that by compiling tools/bench/input-bytes-driver.rs
// against the termai-core rlib that `cargo build --release -p termai-core` just produced, and
// feeding the corpus to it over a tab-separated line protocol. No crate is modified and no encoder
// is re-implemented anywhere: the driver contains no encoding logic, only a sink that records what
// the production encoder emitted.
//
// Judgement: every corpus case carries expected PTY bytes with an authority (a kernel/05 clause id
// or an xterm ctlseqs rule). A case passes only when the bytes AND the EncodeOutcome match
// byte-for-byte; H12 = matched / exercised * 100. Cases whose expectation has no citable authority
// are NOT in the corpus at all -- they are listed in the corpus' `omitted` array and printed here,
// so the number can never be mistaken for a claim about the whole of kernel/05 section 5.
//
// Honesty boundary:
//   * H12 is judged here on the bytes only. The corpus exercises S3 (and, for paste, S5's
//     PasteGate decision); it does NOT exercise the platform half of the input stack (S0-S2:
//     FocusRouter, KeyTranslator, dead-key composition, the native IME host), which does not exist
//     yet. The per-source breakdown printed below says exactly which sources ran and which are
//     absent, and the report's `method` field repeats it.
//   * the row is machine-free, so `check.mjs` never counts it as a *machine-bound* gate number:
//     ADR-0029 D-5 restricts `gatingNumbersProduced` to rows whose registry `machine` is RM-A /
//     RM-C. The report still declares `gating: true`, because HARNESS section 5 does gate this row
//     and nothing about it needs a reference machine (the same shape as H13, governed yes /
//     machine none).
//
// Exit code: 0 when H12 = 100, 1 when a case mismatched (the report is still written -- the
// mismatch IS the measurement), 2 when the producer itself could not run (no cargo/rustc, a broken
// corpus, an unusable driver). A missing toolchain is reported, never papered over with a stub.

import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

import * as B from './lib.mjs';
import * as R from './registry.mjs';
import * as V from './values.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..', '..');
const DRIVER_SRC = path.join(HERE, 'input-bytes-driver.rs');
const BUILD_DIR = path.join(ROOT, 'target', 'bench-input-bytes');
const DEFAULT_CORPUS = path.join(HERE, 'input-corpus.json');
const DEFAULT_OUT = path.join(ROOT, 'target', 'bench-reports', 'input-bytes-report.json');

const METRIC_ID = 'H12';
const UNIT = 'pct';

// The five byte sources of AR-29 item 3 / K-01, plus the focus pair kernel/05 section 3.1 lists in
// InputEvent. Every one of them must either appear in the coverage table or be reported absent.
export const DECLARED_SOURCES = ['keyboard', 'paste', 'ime_commit', 'mouse', 'api_inject', 'focus'];

// Sources that exist in the design but have no implementation to drive yet. They are printed as
// absent so a reader cannot mistake this corpus for the whole of kernel/05 section 5.
export const ABSENT_SOURCES = [
  {
    source: 'IME platform host (preedit / candidate window lifecycle)',
    detail: 'kernel/05 §3.4 platform table (TSF / NSTextInputClient / text-input-v3) is unimplemented, so no preedit or commit lifecycle can be replayed. The S3 encoding of InputEvent::TextCommit IS exercised (source ime_commit).',
  },
  {
    source: 'KeyTranslator / dead-key composition (S2)',
    detail: 'kernel/05 §3.1 S2 and §3.2 dead-key state machine are unimplemented; only "a bare Dead event produces 0 bytes" can be asserted (case L14).',
  },
  {
    source: 'FocusRouter (S1) + IN-04 Super routing',
    detail: 'kernel/05 §3.1 S1 / §3.2 row 4 ("Super / Meta 默认不发往 PTY，交 chrome Action Registry") is a routing rule above S3; with no FocusRouter, driving S3 cannot exercise it.',
  },
  {
    source: 'local clipboard read + OSC 52 read policy (IN-AC-10)',
    detail: 'kernel/05 §3.7 OSC 52 is an application-driven output/response path, not an S3 input byte path; the paste friction it feeds IS exercised (source paste).',
  },
];

const OMIT_NOTE = 'expectations deliberately left out of the corpus for lack of a citable authority';

// --------------------------------------------------- helpers

function parseArgs(argv) {
  const out = { corpus: DEFAULT_CORPUS, out: DEFAULT_OUT, json: false, build: true, inject: null, help: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const next = function () { return argv[++i]; };
    if (a === '--corpus') out.corpus = path.resolve(next());
    else if (a === '--out') out.out = path.resolve(next());
    else if (a === '--inject-mismatch') out.inject = next();
    else if (a === '--json') out.json = true;
    else if (a === '--no-build') out.build = false;
    else if (a === '--help' || a === '-h') out.help = true;
    else throw new Error('unknown argument ' + a);
  }
  return out;
}

function tryExec(file, args) {
  try {
    return execFileSync(file, args, { cwd: ROOT, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
  } catch (e) {
    return null;
  }
}

function readText(abs) {
  return fs.readFileSync(abs, 'utf8');
}

function hexOf(text) {
  return Buffer.from(text, 'utf8').toString('hex');
}

function hexPretty(hex) {
  if (!hex) return '(0 bytes)';
  const pairs = hex.match(/../g) || [];
  return pairs.map(function (p) { return '<' + p + '>'; }).join('');
}

// --------------------------------------------------- driver build / run

function driverExe() {
  return path.join(BUILD_DIR, 'input-bytes-driver' + (process.platform === 'win32' ? '.exe' : ''));
}

function rlibPath() {
  return path.join(ROOT, 'target', 'release', 'libtermai_core.rlib');
}

// The encoder "scene": every source file of the crate under test, hashed together. kernel/06 §3.6
// D0 asks a scene to be frozen before it is compared; the frozen scene here is
// (encoder sources) x (corpus), and both hashes go into the report.
export function encoderSourceFiles() {
  const files = [];
  const walk = function (dir) {
    for (const name of fs.readdirSync(dir).sort()) {
      const abs = path.join(dir, name);
      const st = fs.statSync(abs);
      if (st.isDirectory()) walk(abs);
      else if (/\.rs$/.test(name)) files.push(abs);
    }
  };
  walk(path.join(ROOT, 'crates', 'termai-core', 'src'));
  files.push(path.join(ROOT, 'crates', 'termai-core', 'Cargo.toml'));
  return files;
}

export function encoderSceneHash() {
  const files = encoderSourceFiles();
  const parts = files.map(function (abs) {
    return path.relative(ROOT, abs).split(path.sep).join('/') + ':' + B.sha256Hex(readText(abs));
  });
  return { hash: B.sha256Hex(parts.join('\n')), files: files.map(function (abs) { return path.relative(ROOT, abs).split(path.sep).join('/'); }) };
}

function buildDriver(opts) {
  fs.mkdirSync(BUILD_DIR, { recursive: true });
  if (opts.build) {
    execFileSync('cargo', ['build', '--release', '-p', 'termai-core'], { cwd: ROOT, stdio: 'inherit' });
  }
  const rlib = rlibPath();
  if (!fs.existsSync(rlib)) {
    throw new Error('termai-core rlib not found at ' + path.relative(ROOT, rlib) + '; run without --no-build (this producer never substitutes a JS encoder)');
  }
  const exe = driverExe();
  const stale = !fs.existsSync(exe)
    || fs.statSync(exe).mtimeMs < Math.max(fs.statSync(DRIVER_SRC).mtimeMs, fs.statSync(rlib).mtimeMs);
  if (opts.build || stale) {
    if (!opts.build) {
      // --no-build still has to compile the driver once; it just does not re-run cargo.
      if (!fs.existsSync(rlib)) throw new Error('termai-core rlib missing; drop --no-build');
    }
    execFileSync('rustc', [
      '--edition', '2021',
      '-C', 'opt-level=2',
      '-C', 'debuginfo=0',
      '--extern', 'termai_core=' + rlib,
      DRIVER_SRC,
      '-o', exe,
    ], { cwd: ROOT, stdio: 'inherit' });
  }
  if (!fs.existsSync(exe)) throw new Error('the driver was not produced at ' + path.relative(ROOT, exe));
  return { exe: exe, rlib: rlib, built: true };
}

function driverCasesFile(corpus) {
  return corpus.cases.map(function (c) {
    const opts = Array.isArray(c.opts) && c.opts.length ? c.opts.join(',') : '-';
    return [c.id, c.mode, opts, c.event].join('\t');
  }).join('\n') + '\n';
}

// One driver invocation. The driver writes its results to a FILE (never a pipe), so the byte-level
// evidence survives even when stdio is constrained.
function runDriver(exe, casesPath, runIndex) {
  const outPath = path.join(BUILD_DIR, 'driver-out-' + process.pid + '-' + runIndex + '.tsv');
  execFileSync(exe, [casesPath, outPath], { cwd: ROOT, stdio: ['ignore', 'ignore', 'inherit'] });
  const text = readText(outPath);
  fs.rmSync(outPath, { force: true });
  const rows = new Map();
  for (const line of text.split('\n')) {
    if (!line) continue;
    const f = line.split('\t');
    if (f.length !== 3) throw new Error('driver emitted a malformed row: ' + JSON.stringify(line));
    rows.set(f[0], { bytes: f[1], outcome: f[2] });
  }
  return rows;
}

// --------------------------------------------------- corpus integrity

export function validateCorpus(corpus) {
  const errors = [];
  if (!corpus || typeof corpus !== 'object') { errors.push('corpus is not a JSON object'); return errors; }
  if (!Array.isArray(corpus.cases) || corpus.cases.length === 0) errors.push('corpus has no cases array');
  const seen = {};
  for (const c of corpus.cases || []) {
    const where = c && c.id ? c.id : '(case without id)';
    if (!c.id) errors.push('case without id');
    if (seen[c.id]) errors.push(where + ': duplicate case id');
    seen[c.id] = true;
    for (const field of ['source', 'mode', 'event', 'expectOutcome', 'authority', 'authorityKind']) {
      if (typeof c[field] !== 'string' || c[field] === '') errors.push(where + ': missing/empty field ' + field);
    }
    // expectText / expectBytes may legitimately be the empty string: "0 bytes must reach the PTY"
    // is the expectation for a dropped, consumed or needs-confirm event, and it is asserted.
    for (const field of ['expectText', 'expectBytes']) {
      if (typeof c[field] !== 'string') errors.push(where + ': missing field ' + field + ' (use "" for zero bytes)');
    }
    if (!Array.isArray(c.opts)) errors.push(where + ': opts must be an array (use [] for StandardEncoder::new())');
    if (typeof c.expectText === 'string' && typeof c.expectBytes === 'string' && hexOf(c.expectText) !== c.expectBytes) {
      errors.push(where + ': expectBytes ' + JSON.stringify(c.expectBytes) + ' is not hex(utf8(expectText)) ' + JSON.stringify(hexOf(c.expectText)) + ' -- the corpus pair disagrees');
    }
    if (typeof c.expectBytes === 'string' && !/^[0-9a-f]*$/.test(c.expectBytes)) errors.push(where + ': expectBytes is not lowercase hex');
    if (typeof c.event === 'string' && !/^(key|commit|paste|mouse|focus|inject);/.test(c.event)) errors.push(where + ': unknown event kind in ' + JSON.stringify(c.event));
  }
  if (Array.isArray(corpus.cases)) {
    const sources = Array.from(new Set(corpus.cases.map(function (c) { return c.source; })));
    for (const s of sources) if (DECLARED_SOURCES.indexOf(s) < 0) errors.push('case source ' + JSON.stringify(s) + ' is not one of the declared byte sources ' + DECLARED_SOURCES.join(', '));
    const absent = DECLARED_SOURCES.filter(function (s) { return sources.indexOf(s) < 0; });
    if (absent.length && !Array.isArray(corpus.absentSources)) errors.push('the corpus does not exercise ' + absent.join(', ') + ' and does not declare it in absentSources');
  }
  if (!Array.isArray(corpus.omitted) || corpus.omitted.length === 0) errors.push('corpus.omitted must list the expectations that were left out for lack of an authority');
  return errors;
}

// --------------------------------------------------- comparison

function compare(corpus, rows, injectedId) {
  const results = corpus.cases.map(function (c) {
    const actual = rows.get(c.id);
    if (!actual) throw new Error('the driver returned no row for case ' + c.id);
    let expectBytes = c.expectBytes;
    let expectOutcome = c.expectOutcome;
    let injected = false;
    if (injectedId && c.id === injectedId) {
      // Injected fault (bench --selftest rule 10 control): the EXPECTATION is corrupted, never the
      // encoder output, so the mismatch cannot be confused with an encoder defect.
      expectBytes = expectBytes === 'ff' ? 'fe' : 'ff';
      injected = true;
    }
    const bytesOk = actual.bytes === expectBytes;
    const outcomeOk = actual.outcome === expectOutcome;
    return {
      id: c.id,
      source: c.source,
      mode: c.mode,
      opts: c.opts,
      event: c.event,
      expectBytes: expectBytes,
      expectOutcome: expectOutcome,
      expectText: c.expectText,
      actualBytes: actual.bytes,
      actualOutcome: actual.outcome,
      bytesOk: bytesOk,
      outcomeOk: outcomeOk,
      pass: bytesOk && outcomeOk,
      injected: injected,
      authority: c.authority,
      authorityKind: c.authorityKind,
      note: c.note || '',
    };
  });
  const matched = results.filter(function (r) { return r.pass; }).length;
  const total = results.length;
  const value = total ? Number(((matched / total) * 100).toFixed(4)) : 0;
  const bySource = {};
  for (const r of results) {
    const s = bySource[r.source] || (bySource[r.source] = { source: r.source, cases: 0, matched: 0, mismatched: [] });
    s.cases += 1;
    if (r.pass) s.matched += 1;
    else s.mismatched.push(r.id);
  }
  const byMode = {};
  for (const r of results) {
    const key = r.source + '/' + r.mode;
    const m = byMode[key] || (byMode[key] = { key: key, cases: 0, matched: 0 });
    m.cases += 1;
    if (r.pass) m.matched += 1;
  }
  return {
    results: results,
    matched: matched,
    total: total,
    value: value,
    verdict: matched === total ? 'PASS' : 'FAIL',
    bySource: Object.keys(bySource).map(function (k) { return bySource[k]; }),
    byMode: Object.keys(byMode).map(function (k) { return byMode[k]; }),
    mismatches: results.filter(function (r) { return !r.pass; }),
    injected: results.filter(function (r) { return r.injected; }),
  };
}

// --------------------------------------------------- report

function commitInfo() {
  const sha = tryExec('git', ['rev-parse', 'HEAD']);
  const status = tryExec('git', ['status', '--porcelain']);
  return { sha: sha || 'unknown', dirty: status === null ? null : status.length > 0 };
}

function toolchainInfo() {
  const rustc = tryExec('rustc', ['--version']);
  let channel = 'unknown';
  try {
    const text = readText(path.join(ROOT, 'rust-toolchain.toml'));
    const m = /channel\s*=\s*"([^"]+)"/.exec(text);
    if (m) channel = m[1];
  } catch (e) { /* documentation, not a control */ }
  return {
    rustc: rustc || 'unknown',
    channel: rustc ? (channel + ' (pinned; rustc reports ' + rustc.replace(/^rustc\s*/, '') + ')') : channel,
  };
}

function methodText(info) {
  const sources = info.cmp.bySource.map(function (s) { return s.source + '(' + s.matched + '/' + s.cases + ')'; }).join(', ');
  const modes = info.cmp.byMode.map(function (m) { return m.key + '(' + m.matched + '/' + m.cases + ')'; }).join(', ');
  return [
    'carrier = the real S3 encoder termai_core::input::StandardEncoder (AR-29 item 3 / kernel/05 K-01), driven by ' + path.relative(ROOT, DRIVER_SRC).split(path.sep).join('/') + ' compiled with rustc against the workspace rlib ' + path.relative(ROOT, info.rlib).split(path.sep).join('/') + ' produced by "cargo build --release -p termai-core"; the driver holds no encoding logic',
    'corpus = kernel/05 §5 IN-AC byte assertions; every case cites kernel/05 clause id or xterm ctlseqs rule (corpus authority). case count = ' + info.cmp.total + ', corpus sha256 = ' + info.corpusHash,
    'judgement = per case: emitted bytes must equal the cited expectation byte-for-byte AND the EncodeOutcome must be the declared one; H12 = matched / exercised * 100 with gateOp "=" 100 pct (registry row ' + METRIC_ID + ', family exact, statistic exact)',
    'scene freeze (kernel/06 §3.6 D0) = encoder sources sha256 ' + info.scene.hash + ' over ' + info.scene.files.length + ' file(s) x corpus sha256 ' + info.corpusHash + '; two driver runs on the same inputs were compared byte-for-byte: ' + (info.deterministic ? 'identical' : 'NOT identical'),
    'coverage (exercised) = ' + sources + '; keyboard modes = ' + modes,
    'coverage (absent, NOT claimed by this number) = ' + [].concat(ABSENT_SOURCES).concat(info.absent).map(function (a) { return a.source; }).join('; '),
    'omitted expectations (no citable authority, therefore not in the corpus) = ' + info.omitted.map(function (o) { return o.what; }).join('; '),
    'machine binding = registry ' + METRIC_ID + ' is machine "none" (byte equality is machine independent), so ADR-0029 D-5 keeps this row out of gatingNumbersProduced (machine-bound RM-A / RM-C numbers only); the row is still a HARNESS §5 release gate, hence gating=true (same shape as H13 governed yes / machine none)',
    'limitation = the corpus drives S3 (plus the paste friction decision S5 consumes). The platform half of the input stack (S0-S2: FocusRouter, KeyTranslator, dead-key composition, native IME host) does not exist yet and is NOT covered; see the per-source breakdown. IME preedit correctness (IN-AC-04) is explicitly outside HARNESS §5 (AR-31 item 4)',
  ].join(' | ');
}

function buildReport(info) {
  const row = R.findMapping(METRIC_ID);
  if (!row) throw new Error('registry has no row ' + METRIC_ID);
  if (row.unit !== UNIT) throw new Error('registry row ' + METRIC_ID + ' unit is ' + row.unit + ', this producer writes ' + UNIT);
  const now = new Date().toISOString();
  const minutes = (Date.now() - info.startedAt) / 60000;
  const corpusBytes = Buffer.byteLength(info.corpusText, 'utf8');
  const metric = {
    metric: METRIC_ID,
    value: info.cmp.value,
    unit: row.unit,
    samples: info.cmp.total,
    runner: 'no-reference-machine-required',
    commit: info.commit.sha,
    toolchain: info.toolchain.rustc,
    ts: now,
    statistic: row.statistic,
    runs: 1,
    runStat: 'exact',
    madOverMedian: 0,
    gate: row.gate,
    target: null,
    gating: true,
    verdict: info.cmp.verdict,
    method: methodText(info),
    dCalibrationMs: null,
    artifacts: [path.relative(ROOT, info.out).split(path.sep).join('/'), path.relative(ROOT, casesArtifactPath(info.out)).split(path.sep).join('/')],
  };
  const report = {
    schemaVersion: '1.0.0',
    reportId: 'input-bytes-' + info.commit.sha.slice(0, 8) + '-' + info.startedAt + (info.injectedId ? '-INJECTED-FAULT' : ''),
    generatedAt: now,
    commit: { sha: info.commit.sha, dirty: info.commit.dirty === true },
    toolchain: { rustc: info.toolchain.rustc, channel: info.toolchain.channel },
    runner: { class: 'self-hosted', machineId: 'no-reference-machine-required', role: 'none', backend: 'not-applicable (byte equality involves no T0 backend)' },
    // kernel/06 §3.7 requires 64 hex chars. This host has no registered machine fingerprint and
    // H12 does not need one, so the honest value is the all-zero sentinel plus an explicit
    // environment.valid = false.
    fingerprintSha256: '0'.repeat(64),
    environment: {
      powerPlan: 'not controlled (byte equality is machine independent)',
      exclusive: false,
      warmed: 0,
      valid: false,
    },
    selfcheck: {
      scene: { verdict: info.deterministic ? 'PASS' : 'INVALID', frames: info.cmp.total, hash: B.sha256Hex(info.scene.hash + ':' + info.corpusHash) },
      doubleRun: { verdict: info.deterministic ? 'PASS' : 'INVALID', deltaPct: info.deterministic ? 0 : 1 },
      verdict: info.deterministic ? 'PASS' : 'INVALID',
    },
    corpus: { manifestSha256: info.corpusHash, shardsOk: info.cmp.bySource.length, bytes: corpusBytes },
    metrics: [metric],
    verdict: info.cmp.verdict,
    cost: { minutes: Number(minutes.toFixed(3)), runnerClass: 'self-hosted', estUsd: Number((minutes / 60 * 8 * 0.04).toFixed(4)) },
    flatProjection: {
      metric: metric.metric,
      value: metric.value,
      unit: metric.unit,
      samples: metric.samples,
      runner: metric.runner,
      commit: metric.commit,
      toolchain: metric.toolchain,
      ts: metric.ts,
    },
  };
  return report;
}

function casesArtifactPath(out) {
  return out.replace(/\.json$/, '') + '.cases.json';
}

// --------------------------------------------------- printing

function printCoverage(info) {
  console.log('  source coverage (which InputEvent sources this corpus exercises):');
  const width = Math.max.apply(null, info.cmp.bySource.map(function (s) { return s.source.length; }));
  for (const s of info.cmp.bySource) {
    const verdict = s.matched === s.cases ? 'all matched' : s.cases - s.matched + ' MISMATCH (' + s.mismatched.join(', ') + ')';
    console.log('    ' + s.source.padEnd(width) + '  ' + String(s.cases).padStart(3) + ' case(s)  ' + String(s.matched).padStart(3) + ' matched  ' + verdict);
  }
  for (const s of DECLARED_SOURCES) {
    if (!info.cmp.bySource.some(function (x) { return x.source === s; })) console.log('    ' + s.padEnd(width) + '    0 case(s)  ABSENT -- not exercised by this corpus');
  }
  console.log('    keyboard modes: ' + info.cmp.byMode.map(function (m) { return m.key + ' ' + m.matched + '/' + m.cases; }).join(', '));
  console.log('  absent sources (NOT claimed by H12; the row covers only what ran):');
  for (const a of [].concat(ABSENT_SOURCES).concat(info.absent)) console.log('    - ' + a.source + ': ' + a.detail);
  console.log('  omitted expectations (' + info.omitted.length + ', no citable authority -> never invented):');
  for (const o of info.omitted) console.log('    - ' + o.what + ' -- ' + o.why);
}

function printRepresentative(info) {
  console.log('  byte-level comparison, representative cases (input -> expected -> actual):');
  const pick = ['L03', 'L06', 'M02', 'P03', 'S01', 'I02'].map(function (id) {
    return info.cmp.results.filter(function (r) { return r.id === id; })[0];
  }).filter(Boolean);
  for (const r of pick) {
    console.log('    ' + r.id + '  ' + r.source + '/' + r.mode + '  event=' + r.event);
    console.log('       expect ' + (r.expectBytes || '(0 bytes)') + '  ' + hexPretty(r.expectBytes) + '  [' + r.expectOutcome + ']');
    console.log('       actual ' + (r.actualBytes || '(0 bytes)') + '  ' + hexPretty(r.actualBytes) + '  [' + r.actualOutcome + ']  -> ' + (r.pass ? 'MATCH' : 'MISMATCH'));
    console.log('       authority: ' + r.authority);
  }
  if (info.cmp.mismatches.length) {
    console.log('  mismatches (byte-for-byte, each one is a contract breach of the cited clause):');
    for (const r of info.cmp.mismatches) {
      console.log('    ' + r.id + '  ' + r.source + '/' + r.mode + '  event=' + r.event + (r.injected ? '  [INJECTED FAULT: the expectation was corrupted on purpose]' : ''));
      console.log('       expect ' + (r.expectBytes || '(0 bytes)') + '  ' + hexPretty(r.expectBytes) + '  [' + r.expectOutcome + ']');
      console.log('       actual ' + (r.actualBytes || '(0 bytes)') + '  ' + hexPretty(r.actualBytes) + '  [' + r.actualOutcome + ']');
      console.log('       authority: ' + r.authority);
      if (r.note) console.log('       note: ' + r.note);
    }
  }
}

// --------------------------------------------------- main

function main(argv) {
  const opts = parseArgs(argv);
  if (opts.help) {
    console.log('usage: node tools/bench/input-bytes.mjs [--corpus <path>] [--out <path>] [--json] [--no-build] [--inject-mismatch <caseId>]');
    console.log('  produces HARNESS §5 H12 (input byte equality) by driving termai-core\'s InputEncoder over tools/bench/input-corpus.json');
    console.log('  exit: 0 = H12 100%, 1 = a case mismatched (report still written), 2 = the producer could not run');
    return 0;
  }
  const startedAt = Date.now();
  const commit = commitInfo();
  const toolchain = toolchainInfo();
  const corpusText = readText(opts.corpus);
  const corpus = JSON.parse(corpusText);
  const corpusHash = B.sha256Hex(corpusText);

  console.log('input byte equality producer (HARNESS §5 ' + METRIC_ID + ' = 100%; corpus authority kernel/05 §5 IN-AC, encoder AR-29 item 3 / K-01)');
  console.log('  corpus : ' + path.relative(ROOT, opts.corpus).split(path.sep).join('/') + '  sha256=' + corpusHash.slice(0, 16) + '…  cases=' + (corpus.cases ? corpus.cases.length : 0));

  const corpusErrors = validateCorpus(corpus);
  if (corpusErrors.length) {
    for (const e of corpusErrors.slice(0, 10)) console.error('  corpus: ' + e);
    throw new Error('the corpus is not usable (' + corpusErrors.length + ' problem(s)); a broken corpus must not become a measurement');
  }

  const built = buildDriver(opts);
  const scene = encoderSceneHash();
  console.log('  driver : ' + path.relative(ROOT, DRIVER_SRC).split(path.sep).join('/') + ' + ' + path.relative(ROOT, built.rlib).split(path.sep).join('/') + ' -> ' + path.relative(ROOT, built.exe).split(path.sep).join('/'));
  console.log('  encoder scene sha256=' + scene.hash.slice(0, 16) + '… over ' + scene.files.length + ' file(s) of crates/termai-core');

  if (opts.inject) {
    const hit = (corpus.cases || []).some(function (c) { return c.id === opts.inject; });
    if (!hit) throw new Error('--inject-mismatch ' + opts.inject + ': no such case id in the corpus');
    console.log('  *** INJECTED FAULT ACTIVE: the expectation of case ' + opts.inject + ' is corrupted on purpose (bench --selftest rule 10 control) ***');
  }

  const casesPath = path.join(BUILD_DIR, 'cases-' + process.pid + '.tsv');
  fs.mkdirSync(BUILD_DIR, { recursive: true });
  fs.writeFileSync(casesPath, driverCasesFile(corpus));

  const rowsA = runDriver(built.exe, casesPath, 1);
  const rowsB = runDriver(built.exe, casesPath, 2);
  let deterministic = rowsA.size === rowsB.size;
  for (const [id, a] of rowsA) {
    const b = rowsB.get(id);
    if (!b || b.bytes !== a.bytes || b.outcome !== a.outcome) deterministic = false;
  }
  console.log('  D0 self-check: ' + (deterministic ? 'two driver runs over the same corpus produced identical bytes for all ' + rowsA.size + ' case(s)' : 'THE TWO RUNS DISAGREED -- kernel/06 §3.6 D0 makes this INVALID'));
  if (!deterministic) throw new Error('the encoder is not deterministic across two runs; a non-reproducible comparison must not be published (kernel/06 §3.6 D0)');

  const cmp = compare(corpus, rowsA, opts.inject);
  const info = {
    startedAt: startedAt,
    commit: commit,
    toolchain: toolchain,
    corpusText: corpusText,
    corpusHash: corpusHash,
    scene: scene,
    rlib: built.rlib,
    out: opts.out,
    cmp: cmp,
    deterministic: deterministic,
    omitted: corpus.omitted || [],
    absent: corpus.absentSources || [],
    injectedId: opts.inject || null,
  };

  printCoverage(info);
  printRepresentative(info);

  const row = R.findMapping(METRIC_ID);
  console.log('  metric ' + METRIC_ID + ' = ' + cmp.value + ' ' + UNIT + '  (matched ' + cmp.matched + '/' + cmp.total + '; registry gate ' + row.gateOp + ' ' + row.gate + ' ' + row.unit + '; statistic ' + row.statistic + '; family ' + row.family + ')');
  console.log('  verdict: ' + cmp.verdict + (cmp.verdict === 'PASS' ? ' -- every exercised case matched byte-for-byte' : ' -- ' + cmp.mismatches.length + ' exercised case(s) mismatched, so H12 is NOT 100% and must not be reported as passing'));
  console.log('  gating: registry ' + METRIC_ID + ' is machine "' + row.machine + '" (governed "' + row.governed + '"), so ADR-0029 D-5 keeps it OUT of `gating numbers produced by this run` (that counter is machine-bound RM-A / RM-C numbers only); the §5 gate itself is still judged (gating=true in the report, same shape as H13).');

  const report = buildReport(info);
  const schema = B.validateBenchReport(report);
  if (!schema.ok) {
    for (const e of schema.errors.slice(0, 8)) console.error('  schema: ' + e.code + ' ' + e.path + ': ' + e.message);
    throw new Error('the produced report does not satisfy the kernel/06 §3.7 schema');
  }
  const binding = V.collectValues([{ rel: path.relative(ROOT, opts.out), json: report, parseError: null }]);
  if (binding.problems.length) {
    for (const p of binding.problems) console.error('  binding: ' + p);
    throw new Error('the produced report does not bind to the ' + METRIC_ID + ' registry row (unit / gate transcription)');
  }
  if (binding.gatingNumbersProduced !== 0) {
    throw new Error('this producer must not produce a machine-bound gating number on a host with no reference machine (got ' + binding.gatingNumbersProduced + ')');
  }

  fs.mkdirSync(path.dirname(opts.out), { recursive: true });
  fs.writeFileSync(opts.out, JSON.stringify(report, null, 2) + '\n');
  fs.writeFileSync(casesArtifactPath(opts.out), JSON.stringify({
    reportId: report.reportId,
    generatedAt: report.generatedAt,
    corpus: path.relative(ROOT, opts.corpus).split(path.sep).join('/'),
    corpusSha256: corpusHash,
    encoderSceneSha256: scene.hash,
    encoderFiles: scene.files,
    injectedFault: opts.inject || null,
    metric: { id: METRIC_ID, value: cmp.value, unit: UNIT, gate: row.gate, verdict: cmp.verdict, matched: cmp.matched, total: cmp.total },
    coverage: { exercised: cmp.bySource, absent: [].concat(ABSENT_SOURCES).concat(info.absent) },
    omitted: info.omitted,
    cases: cmp.results,
  }, null, 2) + '\n');
  fs.rmSync(casesPath, { force: true });

  console.log('  report : ' + path.relative(ROOT, opts.out).split(path.sep).join('/') + '   per-case detail: ' + path.relative(ROOT, casesArtifactPath(opts.out)).split(path.sep).join('/'));
  console.log('  gate read: node tools/bench/check.mjs --report ' + path.relative(ROOT, opts.out).split(path.sep).join('/'));
  if (opts.json) console.log(JSON.stringify(report, null, 2));
  return cmp.verdict === 'PASS' ? 0 : 1;
}

const invokedDirectly = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) {
  try {
    process.exit(main(process.argv.slice(2)));
  } catch (e) {
    console.error('input-bytes: ' + (e && e.message ? e.message : String(e)));
    process.exit(2);
  }
}
