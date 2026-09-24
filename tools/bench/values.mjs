// tools/bench/values.mjs
// D-6 steps 2-4 of docs/plan/p0-open-decisions.md: read the *values* out of a bench-report.json
// and present them, instead of only schema-validating the file.
//
// Scope: this file reads; it does not measure. Values are produced by the carriers named in the
// registry (tools/tokens gate 5 for H18, tools/design-gates for H17/H19) and written into a report
// by whoever runs them; --report then binds each metric back to its HARNESS section 5 row. That
// binding is mechanical -- a metric that names an H row must carry that row's unit and gate, which
// registry.mjs re-derives from the documents on every run (gates B1/B3) -- so a mislabelled value is
// caught here instead of being published. Round 255 found exactly such a value: the WCAG contrast
// ratio (an out-of-table control, C1) had been labelled H19, which is the grid-alignment row.
//
// Honesty boundary (AR-20 / ADR-0014 iron law 5): every value read here is NON_GATING on a host with
// no registered RM-A / RM-C fingerprint. A metric that declares gating=true is counted by
// gatingNumbersProduced and makes gate B7 fail; check.mjs owns that counter, this file only finds it.
// ADR-0029 D-5 narrows that counter: only a **machine-bound** gate number counts. H13 (package bytes,
// governed yes, machine none) is a release gate that no reference machine judges, so it may declare
// gating=true; H18 (governed no) is not a gate, so declaring gating=true there is a binding violation.

import * as R from './registry.mjs';
import * as REL from './reliability.mjs';

export const REPORTED = 'reported';
export const NOT_REPORTED = 'not_reported';

// The section 5 rows this host can supply without a registered reference machine: the registry says
// machine "none", the gate verdict is not judged by kernel/06 (governed "no"), and the carrier lives
// in this repository. The family "external" rows (H14 / H15 / H16) are server-side dashboards with no
// in-repo carrier, so they are excluded. The set is derived from the registry, never hard-coded.
//
// Registered ambiguity (SD-24): kernel/06 section 3.4 binds the *measurement* of grid alignment and
// the visual golden to RM-C, while the registry says machine "none" for H17 / H19 because their gate
// verdict carrier is tools/design-gates. This file follows the registry (the tool's own verified
// transcription); SD-24 records the conflict for the kernel/06 owner.
export function machineFreeRows() {
  return R.SECTION5_MAPPING.filter(function (r) {
    return r.machine === 'none' && r.governed === 'no' && r.family !== 'external';
  });
}

export function outOfTableControls() {
  return R.OUT_OF_TABLE_CONTROLS.slice();
}

export function registryRow(id) {
  const h = R.findMapping(id);
  if (h) return h;
  for (const c of R.OUT_OF_TABLE_CONTROLS) if (c.id === id) return c;
  return null;
}

// A row that carries the section 5 contract (unit + gate). The out-of-table control C1 carries
// neither, so it is presented without a binding check.
function isSection5Row(row) {
  return !!(row && typeof row.machine === 'string' && typeof row.unit === 'string');
}

// Literal contract comparison, not a judgement: for a metric that names a section 5 row, unit and
// gate must equal the registry's transcription of HARNESS section 5.
export function bindMetric(metric, row) {
  const problems = [];
  if (!isSection5Row(row)) return problems;
  if (metric.unit !== row.unit) {
    problems.push(row.id + ': unit mismatch -- HARNESS section 5 says "' + row.unit + '" but the report says "' + String(metric.unit) + '"');
  }
  if (metric.gate !== row.gate) {
    problems.push(row.id + ': gate mismatch -- the registry says ' + row.gate + ' but the report says ' + String(metric.gate));
  }
  // ADR-0029 D-5: a row that is NOT a gate (governed: no) must not declare gating=true; a gate row
  // that merely has no reference machine (H13: governed yes, machine none) may.
  if (row.machine === 'none' && row.governed === 'no' && metric.gating === true) {
    problems.push(row.id + ': the metric declares gating=true but this row is not a gate (registry governed: no); a value here must never be presented as a gate number');
  }
  return problems;
}

