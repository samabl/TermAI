#!/usr/bin/env node
// tools/audit/check-waivers.mjs
// Validates docs/audit/waivers.json, the register of time-boxed exceptions (ADR-0031).
//
// Why this file exists: ADR-0031 grants the DC-17 shaping crates (rustybuzz, ttf-parser) a
// time-boxed waiver, but condition 4 is "expiry means red" - a waiver that outlives its date
// must turn CI red, not print a warning that a reader has to notice. This checker makes the
// register machine-checked with exactly one hard rule (expires >= today is a failure) and one
// soft one (inside 30 days is a warning that names the days left), so the waiver cannot be
// silently renewed by editing prose. The same shape as kernel/01 K-04's expiring difference
// register and tools/conformance/check-suites.mjs: the declaration is data, the rule is code,
// and the selftest injects the faults the rules claim to catch.
//
//   node tools/audit/check-waivers.mjs              # validate the registry
//   node tools/audit/check-waivers.mjs --selftest   # prove the checks can fail (injection + control)
//   node tools/audit/check-waivers.mjs --file <p>   # validate another registry (selftest / review)
//   node tools/audit/check-waivers.mjs --json       # machine-readable report only
//
// Zero dependencies, Node >= 22.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..', '..');
const DEFAULT_FILE = path.join(ROOT, 'docs', 'audit', 'waivers.json');
const SCHEMA = 'termai-waivers/1';
const WARN_WINDOW_DAYS = 30;
const ISO_DATE_RE = /^[0-9]{4}-[0-9]{2}-[0-9]{2}$/;
const RUSTSEC_RE = /^RUSTSEC-[0-9]{4}-[0-9]{4}$/;
const DAY_MS = 24 * 60 * 60 * 1000;
// A waiver whose kind is about an advisory must carry RUSTSEC ids; any other kind must say why
// in a "reason" instead. The registry's own kind is "advisory-unmaintained".
function isAdvisoryKind(kind) { return typeof kind === 'string' && kind.indexOf('advisory') >= 0; }
function nonEmptyText(v) { return typeof v === 'string' && v.length > 0; }

// Parse YYYY-MM-DD strictly to a UTC midnight Date, or null. Date.UTC alone normalises
// 2026-02-31 to 2026-03-03, so the round-trip is what makes an impossible date an error.
export function parseISO(s) {
  if (typeof s !== 'string' || !ISO_DATE_RE.test(s)) return null;
  const y = Number(s.slice(0, 4));
  const m = Number(s.slice(5, 7));
  const d = Number(s.slice(8, 10));
  const dt = new Date(Date.UTC(y, m - 1, d));
  if (dt.getUTCFullYear() !== y || dt.getUTCMonth() !== m - 1 || dt.getUTCDate() !== d) return null;
  return dt;
}

function currentDay() { return new Date().toISOString().slice(0, 10); }

function daysBetween(fromISO, toISO) {
  const a = parseISO(fromISO);
  const b = parseISO(toISO);
  if (!a || !b) return null;
  return Math.round((b.getTime() - a.getTime()) / DAY_MS);
}

// The expiry rule, split out so both the validator and the human report read the same numbers.
// expires < today is a failure (ADR-0031 condition 4), expires == today is a failure (the
// waiver is spent the moment its date arrives), and the last 30 days are a warning that names
// the remaining days so the owner is on notice before it goes red.
function expireRule(id, expiresISO, todayISO, push, warn) {
  const days = daysBetween(todayISO, expiresISO);
  if (days === null) return;
  if (days < 0) {
    push('WAIVER_EXPIRED', 'expires', 'waiver ' + id + ' expired on ' + expiresISO + ' (' + (-days) + ' day(s) ago); an expired waiver is a failure, not a warning (ADR-0031 condition 4)');
  } else if (days === 0) {
    push('WAIVER_EXPIRES_TODAY', 'expires', 'waiver ' + id + ' expires today (' + expiresISO + '); an expired waiver is a failure, not a warning (ADR-0031 condition 4)');
  } else if (days <= WARN_WINDOW_DAYS) {
    warn('WAIVER_EXPIRES_SOON', 'expires', 'waiver ' + id + ' expires ' + expiresISO + ' in ' + days + ' day(s)');
  }
}

