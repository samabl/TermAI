// tools/bench/lib.mjs
// TermAI performance-measurement methodology library (zero external dependencies, Node >= 22).
//
// Authority (AGENTS.md section 1 / section 7):
//   docs/spec/kernel/06-performance-methodology.md  -- "how to measure / how to judge /
//                                                      what to do when it is not trustworthy"
//   HARNESS.md section 5 (the 19-row budget table) + section 2 AR-19 / AR-24 / AR-27 /
//   AR-30 / AR-31, ADR-0014 (reference machines RM-A / RM-B / RM-C, T0 backend).
//
// This file encodes NO threshold of its own. Every number below is a transcription of
// HARNESS section 5 / kernel/06, and tools/bench/check.mjs re-derives the transcription from
// the documents themselves (gates B1/B2/B3), so a typo or a spec edit is caught mechanically
// instead of being trusted.
//
// Honesty boundary (AR-20 / ADR-0014 iron law 5): this host has no registered RM-A / RM-C
// fingerprint, so nothing computed here is a gate number. evaluate() keeps kernel/06 section
// 3.1's order exactly and returns NON_GATING / INCONCLUSIVE -- never PASS / FAIL -- unless the
// caller supplies a self-hosted T0 run bound to a registered reference machine.

import crypto from 'node:crypto';

// --------------------------------------------------- versions / enums

export const BENCH_REPORT_SCHEMA_VERSION = '1.0.0';

// kernel/06 section 3.1 Verdict: PASS | FAIL | INCONCLUSIVE | INVALID | SKIP | NON_GATING.
// kernel/06 section 3.7 writes the *report* verdict as the four core states
// ("PASS|FAIL|INCONCLUSIVE|INVALID"); a metric row may additionally carry the two terminal
// short-circuits, hence the superset below (documented, not invented).
export const METRIC_VERDICTS = ['PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID', 'SKIP', 'NON_GATING'];
export const REPORT_VERDICTS = ['PASS', 'FAIL', 'INCONCLUSIVE', 'INVALID'];

// kernel/06 section 4 FailKind { RegressionPr, RegressionRel, BackendDegraded, CorpusDrift,
// SchemaViolation }. GATE_BREACH is an extension: section 3.1's evaluate() returns FAIL(v, gate)
// without a kind, and section 4 has no member for an absolute gate breach.
export const SPEC_FAIL_KINDS = ['REGRESSION_PR', 'REGRESSION_REL', 'BACKEND_DEGRADED', 'CORPUS_DRIFT', 'SCHEMA_VIOLATION'];
export const EXTENSION_FAIL_KINDS = ['GATE_BREACH'];

// Reason codes. "spec" = written in kernel/06; "extension" = derived from ADR-0014 / AR-31
// but not literally present in kernel/06's state machine; the tool flags them in its output
// so the spec owner can fold them back in.
export const REASONS = {
  FP_MISSING: { verdict: 'INVALID', origin: 'spec', where: 'kernel/06 3.1' },
  FP_MISMATCH: { verdict: 'INVALID', origin: 'spec', where: 'kernel/06 3.1 / ADR-0014 iron law 2' },
  BACKEND_DEGRADED: { verdict: 'NON_GATING', origin: 'spec', where: 'kernel/06 3.1 / ADR-0014 iron law 1' },
  SCENE_NOT_FROZEN: { verdict: 'INVALID', origin: 'spec', where: 'kernel/06 3.1 / 3.6 D0' },
  MEASUREMENT_UNSTABLE: { verdict: 'INVALID', origin: 'spec', where: 'kernel/06 3.1 / 3.6 D1 / AR-27' },
  INSUFFICIENT_RUNS: { verdict: 'INVALID', origin: 'spec', where: 'kernel/06 3.1 / spec 07 3.8.3-1' },
  INSUFFICIENT_SAMPLES: { verdict: 'INVALID', origin: 'spec', where: 'kernel/06 3.1 K-03' },
  CORPUS_DRIFT: { verdict: 'INVALID', origin: 'spec', where: 'kernel/06 3.6 / 3.7' },
  NOISE_FLOOR: { verdict: 'INCONCLUSIVE', origin: 'spec', where: 'kernel/06 3.1 K-04' },
  SUSPECT_SINGLE_NIGHT: { verdict: 'INCONCLUSIVE', origin: 'spec', where: 'kernel/06 3.1 / 3.5' },
  NO_BASELINE: { verdict: 'SKIP', origin: 'spec', where: 'kernel/06 3.6 (missing baseline)' },
  NO_HOTPATH: { verdict: 'SKIP', origin: 'spec', where: 'AR-31 #8' },
  CLOUD_RUNNER: { verdict: 'NON_GATING', origin: 'extension', where: 'ADR-0014 iron law 5' },
  FLOOR_MACHINE_ONLY: { verdict: 'NON_GATING', origin: 'extension', where: 'ADR-0014 decision 1 / spec 07 3.8.1 (RM-B judges F1-F6 only)' },
  NOT_MACHINE_GATE: { verdict: 'SKIP', origin: 'extension', where: 'kernel/06 3.8 / 3.9 (carrier is tokens/design-gates/server/cost)' },
  REFERENCE_MACHINE_UNAVAILABLE: { verdict: 'INCONCLUSIVE', origin: 'spec', where: 'kernel/06 6 / ADR-0014 iron law 5' },
};

