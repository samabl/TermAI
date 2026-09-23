#!/usr/bin/env node
// tools/conformance/run.mjs  (W1-B: G1 VT consistency runner)
//
//   node tools/conformance/run.mjs [--root <dir>] [--out <file>] [--no-artifacts]
//                                  [--determinism-check] [--quiet]
//
// Feeds byte-level cases to the termai-vt headless entry point (L0 parser lane) and
// writes a deterministic machine-readable report (kernel/01 section 3.9).
//
// Lane policy (AR-25 item 1 / kernel/01 K-02, B-2): PASS/FAIL for the VT compatibility
// gate is produced ONLY on the L0 parser lane. L1/L2 cases are reported as
// REGISTERED / NOT_APPLICABLE and never as PASS/FAIL. The policy itself is asked of the
// Rust implementation (termai_vt::lane_verdict) through the harness, so the runner
// cannot drift from it.
//
// Honesty (AR-20): this runner measures what it runs. The report always says G1 is NOT
// JUDGED, because vttest/esctest are not integrated and the xterm case set is far below
// the AR-31 item 1 floor.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import {
  ensureHarness,
  harnessRaw,
  parseHarnessOutput,
  laneVerdicts,
  hexToText,
} from './lib/harness.mjs';
import { canonicalJson, sha256Hex, ratio } from './lib/report.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(HERE, '..', '..');
const ID_RE = /^[a-z0-9][a-z0-9._-]*$/;
const LANES = ['L0', 'L1', 'L2'];
const ORACLES = ['ecma48', 'xterm-ctlseqs', 'kernel-01-spec', 'termai-corpus', 'invariant'];
const ARTIFACT_ROOT = 'target/conformance';
const TAB = String.fromCharCode(9);

function parseArgs(argv) {
  const out = { root: null, out: null, artifacts: true, determinism: false, quiet: false };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--no-artifacts') out.artifacts = false;
    else if (arg === '--determinism-check') out.determinism = true;
    else if (arg === '--quiet') out.quiet = true;
    else if (arg === '--root') { out.root = argv[i + 1]; i += 1; }
    else if (arg.indexOf('--root=') === 0) out.root = arg.slice(7);
    else if (arg === '--out') { out.out = argv[i + 1]; i += 1; }
    else if (arg.indexOf('--out=') === 0) out.out = arg.slice(6);
    else if (arg === '--help' || arg === '-h') out.help = true;
    else throw new Error('unknown argument: ' + arg);
  }
  return out;
}

function gitRevision(root) {
  const head = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8', windowsHide: true });
  if (head.status !== 0) return 'NO_VCS';
  const status = spawnSync('git', ['status', '--porcelain'], { cwd: root, encoding: 'utf8', windowsHide: true });
  const dirty = String(status.stdout || '').trim().length > 0;
  return String(head.stdout || '').trim() + (dirty ? '+dirty' : '');
}

function readJsonl(file) {
  const out = [];
  const lines = fs.readFileSync(file, 'utf8').split(/\r?\n/);
  for (let i = 0; i < lines.length; i += 1) {
    const raw = lines[i].trim();
    if (!raw || raw.charAt(0) === '#') continue;
    try {
      out.push({ line: i + 1, value: JSON.parse(raw) });
    } catch (err) {
      throw new Error(file + ':' + (i + 1) + ' invalid JSON: ' + err.message);
    }
  }
  return out;
}