// Pure: same doc + same todayISO always produces the same result. todayISO defaults to the
// current UTC day only so the CLI can call it without a date; the selftest always passes one.
export function validate(doc, todayISO) {
  const errors = [];
  const warnings = [];
  const push = function (code, where, message) { errors.push({ code: code, where: where, message: message }); };
  const warn = function (code, where, message) { warnings.push({ code: code, where: where, message: message }); };
  const today = parseISO(todayISO) ? todayISO : currentDay();

  if (doc === null || typeof doc !== 'object' || Array.isArray(doc)) {
    push('WAIVERS_TYPE', '', 'the registry must be a JSON object');
    return { ok: false, errors: errors, warnings: warnings };
  }
  if (doc.schema !== SCHEMA) push('WAIVERS_SCHEMA', 'schema', 'schema must be "' + SCHEMA + '", got ' + JSON.stringify(doc.schema));
  if (!Array.isArray(doc.waivers) || doc.waivers.length === 0) {
    push('WAIVERS_EMPTY', 'waivers', 'waivers must be a non-empty array');
    return { ok: false, errors: errors, warnings: warnings };
  }

  const seenIds = {};
  doc.waivers.forEach(function (w, i) {
    const at = 'waivers[' + i + ']';
    if (!w || typeof w !== 'object' || Array.isArray(w)) { push('WAIVER_TYPE', at, 'a waiver must be an object'); return; }

    if (!nonEmptyText(w.id)) push('WAIVER_ID', at + '.id', 'id is required');
    else if (seenIds[w.id]) push('WAIVER_ID_DUPLICATE', at + '.id', 'waiver id "' + w.id + '" appears more than once');
    seenIds[w.id] = true;

    if (!nonEmptyText(w.kind)) push('WAIVER_KIND', at + '.kind', 'kind must be a non-empty string');

    if (!Array.isArray(w.subject) || w.subject.length === 0) {
      push('WAIVER_SUBJECT', at + '.subject', 'subject must be a non-empty array');
    }

    if (isAdvisoryKind(w.kind)) {
      if (!Array.isArray(w.advisories) || w.advisories.length === 0) {
        push('WAIVER_ADVISORIES', at + '.advisories', 'an advisory waiver needs a non-empty advisories array');
      } else {
        w.advisories.forEach(function (a, j) {
          if (typeof a !== 'string' || !RUSTSEC_RE.test(a)) {
            push('WAIVER_ADVISORY_ID', at + '.advisories[' + j + ']', 'an advisory must be a RUSTSEC-YYYY-NNNN id, got ' + JSON.stringify(a));
          }
        });
      }
    } else if (!nonEmptyText(w.reason)) {
      push('WAIVER_REASON', at + '.reason', 'a non-advisory waiver needs a non-empty reason instead of advisories');
    }

    for (const f of ['owner', 'decision', 'why', 'replacement_trigger', 'evidence']) {
      if (!nonEmptyText(w[f])) push('WAIVER_' + f.toUpperCase(), at + '.' + f, f + ' must be a non-empty string');
    }

    const granted = parseISO(w.granted);
    const expires = parseISO(w.expires);
    if (!granted) push('WAIVER_GRANTED', at + '.granted', 'granted must be an ISO date (YYYY-MM-DD), got ' + JSON.stringify(w.granted));
    if (!expires) push('WAIVER_EXPIRES', at + '.expires', 'expires must be an ISO date (YYYY-MM-DD), got ' + JSON.stringify(w.expires));
    if (granted && expires && expires.getTime() <= granted.getTime()) {
      push('WAIVER_DATE_ORDER', at + '.expires', 'expires (' + w.expires + ') must be strictly after granted (' + w.granted + ')');
    } else if (expires) {
      expireRule(nonEmptyText(w.id) ? w.id : at, w.expires, today, push, warn);
    }

    if (!Number.isInteger(w.expires_within_minors) || w.expires_within_minors <= 0) {
      push('WAIVER_MINORS', at + '.expires_within_minors', 'expires_within_minors must be a positive integer');
    }
    if (!nonEmptyText(w.minor_baseline)) push('WAIVER_MINOR_BASELINE', at + '.minor_baseline', 'minor_baseline must be a non-empty string');
  });

  return { ok: errors.length === 0, errors: errors, warnings: warnings };
}