export const DEFAULT_MIN_RUNS = 10;          // kernel/06 3.1 + spec 07 3.8.3-1
export const TAIL_MIN_SAMPLES = 1e5;         // kernel/06 3.1 (K-03)
export const REGRESSION_LIMIT = 0.05;        // HARNESS 8.1-4 / AR-27 (G4-PR single run > 5%)
export const CONSECUTIVE_NIGHTS_REL = 2;     // spec 07 3.8.3-3 / AR-27 (G4-REL)

// OQ-PM-07, decided by AR-31 item 9: MAD grading by metric family
// (frame 5% / startup 3% / throughput 2% / RSS 1%). kernel/06 3.6's eps_self column carries the
// same grading (frame-time P99 5% / startup P95 3% / throughput 2% / RSS 1% / package 0), so one
// policy table serves both the noise floor and the double-run tolerance.
// "exact" covers kernel/06 3.6 rows that are byte-equal by construction (scene fingerprint /
// grid hash 0, package bytes 0) plus the =0 structural assertions. "default" is kernel/06 3.1's
// base 2% for metrics OQ-PM-07 does not enumerate. "external" is not a machine gate at all.
export const METRIC_FAMILY_POLICY = {
  startup:    { madMaxPct: 0.03, epsSelfPct: 0.03, source: 'AR-31 #9 (OQ-PM-07) + kernel/06 3.6' },
  frame:      { madMaxPct: 0.05, epsSelfPct: 0.05, source: 'AR-31 #9 (OQ-PM-07) + kernel/06 3.6' },
  throughput: { madMaxPct: 0.02, epsSelfPct: 0.02, source: 'AR-31 #9 (OQ-PM-07) + kernel/06 3.6' },
  rss:        { madMaxPct: 0.01, epsSelfPct: 0.01, source: 'AR-31 #9 (OQ-PM-07) + kernel/06 3.6' },
  exact:      { madMaxPct: 0.0,  epsSelfPct: 0.0,  source: 'kernel/06 3.6 (scene fingerprint / grid hash = 0, package bytes = 0)' },
  default:    { madMaxPct: 0.02, epsSelfPct: 0.02, source: 'kernel/06 3.1 base limit 0.02 (families outside OQ-PM-07)' },
  external:   { madMaxPct: null, epsSelfPct: null, source: 'kernel/06 3.8 (not a machine gate)' },
};

export function familyPolicy(family) {
  const p = METRIC_FAMILY_POLICY[family];
  if (!p) throw new Error('unknown metric family: ' + family);
  return p;
}
export function madLimitFor(family) { return familyPolicy(family).madMaxPct; }
export function epsSelfFor(family) { return familyPolicy(family).epsSelfPct; }

// kernel/06 3.6 states eps_self = max(noise floor, 2 x MAD-over-median limit). The document only
// tabulates the resolved per-family value; this helper exposes the formula for callers that have
// measured an explicit noise floor. It is NOT used to override the tabulated values.
export function epsSelfFromFormula(noiseFloorPct, madLimitPct) {
  return Math.max(Number(noiseFloorPct) || 0, 2 * (Number(madLimitPct) || 0));
}

// --------------------------------------------------- hashing / canonical JSON

export function sha256Hex(text) {
  return crypto.createHash('sha256').update(text, 'utf8').digest('hex');
}

// Deterministic serialisation: object keys sorted by code unit, numbers via JSON.stringify
// (ECMAScript number-to-string is deterministic). Same input -> same bytes -> same hash.
export function canonicalJson(value) {
  if (value === null) return 'null';
  const t = typeof value;
  if (t === 'number') {
    if (!Number.isFinite(value)) throw new Error('canonicalJson: non-finite number');
    return JSON.stringify(value);
  }
  if (t === 'string' || t === 'boolean') return JSON.stringify(value);
  if (Array.isArray(value)) return '[' + value.map(canonicalJson).join(',') + ']';
  if (t === 'object') {
    const keys = Object.keys(value).sort();
    return '{' + keys.map(function (k) { return JSON.stringify(k) + ':' + canonicalJson(value[k]); }).join(',') + '}';
  }
  throw new Error('canonicalJson: unsupported type ' + t);
}

// Leaf paths of a fingerprint, sorted. Reported so the hash coverage is auditable
// (kernel/06 3.4 + OQ-PM-11 adopted default: EDID / font-set hash / clock source / input
// report rate all participate).
export function leafPaths(value, prefix) {
  const base = prefix || '';
  if (value === null || typeof value !== 'object') return [base];
  if (Array.isArray(value)) {
    const out = [];
    value.forEach(function (v, i) { out.push.apply(out, leafPaths(v, base + '[' + i + ']')); });
    return out;
  }
  const out = [];
  for (const k of Object.keys(value).sort()) out.push.apply(out, leafPaths(value[k], base ? base + '.' + k : k));
  return out;
}

// machine-fingerprint.json -> stable sha256 (kernel/06 3.4 + 3.7 fingerprints/<machine-id>.json).
// Every field present in the section 3.4 object participates, so any single-field change changes
// the hash ("any field change triggers a baseline reset").
export function computeFingerprintSha256(fingerprint) {
  const env = validateMachineFingerprint(fingerprint);
  if (!env.ok) {
    const e = new Error('machine-fingerprint does not satisfy kernel/06 section 3.4');
    e.errors = env.errors;
    throw e;
  }
  return sha256Hex(canonicalJson(fingerprint));
}

export function fingerprintCoverage(fingerprint) {
  return leafPaths(fingerprint, '').sort();
}

// --------------------------------------------------- minimal schema engine