function loadCases(root) {
  const base = path.join(root, 'tools', 'conformance', 'cases');
  const problems = [];
  const cases = [];
  const seen = new Set();
  const suiteNames = fs
    .readdirSync(base, { withFileTypes: true })
    .filter(function (d) { return d.isDirectory(); })
    .map(function (d) { return d.name; })
    .sort();
  for (const suite of suiteNames) {
    const indexFile = path.join(base, suite, 'index.jsonl');
    if (!fs.existsSync(indexFile)) {
      problems.push('suite "' + suite + '" has no index.jsonl');
      continue;
    }
    for (const row of readJsonl(indexFile)) {
      const meta = row.value;
      const where = 'cases/' + suite + '/index.jsonl:' + row.line;
      if (typeof meta.id !== 'string' || !ID_RE.test(meta.id)) {
        problems.push(where + ': bad or missing id ' + JSON.stringify(meta.id));
        continue;
      }
      if (seen.has(meta.id)) {
        problems.push(where + ': duplicate case id ' + meta.id);
        continue;
      }
      seen.add(meta.id);
      const lane = meta.lane === undefined ? 'L0' : meta.lane;
      if (LANES.indexOf(lane) < 0) {
        problems.push(where + ': bad lane ' + JSON.stringify(meta.lane));
        continue;
      }
      if (ORACLES.indexOf(meta.oracle) < 0) {
        problems.push(where + ': bad oracle ' + JSON.stringify(meta.oracle));
        continue;
      }
      const gating = meta.gating === undefined ? true : meta.gating === true;
      if (meta.oracle === 'invariant' && gating) {
        problems.push(where + ': oracle "invariant" cases must set gating:false (fail-closed)');
        continue;
      }
      if (gating && lane !== 'L0') {
        problems.push(where + ': only L0 cases may be gating (AR-25 item 1)');
        continue;
      }
      const rel = typeof meta.path === 'string'
        ? meta.path
        : 'tools/conformance/cases/' + suite + '/' + meta.id + '.trec';
      const abs = path.resolve(root, rel);
      if (!abs.startsWith(root + path.sep)) {
        problems.push(where + ': case body escapes the repository root');
        continue;
      }
      if (!fs.existsSync(abs)) {
        problems.push(where + ': case body missing: ' + rel);
        continue;
      }
      cases.push({
        id: meta.id,
        suite: suite,
        lane: lane,
        oracle: meta.oracle,
        gating: gating,
        cols: meta.cols === undefined ? 80 : meta.cols,
        rows: meta.rows === undefined ? 24 : meta.rows,
        requires: Array.isArray(meta.requires) ? meta.requires : [],
        ctlseqs_entry: meta.ctlseqs_entry || '',
        ctlseqs_mnemonic: meta.ctlseqs_mnemonic || '',
        documented: meta.documented || '',
        real_corpus: meta.real_corpus === true,
        expect_responses: Array.isArray(meta.expect_responses) ? meta.expect_responses : null,
        rel: rel.split(path.sep).join('/'),
        abs: abs,
      });
    }
  }
  cases.sort(function (a, b) {
    if (a.suite === b.suite) return a.id < b.id ? -1 : 1;
    return a.suite < b.suite ? -1 : 1;
  });
  return { cases: cases, problems: problems };
}

function evaluate(entry, res) {
  const structural = {
    parity_ok: res.parity === 'ok',
    all_consumed: res.all_consumed === true && res.consumed_bytes === res.input_bytes,
    dims_ok: res.dims[0] === entry.cols && res.dims[1] === entry.rows,
    cursor_in_bounds: res.cursor[0] >= 0 && res.cursor[0] < entry.rows
      && res.cursor[1] >= 0 && res.cursor[1] < entry.cols,
    no_control_byte_in_grid: res.grid_control_bytes === 0,
    no_harness_error: res.verdict !== 'ERROR',
  };
  if (entry.expect_responses) {
    const want = entry.expect_responses.map(function (h) { return h.toLowerCase(); });
    const got = (res.responses || []).map(function (h) { return h.toLowerCase(); });
    structural.responses_as_expected = JSON.stringify(want) === JSON.stringify(got);
  }
  let structuralOk = true;
  for (const key of Object.keys(structural)) {
    if (structural[key] !== true) structuralOk = false;
  }
  const assertionsOk = res.verdict === 'PASS';
  const ok = structuralOk && assertionsOk;

  // Lane closure (AR-25 item 1): gate verdicts exist on L0 only.
  let render;
  if (entry.lane !== 'L0') render = ok ? 'NOT_APPLICABLE' : 'REGISTERED';
  else render = ok ? 'PASS' : 'FAIL';

  return {
    structural: structural,
    structural_ok: structuralOk,
    assertions_ok: assertionsOk,
    render_verdict: render,
    first_failure: (res.failures && res.failures.length)
      ? res.failures[0]
      : (structuralOk ? '' : 'structural invariant failed'),
  };
}

