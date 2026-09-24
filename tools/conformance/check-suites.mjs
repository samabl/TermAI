#!/usr/bin/env node
// tools/conformance/check-suites.mjs
// Validates tools/conformance/suites.json, the static declaration of the upstream VT suites
// (ADR-0029 D-2 / kernel/01 K-01).
//
// Why this file exists: a suite's eligible set may be reduced ONLY by a static capability
// precondition (K-01 S_cap). A runtime or manual skip, or a difference entry, is forbidden for a
// 100% suite because it would turn "we did not implement this" into "we passed it". The manifest is
// therefore a declarative artifact with citations, and this checker makes the declaration
// machine-checked rather than prose.
//
//   node tools/conformance/check-suites.mjs              # validate the manifest
//   node tools/conformance/check-suites.mjs --selftest   # prove the checks can fail (injection + control)
//   node tools/conformance/check-suites.mjs --file <p>   # validate another manifest (selftest / review)
//
// Zero dependencies, Node >= 22.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..', '..');
const DEFAULT_MANIFEST = path.join(HERE, 'suites.json');
const SCHEMA = 'termai-conformance-suites/1';
const EXPECTED_TERMINALS = ['xterm', 'iTerm2', 'iTerm2beta'];
// Keys whose presence anywhere would mean "this suite's failures were waived" rather than
// "this capability is statically absent". kernel/01 K-01 forbids that for a 100% suite.
const FORBIDDEN_KEYS = ['skip', 'manual_skip', 'manual', 'xfail', 'known_bug', 'allow_failure', 'waiver'];

export function validate(doc) {
  const errors = [];
  const push = function (code, where, message) { errors.push({ code: code, where: where, message: message }); };
  if (doc === null || typeof doc !== 'object' || Array.isArray(doc)) {
    push('SUITES_TYPE', '', 'the manifest must be a JSON object');
    return { ok: false, errors: errors };
  }
  if (doc.schema !== SCHEMA) push('SUITES_SCHEMA', 'schema', 'schema must be "' + SCHEMA + '", got ' + JSON.stringify(doc.schema));
  if (!Array.isArray(doc.suites) || doc.suites.length === 0) {
    push('SUITES_EMPTY', 'suites', 'suites must be a non-empty array');
    return { ok: false, errors: errors };
  }
  const seenSuites = {};
  doc.suites.forEach(function (s, i) {
    const at = 'suites[' + i + ']';
    if (!s || typeof s !== 'object') { push('SUITE_TYPE', at, 'a suite must be an object'); return; }
    if (typeof s.name !== 'string' || s.name.length === 0) push('SUITE_NAME', at, 'name is required');
    else if (seenSuites[s.name]) push('SUITE_DUPLICATE', at, 'suite "' + s.name + '" appears more than once');
    seenSuites[s.name] = true;
    if (typeof s.pinned_revision !== 'string' || !/^[0-9a-f]{40}$/.test(s.pinned_revision)) {
      push('SUITE_REVISION', at + '.pinned_revision', 'pinned_revision must be a 40-character lowercase git sha');
    }
    const inv = s.invocation;
    if (!inv || typeof inv !== 'object') {
      push('SUITE_INVOCATION', at + '.invocation', 'invocation is required');
    } else {
      if (EXPECTED_TERMINALS.indexOf(inv.expected_terminal) < 0) {
        push('SUITE_TERMINAL', at + '.invocation.expected_terminal', 'expected_terminal must be one of ' + EXPECTED_TERMINALS.join(', '));
      }
      if (!Number.isInteger(inv.xterm_checksum)) push('SUITE_CHECKSUM', at + '.invocation.xterm_checksum', 'xterm_checksum must be an integer');
      if (!Number.isInteger(inv.claimed_vt_level) || inv.claimed_vt_level < 1 || inv.claimed_vt_level > 5) {
        push('SUITE_VT_LEVEL', at + '.invocation.claimed_vt_level', 'claimed_vt_level must be an integer in 1..5 (ADR-0030 D-1)');
      }
    }
    if (!Array.isArray(s.static_capability_exclusions)) {
      push('SUITE_EXCLUSIONS', at + '.static_capability_exclusions', 'static_capability_exclusions must be an array (empty is allowed)');
      return;
    }
    const seenCaps = {};
    s.static_capability_exclusions.forEach(function (e, j) {
      const eat = at + '.static_capability_exclusions[' + j + ']';
      if (!e || typeof e !== 'object') { push('EXCLUSION_TYPE', eat, 'an exclusion must be an object'); return; }
      if (typeof e.capability !== 'string' || e.capability.length === 0) push('EXCLUSION_CAPABILITY', eat, 'capability is required');
      else if (seenCaps[e.capability]) push('EXCLUSION_DUPLICATE', eat, 'capability "' + e.capability + '" is declared twice in the same suite');
      seenCaps[e.capability] = true;
      if (!Array.isArray(e.test_prefixes) || e.test_prefixes.length === 0) {
        push('EXCLUSION_PREFIXES', eat + '.test_prefixes', 'test_prefixes must be a non-empty array of "<Class>." prefixes');
      } else {
        e.test_prefixes.forEach(function (p, k) {
          // Either a class prefix ("SomeClass.", excludes the whole class) or an exact case name
          // ("SomeClass.test_name", excludes one case while its siblings keep running).
          if (typeof p !== 'string' || !/^[A-Za-z0-9_]+\.([A-Za-z0-9_]+\.?)?$/.test(p)) {
            push('EXCLUSION_PREFIX_SHAPE', eat + '.test_prefixes[' + k + ']', 'a prefix must be "SomeClass." or an exact "SomeClass.test_name"');
          }
        });
      }
      if (typeof e.citation !== 'string' || e.citation.indexOf('ADR-') < 0 || e.citation.indexOf('kernel/01') < 0) {
        push('EXCLUSION_CITATION', eat + '.citation', 'citation must name an ADR and a kernel/01 clause (K-04 requires evidence, not a waiver)');
      }
      if (typeof e.reason !== 'string' || e.reason.length === 0) push('EXCLUSION_REASON', eat + '.reason', 'reason is required');
      if (e.excluded_case_count_measured !== undefined && (!Number.isInteger(e.excluded_case_count_measured) || e.excluded_case_count_measured < 0)) {
        push('EXCLUSION_COUNT', eat + '.excluded_case_count_measured', 'a measured count must be a non-negative integer');
      }
    });
  });
  for (const key of FORBIDDEN_KEYS) {
    findKey(doc, key, '', function (where) {
      push('SUITES_FORBIDDEN_KEY', where, '"' + key + '" turns a static capability precondition into a runtime waiver; kernel/01 K-01 forbids it');
    });
  }
  return { ok: errors.length === 0, errors: errors };
}