const S = {
  string: { kind: 'string' },
  number: { kind: 'number' },
  integer: { kind: 'integer' },
  boolean: { kind: 'boolean' },
  stringArray: { kind: 'array', item: { kind: 'string' } },
  enumOf: function (values) { return { kind: 'enum', values: values }; },
  nullable: function (inner) { return { kind: 'nullable', inner: inner }; },
  pattern: function (re, inner) { return { kind: 'pattern', re: re, inner: inner }; },
  object: function (fields) { return { kind: 'object', fields: fields }; },
  arrayOf: function (item, minItems) { return { kind: 'array', item: item, minItems: minItems }; },
};

export function schemaKeys(node, prefix) {
  const base = prefix || '';
  if (node.kind === 'object') {
    const out = [];
    for (const k of Object.keys(node.fields)) {
      out.push(base ? base + '.' + k : k);
      out.push.apply(out, schemaKeys(node.fields[k], base ? base + '.' + k : k));
    }
    return out;
  }
  if (node.kind === 'array') return schemaKeys(node.item, base);
  if (node.kind === 'nullable' || node.kind === 'pattern') return node.inner ? schemaKeys(node.inner, base) : [];
  return [];
}

function typeName(v) {
  if (v === null) return 'null';
  if (Array.isArray(v)) return 'array';
  return typeof v;
}

function checkNode(value, node, path, errors, opts) {
  const push = function (code, expected, actual) {
    errors.push({ code: code, path: path, expected: expected, actual: actual, message: path + ': expected ' + expected + ', got ' + actual });
  };
  switch (node.kind) {
    case 'string':
      if (typeof value !== 'string') push('SCHEMA_TYPE', 'string', typeName(value));
      return;
    case 'number':
      if (typeof value !== 'number' || !Number.isFinite(value)) push('SCHEMA_TYPE', 'finite number', typeName(value));
      return;
    case 'integer':
      if (typeof value !== 'number' || !Number.isInteger(value)) push('SCHEMA_TYPE', 'integer', typeName(value));
      return;
    case 'boolean':
      if (typeof value !== 'boolean') push('SCHEMA_TYPE', 'boolean', typeName(value));
      return;
    case 'enum':
      if (node.values.indexOf(value) < 0) push('SCHEMA_ENUM', node.values.join('|'), JSON.stringify(value));
      return;
    case 'nullable':
      if (value === null) return;
      checkNode(value, node.inner, path, errors, opts);
      return;
    case 'pattern':
      if (typeof value !== 'string') { push('SCHEMA_TYPE', 'string', typeName(value)); return; }
      if (!node.re.test(value)) push('SCHEMA_PATTERN', String(node.re), JSON.stringify(value));
      return;
    case 'array':
      if (!Array.isArray(value)) { push('SCHEMA_TYPE', 'array', typeName(value)); return; }
      if (typeof node.minItems === 'number' && value.length < node.minItems) {
        push('SCHEMA_MIN_ITEMS', '>= ' + node.minItems + ' item(s)', value.length);
      }
      value.forEach(function (v, i) { checkNode(v, node.item, path + '[' + i + ']', errors, opts); });
      return;
    case 'object': {
      if (value === null || typeof value !== 'object' || Array.isArray(value)) {
        push('SCHEMA_TYPE', 'object', typeName(value));
        return;
      }
      const declared = Object.keys(node.fields);
      for (const k of declared) {
        const childPath = path ? path + '.' + k : k;
        if (!Object.prototype.hasOwnProperty.call(value, k)) {
          errors.push({
            code: opts && opts.codeFor ? opts.codeFor(childPath) : 'SCHEMA_MISSING_FIELD',
            path: childPath,
            expected: node.fields[k].kind,
            actual: 'absent',
            message: childPath + ': required field is absent',
          });
          continue;
        }
        checkNode(value[k], node.fields[k], childPath, errors, opts);
      }
      // Forward compatibility: kernel/06 3.7 allows new fields for 1 minor. Unknown keys are
      // collected as notes, never as errors.
      if (opts && opts.extraSink) {
        for (const k of Object.keys(value)) {
          if (declared.indexOf(k) < 0) opts.extraSink.push(path ? path + '.' + k : k);
        }
      }
      return;
    }
    default:
      throw new Error('unknown schema node kind: ' + node.kind);
  }
}

// --------------------------------------------------- bench-report.json schema (kernel/06 3.7)

// The 9 legacy fields: kernel/06 3.7 lists eight metric-level names and the top-level "commit".
// They must survive verbatim (spec 07 4.1 field stability) while new fields stay additive.
export const LEGACY_METRIC_FIELDS = ['metric', 'value', 'unit', 'samples', 'runner', 'commit', 'toolchain', 'ts'];
export const LEGACY_TOPLEVEL_FIELDS = ['commit'];
export const LEGACY_FIELD_COUNT = LEGACY_METRIC_FIELDS.length + LEGACY_TOPLEVEL_FIELDS.length; // 9

export const METRIC_SCHEMA = S.object({
  metric: S.string,
  value: S.number,
  unit: S.string,
  samples: S.integer,
  runner: S.string,
  commit: S.string,
  toolchain: S.string,
  ts: S.string,
  statistic: S.string,
  runs: S.integer,
  runStat: S.string,
  madOverMedian: S.number,
  gate: S.number,
  target: S.nullable(S.number),          // HARNESS section 5 "target" column may be a dash
  gating: S.boolean,
  verdict: S.enumOf(METRIC_VERDICTS),
  method: S.string,
  dCalibrationMs: S.nullable(S.number),  // only meaningful for latency; null elsewhere
  artifacts: S.stringArray,
});