// One summary row per waiver for the human report and the JSON payload, kept separate from
// validate() so a malformed entry still prints its id rather than aborting the listing.
export function summarize(doc, todayISO) {
  const out = [];
  if (!doc || typeof doc !== 'object' || Array.isArray(doc) || !Array.isArray(doc.waivers)) return out;
  const today = parseISO(todayISO) ? todayISO : currentDay();
  doc.waivers.forEach(function (w, i) {
    if (!w || typeof w !== 'object' || Array.isArray(w)) {
      out.push({ index: i, id: '(not an object)', subject: [], expires: null, days_remaining: null });
      return;
    }
    out.push({
      index: i,
      id: nonEmptyText(w.id) ? w.id : '(missing id)',
      subject: Array.isArray(w.subject) ? w.subject.map(String) : [],
      expires: nonEmptyText(w.expires) ? w.expires : null,
      days_remaining: daysBetween(today, w.expires),
    });
  });
  return out;
}

function load(file, today) {
  const abs = path.resolve(file);
  if (!fs.existsSync(abs)) {
    return { ok: false, errors: [{ code: 'WAIVERS_MISSING', where: file, message: file + ' does not exist' }], warnings: [], entries: [], doc: null };
  }
  let doc;
  try {
    doc = JSON.parse(fs.readFileSync(abs, 'utf8'));
  } catch (err) {
    return { ok: false, errors: [{ code: 'WAIVERS_JSON', where: file, message: 'not valid JSON: ' + String(err && err.message ? err.message : err) }], warnings: [], entries: [], doc: null };
  }
  const res = validate(doc, today);
  return { ok: res.ok, errors: res.errors, warnings: res.warnings, entries: summarize(doc, today), doc: doc };
}

// --------------------------------------------------- selftest fixtures

function goodFixture() {
  return {
    schema: SCHEMA,
    waivers: [{
      id: 'W-99',
      kind: 'advisory-unmaintained',
      subject: ['example-crate@1.0.0'],
      advisories: ['RUSTSEC-2026-0001'],
      why: 'selftest fixture, never a real waiver',
      decision: 'ADR-9999',
      owner: 'selftest',
      granted: '2026-01-01',
      expires: '2099-01-01',
      expires_within_minors: 2,
      minor_baseline: '0.1.0',
      replacement_trigger: 'a maintained replacement exists',
      evidence: 'tools/audit/check-waivers.mjs',
      visible_in: ['selftest'],
    }],
  };
}

function clone(v) { return JSON.parse(JSON.stringify(v)); }
function hasCode(result, code) { return result.errors.some(function (e) { return e.code === code; }); }
function codes(result) { return result.errors.map(function (e) { return e.code; }).join(','); }

function readJson(file) { return JSON.parse(fs.readFileSync(path.resolve(file), 'utf8')); }