function caseReport(entry, res, verdict) {
  return {
    id: entry.id,
    suite: entry.suite,
    lane: entry.lane,
    oracle: entry.oracle,
    gating: entry.gating,
    requires: entry.requires,
    documented: entry.documented,
    ctlseqs_entry: entry.ctlseqs_entry,
    ctlseqs_mnemonic: entry.ctlseqs_mnemonic,
    real_corpus: entry.real_corpus,
    body: entry.rel,
    executed: true,
    verdict: verdict.render_verdict,
    assertions_verdict: res.verdict,
    structural: verdict.structural,
    first_failure: verdict.first_failure,
    steps_passed: res.steps_passed,
    steps_failed: res.steps_failed,
    input_bytes: res.input_bytes,
    consumed_bytes: res.consumed_bytes,
    cursor: res.cursor,
    grid_hash: res.grid_hash,
    grid_control_bytes: res.grid_control_bytes,
    responses: (res.responses || []).map(function (h) { return h.toLowerCase(); }),
    counters: res.counters,
  };
}

function writeArtifacts(root, entry, res, verdict, env) {
  const dir = path.join(root, ARTIFACT_ROOT, entry.suite, entry.id);
  fs.mkdirSync(dir, { recursive: true });
  const text = fs.readFileSync(entry.abs, 'utf8');
  fs.writeFileSync(path.join(dir, 'input.trec'), text);
  fs.writeFileSync(path.join(dir, 'stream.bin'), Buffer.from(res.stream || '', 'hex'));
  const golden = (res.golden || []).map(hexToText).join('\n');
  fs.writeFileSync(path.join(dir, 'actual.grid'), golden + (golden ? '\n' : ''));
  const expected = text.split(/\r?\n/).filter(function (l) { return /^\s*ASSERT\b/.test(l); });
  fs.writeFileSync(path.join(dir, 'expected.txt'), expected.join('\n') + (expected.length ? '\n' : ''));
  const diff = [
    'case: ' + entry.id,
    'suite: ' + entry.suite,
    'body: ' + entry.rel,
    'structural: ' + JSON.stringify(verdict.structural),
    'assertions_verdict: ' + res.verdict,
    'first divergence: ' + (verdict.first_failure || '(none)'),
    'input bytes: ' + res.input_bytes + ', consumed: ' + res.consumed_bytes,
    'grid control bytes: ' + res.grid_control_bytes,
    '',
    'note: byte offsets are unavailable for the pinned vte backend',
    '      (BackendCaps.byte_offsets = false); kernel/01 section 3.8 asks for the input',
    '      byte offset of the first divergence and that gap is registered (SD-08.3).',
    'note: expected.grid is absent by design: L0 cases assert rows/counters rather than',
    '      a whole grid, so expectations are listed in expected.txt instead.',
    '',
    'assertion failures:',
  ].concat(res.failures && res.failures.length
    ? res.failures.map(function (f) { return '  - ' + f; })
    : ['  (none)']);
  fs.writeFileSync(path.join(dir, 'diff.txt'), diff.join('\n') + '\n');
  fs.writeFileSync(path.join(dir, 'env.json'), canonicalJson(env));
  fs.writeFileSync(path.join(dir, 'report.json'), canonicalJson(caseReport(entry, res, verdict)));
  const ps = [
    '$ErrorActionPreference = "Stop"',
    'Set-Location ' + JSON.stringify(root),
    'cargo build --quiet -p termai-vt --bin termai-vt-conformance',
    JSON.stringify(entry.id + TAB + entry.abs + TAB + entry.cols + TAB + entry.rows)
      + ' | & target/debug/termai-vt-conformance.exe',
  ];
  fs.writeFileSync(path.join(dir, 'repro.ps1'), ps.join('\n') + '\n');
  const sh = [
    '#!/bin/sh',
    'set -e',
    'cd ' + JSON.stringify(root.split(path.sep).join('/')),
    'cargo build --quiet -p termai-vt --bin termai-vt-conformance',
    'printf "%s\\n" ' + JSON.stringify(entry.id + '\t' + entry.abs + '\t' + entry.cols + '\t' + entry.rows)
      + ' | ./target/debug/termai-vt-conformance',
  ];
  fs.writeFileSync(path.join(dir, 'repro.sh'), sh.join('\n') + '\n');
  return {
    dir: (ARTIFACT_ROOT + '/' + entry.suite + '/' + entry.id).split(path.sep).join('/'),
    files: ['actual.grid', 'diff.txt', 'env.json', 'expected.txt', 'input.trec', 'repro.ps1', 'repro.sh', 'report.json', 'stream.bin'],
  };
}