export const BENCH_REPORT_SCHEMA = S.object({
  schemaVersion: S.pattern(/^\d+\.\d+\.\d+$/, null),
  reportId: S.string,
  generatedAt: S.string,
  commit: S.object({ sha: S.string, dirty: S.boolean }),
  toolchain: S.object({ rustc: S.string, channel: S.string }),
  runner: S.object({ class: S.string, machineId: S.string, role: S.string, backend: S.string }),
  fingerprintSha256: S.pattern(/^[0-9a-f]{64}$/, null),
  environment: S.object({ powerPlan: S.string, exclusive: S.boolean, warmed: S.integer, valid: S.boolean }),
  selfcheck: S.object({
    scene: S.object({ verdict: S.enumOf(REPORT_VERDICTS), frames: S.integer, hash: S.string }),
    doubleRun: S.object({ verdict: S.enumOf(REPORT_VERDICTS), deltaPct: S.number }),
    verdict: S.enumOf(REPORT_VERDICTS),
  }),
  corpus: S.object({ manifestSha256: S.string, shardsOk: S.integer, bytes: S.integer }),
  metrics: S.arrayOf(METRIC_SCHEMA, 1),
  verdict: S.enumOf(REPORT_VERDICTS),
  cost: S.object({ minutes: S.number, runnerClass: S.string, estUsd: S.number }),
});

export const MACHINE_FINGERPRINT_SCHEMA = S.object({
  id: S.string,
  role: S.string,
  cpu: S.object({ model: S.string, microcode: S.string, cores: S.integer, threads: S.integer, r23_single: S.number, r23_multi: S.number }),
  mem: S.object({ size_gb: S.number, modules: S.string, speed: S.string, timings: S.string }),
  gpu: S.object({ model: S.string, driver: S.string, api: S.string, fl: S.string }),
  display: S.object({ model: S.string, edid_sha256: S.string, mode: S.string, vrr: S.boolean, hdr: S.boolean, dpi_scale: S.number, panel_gtg_ms: S.number }),
  storage: S.object({ model: S.string, fw: S.string, seq_read_mbps: S.number }),
  os: S.object({ name: S.string, build: S.string, arch: S.string }),
  power: S.object({ plan: S.string, governor: S.string, app_nap: S.boolean }),
  clocks: S.object({ source: S.string, qpc_freq_hz: S.number }),
  fonts: S.object({ set_sha256: S.string, render_backend: S.string }),
  input: S.object({ inject: S.string, keyboard_report_hz: S.number }),
  hypervisor: S.boolean,
  updated_at: S.string,
  superseded_by: S.nullable(S.string),
});

function legacyCodeFor(path) {
  for (const f of LEGACY_TOPLEVEL_FIELDS) if (path === f) return 'SCHEMA_MISSING_LEGACY_FIELD';
  for (const f of LEGACY_METRIC_FIELDS) if (/^metrics\[\d+\]\./.test(path) && path.endsWith('.' + f)) return 'SCHEMA_MISSING_LEGACY_FIELD';
  return 'SCHEMA_MISSING_FIELD';
}

// A single-metric run must additionally emit the legacy flat projection (kernel/06 3.7:
// "single-metric calls additionally emit a top-level flat projection"; spec 07 3.8.2 defines the
// flat legacy object as {metric, value, unit, samples, runner, commit, toolchain, ts}).
//
// FLAGGED SPEC AMBIGUITY (do not silently paper over): three of those eight legacy names --
// commit, toolchain and runner -- are ALSO top-level kernel/06 3.7 fields, and there they are
// objects ({sha,dirty} / {rustc,channel} / {class,machineId,role,backend}). Merging the flat
// projection into the top level would therefore contradict the same section's schema, so the tool
// carries it as ONE additive field, "flatProjection": kernel/06 3.7 permits additive fields to be
// backward compatible for 1 minor, and this keeps both contracts satisfiable at once. The container
// is declared here as a known additive field so gate B2 can still require the *contract* field set
// to match the spec character-for-character. Owner action: fold this decision (or an alternative)
// back into kernel/06 3.7 / spec 07 3.8.2.
export const FLAT_PROJECTION_CONTAINER = 'flatProjection';
export const FLAT_PROJECTION_FIELDS = LEGACY_METRIC_FIELDS.slice(); // metric, value, unit, samples, runner, commit, toolchain, ts
export const KNOWN_ADDITIVE_TOPLEVEL_FIELDS = [FLAT_PROJECTION_CONTAINER];

export const FLAT_PROJECTION_SCHEMA = S.object({
  metric: S.string,
  value: S.number,
  unit: S.string,
  samples: S.integer,
  runner: S.string,
  commit: S.string,
  toolchain: S.string,
  ts: S.string,
});