// reportFiles: [{ rel, json, parseError }]. Parse failures belong to B7 / B8, not here.
export function collectValues(reportFiles) {
  const files = (reportFiles || []).filter(function (f) { return f && f.json && !f.parseError; });
  const entries = [];
  for (const f of files) {
    const metrics = Array.isArray(f.json.metrics) ? f.json.metrics : [];
    for (const m of metrics) entries.push({ source: f.rel, metric: m });
  }

  const values = [];
  const unbound = [];
  const gating = [];
  const problems = [];
  for (const e of entries) {
    const m = e.metric || {};
    const id = String(m.metric);
    const row = registryRow(id);
    // ADR-0029 D-5: count only machine-bound gate numbers; an unbound metric is counted fail-closed
    // (we cannot prove it is not machine-bound).
    if (m.gating === true && (!row || row.machine === 'RM-A' || row.machine === 'RM-C')) {
      gating.push({ source: e.source, id: id, value: m.value });
    }
    if (!row) {
      // A metric that names a reliability row (HARNESS section 8.2 / kernel/06 section 3.10) is
      // bound by its OWN registry further down; it is not "not bound to a section 5 row", which
      // would be a misleading label for a correctly registered measurement.
      if (!reliabilityRow(id)) unbound.push({ source: e.source, id: id });
      continue;
    }
    for (const p of bindMetric(m, row)) problems.push(p + ' (' + e.source + ')');
    values.push({
      id: row.id,
      status: REPORTED,
      label: row.label,
      owner: row.owner,
      carrier: row.carrier,
      machine: row.machine || null,
      governed: row.governed,
      unit: row.unit || null,
      gate: typeof row.gate === 'number' ? row.gate : null,
      gateOp: row.gateOp || null,
      reportedUnit: m.unit,
      reportedGate: typeof m.gate === 'number' ? m.gate : null,
      value: m.value,
      verdict: m.verdict,
      gating: m.gating === true,
      source: e.source,
      outOfTable: R.OUT_OF_TABLE_IDS.indexOf(row.id) >= 0,
    });
  }

  const free = machineFreeRows();
  const rows = [];
  const missing = [];
  for (const r of free) {
    let hit = null;
    for (const v of values) if (v.id === r.id) { hit = v; break; }
    if (hit) {
      rows.push(hit);
      continue;
    }
    rows.push({
      id: r.id,
      status: NOT_REPORTED,
      label: r.label,
      owner: r.owner,
      carrier: r.carrier,
      machine: r.machine,
      governed: r.governed,
      unit: r.unit,
      gate: r.gate,
      gateOp: r.gateOp,
      reason: 'no metric named ' + r.id + ' in the read report(s)',
    });
    missing.push(r.id);
  }

  const controls = values.filter(function (v) { return v.outOfTable; });
  const extra = values.filter(function (v) {
    if (v.outOfTable) return false;
    for (const r of rows) if (r.id === v.id) return false;
    return true;
  });

  return {
    expected: free.map(function (r) { return r.id; }),
    rows: rows,
    controls: controls,
    extra: extra,
    unbound: unbound,
    missing: missing,
    sources: files.map(function (f) { return f.rel; }),
    gating: gating,
    gatingNumbersProduced: gating.length,
    problems: problems,
  };
}

export function reportedIds(valueSet) {
  return (valueSet.rows || []).filter(function (r) { return r.status === REPORTED; }).map(function (r) { return r.id; });
}

export function notReportedIds(valueSet) {
  return (valueSet.rows || []).filter(function (r) { return r.status === NOT_REPORTED; }).map(function (r) { return r.id; });
}

// --------------------------------------------------- reliability rows (HARNESS 8.2 / kernel/06 3.10)
//
// ADR-0029 D-4 split the registries: H1..H19 live in registry.mjs (transcribed from HARNESS
// section 5 and re-derived by B1/B3), and the AR-26 item 4 reliability rows live in
// reliability.mjs (re-derived from kernel/06 section 3.10 by B9). Reading the values out of a
// report has to follow that split, so the binding below is deliberately a separate function
// rather than a branch inside bindMetric(): a reliability metric never claims a section 5 row,
// and a section 5 metric never claims a reliability row.

// A metric id that names a reliability row, e.g. reliability.sessiond_rebuild.p95 -> R1.
export function reliabilityRow(metricId) {
  for (const r of REL.RELIABILITY_MAPPING) if (r.metric === metricId) return r;
  return null;
}

