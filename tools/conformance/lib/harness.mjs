// tools/conformance/lib/harness.mjs
//
// Thin wrapper around the termai-vt headless conformance harness binary
// (crates/termai-vt/src/bin/termai-vt-conformance.rs). No Rust dependency is added:
// the binary is auto-discovered by cargo inside the existing crate, and it calls only
// the public termai-vt API.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

export const HARNESS_BIN_NAME =
  process.platform === 'win32' ? 'termai-vt-conformance.exe' : 'termai-vt-conformance';
export const HARNESS_TARGET = 'termai-vt-conformance';

export function harnessPath(root) {
  return path.join(root, 'target', 'debug', HARNESS_BIN_NAME);
}

function run(cmd, args, cwd, input) {
  return spawnSync(cmd, args, {
    cwd: cwd,
    encoding: 'utf8',
    input: input,
    maxBuffer: 256 * 1024 * 1024,
    shell: false,
    windowsHide: true,
  });
}

export function ensureHarness(root) {
  const built = run('cargo', ['build', '--quiet', '-p', 'termai-vt', '--bin', HARNESS_TARGET], root);
  if (built.error) {
    throw new Error('cargo could not start: ' + (built.error.message || String(built.error)));
  }
  if (built.status !== 0) {
    throw new Error(
      'cargo build -p termai-vt --bin ' + HARNESS_TARGET + ' failed:\n' +
      String(built.stderr || '') + String(built.stdout || '')
    );
  }
  const bin = harnessPath(root);
  if (!fs.existsSync(bin)) throw new Error('harness binary missing after build: ' + bin);
  return bin;
}

export function hexToText(hex) {
  return Buffer.from(hex, 'hex').toString('utf8');
}

export function textToHex(text) {
  return Buffer.from(text, 'utf8').toString('hex');
}

// Feed id/abs-path/cols/rows lines; return the raw stdout string.
export function harnessRaw(bin, root, lines) {
  const input = lines.join('\n') + '\n';
  const r = run(bin, [], root, input);
  if (r.error) throw new Error('harness could not start: ' + (r.error.message || String(r.error)));
  if (r.status !== 0) {
    throw new Error('harness exited ' + r.status + ': ' + String(r.stderr || ''));
  }
  return r.stdout || '';
}

// Parse one harness batch stdout into a Map id -> result object.
export function parseHarnessOutput(text) {
  const results = new Map();
  let current = null;
  for (const raw of text.split(/\r?\n/)) {
    if (raw === 'END') {
      if (current) results.set(current.id, current);
      current = null;
      continue;
    }
    const sp = raw.indexOf(' ');
    const key = sp < 0 ? raw : raw.slice(0, sp);
    const rest = sp < 0 ? '' : raw.slice(sp + 1);
    if (key === 'CASE') {
      current = newBlock(rest);
      continue;
    }
    if (!current) continue;
    switch (key) {
      case 'VERDICT': current.verdict = rest; break;
      case 'PARITY': current.parity = rest; break;
      case 'STEPS_PASSED': current.steps_passed = Number(rest); break;
      case 'STEPS_FAILED': current.steps_failed = Number(rest); break;
      case 'INPUT_BYTES': current.input_bytes = Number(rest); break;
      case 'CONSUMED_BYTES': current.consumed_bytes = Number(rest); break;
      case 'ALL_CONSUMED': current.all_consumed = rest === 'true'; break;
      case 'GRID_HASH': current.grid_hash = rest; break;
      case 'GRID_CONTROL_BYTES': current.grid_control_bytes = Number(rest); break;
      case 'RESPONSES': current.responses = current.responses || []; break;
      case 'RESPONSE': (current.responses = current.responses || []).push(rest); break;
      case 'STREAM': current.stream = rest; break;
      case 'COUNTER': {
        const at = rest.lastIndexOf(' ');
        current.counters[rest.slice(0, at)] = Number(rest.slice(at + 1));
        break;
      }
      case 'FAIL': (current.failures = current.failures || []).push(hexToText(rest)); break;
      case 'GOLDEN': (current.golden = current.golden || []).push(rest); break;
      case 'DIMS': {
        const parts = rest.split(' ');
        current.dims = [Number(parts[0]), Number(parts[1])];
        break;
      }
      case 'CURSOR': {
        const parts = rest.split(' ');
        current.cursor = [Number(parts[0]), Number(parts[1])];
        break;
      }
      default: break;
    }
  }
  return results;
}

function newBlock(id) {
  return {
    id: id,
    verdict: 'MISSING',
    parity: 'n/a',
    steps_passed: 0,
    steps_failed: 0,
    input_bytes: 0,
    consumed_bytes: 0,
    all_consumed: false,
    dims: [0, 0],
    cursor: [0, 0],
    grid_hash: '',
    grid_control_bytes: -1,
    responses: [],
    counters: {},
    failures: [],
    golden: [],
    stream: '',
  };
}

// Ask the real Rust lane policy (termai_vt::lane_verdict) for a verdict key.
export function laneVerdicts(bin, root) {
  const probes = [
    ['l0', 2, 2],
    ['l0', 2, 3],
    ['l1', 2, 2],
    ['l1', 2, 3],
    ['l2', 2, 2],
    ['l2', 2, 3],
  ];
  const lines = probes.map(function (p) { return 'LANE ' + p[0] + ' ' + p[1] + ' ' + p[2]; });
  lines.push('QUIT');
  const r = run(bin, ['--server'], root, lines.join('\n') + '\n');
  if (r.error || r.status !== 0) {
    throw new Error('lane probe failed: ' + String((r.error && r.error.message) || r.stderr || ''));
  }
  const out = [];
  let index = 0;
  for (const raw of String(r.stdout || '').split(/\r?\n/)) {
    if (raw.indexOf('LANE ') !== 0) continue;
    const verdict = raw.slice(5);
    const probe = probes[index];
    out.push({ lane: probe[0], passed: probe[1], total: probe[2], verdict: verdict });
    index += 1;
  }
  if (out.length !== probes.length) throw new Error('lane probe returned ' + out.length + ' results');
  return out;
}