export function validateFlatProjection(report) {
  const errors = [];
  if (!report || !Array.isArray(report.metrics) || report.metrics.length !== 1) {
    return { applicable: false, ok: true, errors: errors };
  }
  const m = report.metrics[0];
  const container = report[FLAT_PROJECTION_CONTAINER];
  if (container === undefined) {
    errors.push({
      code: 'COMPAT_FLAT_PROJECTION',
      path: FLAT_PROJECTION_CONTAINER,
      expected: 'object with ' + FLAT_PROJECTION_FIELDS.join(', '),
      actual: 'absent',
      message: 'single-metric report is missing the legacy flat projection (' + FLAT_PROJECTION_CONTAINER + '); the top-level names commit/toolchain/runner already carry the structured 1.0.0 shape (kernel/06 3.7 / spec 07 3.8.2)',
    });
    return { applicable: true, ok: false, errors: errors };
  }
  checkNode(container, FLAT_PROJECTION_SCHEMA, FLAT_PROJECTION_CONTAINER, errors, { codeFor: function () { return 'COMPAT_FLAT_PROJECTION'; } });
  if (container !== null && typeof container === 'object' && !Array.isArray(container)) {
    for (const f of FLAT_PROJECTION_FIELDS) {
      if (!Object.prototype.hasOwnProperty.call(container, f)) continue;
      if (container[f] !== m[f]) {
        errors.push({
          code: 'COMPAT_FLAT_PROJECTION',
          path: FLAT_PROJECTION_CONTAINER + '.' + f,
          expected: JSON.stringify(m[f]),
          actual: JSON.stringify(container[f]),
          message: 'flat projection field "' + f + '" does not equal metrics[0].' + f,
        });
      }
    }
  }
  return { applicable: true, ok: errors.length === 0, errors: errors };
}

export function validateBenchReport(report) {
  const errors = [];
  const extras = [];
  if (report === null || typeof report !== 'object' || Array.isArray(report)) {
    errors.push({ code: 'SCHEMA_TYPE', path: '', expected: 'object', actual: typeName(report), message: 'bench-report must be a JSON object' });
    return { ok: false, errors: errors, extras: extras, flatProjection: { applicable: false, ok: false, errors: errors }, legacyMissing: [] };
  }
  checkNode(report, BENCH_REPORT_SCHEMA, '', errors, { codeFor: legacyCodeFor, extraSink: extras });
  if (Object.prototype.hasOwnProperty.call(report, 'schemaVersion') && typeof report.schemaVersion === 'string' && report.schemaVersion !== BENCH_REPORT_SCHEMA_VERSION) {
    errors.push({
      code: 'SCHEMA_VERSION',
      path: 'schemaVersion',
      expected: BENCH_REPORT_SCHEMA_VERSION,
      actual: report.schemaVersion,
      message: 'schemaVersion "' + report.schemaVersion + '" is not the current contract version "' + BENCH_REPORT_SCHEMA_VERSION + '" (kernel/06 3.7)',
    });
  }
  const flat = validateFlatProjection(report);
  errors.push.apply(errors, flat.errors);
  const legacyMissing = errors.filter(function (e) { return e.code === 'SCHEMA_MISSING_LEGACY_FIELD'; }).map(function (e) { return e.path; });
  // Split forward-compatible additions into "declared by this tool" and "unknown to this tool".
  // Unknown keys are notes, never errors (kernel/06 3.7 allows additive fields for 1 minor).
  const declaredAdditive = extras.filter(function (k) { return KNOWN_ADDITIVE_TOPLEVEL_FIELDS.indexOf(k) >= 0; });
  const unknownAdditive = extras.filter(function (k) { return KNOWN_ADDITIVE_TOPLEVEL_FIELDS.indexOf(k) < 0; });
  return { ok: errors.length === 0, errors: errors, extras: unknownAdditive, declaredAdditive: declaredAdditive, flatProjection: flat, legacyMissing: legacyMissing };
}

export function validateMachineFingerprint(fp) {
  const errors = [];
  const extras = [];
  if (fp === null || typeof fp !== 'object' || Array.isArray(fp)) {
    errors.push({ code: 'SCHEMA_TYPE', path: '', expected: 'object', actual: typeName(fp), message: 'machine-fingerprint must be a JSON object' });
    return { ok: false, errors: errors, extras: extras };
  }
  checkNode(fp, MACHINE_FINGERPRINT_SCHEMA, '', errors, { codeFor: function () { return 'SCHEMA_MISSING_FIELD'; }, extraSink: extras });
  return { ok: errors.length === 0, errors: errors, extras: extras };
}

// --------------------------------------------------- statistics

export function median(values) {
  const a = values.slice().sort(function (x, y) { return x - y; });
  if (!a.length) throw new Error('median of empty list');
  const mid = a.length >> 1;
  return a.length % 2 ? a[mid] : (a[mid - 1] + a[mid]) / 2;
}

export function medianAbsoluteDeviation(values) {
  const m = median(values);
  return median(values.map(function (v) { return Math.abs(v - m); }));
}

// kernel/06 3.1: mad_over_median(runs). Computed across the valid run statistics (3.2 reads
// "the MAD/median of the Run-P95"), with an explicit override for callers that supply a value.
export function madOverMedian(runs) {
  const stats = runs.filter(function (r) { return r.valid !== false; }).map(function (r) { return r.stat; });
  if (!stats.length) return null;
  const m = median(stats);
  if (m === 0) return medianAbsoluteDeviation(stats) === 0 ? 0 : Infinity;
  return medianAbsoluteDeviation(stats) / m;
}

// kernel/06 3.1: v = median(run.stat) + calibration_delta(metric)
export function observedValue(runs, calibrationDelta) {
  const stats = runs.filter(function (r) { return r.valid !== false; }).map(function (r) { return r.stat; });
  return median(stats) + (calibrationDelta || 0);
}

// kernel/06 3.2 throughput row: "weighted harmonic mean by bytes". Equal shard weights are the
// frozen default (OQ-PM-08 / AR-31 #10); per-shard weights come from corpus.manifest.json.
export function weightedHarmonicMean(values, weights) {
  const w = weights && weights.length === values.length ? weights : values.map(function () { return 1; });
  let num = 0;
  let den = 0;
  for (let i = 0; i < values.length; i++) {
    if (values[i] === 0) return 0;
    num += w[i];
    den += w[i] / values[i];
  }
  return den === 0 ? 0 : num / den;
}

