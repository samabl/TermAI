// tools/bench/fixtures.mjs
// Synthetic logic fixtures for the W1-C bench methodology tools.
//
// READ THIS FIRST: these values are NOT measurements and they are NOT gate numbers. They are
// deliberately fake inputs whose only purpose is to drive tools/bench/lib.mjs through every branch
// of the kernel/06 section 3.1 state machine so that --selftest can prove the logic is not
// always-green. Nothing here is compared with a real baseline, published, or reported as a result;
// tools/bench/check.mjs prints them under the label "SYNTHETIC LOGIC FIXTURE".
//
// The synthetic machine id says so out loud so a fixture can never be mistaken for an RM-A / RM-C
// registration.

import { computeFingerprintSha256 } from './lib.mjs';

export const SYNTHETIC_MACHINE_ID = 'SYNTHETIC-NOT-A-REAL-MACHINE';

export function syntheticFingerprint(overrides) {
  const base = {
    id: SYNTHETIC_MACHINE_ID,
    role: 'RM-A',
    cpu: { model: 'synthetic-cpu', microcode: 'synthetic-ucode', cores: 8, threads: 16, r23_single: 1, r23_multi: 1 },
    mem: { size_gb: 1, modules: 'synthetic', speed: 'synthetic', timings: 'synthetic' },
    gpu: { model: 'synthetic-gpu', driver: 'synthetic-driver', api: 'synthetic-api', fl: 'synthetic-fl' },
    display: { model: 'synthetic-panel', edid_sha256: 'synthetic-edid', mode: 'synthetic-mode', vrr: false, hdr: false, dpi_scale: 100, panel_gtg_ms: 0 },
    storage: { model: 'synthetic-nvme', fw: 'synthetic-fw', seq_read_mbps: 1 },
    os: { name: 'synthetic-os', build: 'synthetic-build', arch: 'x64' },
    power: { plan: 'synthetic-plan', governor: 'synthetic-governor', app_nap: false },
    clocks: { source: 'synthetic-clock', qpc_freq_hz: 1 },
    fonts: { set_sha256: 'synthetic-fonts', render_backend: 'synthetic-backend' },
    input: { inject: 'synthetic-inject', keyboard_report_hz: 1000 },
    hypervisor: false,
    updated_at: '1970-01-01T00:00:00Z',
    superseded_by: null,
  };
  return Object.assign({}, base, overrides || {});
}

export function syntheticFingerprintSha(overrides) {
  return computeFingerprintSha256(syntheticFingerprint(overrides));
}

// A self-hosted T0 machine whose fingerprint matches the supplied hash.
export function syntheticMachine(overrides) {
  const o = overrides || {};
  return Object.assign(
    {
      class: 'self-hosted',
      role: 'RM-A',
      backend: 't0',
      fingerprintRecorded: true,
      fingerprintSha256: syntheticFingerprintSha(),
    },
    o
  );
}

// 10 runs whose median is exactly "center" and whose MAD/median is exactly "spread".
// Alternating center*(1-s) / center*(1+s) makes both quantities exact, which lets the fixtures
// target a grading boundary (e.g. exactly 5% for the frame family) instead of approximating it.
export function syntheticRuns(center, spread, opts) {
  const o = opts || {};
  const count = o.count || 10;
  const n = typeof o.n === 'number' ? o.n : 100000;
  const validCount = typeof o.validCount === 'number' ? o.validCount : count;
  const out = [];
  for (let i = 0; i < count; i++) {
    const stat = (i % 2 === 0) ? center * (1 - spread) : center * (1 + spread);
    out.push({ valid: i < validCount, n: n, stat: stat });
  }
  return out;
}

// A schema-valid single-metric bench-report. The flat projection is included because kernel/06
// section 3.7 requires it for single-metric calls.
export function syntheticReport(opts) {
  const o = opts || {};
  const metric = o.metric || 'synthetic.metric';
  const value = typeof o.value === 'number' ? o.value : 1;
  const sha = 'c0ffee'.repeat(10) + 'c0ff';
  const report = {
    schemaVersion: '1.0.0',
    reportId: 'synthetic-report',
    generatedAt: '1970-01-01T00:00:00Z',
    commit: { sha: 'synthetic-commit', dirty: false },
    toolchain: { rustc: 'synthetic-rustc', channel: 'synthetic-channel' },
    runner: { class: 'self-hosted', machineId: SYNTHETIC_MACHINE_ID, role: 'RM-A', backend: 't0/dx12' },
    fingerprintSha256: sha,
    environment: { powerPlan: 'synthetic-plan', exclusive: true, warmed: 2, valid: true },
    selfcheck: {
      scene: { verdict: 'PASS', frames: 1, hash: 'synthetic-scene-hash' },
      doubleRun: { verdict: 'PASS', deltaPct: 0.001 },
      verdict: 'PASS',
    },
    corpus: { manifestSha256: 'synthetic-manifest', shardsOk: 9, bytes: 1 },
    metrics: [
      {
        metric: metric,
        value: value,
        unit: o.unit || 'ms',
        samples: 100000,
        runner: SYNTHETIC_MACHINE_ID,
        commit: 'synthetic-commit',
        toolchain: 'synthetic-rustc',
        ts: '1970-01-01T00:00:00Z',
        statistic: 'p99',
        runs: 10,
        runStat: 'median',
        madOverMedian: 0.001,
        gate: typeof o.gate === 'number' ? o.gate : 16,
        target: null,
        gating: true,
        verdict: o.verdict || 'PASS',
        method: 'synthetic',
        dCalibrationMs: null,
        artifacts: [],
      },
    ],
    verdict: o.reportVerdict || 'PASS',
    cost: { minutes: 0, runnerClass: 'self-hosted', estUsd: 0 },
    // Additive field: the legacy flat projection for a single-metric call (kernel/06 3.7 /
    // spec 07 3.8.2). It lives in its own container because commit / toolchain / runner are
    // structured objects at the top level of the 1.0.0 report.
    flatProjection: {
      metric: metric,
      value: value,
      unit: o.unit || 'ms',
      samples: 100000,
      runner: SYNTHETIC_MACHINE_ID,
      commit: 'synthetic-commit',
      toolchain: 'synthetic-rustc',
      ts: '1970-01-01T00:00:00Z',
    },
  };
  if (o.mutate) o.mutate(report);
  return report;
}

// A deep clone so selftest mutations can never leak between injections.
export function clone(value) {
  return JSON.parse(JSON.stringify(value));
}
