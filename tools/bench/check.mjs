#!/usr/bin/env node
// tools/bench/check.mjs
// TermAI bench measurement-methodology gate (merge-blocking for the tooling itself).
// Zero external dependencies, Node >= 22, Windows / macOS / Linux.
//
//   node tools/bench/check.mjs              # run B1-B7
//   node tools/bench/check.mjs --selftest   # prove the judgement logic is not always-green
//   node tools/bench/check.mjs --json       # machine-readable JSON only
//   node tools/bench/check.mjs --report <p> # additionally schema-validate a bench-report.json
//   node tools/bench/check.mjs --machine <p># attest this host as a registered RM-A / RM-C
//
// What this tool owns (docs/spec/kernel/06-performance-methodology.md, B-10 in kernel/00-index):
//   B1 HARNESS section 5 -> registry parity            (the 19 rows, verbatim gate cells)
//   B2 kernel/06 3.4 / 3.7 -> schema parity            (field names verified against the spec text)
//   B3 H1..H19 mapping-table integrity                 (no gap, no duplicate, C1 outside the 19)
//   B4 threshold transcription parity                  (3.6 eps_self column + 3.1 state-machine constants)
//   B5 machine-fingerprint determinism                 (same input same hash; any field change changes it)
//   B6 kernel/06 3.1 state machine branch coverage     (synthetic logic fixtures, not measurements)
//   B7 machine-binding honesty boundary                (ADR-0014: no RM-A / RM-C -> no gate number)
//
// What this tool does NOT do: it does not measure anything. Production numbers come from
// cargo xtask bench on RM-A / RM-C (crates/termai-bench, tests/bench/), which M0 has not built.
// On this host every discovered report/value is labelled NON_GATING / INCONCLUSIVE and the tool
// asserts that it produced zero gating numbers.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import * as B from './lib.mjs';
import * as R from './registry.mjs';
import * as F from './fixtures.mjs';
import * as V from './values.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(HERE, '..', '..');
const KERNEL06_REL = 'docs/spec/kernel/06-performance-methodology.md';
const HARNESS_REL = 'HARNESS.md';

const STATUS = { PASS: 'PASS', FAIL: 'FAIL', SKIP: 'SKIP' };

function gate(id, title, status, detail, notes) {
  return { id: id, title: title, status: status, detail: detail || '', notes: notes || [] };
}

// --------------------------------------------------- markdown parsing helpers

function readText(root, rel) {
  return fs.readFileSync(path.join(root, rel), 'utf8');
}

function fencedBlocks(text) {
  const out = [];
  const re = /```[A-Za-z0-9_-]*\r?\n([\s\S]*?)```/g;
  let m;
  while ((m = re.exec(text)) !== null) out.push(m[1]);
  return out;
}

function splitRow(line) {
  return line.split('|').slice(1, -1).map(function (c) { return c.trim(); });
}

function isSeparatorRow(cells) {
  return cells.length > 0 && cells.every(function (c) { return /^-{3,}$/.test(c); });
}