// --------------------------------------------------- section 3.1 state machine

function verdictOf(state, extra) {
  const v = { state: state };
  for (const k of Object.keys(extra || {})) v[k] = extra[k];
  return v;
}

export function tolerant(reason, detail) {
  const r = REASONS[reason];
  if (!r) throw new Error('unknown verdict reason: ' + reason);
  return verdictOf(r.verdict, { reason: reason, origin: r.origin, where: r.where, detail: detail || '' });
}

export function passesGate(op, value, gate) {
  if (op === '<') return value < gate;
  if (op === '<=') return value <= gate;
  if (op === '>') return value > gate;
  if (op === '>=') return value >= gate;
  if (op === '=') return value === gate;
  throw new Error('unknown gate operator: ' + String(op));
}

// evaluate(): kernel/06 section 3.1, step order preserved exactly.
//   1 !fingerprint.recorded               -> INVALID(FP_MISSING)
//   2 fingerprint != machine.now()        -> INVALID(FP_MISMATCH)
//   3 backend != T0                       -> NON_GATING(BACKEND_DEGRADED)
//   4 !scene_frozen()                     -> INVALID(SCENE_NOT_FROZEN)
//   5 !double_run_consistent(eps_self)    -> INVALID(MEASUREMENT_UNSTABLE)
//   6 runs.valid < 10                     -> INVALID(INSUFFICIENT_RUNS)
//   7 metric.is_tail && any(run.n < 1e5)  -> INVALID(INSUFFICIENT_SAMPLES)
//   8 mad_over_median(runs) > limit       -> INCONCLUSIVE(NOISE_FLOOR)   [AR-31 #9 grading]
//   9 v = median(run.stat) + calibration_delta; v <= gate ? PASS : FAIL
// Steps 3a/3b (cloud runner, RM-B floor machine) are ADR-0014 additions to step 3; they are
// reported with their own reason codes and never turn a degraded run into PASS/FAIL.
export function evaluate(input) {
  const metric = input.metric || {};
  const fp = input.fingerprint || {};
  const machine = input.machine || {};
  const runs = Array.isArray(input.runs) ? input.runs : [];

  if (metric.machine !== 'RM-A' && metric.machine !== 'RM-C') {
    return tolerant('NOT_MACHINE_GATE', 'metric "' + String(metric.id) + '" is not a reference-machine gate; carrier = ' + String(metric.carrier || 'unknown'));
  }
  if (!fp.recorded) return tolerant('FP_MISSING', 'no machine-fingerprint registered for this run (ADR-0014 iron law 2)');
  if (fp.sha256 !== machine.fingerprintSha256) {
    return tolerant('FP_MISMATCH', 'recorded fingerprint ' + String(fp.sha256) + ' != current ' + String(machine.fingerprintSha256) + ' (part swap / driver change -> baseline must be reset)');
  }
  if (machine.class && machine.class !== 'self-hosted') {
    return tolerant('CLOUD_RUNNER', 'runner class "' + machine.class + '" is not a self-hosted reference machine; cloud results are NON-GATING (ADR-0014 iron law 5)');
  }
  if (machine.role === 'RM-B') {
    return tolerant('FLOOR_MACHINE_ONLY', 'RM-B judges the F1-F6 floor thresholds only, never HARNESS section 5 gates (ADR-0014 decision 1 / spec 07 3.8.1)');
  }
  if (machine.backend !== 't0') {
    return tolerant('BACKEND_DEGRADED', 'backend "' + String(machine.backend) + '" is not T0 (ADR-0014 iron law 1)');
  }
  if (!input.scene || input.scene.frozen !== true) {
    return tolerant('SCENE_NOT_FROZEN', 'D0 failed: scene fingerprint is not byte-equal across two runs (grid_fingerprint / damage rect list)');
  }
  const eps = epsSelfFor(metric.family);
  const dr = input.doubleRun;
  if (dr && typeof dr.a === 'number' && typeof dr.b === 'number') {
    const denom = dr.a === 0 ? 1 : Math.abs(dr.a);
    const delta = Math.abs(dr.b - dr.a) / denom;
    if (!(delta <= eps)) {
      return tolerant('MEASUREMENT_UNSTABLE', 'D1 failed: consecutive runs differ by ' + (delta * 100).toFixed(3) + '% > eps_self ' + (eps * 100).toFixed(3) + '% for family "' + metric.family + '"');
    }
  } else if (input.doubleRun && input.doubleRun.consistent === false) {
    return tolerant('MEASUREMENT_UNSTABLE', 'D1 failed: caller reported inconsistent consecutive runs');
  }
  const valid = runs.filter(function (r) { return r.valid !== false; });
  const minRuns = typeof metric.minRuns === 'number' ? metric.minRuns : DEFAULT_MIN_RUNS;
  if (valid.length < minRuns) {
    return tolerant('INSUFFICIENT_RUNS', valid.length + ' valid run(s) < required ' + minRuns + ' (spec 07 3.8.3-1)');
  }
  if (metric.isTail) {
    const minSamples = typeof metric.minSamplesPerRun === 'number' ? metric.minSamplesPerRun : TAIL_MIN_SAMPLES;
    const thin = valid.filter(function (r) { return !(r.n >= minSamples); });
    if (thin.length) {
      return tolerant('INSUFFICIENT_SAMPLES', thin.length + ' tail run(s) carry fewer than ' + minSamples + ' samples; P99 is meaningless below that (K-03)');
    }
  }
  const limit = madLimitFor(metric.family);
  const mad = typeof input.madOverMedian === 'number' ? input.madOverMedian : madOverMedian(valid);
  if (limit !== null && mad !== null && mad > limit) {
    return tolerant('NOISE_FLOOR', 'MAD/median = ' + (mad * 100).toFixed(3) + '% > ' + (limit * 100).toFixed(3) + '% for family "' + metric.family + '" (' + familyPolicy(metric.family).source + '); fix the environment, do not judge');
  }
  const cal = typeof input.calibrationDelta === 'number' ? input.calibrationDelta : 0;
  const v = observedValue(valid, cal);
  const gate = metric.gate;
  const op = metric.gateOp || '<=';
  if (typeof gate !== 'number') {
    return tolerant('NOT_MACHINE_GATE', 'metric "' + String(metric.id) + '" has no numeric gate in HARNESS section 5');
  }
  const margin = (op === '>=' || op === '>') ? (v - gate) : (op === '=' ? 0 : (gate - v));
  if (passesGate(op, v, gate)) {
    return verdictOf('PASS', { value: v, gate: gate, gateOp: op, margin: margin, madOverMedian: mad, epsSelfPct: eps, madLimitPct: limit, runs: valid.length, calibrationDelta: cal });
  }
  return verdictOf('FAIL', { kind: 'GATE_BREACH', value: v, gate: gate, gateOp: op, delta: v - gate, deltaPct: gate === 0 ? null : (v - gate) / gate, madOverMedian: mad, runs: valid.length, calibrationDelta: cal });
}

