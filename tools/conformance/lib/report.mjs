// tools/conformance/lib/report.mjs
//
// Deterministic JSON helpers for conformance-report.json (kernel/01 section 3.9).
// Object keys are sorted and arrays are ordered by the caller, so two runs over the
// same inputs produce byte-identical output.
import { createHash } from 'node:crypto';

function sortValue(value) {
  if (Array.isArray(value)) return value.map(sortValue);
  if (value !== null && typeof value === 'object') {
    const out = {};
    for (const key of Object.keys(value).sort()) out[key] = sortValue(value[key]);
    return out;
  }
  return value;
}

export function canonicalJson(value) {
  return JSON.stringify(sortValue(value), null, 2) + '\n';
}

export function sha256Hex(text) {
  return createHash('sha256').update(text, 'utf8').digest('hex');
}

// Deterministic ratio with a fixed number of decimals (avoids float noise in JSON).
export function ratio(passed, total) {
  if (total <= 0) return null;
  return Math.round((passed / total) * 1e6) / 1e6;
}

export function relPosix(root, abs) {
  const rel = path_relative(root, abs);
  return rel.split('\\').join('/');
}

function path_relative(from, to) {
  // Minimal path.relative without importing path twice in this module.
  const p = from.split(/[\\/]+/).filter(Boolean);
  const t = to.split(/[\\/]+/).filter(Boolean);
  let i = 0;
  while (i < p.length && i < t.length && p[i].toLowerCase() === t[i].toLowerCase()) i += 1;
  const up = new Array(p.length - i).fill('..');
  return up.concat(t.slice(i)).join('/');
}