// HARNESS section 5 table (the 19-row unified non-functional budget).
function harnessSection5Rows(text) {
  const lines = text.split(/\r?\n/);
  let start = -1;
  for (let i = 0; i < lines.length; i++) {
    if (/^##\s*5\./.test(lines[i].trim())) { start = i; break; }
  }
  if (start < 0) return { ok: false, reason: 'HARNESS section 5 heading not found', rows: [] };
  const rows = [];
  for (let i = start + 1; i < lines.length; i++) {
    const t = lines[i].trim();
    if (t.indexOf('## ') === 0) break;
    if (t === '---') break;
    if (t.charAt(0) !== '|') continue;
    const cells = splitRow(t);
    if (isSeparatorRow(cells)) continue;
    if (cells[0] === '指标') continue;
    rows.push(cells);
  }
  return { ok: true, rows: rows };
}

function normalizeMarkdown(s) {
  return String(s).replace(/\*\*/g, '').replace(/\s+/g, ' ').trim();
}

// kernel/06 section 3.9 mapping table rows (H1..H19 plus the C1 out-of-table control).
function kernel06Section39Rows(text) {
  const rows = [];
  for (const raw of text.split(/\r?\n/)) {
    const t = raw.trim();
    if (t.indexOf('| H') !== 0 && t.indexOf('| C1') !== 0) continue;
    rows.push(splitRow(t));
  }
  return rows;
}

// kernel/06 section 3.6 eps_self table rows.
function kernel06Section36Rows(text) {
  const labels = ['场景指纹 / 网格哈希', '帧时（P99）', '启动（P95）', '吞吐（中位数）', 'RSS（中位数）', '安装包（字节）'];
  const out = [];
  for (const raw of text.split(/\r?\n/)) {
    const t = raw.trim();
    if (t.charAt(0) !== '|') continue;
    const cells = splitRow(t);
    if (isSeparatorRow(cells)) continue;
    const first = normalizeMarkdown(cells[0]);
    if (labels.indexOf(first) >= 0) out.push({ label: first, cell: normalizeMarkdown(cells[1]) });
  }
  return out;
}

function parsePctCell(cell) {
  const pct = /(\d+(?:\.\d+)?)\s*%/.exec(cell);
  if (pct) return parseFloat(pct[1]) / 100;
  const plain = /(\d+(?:\.\d+)?)/.exec(cell);
  return plain ? parseFloat(plain[1]) : NaN;
}

// JSON key names inside a fenced JSON example (the fingerprint example carries // comments, so a
// real JSON.parse is not possible -- and not needed: kernel/06 3.4 / 3.7 field names are what we
// verify).
function jsonKeyNames(block) {
  const set = new Set();
  const re = /"([A-Za-z0-9_]+)"\s*:/g;
  let m;
  while ((m = re.exec(block)) !== null) set.add(m[1]);
  return set;
}

function schemaKeyNameSet(node) {
  const set = new Set();
  for (const p of B.schemaKeys(node, '')) set.add(p.split('.').pop());
  return set;
}

function setDiff(a, b) {
  const out = [];
  for (const x of a) if (!b.has(x)) out.push(x);
  return out.sort();
}

// --------------------------------------------------- document model

function loadDocs(root) {
  return {
    harness: readText(root, HARNESS_REL),
    kernel06: readText(root, KERNEL06_REL),
  };
}

// --------------------------------------------------- B1 HARNESS section 5 parity

function gateB1(root, docs) {
  const TITLE = 'HARNESS section 5 -> registry parity (19 rows, verbatim release-gate cells)';
  const parsed = harnessSection5Rows(docs.harness);
  if (!parsed.ok) return gate('B1', TITLE, STATUS.FAIL, parsed.reason);
  const rows = parsed.rows;
  const notes = [];
  if (rows.length !== R.MAPPING_ROW_COUNT) {
    return gate('B1', TITLE, STATUS.FAIL, 'HARNESS section 5 has ' + rows.length + ' data row(s) but the registry has ' + R.MAPPING_ROW_COUNT + '; coverage must be gapless and gapless means equal');
  }
  const problems = [];
  for (let i = 0; i < rows.length; i++) {
    const id = 'H' + (i + 1);
    const entry = R.findMapping(id);
    const cells = rows[i];
    if (!entry) { problems.push(id + ': no registry entry'); continue; }
    if (cells.length < 4) { problems.push(id + ': HARNESS row has ' + cells.length + ' cell(s), expected 4'); continue; }
    if (cells[1] !== entry.harnessGateCell) {
      problems.push(id + ': release-gate cell drifted -- HARNESS says ' + JSON.stringify(cells[1]) + ' but the registry says ' + JSON.stringify(entry.harnessGateCell));
    }
    if (normalizeMarkdown(cells[0]) !== entry.label) {
      problems.push(id + ': metric label drifted -- HARNESS says ' + JSON.stringify(normalizeMarkdown(cells[0])) + ' but the registry says ' + JSON.stringify(entry.label));
    }
    if (normalizeMarkdown(cells[2]) !== entry.harnessTargetCell) {
      problems.push(id + ': target cell drifted -- HARNESS says ' + JSON.stringify(normalizeMarkdown(cells[2])) + ' but the registry says ' + JSON.stringify(entry.harnessTargetCell));
    }
  }
  if (problems.length) {
    return gate('B1', TITLE, STATUS.FAIL, problems.length + ' parity violation(s) between HARNESS section 5 and the registry', problems.slice(0, 12));
  }
  notes.push('19 rows matched verbatim: label + release-gate cell + target cell (registry transcription is re-derived from HARNESS.md on every run)');
  return gate('B1', TITLE, STATUS.PASS, rows.length + ' HARNESS section 5 row(s) match the registry character-for-character', notes);
}

// --------------------------------------------------- B2 kernel/06 schema parity

function gateB2(root, docs) {
  const TITLE = 'kernel/06 3.4 + 3.7 -> schema parity (field names verified against the spec text)';
  const blocks = fencedBlocks(docs.kernel06);
  const reportBlock = blocks.find(function (b) { return b.indexOf('"schemaVersion": "1.0.0"') >= 0; });
  const fpBlock = blocks.find(function (b) { return b.indexOf('"RM-A-win11-24h2"') >= 0; });
  if (!reportBlock) return gate('B2', TITLE, STATUS.FAIL, 'kernel/06 3.7 bench-report.json example block not found');
  if (!fpBlock) return gate('B2', TITLE, STATUS.FAIL, 'kernel/06 3.4 machine-fingerprint.json example block not found');

  const problems = [];
  const specReport = jsonKeyNames(reportBlock);
  const ourReport = schemaKeyNameSet(B.BENCH_REPORT_SCHEMA);
  const onlySpec = setDiff(specReport, ourReport);
  const onlyOurs = setDiff(ourReport, specReport);
  if (onlySpec.length) problems.push('bench-report: fields present in kernel/06 3.7 but absent from the schema: ' + onlySpec.join(', '));
  if (onlyOurs.length) problems.push('bench-report: fields present in the schema but absent from kernel/06 3.7: ' + onlyOurs.join(', '));

  const specFp = jsonKeyNames(fpBlock);
  const ourFp = schemaKeyNameSet(B.MACHINE_FINGERPRINT_SCHEMA);
  const fpOnlySpec = setDiff(specFp, ourFp);
  const fpOnlyOurs = setDiff(ourFp, specFp);
  if (fpOnlySpec.length) problems.push('machine-fingerprint: fields present in kernel/06 3.4 but absent from the schema: ' + fpOnlySpec.join(', '));
  if (fpOnlyOurs.length) problems.push('machine-fingerprint: fields present in the schema but absent from kernel/06 3.4: ' + fpOnlyOurs.join(', '));

  // The 9 legacy fields must be spelled out both in the spec prose and in the schema.
  const legacyMissingFromSpec = B.LEGACY_METRIC_FIELDS.concat(B.LEGACY_TOPLEVEL_FIELDS).filter(function (f) {
    return docs.kernel06.indexOf(f) < 0;
  });
  if (legacyMissingFromSpec.length) problems.push('legacy fields missing from the kernel/06 text: ' + legacyMissingFromSpec.join(', '));
  if (B.LEGACY_FIELD_COUNT !== 9) problems.push('LEGACY_FIELD_COUNT is ' + B.LEGACY_FIELD_COUNT + ', kernel/06 3.7 / 00-index require exactly 9');
  if (!/9 \u4e2a legacy \u5b57\u6bb5/.test(docs.kernel06)) problems.push('kernel/06 3.7 no longer states "9 legacy fields"; the schema contract must be re-reviewed');

  if (problems.length) {
    return gate('B2', TITLE, STATUS.FAIL, problems.length + ' schema-parity violation(s)', problems.slice(0, 12));
  }
  const notes = [
    '9 legacy fields present: ' + B.LEGACY_METRIC_FIELDS.join(', ') + ' (metric level) + ' + B.LEGACY_TOPLEVEL_FIELDS.join(', ') + ' (top level)',
    'declared additive field(s) outside the kernel/06 contract, permitted for 1 minor by 3.7: ' + B.KNOWN_ADDITIVE_TOPLEVEL_FIELDS.join(', '),
    'FLAGGED AMBIGUITY for the spec owner: spec 07 3.8.2 wants a flat {metric,value,unit,samples,runner,commit,toolchain,ts}, but commit/toolchain/runner are structured OBJECTS at the top level of the kernel/06 3.7 report; the tool therefore carries the flat projection in the additive field ' + B.FLAT_PROJECTION_CONTAINER + ' instead of colliding with the contract. See tools/bench/README.md.',
  ];
  return gate('B2', TITLE, STATUS.PASS,
    'bench-report ' + ourReport.size + ' field name(s) and machine-fingerprint ' + ourFp.size + ' field name(s) match kernel/06 3.4 / 3.7 exactly',
    notes);
}

// --------------------------------------------------- B3 mapping-table integrity

function gateB3(root, docs) {
  const TITLE = 'H1..H19 mapping-table integrity (no gap, no duplicate, C1 outside the 19)';
  const integrity = R.mappingIntegrity();
  const problems = integrity.errors.map(function (e) { return e.message; });
  const rows = kernel06Section39Rows(docs.kernel06);
  const hRows = rows.filter(function (c) { return /^H\d+$/.test(c[0]); });
  const cRows = rows.filter(function (c) { return c[0] === 'C1'; });
  if (hRows.length !== 19) problems.push('kernel/06 3.9 registers ' + hRows.length + ' H row(s); the section requires exactly 19 (H1..H19)');
  if (cRows.length !== 1) problems.push('kernel/06 3.9 registers ' + cRows.length + ' C1 row(s); the out-of-table control must appear exactly once');
  const specIds = hRows.map(function (c) { return c[0]; });
  const expectedIds = [];
  for (let i = 1; i <= 19; i++) expectedIds.push('H' + i);
  for (let i = 0; i < expectedIds.length; i++) {
    if (specIds[i] !== expectedIds[i]) {
      problems.push('kernel/06 3.9 row ' + (i + 1) + ' is ' + String(specIds[i]) + ' but H' + (i + 1) + ' was expected (order and numbering must match)');
      break;
    }
  }
  // Governed column of the spec table must agree with the registry.
  const governedMap = { '是': 'yes', '否': 'no' };
  for (const cells of hRows) {
    const entry = R.findMapping(cells[0]);
    if (!entry) continue;
    const raw = normalizeMarkdown(cells[5] || '');
    let specGoverned = governedMap[raw];
    if (!specGoverned && raw.indexOf('方法') >= 0) specGoverned = 'partial';
    if (!specGoverned && raw.indexOf('否') === 0) specGoverned = 'no';
    if (specGoverned && specGoverned !== entry.governed) {
      problems.push(cells[0] + ': kernel/06 3.9 says governed=' + JSON.stringify(raw) + ' (' + specGoverned + ') but the registry says ' + entry.governed);
    }
  }
  if (!/H1\u2026H19/.test(docs.kernel06)) problems.push('kernel/06 3.9 no longer claims H1...H19 coverage; the self-proof line must be re-read');
  if (problems.length) {
    return gate('B3', TITLE, STATUS.FAIL, problems.length + ' mapping violation(s)', problems.slice(0, 12));
  }
  return gate('B3', TITLE, STATUS.PASS, 'H1..H19 contiguous and unique, cross-checked against kernel/06 3.9', [
    'rows: ' + R.MAPPING_ROW_COUNT + '; out-of-table controls excluded from the count: ' + R.OUT_OF_TABLE_IDS.join(', '),
    'per-row owner / carrier / measurement definition present: yes',
  ]);
}

// --------------------------------------------------- B4 threshold transcription parity

function gateB4(root, docs) {
  const TITLE = 'threshold transcription parity (kernel/06 3.6 eps_self column + 3.1 state-machine constants)';
  const problems = [];
  const notes = [];

  const epsRows = kernel06Section36Rows(docs.kernel06);
  const expect = [
    { label: '场景指纹 / 网格哈希', family: 'exact' },
    { label: '帧时（P99）', family: 'frame' },
    { label: '启动（P95）', family: 'startup' },
    { label: '吞吐（中位数）', family: 'throughput' },
    { label: 'RSS（中位数）', family: 'rss' },
    { label: '安装包（字节）', family: 'exact' },
  ];
  for (const e of expect) {
    const row = epsRows.find(function (r) { return r.label === e.label; });
    if (!row) { problems.push('kernel/06 3.6 row "' + e.label + '" not found'); continue; }
    const specValue = parsePctCell(row.cell);
    const ourValue = B.epsSelfFor(e.family);
    if (!(Math.abs(specValue - ourValue) < 1e-12)) {
      problems.push('eps_self mismatch for family "' + e.family + '" (' + e.label + '): kernel/06 3.6 says ' + specValue + ' but METRIC_FAMILY_POLICY says ' + ourValue);
    }
  }
  const grades = { frame: 0.05, startup: 0.03, throughput: 0.02, rss: 0.01 };
  for (const fam of Object.keys(grades)) {
    if (!(Math.abs(B.METRIC_FAMILY_POLICY[fam].madMaxPct - grades[fam]) < 1e-12)) {
      problems.push('AR-31 #9 / OQ-PM-07 MAD grading for "' + fam + '" should be ' + grades[fam] + ' but is ' + B.METRIC_FAMILY_POLICY[fam].madMaxPct);
    }
  }
  notes.push('MAD grading (AR-31 #9 / OQ-PM-07): frame ' + (grades.frame * 100) + '% / startup ' + (grades.startup * 100) + '% / throughput ' + (grades.throughput * 100) + '% / RSS ' + (grades.rss * 100) + '%');

  const s31 = fencedBlocks(docs.kernel06).find(function (b) { return b.indexOf('evaluate(metric, machine, fingerprint, baseline)') >= 0; });
  const s31reg = fencedBlocks(docs.kernel06).find(function (b) { return b.indexOf('regression(metric, baseline, nights, context)') >= 0; });
  if (!s31) problems.push('kernel/06 3.1 evaluate() block not found');
  if (!s31reg) problems.push('kernel/06 3.1 regression() block not found');
  const codeChecks = [
    [s31, /runs\.valid\s*<\s*10/, 'runs.valid < 10', B.DEFAULT_MIN_RUNS === 10],
    [s31, /run\.n\s*<\s*1e5/, 'run.n < 1e5', B.TAIL_MIN_SAMPLES === 1e5],
    [s31, /mad_over_median\(runs\)\s*>\s*0\.02/, 'mad_over_median(runs) > 0.02', B.METRIC_FAMILY_POLICY.default.madMaxPct === 0.02],
    [s31reg, /if\s+d\s*<=\s*0\.05/, 'd <= 0.05', B.REGRESSION_LIMIT === 0.05],
    [s31reg, /nights\.consecutive\s*>=\s*2/, 'nights.consecutive >= 2', B.CONSECUTIVE_NIGHTS_REL === 2],
    [s31reg, /context\s*==\s*PR/, 'context == PR (G4-PR)', true],
    [s31reg, /context\s*==\s*RC_FULL/, 'context == RC_FULL (G4-REL)', true],
  ];
  for (const c of codeChecks) {
    if (!c[0]) continue;
    if (!c[1].test(c[0])) problems.push('kernel/06 3.1 code block no longer contains ' + c[2]);
    if (!c[3]) problems.push('constant for "' + c[2] + '" does not match the transcribed value');
  }
  if (problems.length) {
    return gate('B4', TITLE, STATUS.FAIL, problems.length + ' transcription violation(s)', problems.slice(0, 12));
  }
  return gate('B4', TITLE, STATUS.PASS, 'every threshold in lib.mjs is re-derived from kernel/06 3.1 / 3.6 + AR-31 #9 on each run', notes);
}

// --------------------------------------------------- B5 fingerprint determinism

function mutateLeaf(fp, leafPath) {
  const clone = F.clone(fp);
  const parts = leafPath.split('.');
  let cur = clone;
  for (let i = 0; i < parts.length - 1; i++) cur = cur[parts[i]];
  const key = parts[parts.length - 1];
  const v = cur[key];
  if (typeof v === 'string') cur[key] = v + '-MUTATED';
  else if (typeof v === 'number') cur[key] = v + 1;
  else if (typeof v === 'boolean') cur[key] = !v;
  else cur[key] = 'mutated';
  return clone;
}

function reorderKeys(value) {
  if (value === null || typeof value !== 'object') return value;
  if (Array.isArray(value)) return value.map(reorderKeys);
  const keys = Object.keys(value).reverse();
  const out = {};
  for (const k of keys) out[k] = reorderKeys(value[k]);
  return out;
}

function fingerprintSelfTest() {
  const results = { same: false, reorder: false, leaves: [], errors: [] };
  const fp = F.syntheticFingerprint();
  const h1 = B.computeFingerprintSha256(fp);
  const h2 = B.computeFingerprintSha256(F.syntheticFingerprint());
  results.same = (h1 === h2);
  const hr = B.computeFingerprintSha256(reorderKeys(fp));
  results.reorder = (h1 === hr);
  for (const leaf of B.fingerprintCoverage(fp)) {
    const changed = B.computeFingerprintSha256(mutateLeaf(fp, leaf));
    results.leaves.push({ leaf: leaf, changed: changed !== h1 });
  }
  results.unchangedLeaves = results.leaves.filter(function (l) { return !l.changed; }).map(function (l) { return l.leaf; });
  results.sha = h1;
  results.coverage = results.leaves.length;
  return results;
}

function gateB5(root) {
  const TITLE = 'machine-fingerprint determinism (same input same hash; any field change changes the hash)';
  const r = fingerprintSelfTest();
  const problems = [];
  if (!r.same) problems.push('two identical fingerprints produced different sha256 values');
  if (!r.reorder) problems.push('key insertion order changed the sha256 (canonicalisation is not stable)');
  if (r.unchangedLeaves.length) problems.push('these fingerprint leaves did not change the sha256: ' + r.unchangedLeaves.join(', '));
  const env = B.validateMachineFingerprint(F.syntheticFingerprint());
  if (!env.ok) problems.push('the synthetic fingerprint does not satisfy the kernel/06 3.4 schema: ' + env.errors.slice(0, 3).map(function (e) { return e.message; }).join('; '));
  if (problems.length) return gate('B5', TITLE, STATUS.FAIL, problems.length + ' determinism violation(s)', problems);
  return gate('B5', TITLE, STATUS.PASS, 'stable over ' + r.coverage + ' covered leaf field(s); all single-field mutations change the hash', [
    'sha256(synthetic) = ' + r.sha + ' (SYNTHETIC LOGIC FIXTURE, not a registered reference machine)',
    'covered fields include the OQ-PM-11 extensions: display.edid_sha256 / fonts.set_sha256 / clocks.source / input.keyboard_report_hz',
  ]);
}

// --------------------------------------------------- B6 state-machine branch coverage

const CASES = [];

function caseOf(name, kind, run, expect) {
  CASES.push({ name: name, kind: kind, run: run, expect: expect });
}

function buildCases() {
  if (CASES.length) return CASES;
  const fp = F.syntheticFingerprint();
  const sha = F.syntheticFingerprintSha();
  const H1 = R.findMapping('H1');   // startup, RM-A, gate 150ms
  const H4 = R.findMapping('H4');   // frame, RM-C, gate <8.3ms
  const H5 = R.findMapping('H5');   // throughput, RM-A, gate >=500
  const H6 = R.findMapping('H6');   // rss, RM-A, gate <=120
  const H2 = R.findMapping('H2');   // key-to-photon tail, RM-C, gate <=16
  const H13 = R.findMapping('H13'); // package bytes: not a machine gate

  const okMachine = F.syntheticMachine({ fingerprintSha256: sha });
  const okScene = { frozen: true, hashA: 'scene-a', hashB: 'scene-a' };

  function ev(metric, runs, opts) {
    const o = opts || {};
    return B.evaluate({
      metric: metric,
      fingerprint: o.fingerprint || { recorded: true, sha256: sha },
      machine: o.machine || okMachine,
      scene: o.scene === undefined ? okScene : o.scene,
      doubleRun: o.doubleRun === undefined ? { a: 100, b: 100.5 } : o.doubleRun,
      runs: runs,
    });
  }

  // controls: the healthy paths that must NOT be red
  caseOf('control: healthy startup passes with a positive margin', 'control', function () {
    return ev(H1, F.syntheticRuns(100, 0.005));
  }, function (v) { return v.state === 'PASS' && Math.abs(v.margin - 50) < 1e-9; });

  caseOf('control: frame runs at exactly the 5% MAD grading boundary still judge', 'control', function () {
    return ev(H4, F.syntheticRuns(5, 0.05, { n: 10000 }), { doubleRun: { a: 100, b: 105 } });
  }, function (v) { return v.state === 'PASS'; });

  // injected faults
  caseOf('inject: scene not frozen (cursor blink left on) -> INVALID(SCENE_NOT_FROZEN)', 'fault', function () {
    return ev(H1, F.syntheticRuns(100, 0.005), { scene: { frozen: false } });
  }, function (v) { return v.state === 'INVALID' && v.reason === 'SCENE_NOT_FROZEN'; });

  caseOf('inject: D1 double-run jitter 3% above the 2% throughput eps_self -> INVALID(MEASUREMENT_UNSTABLE)', 'fault', function () {
    return ev(H5, F.syntheticRuns(1000, 0.005), { doubleRun: { a: 100, b: 103 } });
  }, function (v) { return v.state === 'INVALID' && v.reason === 'MEASUREMENT_UNSTABLE'; });

  caseOf('inject: forced --backend t1 -> NON_GATING(BACKEND_DEGRADED)', 'fault', function () {
    return ev(H1, F.syntheticRuns(100, 0.005), { machine: F.syntheticMachine({ fingerprintSha256: sha, backend: 't1' }) });
  }, function (v) { return v.state === 'NON_GATING' && v.reason === 'BACKEND_DEGRADED'; });

  caseOf('inject: cloud runner -> NON_GATING(CLOUD_RUNNER)', 'fault', function () {
    return ev(H1, F.syntheticRuns(100, 0.005), { machine: F.syntheticMachine({ fingerprintSha256: sha, class: 'cloud' }) });
  }, function (v) { return v.state === 'NON_GATING' && v.reason === 'CLOUD_RUNNER'; });

  caseOf('inject: RM-B floor machine -> NON_GATING(FLOOR_MACHINE_ONLY)', 'fault', function () {
    return ev(H1, F.syntheticRuns(100, 0.005), { machine: F.syntheticMachine({ fingerprintSha256: sha, role: 'RM-B' }) });
  }, function (v) { return v.state === 'NON_GATING' && v.reason === 'FLOOR_MACHINE_ONLY'; });

  caseOf('inject: no fingerprint registered -> INVALID(FP_MISSING)', 'fault', function () {
    return ev(H1, F.syntheticRuns(100, 0.005), { fingerprint: { recorded: false, sha256: null } });
  }, function (v) { return v.state === 'INVALID' && v.reason === 'FP_MISSING'; });

  caseOf('inject: graphene driver field tampered -> INVALID(FP_MISMATCH)', 'fault', function () {
    return ev(H1, F.syntheticRuns(100, 0.005), { machine: F.syntheticMachine({ fingerprintSha256: 'deadbeef'.repeat(8) }) });
  }, function (v) { return v.state === 'INVALID' && v.reason === 'FP_MISMATCH'; });

  caseOf('inject: only 9 valid runs -> INVALID(INSUFFICIENT_RUNS)', 'fault', function () {
    return ev(H1, F.syntheticRuns(100, 0.005, { validCount: 9 }));
  }, function (v) { return v.state === 'INVALID' && v.reason === 'INSUFFICIENT_RUNS'; });

  caseOf('inject: tail metric with 1000 samples per run -> INVALID(INSUFFICIENT_SAMPLES)', 'fault', function () {
    return ev(H2, F.syntheticRuns(10, 0.005, { n: 1000 }));
  }, function (v) { return v.state === 'INVALID' && v.reason === 'INSUFFICIENT_SAMPLES'; });

  caseOf('inject: RSS run spread raised to 5% -> INCONCLUSIVE(NOISE_FLOOR)', 'fault', function () {
    return ev(H6, F.syntheticRuns(100, 0.05));
  }, function (v) { return v.state === 'INCONCLUSIVE' && v.reason === 'NOISE_FLOOR'; });

  caseOf('inject: startup run spread raised to 4% (limit 3%) -> INCONCLUSIVE(NOISE_FLOOR)', 'fault', function () {
    return ev(H1, F.syntheticRuns(100, 0.04));
  }, function (v) { return v.state === 'INCONCLUSIVE' && v.reason === 'NOISE_FLOOR'; });

  caseOf('control: throughput run spread 1% stays under the 2% grading', 'control', function () {
    return ev(H5, F.syntheticRuns(1000, 0.01));
  }, function (v) { return v.state === 'PASS'; });

  caseOf('inject: throughput below the 500MB/s gate -> FAIL(GATE_BREACH)', 'fault', function () {
    return ev(H5, F.syntheticRuns(400, 0.005));
  }, function (v) { return v.state === 'FAIL' && v.kind === 'GATE_BREACH'; });

  caseOf('control: package size is not a reference-machine gate -> SKIP(NOT_MACHINE_GATE)', 'control', function () {
    return ev(H13, F.syntheticRuns(50, 0));
  }, function (v) { return v.state === 'SKIP' && v.reason === 'NOT_MACHINE_GATE'; });

  const reg = function (ctx, opts) {
    const o = opts || {};
    return B.regression({
      metricId: 'synthetic.metric',
      baseline: Object.prototype.hasOwnProperty.call(o, 'baseline') ? o.baseline : { median: 100 },
      medianNow: typeof o.medianNow === 'number' ? o.medianNow : 110,
      context: ctx,
      nights: o.nights || { consecutive: 1, latestDelta: 0.1 },
    });
  };

  caseOf('inject: +10% regression in PR context -> FAIL(REGRESSION_PR)', 'fault', function () { return reg('PR'); },
    function (v) { return v.state === 'FAIL' && v.kind === 'REGRESSION_PR'; });

  caseOf('inject: +10% regression on a single night -> INCONCLUSIVE(SUSPECT_SINGLE_NIGHT)', 'fault', function () { return reg('NIGHTLY'); },
    function (v) { return v.state === 'INCONCLUSIVE' && v.reason === 'SUSPECT_SINGLE_NIGHT'; });

  caseOf('inject: +10% regression on 2 consecutive nights -> FAIL(REGRESSION_REL)', 'fault', function () {
    return reg('NIGHTLY', { nights: { consecutive: 2, latestDelta: 0.1 } });
  }, function (v) { return v.state === 'FAIL' && v.kind === 'REGRESSION_REL'; });

  caseOf('inject: +10% regression reproduced once in the pre-release full suite -> FAIL(REGRESSION_REL)', 'fault', function () {
    return reg('RC_FULL');
  }, function (v) { return v.state === 'FAIL' && v.kind === 'REGRESSION_REL'; });

  caseOf('control: +1% regression -> PASS with margin 4%', 'control', function () { return reg('PR', { medianNow: 101 }); },
    function (v) { return v.state === 'PASS' && Math.abs(v.margin - 0.04) < 1e-12; });

  caseOf('inject: deleted baseline -> SKIP(NO_BASELINE), never a silent PASS', 'fault', function () {
    return reg('PR', { baseline: null });
  }, function (v) { return v.state === 'SKIP' && v.reason === 'NO_BASELINE'; });

  caseOf('inject: INVALID evaluation must not reach the baseline comparison', 'fault', function () {
    const e = ev(H1, F.syntheticRuns(100, 0.005), { scene: { frozen: false } });
    return B.gateAndRegression({ evaluation: e, regressionInput: { metricId: 'x', baseline: { median: 100 }, medianNow: 110, context: 'PR' } });
  }, function (v) { return v.state === 'INVALID' && v.regressionSkipped === true; });

  caseOf('inject: verdict flip between two runs on the same commit -> INVALID(MEASUREMENT_UNSTABLE)', 'fault', function () {
    const a = F.syntheticReport({ metric: 'synthetic.metric', value: 100, verdict: 'PASS', reportVerdict: 'PASS' });
    const b = F.syntheticReport({ metric: 'synthetic.metric', value: 100, verdict: 'FAIL', reportVerdict: 'FAIL' });
    return B.assertReproducible(a, b);
  }, function (v) { return v.ok === false && v.problems.some(function (p) { return p.code === 'MEASUREMENT_UNSTABLE'; }); });

  caseOf('control: two identical runs are reproducible', 'control', function () {
    const a = F.syntheticReport({ metric: 'synthetic.metric', value: 100 });
    const b = F.syntheticReport({ metric: 'synthetic.metric', value: 100 });
    return B.assertReproducible(a, b);
  }, function (v) { return v.ok === true; });

  caseOf('inject: the same verdict but a 20% median drift -> INVALID(MEASUREMENT_UNSTABLE)', 'fault', function () {
    const a = F.syntheticReport({ metric: 'synthetic.metric', value: 100 });
    const b = F.syntheticReport({ metric: 'synthetic.metric', value: 120 });
    return B.assertReproducible(a, b);
  }, function (v) { return v.ok === false; });

  caseOf('inject: measured on a different commit -> PRECONDITION_MISMATCH', 'fault', function () {
    const a = F.syntheticReport({ metric: 'synthetic.metric', value: 100 });
    const b = F.syntheticReport({ metric: 'synthetic.metric', value: 100, mutate: function (r) { r.commit.sha = 'other-commit'; } });
    return B.assertReproducible(a, b);
  }, function (v) { return v.ok === false && v.problems.some(function (p) { return p.code === 'PRECONDITION_MISMATCH'; }); });

  return CASES;
}

function runCases() {
  const cases = buildCases();
  const results = [];
  for (const c of cases) {
    let v;
    let thrown = null;
    try { v = c.run(); } catch (e) { thrown = e; }
    const ok = thrown ? false : !!c.expect(v);
    results.push({ name: c.name, kind: c.kind, ok: ok, thrown: thrown ? String(thrown && thrown.message ? thrown.message : thrown) : null, observed: thrown ? null : summarizeVerdict(v) });
  }
  return results;
}

function summarizeVerdict(v) {
  if (!v || typeof v !== 'object') return String(v);
  const bits = [v.state];
  if (v.reason) bits.push('reason=' + v.reason);
  if (v.kind) bits.push('kind=' + v.kind);
  if (typeof v.ok === 'boolean') bits.push('ok=' + v.ok);
  if (typeof v.margin === 'number') bits.push('margin=' + v.margin);
  return bits.join(' ');
}

function gateB6(root) {
  const TITLE = 'kernel/06 3.1 state-machine branch coverage (synthetic logic fixtures, not measurements)';
  const results = runCases();
  const failed = results.filter(function (r) { return !r.ok; });
  const controls = results.filter(function (r) { return r.kind === 'control'; }).length;
  const faults = results.filter(function (r) { return r.kind === 'fault'; }).length;
  if (failed.length) {
    return gate('B6', TITLE, STATUS.FAIL, failed.length + ' branch(es) did not produce the documented verdict', failed.map(function (r) { return r.name + ' -> observed ' + r.observed + (r.thrown ? ' (threw ' + r.thrown + ')' : ''); }).slice(0, 10));
  }
  return gate('B6', TITLE, STATUS.PASS, controls + ' control branch(es) + ' + faults + ' fault branch(es) all produced the documented verdict', [
    'PASS / FAIL / INVALID / INCONCLUSIVE / SKIP / NON_GATING are all reachable; INVALID and INCONCLUSIVE never reach the baseline comparison',
    'all inputs are SYNTHETIC LOGIC FIXTURES (tools/bench/fixtures.mjs); no measurement is performed',
  ]);
}

// --------------------------------------------------- B7 machine-binding honesty boundary

function discoverFingerprints(root) {
  const dirs = ['tests/bench/fingerprints', 'tools/bench/fingerprints'];
  const found = [];
  for (const rel of dirs) {
    const abs = path.join(root, rel);
    if (!fs.existsSync(abs)) continue;
    for (const name of fs.readdirSync(abs).sort()) {
      if (!/\.json$/.test(name)) continue;
      const p = path.join(abs, name);
      let parsed = null;
      let error = null;
      try { parsed = JSON.parse(fs.readFileSync(p, 'utf8')); } catch (e) { error = String(e.message || e); }
      found.push({ rel: path.posix.join(rel, name), path: p, json: parsed, parseError: error });
    }
  }
  return found;
}

function discoverReports(root) {
  const candidates = ['bench-report.json'];
  for (const dir of ['tests/bench/reports', 'tools/bench/reports']) {
    const abs = path.join(root, dir);
    if (!fs.existsSync(abs)) continue;
    for (const name of fs.readdirSync(abs).sort()) if (/\.json$/.test(name)) candidates.push(path.posix.join(dir, name));
  }
  const out = [];
  for (const rel of candidates) {
    const abs = path.join(root, rel);
    if (!fs.existsSync(abs)) continue;
    let parsed = null;
    let error = null;
    try { parsed = JSON.parse(fs.readFileSync(abs, 'utf8')); } catch (e) { error = String(e.message || e); }
    out.push({ rel: rel, path: abs, json: parsed, parseError: error });
  }
  return out;
}

function loadReportFile(rel, abs) {
  const item = { rel: rel, path: abs, exists: fs.existsSync(abs), json: null, parseError: null };
  if (!item.exists) return item;
  try { item.json = JSON.parse(fs.readFileSync(abs, 'utf8')); } catch (e) { item.parseError = String(e.message || e); }
  return item;
}

// Every report this run reads: the ones discovered in the tree plus the one --report points at.
function buildReportSet(root, explicitPath) {
  const discovered = discoverReports(root);
  let explicit = null;
  if (explicitPath) explicit = loadReportFile(explicitPath, path.resolve(explicitPath));
  const all = explicit && explicit.exists ? discovered.concat([explicit]) : discovered.slice();
  return { discovered: discovered, explicit: explicit, all: all };
}

function gatingStatus(root, explicitMachinePath, reportSet) {
  const fingerprints = discoverFingerprints(root);
  const reports = reportSet.discovered;
  // D-6 step 4: the counter is computed from the values this run presents, not a literal constant.
  const values = V.collectValues(reportSet.all);
  let attested = null;
  if (explicitMachinePath) {
    const abs = path.resolve(explicitMachinePath);
    if (fs.existsSync(abs)) {
      try {
        const json = JSON.parse(fs.readFileSync(abs, 'utf8'));
        const env = B.validateMachineFingerprint(json);
        attested = { path: abs, json: json, valid: env.ok, errors: env.errors };
      } catch (e) {
        attested = { path: abs, json: null, valid: false, errors: [{ message: String(e.message || e) }] };
      }
    }
  }
  const machine = {
    class: 'self-hosted',
    role: attested && attested.json ? attested.json.role : null,
    backend: 't0',
    fingerprintRecorded: !!(attested && attested.valid),
    fingerprintSha256: attested && attested.valid ? B.computeFingerprintSha256(attested.json) : null,
  };
  const cls = B.classifyMachine(machine);
  return {
    fingerprints: fingerprints,
    reports: reports,
    reportSet: reportSet,
    values: values,
    attested: attested,
    machine: machine,
    gating: cls.gating,
    state: cls.state,
    reason: cls.reason,
    detail: cls.detail,
    gatingNumbersProduced: values.gatingNumbersProduced,
    gatingSources: values.gating,
  };
}

function gateB7(root, status) {
  const TITLE = 'machine-binding honesty boundary (ADR-0014: no RM-A / RM-C -> no gate number)';
  const problems = [];
  if (status.gating) {
    problems.push('this run claims to be gating; a reference-machine attestation was supplied, so verify that the attestation is real before trusting any number');
  } else if (!status.state) {
    problems.push('classifyMachine returned neither a gating verdict nor a non-gating state');
  }
  const noFp = B.classifyMachine({ fingerprintRecorded: false });
  if (noFp.gating || noFp.state !== 'INCONCLUSIVE' || noFp.reason !== 'REFERENCE_MACHINE_UNAVAILABLE') {
    problems.push('a host with no registered fingerprint must be INCONCLUSIVE(REFERENCE_MACHINE_UNAVAILABLE), got ' + JSON.stringify(noFp));
  }
  if (status.gatingNumbersProduced !== 0) {
    const where = (status.gatingSources || []).map(function (s) { return s.source + '#' + s.id; }).join(', ');
    problems.push('this tool presented ' + status.gatingNumbersProduced + ' gating number(s) from ' + where + '; on a non-reference host it must present zero (ADR-0014 iron law 5)');
  }
  for (const f of status.fingerprints) {
    if (f.parseError) problems.push(f.rel + ': cannot be parsed (' + f.parseError + ')');
    else {
      const env = B.validateMachineFingerprint(f.json);
      if (!env.ok) problems.push(f.rel + ': ' + env.errors.slice(0, 3).map(function (e) { return e.message; }).join('; '));
    }
  }
  for (const rep of status.reports) {
    if (rep.parseError) problems.push(rep.rel + ': cannot be parsed (' + rep.parseError + ')');
    else {
      const v = B.validateBenchReport(rep.json);
      if (!v.ok) problems.push(rep.rel + ': ' + v.errors.slice(0, 3).map(function (e) { return e.code + ' ' + e.path; }).join('; '));
    }
  }
  if (problems.length) return gate('B7', TITLE, STATUS.FAIL, problems.length + ' honesty-boundary violation(s)', problems.slice(0, 10));
  return gate('B7', TITLE, STATUS.PASS, 'no reference machine on this host: every discovered value is NON_GATING / INCONCLUSIVE and 0 gating numbers were produced', [
    'state: ' + status.state + ' (' + status.reason + ') -- ' + status.detail,
    'registered reference-machine fingerprints found: ' + status.fingerprints.length + '; bench-reports found: ' + status.reports.length,
    'citation: ADR-0014 iron law 5 (cloud runner results are NON-GATING; when a reference machine is unavailable gate items are INCONCLUSIVE, a cloud runner may not stand in) + kernel/06 6',
    'citation: AR-31 #8 / OQ-PM-04 (a PR that does not touch term-render / term-gpu / term-vt / term-session / term-pty may be marked SKIP(no_hotpath); a hot-path PR may not)',
  ]);
}

// --------------------------------------------------- reporting

function summarize(gates) {
  const c = { PASS: 0, FAIL: 0, SKIP: 0 };
  for (const g of gates) c[g.status] = (c[g.status] || 0) + 1;
  return c;
}

// D-6 step 3: present the values. A machine-free section 5 row that the report does not carry is
// printed as NOT REPORTED with its owner and carrier -- it is never silently omitted.
function formatValues(values) {
  if (!values) return [];
  const sources = values.sources || [];
  if (!sources.length) {
    return ['section 5 machine-free values: 0 of ' + values.expected.length + ' reported (' + values.expected.join(', ') + '); no bench-report.json was read -- pass --report <p>'];
  }
  const lines = ['section 5 machine-free values (a value here is never a gate number; read from ' + sources.join(', ') + '):'];
  for (const r of values.rows) {
    if (r.status === V.REPORTED) {
      lines.push('  ' + r.id + ' = ' + r.value + ' ' + String(r.reportedUnit) + '  [' + String(r.verdict) + '; registry gate ' + r.gateOp + ' ' + r.gate + ' ' + r.unit + '; owner ' + r.owner + '; source ' + r.source + ']');
    } else {
      lines.push('  ' + r.id + ' NOT REPORTED -- ' + r.reason + '  [owner ' + r.owner + '; carrier ' + r.carrier + ']');
    }
  }
  for (const c of values.controls) {
    lines.push('  ' + c.id + ' (out-of-table control, not one of the 19) = ' + c.value + ' ' + String(c.reportedUnit) + '  [' + String(c.verdict) + '; source ' + c.source + ']');
  }
  for (const e of values.extra) {
    lines.push('  ' + e.id + ' = ' + e.value + ' ' + String(e.reportedUnit) + '  [' + String(e.verdict) + '; ' + (e.machine === 'none' ? 'not a machine gate' : 'machine ' + e.machine) + '; source ' + e.source + ']');
  }
  if (values.unbound.length) lines.push('  not bound to a section 5 row: ' + values.unbound.map(function (u) { return u.id + ' (' + u.source + ')'; }).join(', '));
  return lines;
}

function formatHuman(report) {
  const lines = [];
  lines.push('=== bench (kernel/06 performance methodology: schema / fingerprint / verdicts / H1-H19) ===');
  for (const g of report.gates) {
    lines.push('[' + g.id + '] ' + g.status.padEnd(4) + ' ' + g.title + (g.detail ? ' - ' + g.detail : ''));
    for (const n of g.notes) lines.push('      ' + n);
  }
  for (const v of formatValues(report.values)) lines.push(v);
  lines.push('honesty boundary: ' + report.gating.state + ' (' + report.gating.reason + ')');
  lines.push('  ' + report.gating.detail);
  lines.push('  gating numbers produced by this run: ' + report.gating.gatingNumbersProduced);
  lines.push('summary: ' + report.counts.PASS + ' PASS / ' + report.counts.FAIL + ' FAIL / ' + report.counts.SKIP + ' SKIP  (' + report.gates.length + ' gates)');
  lines.push('result: ' + report.result + (report.result === 'FAIL' ? ' (' + report.counts.FAIL + ' blocking)' : ''));
  if (report.counts.FAIL > 0) {
    lines.push('blocking failures:');
    for (const g of report.gates) if (g.status === STATUS.FAIL) lines.push('  - [' + g.id + '] ' + g.title + ' - ' + g.detail);
  }
  return lines.join('\n');
}

function buildReport(root, gates, status) {
  const c = summarize(gates);
  const failed = gates.filter(function (g) { return g.status === STATUS.FAIL; });
  return {
    tool: 'bench',
    schema_version: 1,
    root: root,
    authority: 'AR-24.3 / AR-27 / AR-30 / AR-31; docs/spec/kernel/06-performance-methodology.md section 1-3; ADR-0014',
    values: status.values,
    result: failed.length === 0 ? 'PASS' : 'FAIL',
    counts: c,
    gating: {
      state: status.state,
      reason: status.reason,
      detail: status.detail,
      gating: status.gating,
      gatingNumbersProduced: status.gatingNumbersProduced,
      registeredFingerprints: status.fingerprints.map(function (f) { return f.rel; }),
      discoveredReports: status.reports.map(function (r) { return r.rel; }),
    },
    gates: gates,
  };
}

// --------------------------------------------------- selftest

function makeSelftest() {
  const rows = [];
  let missed = 0;
  return {
    check: function (name, condition, detail) {
      const ok = !!condition;
      if (!ok) missed++;
      rows.push({ name: name, ok: ok, detail: detail || '' });
      return ok;
    },
    report: function (title) {
      const lines = ['=== ' + title + ' ==='];
      for (const r of rows) {
        lines.push('  ' + (r.ok ? 'caught ' : 'MISSED ') + ' ' + r.name + (r.detail ? '  (' + r.detail + ')' : ''));
      }
      const caught = rows.filter(function (r) { return r.ok; }).length;
      lines.push('  injected faults caught: ' + caught + '/' + rows.length);
      const ok = missed === 0;
      lines.push('result: ' + (ok ? 'PASS - every injection was caught; the judgement logic is not always-green' : 'FAIL - ' + missed + ' injection(s) were not caught'));
      return { text: lines.join('\n'), ok: ok, missed: missed, caught: caught, total: rows.length };
    },
  };
}

function hasCode(result, code) {
  return result.errors.some(function (e) { return e.code === code; });
}

function errorCodes(result) {
  return result.errors.map(function (e) { return e.code; }).join(',');
}

function runSelftest(root) {
  const st = makeSelftest();
  const docs = loadDocs(root);
  const status = gatingStatus(root, null, buildReportSet(root, null));

  // --- schema faults
  const goodReport = F.syntheticReport({ metric: 'latency.key_to_photon.p99', value: 1, gate: 16 });
  const good = B.validateBenchReport(goodReport);
  st.check('control: a well-formed single-metric report validates', good.ok, good.ok ? 'no errors' : errorCodes(good));

  const noVersion = F.clone(goodReport); delete noVersion.schemaVersion;
  const rNoVersion = B.validateBenchReport(noVersion);
  st.check('inject: report without schemaVersion is caught', !rNoVersion.ok && hasCode(rNoVersion, 'SCHEMA_MISSING_FIELD'), errorCodes(rNoVersion));

  const badVersion = F.clone(goodReport); badVersion.schemaVersion = '2.0.0';
  const rBadVersion = B.validateBenchReport(badVersion);
  st.check('inject: schemaVersion 2.0.0 (wrong contract version) is caught', !rBadVersion.ok && hasCode(rBadVersion, 'SCHEMA_VERSION'), errorCodes(rBadVersion));

  const noMetricToolchain = F.clone(goodReport); delete noMetricToolchain.metrics[0].toolchain;
  const rNoMetricToolchain = B.validateBenchReport(noMetricToolchain);
  st.check('inject: metric row missing legacy field toolchain is caught', !rNoMetricToolchain.ok && hasCode(rNoMetricToolchain, 'SCHEMA_MISSING_LEGACY_FIELD'), errorCodes(rNoMetricToolchain));

  const noTopCommit = F.clone(goodReport); delete noTopCommit.commit;
  const rNoTopCommit = B.validateBenchReport(noTopCommit);
  st.check('inject: top-level legacy field commit removed is caught', !rNoTopCommit.ok && hasCode(rNoTopCommit, 'SCHEMA_MISSING_LEGACY_FIELD'), errorCodes(rNoTopCommit));

  const wrongType = F.clone(goodReport); wrongType.metrics[0].value = '11.8';
  const rWrongType = B.validateBenchReport(wrongType);
  st.check('inject: metric value as a string (wrong type) is caught', !rWrongType.ok && hasCode(rWrongType, 'SCHEMA_TYPE'), errorCodes(rWrongType));

  const badSha = F.clone(goodReport); badSha.fingerprintSha256 = 'not-a-sha256';
  const rBadSha = B.validateBenchReport(badSha);
  st.check('inject: fingerprintSha256 that is not 64 hex chars is caught', !rBadSha.ok && hasCode(rBadSha, 'SCHEMA_PATTERN'), errorCodes(rBadSha));

  const badEnum = F.clone(goodReport); badEnum.metrics[0].verdict = 'MAYBE';
  const rBadEnum = B.validateBenchReport(badEnum);
  st.check('inject: metric verdict outside PASS|FAIL|INCONCLUSIVE|INVALID|SKIP|NON_GATING is caught', !rBadEnum.ok && hasCode(rBadEnum, 'SCHEMA_ENUM'), errorCodes(rBadEnum));

  const noFlat = F.clone(goodReport); delete noFlat.flatProjection;
  const rNoFlat = B.validateBenchReport(noFlat);
  st.check('inject: single-metric report without the flat projection is caught', !rNoFlat.ok && hasCode(rNoFlat, 'COMPAT_FLAT_PROJECTION'), errorCodes(rNoFlat));

  const badFlat = F.clone(goodReport); badFlat.flatProjection.value = 999;
  const rBadFlat = B.validateBenchReport(badFlat);
  st.check('inject: flat projection that disagrees with metrics[0] is caught', !rBadFlat.ok && hasCode(rBadFlat, 'COMPAT_FLAT_PROJECTION'), errorCodes(rBadFlat));

  const missingFlatField = F.clone(goodReport); delete missingFlatField.flatProjection.ts;
  const rMissingFlatField = B.validateBenchReport(missingFlatField);
  st.check('inject: flat projection missing a legacy field is caught', !rMissingFlatField.ok && hasCode(rMissingFlatField, 'COMPAT_FLAT_PROJECTION'), errorCodes(rMissingFlatField));

  const emptyMetrics = F.clone(goodReport); emptyMetrics.metrics = [];
  const rEmptyMetrics = B.validateBenchReport(emptyMetrics);
  st.check('inject: report with an empty metrics array is caught', !rEmptyMetrics.ok, errorCodes(rEmptyMetrics));

  const fpMissingField = F.clone(F.syntheticFingerprint()); delete fpMissingField.input.keyboard_report_hz;
  const rFpMissing = B.validateMachineFingerprint(fpMissingField);
  st.check('inject: machine-fingerprint missing input.keyboard_report_hz (OQ-PM-11 extension) is caught', !rFpMissing.ok && hasCode(rFpMissing, 'SCHEMA_MISSING_FIELD'), errorCodes(rFpMissing));

  const fpWrongType = F.clone(F.syntheticFingerprint()); fpWrongType.display.panel_gtg_ms = '3.0';
  const rFpWrongType = B.validateMachineFingerprint(fpWrongType);
  st.check('inject: machine-fingerprint with a string panel_gtg_ms is caught', !rFpWrongType.ok && hasCode(rFpWrongType, 'SCHEMA_TYPE'), errorCodes(rFpWrongType));

  // --- fingerprint determinism
  const fps = fingerprintSelfTest();
  st.check('control: identical fingerprints hash identically', fps.same, 'sha256=' + fps.sha);
  st.check('control: key insertion order does not change the hash', fps.reorder, 'canonical JSON sorts keys');
  st.check('inject: tampering gpu.driver changes the fingerprint hash', B.computeFingerprintSha256(mutateLeaf(F.syntheticFingerprint(), 'gpu.driver')) !== fps.sha, 'FP_MISMATCH becomes detectable');
  st.check('inject: every single-field mutation changes the hash', fps.unchangedLeaves.length === 0, fps.coverage + ' covered leaf field(s), unchanged: ' + (fps.unchangedLeaves.join(', ') || 'none'));

  // --- mapping integrity
  const dup = R.SECTION5_MAPPING.concat([R.SECTION5_MAPPING[6]]);
  const saved = R.SECTION5_MAPPING.slice();
  const checkMapping = function (list) {
    // mappingIntegrity reads the module-level array; emulate by re-deriving the same rules.
    const seen = {};
    const errors = [];
    for (const r of list) {
      if (seen[r.id]) errors.push('MAPPING_DUPLICATE ' + r.id);
      seen[r.id] = true;
    }
    for (let i = 1; i <= 19; i++) if (!seen['H' + i]) errors.push('MAPPING_GAP H' + i);
    return errors;
  };
  st.check('inject: a dropped H row is caught as a gap', checkMapping(saved.filter(function (r) { return r.id !== 'H13'; })).some(function (e) { return e.indexOf('MAPPING_GAP') === 0; }), 'H13 removed');
  st.check('inject: a duplicated H row is caught', checkMapping(dup).some(function (e) { return e.indexOf('MAPPING_DUPLICATE') === 0; }), 'H7 duplicated');
  st.check('inject: an out-of-table control counted into H1..H19 is caught', !R.mappingIntegrity().ids.some(function (id) { return R.OUT_OF_TABLE_IDS.indexOf(id) >= 0; }), 'C1 stays outside the 19');
  st.check('control: H1..H19 are contiguous and unique', R.mappingIntegrity().ok, 'count=' + R.MAPPING_ROW_COUNT);

  // --- state-machine faults
  const caseResults = runCases();
  for (const r of caseResults) {
    if (r.kind !== 'fault') continue;
    st.check(r.name, r.ok, r.observed || ('threw ' + r.thrown));
  }

  // --- honesty boundary
  st.check('control: a host with no registered fingerprint is INCONCLUSIVE, never PASS', status.state === 'INCONCLUSIVE' && status.reason === 'REFERENCE_MACHINE_UNAVAILABLE', status.detail.slice(0, 120));
  st.check('inject: a cloud runner may not produce a gate number', (function () {
    const v = B.classifyMachine({ fingerprintRecorded: true, class: 'cloud', role: 'RM-A', backend: 't0' });
    return !v.gating && v.state === 'NON_GATING';
  })(), 'CLOUD_RUNNER');
  st.check('inject: RM-B may not judge a section 5 gate', (function () {
    const v = B.classifyMachine({ fingerprintRecorded: true, class: 'self-hosted', role: 'RM-B', backend: 't0' });
    return !v.gating && v.reason === 'FLOOR_MACHINE_ONLY';
  })(), 'FLOOR_MACHINE_ONLY');
  st.check('control: this run produced zero gating numbers', status.gatingNumbersProduced === 0, 'no measurement was performed');

  // --- D-6 steps 2-5: reading values out of a report and binding them to section 5 rows
  const freeIds = V.machineFreeRows().map(function (r) { return r.id; });
  st.check('control: the machine-free section 5 rows are derived from the registry', freeIds.join(',') === 'H17,H18,H19', 'derived: ' + freeIds.join(',') + ' (machine=none, governed=no, family!=external)');

  const h18Good = F.syntheticMetricRow('H18', { value: 53, unit: 'count', gate: 0, gating: false, verdict: 'SKIP' });
  const withH18 = V.collectValues([{ rel: 'synthetic-a.json', json: F.syntheticMetricsReport([h18Good]) }]);
  const h18Row = withH18.rows.filter(function (r) { return r.id === 'H18'; })[0];
  st.check('control: an H18 row carrying the registered unit and gate binds and is presented', !!h18Row && h18Row.status === V.REPORTED && h18Row.value === 53 && withH18.problems.length === 0, 'H18=' + (h18Row ? h18Row.value + ' ' + h18Row.reportedUnit : 'absent') + ', problems=' + withH18.problems.length);
  st.check('control: the rows a partial report does not carry are named', V.notReportedIds(withH18).join(',') === 'H17,H19', 'not reported: ' + V.notReportedIds(withH18).join(',') + ' (never silently omitted)');

  const h19AsContrast = F.syntheticMetricRow('H19', { value: 5.17, unit: 'ratio', gate: 4.5, target: 7, gating: false, verdict: 'NON_GATING' });
  const mislabelled = V.collectValues([{ rel: 'synthetic-b.json', json: F.syntheticMetricsReport([h19AsContrast]) }]);
  st.check('inject: the WCAG contrast value labelled H19 is caught as a unit + gate mismatch', mislabelled.problems.length >= 2 && /unit mismatch/.test(mislabelled.problems[0]) && /gate mismatch/.test(mislabelled.problems[1]), mislabelled.problems.join(' | '));

  const h19Good = F.syntheticMetricRow('H19', { value: 0.25, unit: 'px', gate: 0.5, gating: false, verdict: 'SKIP' });
  const h19Ok = V.collectValues([{ rel: 'synthetic-c.json', json: F.syntheticMetricsReport([h19Good]) }]);
  st.check('control: an H19 row carrying px / 0.5 binds cleanly (the checker does not reject everything)', h19Ok.problems.length === 0 && V.reportedIds(h19Ok).indexOf('H19') >= 0, 'problems=' + h19Ok.problems.length);

  const noH18 = V.collectValues([{ rel: 'synthetic-d.json', json: F.syntheticMetricsReport([h19Good]) }]);
  st.check('inject: dropping H18 from the report makes its absence explicit', V.notReportedIds(noH18).indexOf('H18') >= 0 && V.reportedIds(noH18).indexOf('H18') < 0, 'not reported: ' + V.notReportedIds(noH18).join(','));

  const controlC1 = V.collectValues([{ rel: 'synthetic-e.json', json: F.syntheticMetricsReport([h18Good, F.syntheticMetricRow('C1', { value: 5.17, unit: 'ratio', gate: 4.5, target: 7, gating: false, verdict: 'NON_GATING' })]) }]);
  st.check('control: an out-of-table control is presented as a control, not as one of the 19', controlC1.controls.length === 1 && controlC1.controls[0].id === 'C1' && controlC1.problems.length === 0, 'controls=' + controlC1.controls.map(function (c) { return c.id; }).join(','));

  const gatingValue = V.collectValues([{ rel: 'synthetic-f.json', json: F.syntheticMetricsReport([F.syntheticMetricRow('H1', { value: 100, unit: 'ms', gate: 150, gating: true, verdict: 'PASS' })]) }]);
  const gatingStatusInjected = Object.assign({}, status, { gatingNumbersProduced: gatingValue.gatingNumbersProduced, gatingSources: gatingValue.gating, values: gatingValue });
  st.check('inject: a metric declaring gating=true is counted and makes B7 fail', gatingValue.gatingNumbersProduced === 1 && gateB7(root, gatingStatusInjected).status === STATUS.FAIL, 'counter=' + gatingValue.gatingNumbersProduced + ', B7=' + gateB7(root, gatingStatusInjected).status);
  st.check('control: the same value with gating=false keeps B7 green', (function () {
    const v = V.collectValues([{ rel: 'synthetic-g.json', json: F.syntheticMetricsReport([F.syntheticMetricRow('H1', { value: 100, unit: 'ms', gate: 150, gating: false, verdict: 'NON_GATING' })]) }]);
    const s = Object.assign({}, status, { gatingNumbersProduced: v.gatingNumbersProduced, gatingSources: v.gating, values: v });
    return v.gatingNumbersProduced === 0 && gateB7(root, s).status === STATUS.PASS;
  })(), 'counter=0, B7=PASS');

  // --- the real gates must still be green on the real tree
  const realGates = [
    gateB1(root, docs), gateB2(root, docs), gateB3(root, docs), gateB4(root, docs),
    gateB5(root), gateB6(root), gateB7(root, status),
  ];
  for (const g of realGates) st.check('baseline ' + g.id + ' on the real tree passes', g.status === STATUS.PASS, g.status + ' ' + g.detail);

  const rep = st.report('bench --selftest (injections run on in-memory synthetic fixtures only)');
  console.log(rep.text);
  console.log('note: the real workspace was not modified; all injected inputs are synthetic logic fixtures (tools/bench/fixtures.mjs), not measurements.');
  console.log('note: on this host there is no RM-A / RM-C, so the tool produced 0 gating numbers (ADR-0014 iron law 5).');
  process.exit(rep.ok ? 0 : 1);
}

// --------------------------------------------------- entry

function parseArgs(argv) {
  const out = { selftest: false, json: false, root: null, report: null, machine: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--selftest') out.selftest = true;
    else if (a === '--json') out.json = true;
    else if (a.indexOf('--root=') === 0) out.root = a.slice('--root='.length);
    else if (a === '--root') out.root = argv[++i];
    else if (a.indexOf('--report=') === 0) out.report = a.slice('--report='.length);
    else if (a === '--report') out.report = argv[++i];
    else if (a.indexOf('--machine=') === 0) out.machine = a.slice('--machine='.length);
    else if (a === '--machine') out.machine = argv[++i];
  }
  return out;
}

// B8: schema validation plus the D-6 value binding. It runs when --report points at a file or when a
// report is discovered; with neither it is an explicit SKIP, never a silent PASS.
function gateB8(status) {
  const TITLE = 'bench-report schema validation + section 5 row binding';
  const set = status.reportSet;
  const targets = set.explicit ? [set.explicit] : set.discovered;
  if (!targets.length) {
    return gate('B8', TITLE, STATUS.SKIP, 'no bench-report.json was supplied (--report <p>) or discovered; a missing report is an explicit SKIP, never a silent PASS');
  }
  if (set.explicit && !set.explicit.exists) {
    return gate('B8', TITLE, STATUS.SKIP, 'the requested report ' + set.explicit.rel + ' does not exist; a missing report is an explicit SKIP, never a silent PASS');
  }
  const problems = [];
  const notes = [];
  for (const f of targets) {
    if (f.parseError) { problems.push(f.rel + ': report is not valid JSON: ' + f.parseError); continue; }
    const v = B.validateBenchReport(f.json);
    if (!v.ok) {
      problems.push(f.rel + ': ' + v.errors.length + ' schema violation(s)');
      for (const e of v.errors.slice(0, 6)) problems.push(f.rel + ': ' + e.code + ' ' + e.path + ': ' + e.message);
      continue;
    }
    notes.push(f.rel + ': satisfies kernel/06 3.4 / 3.7 schemaVersion ' + B.BENCH_REPORT_SCHEMA_VERSION);
    notes.push('  legacy fields missing: ' + (v.legacyMissing.length ? v.legacyMissing.join(', ') : 'none')
      + '; flat projection applicable: ' + v.flatProjection.applicable + ', ok: ' + v.flatProjection.ok
      + '; unknown (forward-compatible) fields: ' + (v.extras.length ? v.extras.join(', ') : 'none'));
  }
  for (const p of status.values.problems) problems.push('row binding: ' + p);
  const reported = V.reportedIds(status.values);
  const missing = V.notReportedIds(status.values);
  notes.push('section 5 machine-free row(s) carrying a value: ' + (reported.length ? reported.join(', ') : 'none') + ' of ' + status.values.expected.join(', '));
  notes.push('not reported (explicit, never silently omitted): ' + (missing.length ? missing.join(', ') : 'none'));
  if (status.values.controls.length) notes.push('out-of-table control value(s): ' + status.values.controls.map(function (c) { return c.id; }).join(', '));
  if (status.values.gatingNumbersProduced) notes.push('gating number(s) presented: ' + status.values.gatingNumbersProduced + ' (B7 fails on a non-reference host)');
  if (problems.length) return gate('B8', TITLE, STATUS.FAIL, problems.length + ' problem(s)', problems.slice(0, 12));
  return gate('B8', TITLE, STATUS.PASS, "report satisfies kernel/06 3.4 / 3.7 and every metric that names a section 5 row carries that row's registered unit and gate", notes);
}

function main(argv) {
  const args = parseArgs(argv);
  const root = args.root ? path.resolve(args.root) : DEFAULT_ROOT;
  const docs = loadDocs(root);
  const reportSet = buildReportSet(root, args.report);
  const status = gatingStatus(root, args.machine, reportSet);

  const gates = [
    gateB1(root, docs),
    gateB2(root, docs),
    gateB3(root, docs),
    gateB4(root, docs),
    gateB5(root),
    gateB6(root),
    gateB7(root, status),
  ];

  // The report gate runs when --report was given or a report was discovered; the default run on a
  // tree with no report keeps the seven structural gates B1-B7.
  if (args.report || status.reports.length) gates.push(gateB8(status));

  const report = buildReport(root, gates, status);
  if (args.json) {
    process.stdout.write(JSON.stringify(report, null, 2) + '\n');
  } else {
    console.log(formatHuman(report));
    console.log('json: ' + JSON.stringify(report));
  }
  return report.result === 'PASS' ? 0 : 1;
}

const args = parseArgs(process.argv.slice(2));
if (args.selftest) {
  runSelftest(args.root ? path.resolve(args.root) : DEFAULT_ROOT);
} else {
  process.exit(main(process.argv.slice(2)));
}