// regression(): kernel/06 section 3.1 second block, G4-PR / G4-REL split (AR-27 / K-05).
//   d = (median_now - baseline.median) / baseline.median
//   d <= 0.05                             -> PASS
//   context == PR                         -> FAIL(REGRESSION_PR)
//   nights.consecutive >= 2 && d2 > 0.05  -> FAIL(REGRESSION_REL)
//   context == RC_FULL                    -> FAIL(REGRESSION_REL)
//   else                                  -> INCONCLUSIVE(SUSPECT_SINGLE_NIGHT)
export const REGRESSION_CONTEXTS = ['PR', 'NIGHTLY', 'RC_FULL'];

export function regression(input) {
  const baseline = input.baseline || {};
  const nights = input.nights || { consecutive: 0, latestDelta: null };
  const context = input.context || 'NIGHTLY';
  if (REGRESSION_CONTEXTS.indexOf(context) < 0) throw new Error('unknown regression context: ' + context);
  if (typeof baseline.median !== 'number') {
    return tolerant('NO_BASELINE', 'no baseline median for ' + String(input.metricId || 'metric') + '; regenerate with the kernel/06 3.4 step 3 command (baseline-mode=new)');
  }
  const medianNow = input.medianNow;
  const d = (medianNow - baseline.median) / baseline.median;
  const latest = typeof nights.latestDelta === 'number' ? nights.latestDelta : d;
  const base = { delta: d, deltaPct: d, baselineMedian: baseline.median, medianNow: medianNow, context: context, consecutiveNights: nights.consecutive || 0 };
  if (d <= REGRESSION_LIMIT) {
    return verdictOf('PASS', Object.assign(base, { margin: REGRESSION_LIMIT - d, marginPct: REGRESSION_LIMIT - d }));
  }
  if (context === 'PR') {
    return verdictOf('FAIL', Object.assign(base, { kind: 'REGRESSION_PR', gateName: 'G4-PR', detail: 'single valid run regressed ' + (d * 100).toFixed(2) + '% > 5%; merge blocking (HARNESS 8.1-4 / AR-27)' }));
  }
  if ((nights.consecutive || 0) >= CONSECUTIVE_NIGHTS_REL && latest > REGRESSION_LIMIT) {
    return verdictOf('FAIL', Object.assign(base, { kind: 'REGRESSION_REL', gateName: 'G4-REL', detail: 'regression reproduced on ' + nights.consecutive + ' consecutive nights; release blocking (spec 07 3.8.3-3 / AR-27)' }));
  }
  if (context === 'RC_FULL') {
    return verdictOf('FAIL', Object.assign(base, { kind: 'REGRESSION_REL', gateName: 'G4-REL', detail: 'pre-release full suite reproduced the regression once; release blocking (AR-27)' }));
  }
  return tolerant('SUSPECT_SINGLE_NIGHT', 'single night regressed ' + (d * 100).toFixed(2) + '%; not blocking until the 2nd consecutive night or the pre-release full run (AR-27)');
}

// gateAndRegression(): kernel/06 section 3.5 flow. INVALID / INCONCLUSIVE / SKIP / NON_GATING
// never reach the baseline comparison ("they do not enter the regression judgement").
export function gateAndRegression(input) {
  const ev = input.evaluation;
  if (ev.state !== 'PASS' && ev.state !== 'FAIL') {
    return verdictOf(ev.state, Object.assign({}, ev, { regressionSkipped: true, regressionSkipReason: 'measurement did not produce PASS/FAIL; baseline comparison is forbidden (kernel/06 3.5 / AR-27)' }));
  }
  if (ev.state === 'FAIL') return ev;
  if (!input.regressionInput) return ev;
  return regression(input.regressionInput);
}