function selftest() {
  const FIXTURE_TODAY = '2026-06-01';
  let missed = 0;
  const check = function (name, ok, detail) {
    if (!ok) missed += 1;
    console.log((ok ? '  caught  ' : '  MISSED  ') + name + (detail ? '  (' + detail + ')' : ''));
  };

  // control: the real registry, at the real current day. If this fails because the waiver has
  // actually expired, that is the gate doing its job - the registry needs renewing, not the check.
  const real = validate(readJson(DEFAULT_FILE), currentDay());
  check('control: the real registry validates clean today', real.ok, real.ok ? (real.warnings.length ? real.warnings.length + ' warning(s)' : 'no errors') : codes(real));

  // control: a valid future-dated registry passes, so the checker is not rejecting everything.
  const good = validate(clone(goodFixture()), FIXTURE_TODAY);
  check('control: a valid future-dated registry passes', good.ok, good.ok ? 'no errors' : codes(good));

  // (a) an expired waiver must be an error (WAIVER_EXPIRED), not a warning.
  const expired = clone(goodFixture());
  expired.waivers[0].granted = '2019-01-01';
  expired.waivers[0].expires = '2020-01-01';
  const rExpired = validate(expired, FIXTURE_TODAY);
  check('inject: an expired waiver is caught (WAIVER_EXPIRED)', !rExpired.ok && hasCode(rExpired, 'WAIVER_EXPIRED'), codes(rExpired));

  // (b) expires before granted must be an ordering error.
  const badOrder = clone(goodFixture());
  badOrder.waivers[0].granted = '2099-06-01';
  badOrder.waivers[0].expires = '2099-01-01';
  const rBadOrder = validate(badOrder, FIXTURE_TODAY);
  check('inject: expires before granted is caught (WAIVER_DATE_ORDER)', !rBadOrder.ok && hasCode(rBadOrder, 'WAIVER_DATE_ORDER'), codes(rBadOrder));

  // (c) a duplicate id must be reported.
  const dup = clone(goodFixture());
  dup.waivers.push(clone(dup.waivers[0]));
  const rDup = validate(dup, FIXTURE_TODAY);
  check('inject: a duplicate waiver id is caught (WAIVER_ID_DUPLICATE)', !rDup.ok && hasCode(rDup, 'WAIVER_ID_DUPLICATE'), codes(rDup));

  const total = 5;
  console.log('  injected faults caught: ' + (total - missed) + '/' + total);
  if (missed) {
    console.log('result: FAIL - ' + missed + ' injection(s)/control(s) not caught');
    process.exit(1);
  }
  console.log('result: PASS - every injection was caught and the controls hold; the assertions are not always-green');
}

// --------------------------------------------------- entry

function buildJson(file, today, res) {
  return {
    tool: 'waivers',
    schema_version: 1,
    file: path.relative(ROOT, path.resolve(file)).replace(/\\/g, '/'),
    today: today,
    result: res.ok ? 'PASS' : 'FAIL',
    waivers: res.entries,
    warnings: res.warnings,
    errors: res.errors,
  };
}

function report(file, today, res) {
  const rel = path.relative(ROOT, path.resolve(file)).replace(/\\/g, '/');
  console.log('=== audit waivers (' + rel + ') ===');
  console.log('today: ' + today);
  if (!res.entries.length) console.log('  (no waiver entries readable)');
  for (const e of res.entries) {
    const remaining = e.days_remaining === null ? 'days remaining unknown' : e.days_remaining + ' day(s) remaining';
    console.log('  ' + e.id + '  [' + e.subject.join(', ') + ']  expires ' + (e.expires || '(no expiry)') + '  (' + remaining + ')');
  }
  for (const w of res.warnings) console.log('  WARN ' + w.code + ' ' + w.where + ': ' + w.message);
  if (res.ok) {
    console.log('result: PASS - ' + res.entries.length + ' waiver(s), none expired (ADR-0031 condition 4)');
  } else {
    for (const e of res.errors) console.log('  ' + e.code + ' ' + e.where + ': ' + e.message);
    console.log('result: FAIL - ' + res.errors.length + ' violation(s)');
  }
}

const args = process.argv.slice(2);
if (args.indexOf('--selftest') >= 0) { selftest(); process.exit(0); }
const fileArg = args.indexOf('--file');
const file = fileArg >= 0 ? args[fileArg + 1] : DEFAULT_FILE;
const today = currentDay();
const result = load(file, today);
if (args.indexOf('--json') >= 0) {
  process.stdout.write(JSON.stringify(buildJson(file, today, result), null, 2) + '\n');
} else {
  report(file, today, result);
}
process.exit(result.ok ? 0 : 1);