// Literal contract comparison (unit / gate / statistic must equal the reliability registry,
// which B9 re-derives from the section 3.10 text on every run) plus the one honesty rule
// section 3.10 adds on top of it: on a host with no registered reference machine the row may
// not be presented as a gate number at all. kernel/06 section 3.10「本机状态」/ ADR-0014 iron
// law 5: no RM-A -> INCONCLUSIVE / NON_GATING, never PASS, and `gating: true` is a false claim.
export function bindReliabilityMetric(metric, row, host) {
  const problems = [];
  if (metric.unit !== row.unit) {
    problems.push(row.id + ': unit mismatch -- kernel/06 3.10 registers "' + row.unit + '" but the report says "' + String(metric.unit) + '"');
  }
  if (metric.gate !== row.gate) {
    problems.push(row.id + ': gate mismatch -- the reliability registry says ' + row.gate + ' but the report says ' + String(metric.gate));
  }
  if (metric.statistic !== row.statistic) {
    problems.push(row.id + ': statistic mismatch -- kernel/06 3.10 registers ' + row.statistic + ' but the report says ' + String(metric.statistic));
  }
  if (host && host.gating === false) {
    if (metric.gating === true) {
      problems.push(row.id + ': the metric declares gating=true while this host has no registered reference machine; ADR-0014 iron law 5 forbids presenting it as a gate number');
    } else if (metric.verdict !== 'INCONCLUSIVE' && metric.verdict !== 'NON_GATING') {
      problems.push(row.id + ': verdict ' + String(metric.verdict) + ' on a host with no registered RM-A; kernel/06 3.10「本机状态」+ ADR-0014 iron law 5 require INCONCLUSIVE / NON_GATING, never PASS');
    }
  }
  return problems;
}

// reportFiles: [{ rel, json, parseError }]; host: { gating, state, reason } from
// lib.classifyMachine. A row the read report does not carry is REPORTED as NOT_REPORTED, never
// silently dropped (the same rule the section 5 value set follows).
export function collectReliabilityValues(reportFiles, host) {
  const files = (reportFiles || []).filter(function (f) { return f && f.json && !f.parseError; });
  const entries = [];
  for (const f of files) {
    const metrics = Array.isArray(f.json.metrics) ? f.json.metrics : [];
    for (const m of metrics) entries.push({ source: f.rel, metric: m });
  }

  const problems = [];
  const found = [];
  const unbound = [];
  for (const e of entries) {
    const id = String(e.metric.metric);
    const row = reliabilityRow(id);
    if (!row) {
      // Fail-closed: a reliability.* metric this registry does not know is named, not dropped.
      if (/^reliability\./.test(id)) unbound.push({ source: e.source, id: id });
      continue;
    }
    for (const p of bindReliabilityMetric(e.metric, row, host)) problems.push(p + ' (' + e.source + ')');
    found.push({
      id: row.id,
      status: REPORTED,
      label: row.label,
      metric: row.metric,
      owner: row.owner,
      carrier: row.carrier,
      machine: row.machine,
      unit: row.unit,
      gate: row.gate,
      gateOp: row.gateOp,
      statistic: row.statistic,
      value: e.metric.value,
      verdict: e.metric.verdict,
      gating: e.metric.gating === true,
      samples: e.metric.samples,
      runs: e.metric.runs,
      madOverMedian: e.metric.madOverMedian,
      source: e.source,
    });
  }

  const rows = REL.RELIABILITY_MAPPING.map(function (r) {
    for (const v of found) if (v.id === r.id) return v;
    return {
      id: r.id,
      status: NOT_REPORTED,
      label: r.label,
      metric: r.metric,
      owner: r.owner,
      carrier: r.carrier,
      machine: r.machine,
      unit: r.unit,
      gate: r.gate,
      gateOp: r.gateOp,
      statistic: r.statistic,
      reason: 'no metric named ' + r.metric + ' in the read report(s)',
    };
  });

  return {
    expected: REL.RELIABILITY_MAPPING.map(function (r) { return r.id; }),
    rows: rows,
    problems: problems,
    unbound: unbound,
    sources: files.map(function (f) { return f.rel; }),
    reported: rows.filter(function (r) { return r.status === REPORTED; }).map(function (r) { return r.id; }),
    missing: rows.filter(function (r) { return r.status === NOT_REPORTED; }).map(function (r) { return r.id; }),
  };
}