function suiteVersion(root, suite, registry) {
  const file = path.join(root, 'tools', 'conformance', 'cases', suite, 'suite.json');
  if (fs.existsSync(file)) {
    const meta = JSON.parse(fs.readFileSync(file, 'utf8'));
    if (meta.suite_version) return meta.suite_version;
  }
  if (registry) {
    return registry.source.name + ' sha256:' + registry.source.sha256.slice(0, 12)
      + ' xterm-patch#' + registry.source.xterm_patch;
  }
  return 'n/a';
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    console.log('usage: node tools/conformance/run.mjs [--root <dir>] [--out <file>] [--no-artifacts] [--determinism-check] [--quiet]');
    return 0;
  }
  const root = args.root ? path.resolve(args.root) : DEFAULT_ROOT;
  const registryPath = path.join(root, 'tools', 'conformance', 'data', 'ctlseqs-entries.json');
  const registry = fs.existsSync(registryPath) ? JSON.parse(fs.readFileSync(registryPath, 'utf8')) : null;

  const loaded = loadCases(root);
  if (loaded.problems.length) {
    console.error('conformance: case index is invalid (fail-closed):');
    for (const p of loaded.problems) console.error('  - ' + p);
    return 2;
  }
  const cases = loaded.cases;
  if (!cases.length) {
    console.error('conformance: no cases found');
    return 2;
  }

  const bin = ensureHarness(root);
  const lines = cases.map(function (c) {
    return c.id + '\t' + c.abs + '\t' + c.cols + '\t' + c.rows;
  });
  const rawA = harnessRaw(bin, root, lines);
  let determinism = 'not-checked';
  if (args.determinism) {
    const rawB = harnessRaw(bin, root, lines);
    determinism = rawA === rawB ? 'byte-identical' : 'MISMATCH';
  }
  const results = parseHarnessOutput(rawA);

  const laneProbe = laneVerdicts(bin, root);
  const laneSelftestOk = laneProbe.every(function (p) {
    if (p.lane === 'l0') return p.verdict === 'pass' || p.verdict === 'fail';
    return p.verdict !== 'pass' && p.verdict !== 'fail';
  });

  const evaluated = [];
  const laneViolations = [];
  for (const entry of cases) {
    const res = results.get(entry.id);
    if (!res) throw new Error('harness returned no block for case ' + entry.id);
    const verdict = evaluate(entry, res);
    if (entry.lane !== 'L0' && (verdict.render_verdict === 'PASS' || verdict.render_verdict === 'FAIL')) {
      laneViolations.push(entry.id);
    }
    evaluated.push({ entry: entry, res: res, verdict: verdict });
  }

  const excludedCap = evaluated.filter(function (e) { return e.entry.requires.length > 0; });
  const executed = evaluated.filter(function (e) { return e.entry.requires.length === 0; });
  const gating = executed.filter(function (e) { return e.entry.gating && e.entry.lane === 'L0'; });
  const coverageOnly = executed.filter(function (e) { return !e.entry.gating; });
  const gatingPassed = gating.filter(function (e) { return e.verdict.render_verdict === 'PASS'; });
  const structuralFailed = executed.filter(function (e) { return !e.verdict.structural_ok; });

  const suites = [];
  const suiteNames = Array.from(new Set(evaluated.map(function (e) { return e.entry.suite; }))).sort();
  for (const suite of suiteNames) {
    const mine = evaluated.filter(function (e) { return e.entry.suite === suite; });
    const mineExecuted = mine.filter(function (e) { return e.entry.requires.length === 0; });
    const mineGating = mineExecuted.filter(function (e) { return e.entry.gating && e.entry.lane === 'L0'; });
    const mineGatingPassed = mineGating.filter(function (e) { return e.verdict.render_verdict === 'PASS'; });
    const mineStructuralFailed = mineExecuted.filter(function (e) { return !e.verdict.structural_ok; });
    suites.push({
      suite: suite,
      suite_version: suiteVersion(root, suite, registry),
      oracle: Array.from(new Set(mine.map(function (e) { return e.entry.oracle; }))).sort().join('+'),
      lane: 'L0',
      backend: 'vte-0.15.0',
      gpu_tier: 'N/A (headless)',
      machine_fingerprint: process.platform + '-' + process.arch,
      cases: {
        total: mine.length,
        executed: mineExecuted.length,
        passed: mineExecuted.filter(function (e) { return e.verdict.structural_ok; }).length,
        failed: mineStructuralFailed.length,
        excluded_cap: mine.length - mineExecuted.length,
        gating_total: mineGating.length,
        gating_passed: mineGatingPassed.length,
        coverage_only: mineExecuted.length - mineGating.length,
      },
      x_registered: 0,
      r_strict: ratio(mineGatingPassed.length, mineGating.length),
      r_gate: ratio(mineGatingPassed.length, mineGating.length),
      gate: 'NON_GATING',
      gate_reason: 'not run on an RM-A/T0 reference machine (spec 07 section 3.8.3)',
      registry_digest: registry ? 'sha256:' + registry.source.sha256 : 'none',
      artifacts: [],
      commit: gitRevision(root),
    });
  }

  const failures = evaluated.filter(function (e) {
    return !e.verdict.structural_ok
      || (e.entry.gating && e.entry.lane === 'L0' && e.verdict.render_verdict !== 'PASS');
  });

  const env = {
    cols: 80,
    rows: 24,
    term: 'xterm-256color',
    backend: 'vte-0.15.0',
    platform: process.platform,
    arch: process.arch,
    node: process.version,
    gpu_tier: 'N/A',
  };

  const artifacts = [];
  if (args.artifacts) {
    for (const e of failures) artifacts.push(writeArtifacts(root, e.entry, e.res, e.verdict, env));
  }

  let coverage = null;
  if (registry) {
    const withCases = new Set();
    const withGating = new Set();
    for (const e of evaluated) {
      if (e.entry.ctlseqs_entry) withCases.add(e.entry.ctlseqs_entry);
      if (e.entry.ctlseqs_entry && e.entry.gating) withGating.add(e.entry.ctlseqs_entry);
    }
    const resolvedEntries = registry.entries.filter(function (x) { return x.resolved; });
    const mapped = resolvedEntries.filter(function (x) { return withCases.has(x.resolved_pattern); });
    const mappedGating = resolvedEntries.filter(function (x) { return withGating.has(x.resolved_pattern); });
    coverage = {
      source: registry.source,
      entries_total: registry.entries.length,
      entries_resolved: resolvedEntries.length,
      entries_unresolved: registry.entries.length - resolvedEntries.length,
      entries_with_cases: mapped.length,
      entries_with_gating_cases: mappedGating.length,
      entries_unmapped: resolvedEntries
        .filter(function (x) { return !withCases.has(x.resolved_pattern); })
        .map(function (x) { return x.resolved_pattern + ' @ctlseqs.txt:' + x.doc_line; }),
      coverage_ratio: ratio(mapped.length, resolvedEntries.length),
      gating_coverage_ratio: ratio(mappedGating.length, resolvedEntries.length),
      unresolved: registry.entries
        .filter(function (x) { return !x.resolved; })
        .map(function (x) { return x.pattern + ' @ctlseqs.txt:' + x.doc_line; }),
    };
  }

  const realCorpus = evaluated.filter(function (e) { return e.entry.real_corpus; });
  const findings = failures.filter(function (e) { return e.entry.gating; }).map(function (e) {
    return {
      case_id: e.entry.id,
      suite: e.entry.suite,
      lane: e.entry.lane,
      oracle: e.entry.oracle,
      documented: e.entry.documented,
      expected_vs_actual: e.verdict.first_failure,
      counters: e.res.counters,
      evidence_dir: args.artifacts ? (ARTIFACT_ROOT + '/' + e.entry.suite + '/' + e.entry.id) : null,
      registration: 'NOT_REGISTERED',
      note: 'kernel/01 K-04 deviation registration needs a double-signed entry with an expiry; not started',
    };
  });
  const report = {
    schema: 'termai-conformance-report/1',
    tool: 'tools/conformance/run.mjs',
    authority: 'kernel/01 section 3.9 / AR-25 / AR-31 item 1',
    g1_status: 'NOT_JUDGED',
    g1_reason: 'vttest and esctest are not integrated (see tools/conformance/upstream/README.md); '
      + 'the xterm case set is far below the AR-31 item 1 floor of >=2000 cases and >=20% real '
      + 'captures; no RM-A/T0 reference machine is available.',
    environment: {
      os: process.platform,
      arch: process.arch,
      node: process.version,
      backend: 'vte-0.15.0',
      machine_fingerprint: process.platform + '-' + process.arch,
      gpu_tier: 'N/A (headless)',
    },
    commit: gitRevision(root),
    lane_policy: {
      authority: 'AR-25 item 1 / kernel/01 K-02 / index B-2',
      gated_lane: 'L0',
      l1_l2_emit_gate_verdict: false,
      probe_source: 'termai_vt::lane_verdict via termai-vt-conformance --server LANE',
      probe: laneProbe,
      probe_ok: laneSelftestOk,
      violations: laneViolations,
    },
    determinism: { harness_stdout: determinism, runs: args.determinism ? 2 : 1 },
    suites: suites,
    totals: {
      cases_total: evaluated.length,
      cases_executed: executed.length,
      cases_excluded_cap: excludedCap.length,
      gating_total: gating.length,
      gating_passed: gatingPassed.length,
      gating_failed: gating.length - gatingPassed.length,
      coverage_only: coverageOnly.length,
      structural_failures: structuralFailed.length,
      l0_r_strict: ratio(gatingPassed.length, gating.length),
      l0_r_gate: ratio(gatingPassed.length, gating.length),
      real_corpus_cases: realCorpus.length,
      real_corpus_ratio: ratio(realCorpus.length, evaluated.length),
    },
    coverage: coverage,
    findings: findings,
    known_gaps: [
      'G1 is NOT judged: vttest and esctest are not integrated and are not runnable on this host.',
      'AR-31 item 1 floor not met: ' + evaluated.length + ' cases (< 2000) and '
        + String(realCorpus.length) + ' real-world captures (0% < 20%).',
      'Coverage-only cases (oracle "invariant") assert kernel/01 K-06/K-03 structure and are NOT an xterm oracle.',
      'L1 (transport/ConPTY) and L2 (e2e replay) lanes have no suites in this wave; their closure is enforced by the lane probe only.',
      'Byte offsets of the first divergence are unavailable for the pinned vte backend (SD-08.3).',
      'expected.grid is not produced: L0 cases assert rows/counters, not a whole grid (kernel/01 section 3.8 deviation).',
    ],
    artifacts_dir: args.artifacts ? ARTIFACT_ROOT : null,
    cases: evaluated.map(function (e) { return caseReport(e.entry, e.res, e.verdict); }),
  };
  report.artifacts = artifacts;

  const outFile = args.out ? path.resolve(args.out) : path.join(root, ARTIFACT_ROOT, 'conformance-report.json');
  fs.mkdirSync(path.dirname(outFile), { recursive: true });
  const text = canonicalJson(report);
  fs.writeFileSync(outFile, text);
  const digest = sha256Hex(text);

  if (!args.quiet) {
    console.log('=== TermAI conformance (G1 VT consistency, L0 parser lane) ===');
    console.log('cases: ' + evaluated.length + ' total, ' + executed.length + ' executed, '
      + excludedCap.length + ' excluded by capability precondition');
    console.log('gating (L0, real expectations): ' + gatingPassed.length + '/' + gating.length
      + '  R_strict=' + String(report.totals.l0_r_strict) + '  R_gate=' + String(report.totals.l0_r_gate));
    console.log('coverage-only (structural invariants): ' + coverageOnly.length);
    console.log('output lane verdicts: L1/L2 never PASS/FAIL -> probe_ok=' + String(laneSelftestOk));
    if (coverage) {
      console.log('ctlseqs coverage: ' + coverage.entries_with_cases + '/' + coverage.entries_resolved
        + ' resolved entries have >=1 case; ' + coverage.entries_with_gating_cases + ' have a gating case');
    }
    console.log('real-world captures: ' + realCorpus.length + '/' + evaluated.length + ' (AR-31 needs >=20%)');
    console.log('determinism: ' + determinism);
    console.log('gate: NON_GATING (not on RM-A/T0)   G1: NOT_JUDGED');
    if (failures.length) {
      console.log('failures: ' + failures.length);
      for (const f of failures.slice(0, 10)) {
        console.log('  - [' + f.entry.suite + '/' + f.entry.id + '] ' + (f.verdict.first_failure || 'failed'));
      }
      if (failures.length > 10) console.log('  ... ' + (failures.length - 10) + ' more (see report)');
    }
    console.log('report: ' + outFile.split(path.sep).join('/'));
    console.log('report sha256: ' + digest);
  }
  return failures.length ? 1 : 0;
}

let code = 2;
try {
  code = main();
} catch (err) {
  console.error('conformance ERROR: ' + (err && err.stack ? err.stack : String(err)));
  code = 2;
}
process.exit(code);