// AR-27 / kernel/06 3.6 last paragraph: same commit, same fingerprint, two G4 runs -> per-metric
// verdict must not flip and abs(delta median) <= eps_self. A flip is INVALID(MEASUREMENT_UNSTABLE)
// and the run must not be compared with the baseline nor used to block anything.
export function assertReproducible(runA, runB) {
  const problems = [];
  if (!runA || !runB) return { ok: false, problems: [{ code: 'MEASUREMENT_UNSTABLE', detail: 'both runs are required' }], perMetric: [] };
  const a = runA.report || runA;
  const b = runB.report || runB;
  const same = function (x, y, label) {
    if (x !== y) problems.push({ code: 'PRECONDITION_MISMATCH', detail: label + ' differs between the two runs: ' + JSON.stringify(x) + ' != ' + JSON.stringify(y) });
  };
  same(a.commit && a.commit.sha, b.commit && b.commit.sha, 'commit.sha (AR-27 requires the same commit)');
  same(a.fingerprintSha256, b.fingerprintSha256, 'fingerprintSha256 (AR-27 requires the same machine fingerprint)');
  same(a.corpus && a.corpus.manifestSha256, b.corpus && b.corpus.manifestSha256, 'corpus.manifestSha256 (corpus drift is INVALID, kernel/06 3.6)');
  same(a.selfcheck && a.selfcheck.scene && a.selfcheck.scene.hash, b.selfcheck && b.selfcheck.scene && b.selfcheck.scene.hash, 'selfcheck.scene.hash (D0: the scene must stay frozen)');

  const mapB = {};
  ((b.metrics) || []).forEach(function (m) { mapB[m.metric] = m; });
  const perMetric = [];
  for (const ma of (a.metrics || [])) {
    const mb = mapB[ma.metric];
    const entry = { metric: ma.metric, verdictA: ma.verdict, verdictB: mb ? mb.verdict : null };
    if (!mb) {
      entry.state = 'INVALID';
      entry.reason = 'MEASUREMENT_UNSTABLE';
      entry.detail = 'metric present in run A but absent in run B';
      problems.push({ code: 'MEASUREMENT_UNSTABLE', metric: ma.metric, detail: entry.detail });
      perMetric.push(entry);
      continue;
    }
    if (mb.verdict !== ma.verdict) {
      entry.state = 'INVALID';
      entry.reason = 'MEASUREMENT_UNSTABLE';
      entry.detail = 'verdict flipped ' + ma.verdict + ' -> ' + mb.verdict + ' on the same commit and fingerprint (AR-27)';
      problems.push({ code: 'MEASUREMENT_UNSTABLE', metric: ma.metric, detail: entry.detail });
      perMetric.push(entry);
      continue;
    }
    const fam = ma.family || 'default';
    const eps = epsSelfFor(fam);
    const denom = ma.value === 0 ? 1 : Math.abs(ma.value);
    const delta = Math.abs(mb.value - ma.value) / denom;
    entry.deltaMedianPct = delta;
    entry.epsSelfPct = eps;
    if (!(delta <= eps)) {
      entry.state = 'INVALID';
      entry.reason = 'MEASUREMENT_UNSTABLE';
      entry.detail = 'abs(delta median) ' + (delta * 100).toFixed(3) + '% > eps_self ' + (eps * 100).toFixed(3) + '% for family "' + fam + '" (AR-27)';
      problems.push({ code: 'MEASUREMENT_UNSTABLE', metric: ma.metric, detail: entry.detail });
      perMetric.push(entry);
      continue;
    }
    entry.state = ma.verdict;
    perMetric.push(entry);
  }
  const topFlip = (a.verdict !== b.verdict);
  if (topFlip) problems.push({ code: 'MEASUREMENT_UNSTABLE', detail: 'top-level verdict flipped ' + a.verdict + ' -> ' + b.verdict });
  return { ok: problems.length === 0, problems: problems, perMetric: perMetric, topLevelFlip: topFlip };
}

// --------------------------------------------------- machine binding / honesty boundary

// ADR-0014: only RM-A (compute) and RM-C (display/latency) on the T0 backend can produce a gate
// number. Cloud runners, RM-B and degraded backends are NON-GATING; a missing reference machine is
// INCONCLUSIVE -- never a silent PASS.
export function classifyMachine(machine) {
  const m = machine || {};
  if (!m.fingerprintRecorded) {
    return { gating: false, state: 'INCONCLUSIVE', reason: 'REFERENCE_MACHINE_UNAVAILABLE', detail: 'no machine-fingerprint.json is registered for this host; gate items are INCONCLUSIVE and a cloud runner may not stand in (ADR-0014 iron law 5 / kernel/06 6)' };
  }
  if (m.class && m.class !== 'self-hosted') {
    return { gating: false, state: 'NON_GATING', reason: 'CLOUD_RUNNER', detail: 'runner class "' + m.class + '" is NON-GATING (ADR-0014 iron law 5)' };
  }
  if (m.role === 'RM-B') {
    return { gating: false, state: 'NON_GATING', reason: 'FLOOR_MACHINE_ONLY', detail: 'RM-B judges F1-F6 only (ADR-0014 decision 1)' };
  }
  if (m.role !== 'RM-A' && m.role !== 'RM-C') {
    return { gating: false, state: 'NON_GATING', reason: 'FLOOR_MACHINE_ONLY', detail: 'role "' + String(m.role) + '" is not a section 5 gating reference machine (RM-A / RM-C)' };
  }
  if (m.backend !== 't0') {
    return { gating: false, state: 'NON_GATING', reason: 'BACKEND_DEGRADED', detail: 'backend "' + String(m.backend) + '" is not T0 (ADR-0014 iron law 1)' };
  }
  return { gating: true, state: null, reason: null, detail: 'reference machine ' + m.role + ' on T0; gating verdicts are permitted' };
}