function findKey(value, key, where, onHit) {
  if (value === null || typeof value !== 'object') return;
  if (Array.isArray(value)) {
    value.forEach(function (v, i) { findKey(v, key, where + '[' + i + ']', onHit); });
    return;
  }
  for (const k of Object.keys(value)) {
    const here = where ? where + '.' + k : k;
    if (k === key) onHit(here);
    findKey(value[k], key, here, onHit);
  }
}

function load(file) {
  const abs = path.resolve(file);
  if (!fs.existsSync(abs)) return { ok: false, errors: [{ code: 'SUITES_MISSING', where: file, message: file + ' does not exist' }] };
  try {
    return validate(JSON.parse(fs.readFileSync(abs, 'utf8')));
  } catch (err) {
    return { ok: false, errors: [{ code: 'SUITES_JSON', where: file, message: 'not valid JSON: ' + String(err && err.message ? err.message : err) }] };
  }
}

function report(title, result) {
  console.log('=== ' + title + ' ===');
  if (result.ok) {
    console.log('result: PASS - static suite declarations are well-formed and every exclusion cites an ADR and a kernel/01 clause');
    return true;
  }
  for (const e of result.errors) console.log('  ' + e.code + ' ' + e.where + ': ' + e.message);
  console.log('result: FAIL - ' + result.errors.length + ' violation(s)');
  return false;
}

function readManifest(file) {
  return JSON.parse(fs.readFileSync(path.resolve(file), 'utf8'));
}

function clone(v) { return JSON.parse(JSON.stringify(v)); }

function selftest() {
  const real = readManifest(DEFAULT_MANIFEST);
  let missed = 0;
  const check = function (name, ok, detail) {
    if (!ok) missed += 1;
    console.log((ok ? '  caught  ' : '  MISSED  ') + name + (detail ? '  (' + detail + ')' : ''));
  };

  const good = validate(clone(real));
  check('control: the real manifest validates', good.ok, good.ok ? 'no errors' : good.errors.map(function (e) { return e.code; }).join(','));

  const noCitation = clone(real);
  noCitation.suites[0].static_capability_exclusions[0].citation = 'we decided not to';
  const rNoCitation = validate(noCitation);
  check('inject: an exclusion whose citation names no ADR/kernel-01 clause is caught', !rNoCitation.ok && hasCode(rNoCitation, 'EXCLUSION_CITATION'), codes(rNoCitation));

  const waived = clone(real);
  waived.suites[0].static_capability_exclusions[0].xfail = true;
  const rWaived = validate(waived);
  check('inject: an xfail/waiver key is caught (a skip is not a capability)', !rWaived.ok && hasCode(rWaived, 'SUITES_FORBIDDEN_KEY'), codes(rWaived));

  const badRevision = clone(real);
  badRevision.suites[0].pinned_revision = 'HEAD';
  const rBadRevision = validate(badRevision);
  check('inject: a floating revision is caught', !rBadRevision.ok && hasCode(rBadRevision, 'SUITE_REVISION'), codes(rBadRevision));

  const badLevel = clone(real);
  badLevel.suites[0].invocation.claimed_vt_level = 6;
  const rBadLevel = validate(badLevel);
  check('inject: a VT level outside 1..5 is caught', !rBadLevel.ok && hasCode(rBadLevel, 'SUITE_VT_LEVEL'), codes(rBadLevel));

  const dupCap = clone(real);
  dupCap.suites[0].static_capability_exclusions.push(clone(dupCap.suites[0].static_capability_exclusions[0]));
  const rDupCap = validate(dupCap);
  check('inject: a capability declared twice in one suite is caught', !rDupCap.ok && hasCode(rDupCap, 'EXCLUSION_DUPLICATE'), codes(rDupCap));

  const control = validate(clone(real));
  check('control: the same manifest validates again after the injections', control.ok, 'the checker is not rejecting everything');

  console.log('  injected faults caught: ' + (6 - missed) + '/6');
  if (missed) { console.log('result: FAIL - ' + missed + ' injection(s) not caught'); process.exit(1); }
  console.log('result: PASS - every injection was caught and the control holds; the assertions are not always-green');
}

function hasCode(result, code) { return result.errors.some(function (e) { return e.code === code; }); }
function codes(result) { return result.errors.map(function (e) { return e.code; }).join(','); }

const args = process.argv.slice(2);
if (args.indexOf('--selftest') >= 0) { selftest(); process.exit(0); }
const fileArg = args.indexOf('--file');
const file = fileArg >= 0 ? args[fileArg + 1] : DEFAULT_MANIFEST;
process.exit(report('conformance suite declarations (' + path.relative(ROOT, path.resolve(file)).replace(/\\/g, '/') + ')', load(file)) ? 0 : 1);
