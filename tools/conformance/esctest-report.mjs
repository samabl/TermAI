#!/usr/bin/env node
// tools/conformance/esctest-report.mjs
//
// Turns an esctest log plus tools/conformance/suites.json into a report that applies two
// already-decided rules mechanically:
//
//   * ADR-0030 D-1 (level + eligible denominator): every esctest number must be quoted with the
//     claimed VT level and the eligible denominator (passed + failed_real). esctest folds
//     "not run because the declared VT level is too low" into its known-bug tally, so
//     excluded_by_vt_level must be reported SEPARATELY. The log records only the known-bug total
//     and no per-case level attribution, so this tool reports excluded_by_vt_level as UNKNOWN
//     (null) with an explicit note instead of inventing a split.
//   * ADR-0029 D-2 (static capability exclusions): tests whose class prefix is declared
//     statically absent (OSC 4/10/11/12 = color-query) are excluded_by_capability, NOT failures.
//   * A20 / ADR-0030 errata 2 (reverse-wrap patch level): the reconstructed esctest command must
//     carry --xterm-reverse-wrap <n> taken from suites.json invocation.xterm_reverse_wrap. esctest
//     defaults to 0 there, which makes ReverseWraparound() return private mode 45 and judges
//     pre-2023 semantics, so a command that omits the flag reproduces DIFFERENT numbers than the
//     report prints next to it (plan section 6.3 rule 8; SD-20). When the manifest does not
//     declare the field the flag is still printed, with an explicit NOT-DECLARED placeholder:
//     the command is never silently short enough to look complete and reproducible.
//
// Usage:
//   node tools/conformance/esctest-report.mjs --log <esctest.log> [--suites tools/conformance/suites.json]
//        [--suite esctest2] [--json] [--out <path>]
//   node tools/conformance/esctest-report.mjs --selftest
//
// Zero dependencies, Node >= 22. node:* imports only.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..', '..');
const DEFAULT_SUITES = path.join(HERE, 'suites.json');
const DEFAULT_SUITE = 'esctest2';
const DEFAULT_LOG = path.join(ROOT, 'target', 'conformance', 'lvl1-new', 'esctest.log');
const SCHEMA = 'termai-esctest-report/1';

// A20: the reverse-wrap flag is never omitted from a reconstructed command. Keeping the flag name,
// the manifest field name and the not-declared placeholder as constants lets --selftest assert on
// the same strings the report prints (injection + control, plan section 6.3 rule 10).
const REVERSE_WRAP_FLAG = '--xterm-reverse-wrap';
const REVERSE_WRAP_FIELD = 'invocation.xterm_reverse_wrap';
const REVERSE_WRAP_NOT_DECLARED = 'NOT-DECLARED';

// Exact shapes confirmed against target/conformance/lvl1-new/esctest.log:
//   *** 103 tests passed, 378 known bugs, 86 TESTS FAILED ***
//   *** TEST ChangeColorTests.test_ChangeColor_CIELab FAILED:
const SUMMARY_RE = /\*\*\*\s+(\d+)\s+tests passed,\s+(\d+)\s+known bugs,\s+(\d+)\s+TESTS FAILED\s+\*\*\*/;
const FAILED_RE = /^\*\*\*\s+TEST\s+([^\s]+?)\s+FAILED:?\s*$/;
const LIST_HEADER_RE = /^\s*Failing tests:\s*$/;
const CASE_RE = /^\s*([A-Za-z0-9_]+\.[A-Za-z0-9_]+)\s*$/;

const CAVEAT_KNOWN_BUG =
  'known_bug_raw mixes genuine known bugs with tests that esctest did not run because the ' +
  'declared VT level is too low. The log records only the combined total and no per-case ' +
  'level attribution, so the two cannot be separated from the log alone.';
const NOTE_VT_LEVEL =
  'Not derivable from this log. esctest folds "not run because the declared VT level is too ' +
  'low" into known_bug_raw; the log carries no per-case VT-level requirement, so reporting a ' +
  'split would be a fabrication. ADR-0030 D-1 requires this field be reported; it is reported ' +
  'here as null (unknown).';

function rel(p) {
  const abs = path.resolve(p);
  const r = path.relative(ROOT, abs);
  const chosen = r && !r.startsWith('..') && !path.isAbsolute(r) ? r : abs;
  return chosen.split(path.sep).join('/');
}

function parseLog(text) {
  const lines = text.split(/\r?\n/);
  const notes = [];
  let summary = null;
  let summaryLine = null;
  const failedRaw = [];
  let failedLinesTotal = 0;
  const listRaw = [];
  let inList = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (!summary) {
      const m = SUMMARY_RE.exec(line);
      if (m) {
        summary = { passed: Number(m[1]), known_bug: Number(m[2]), failed: Number(m[3]) };
        summaryLine = i + 1;
        continue;
      }
    }
    const f = FAILED_RE.exec(line);
    if (f) { failedRaw.push(f[1]); failedLinesTotal += 1; continue; }
    if (LIST_HEADER_RE.test(line)) { inList = true; continue; }
    if (inList) {
      if (line.trim() === '') continue;
      const c = CASE_RE.exec(line);
      if (c) listRaw.push(c[1]);
      else inList = false;
    }
  }

  const dedupe = function (arr) {
    const seen = new Set();
    const out = [];
    const dupes = [];
    for (const v of arr) {
      if (seen.has(v)) { dupes.push(v); continue; }
      seen.add(v);
      out.push(v);
    }
    return { out: out, dupes: dupes };
  };

  const failedD = dedupe(failedRaw);
  const listD = dedupe(listRaw);
  if (failedD.dupes.length) notes.push('duplicate "*** TEST ... FAILED" lines collapsed: ' + failedD.dupes.join(', '));
  if (listD.dupes.length) notes.push('duplicate names in the "Failing tests:" list collapsed: ' + listD.dupes.join(', '));
  if (summary) {
    if (failedD.out.length !== summary.failed) {
      notes.push('summary says ' + summary.failed + ' TESTS FAILED but ' + failedD.out.length +
        ' distinct "*** TEST ... FAILED" line(s) were found');
    }
    if (failedD.out.length === 0 && summary.failed > 0) {
      notes.push('the log has a failing summary but no "*** TEST <Class>.<test> FAILED" lines; per-case names are unavailable');
    }
  }
  if (listD.out.length) {
    const same = listD.out.length === failedD.out.length && listD.out.every(function (v, i) { return v === failedD.out[i]; });
    if (!same) {
      notes.push('the "Failing tests:" list (' + listD.out.length + ' name(s)) disagrees with the "*** TEST ... FAILED" lines (' +
        failedD.out.length + ' name(s)); classification used the "*** TEST ... FAILED" lines');
    }
  }

  return {
    summary: summary,
    summaryLine: summaryLine,
    failedNames: failedD.out,
    failedLinesTotal: failedLinesTotal,
    listCases: listD.out,
    notes: notes,
  };
}

function findSuite(manifest, suiteName) {
  if (!manifest || typeof manifest !== 'object' || !Array.isArray(manifest.suites)) {
    throw new Error('suites manifest has no "suites" array');
  }
  const suite = manifest.suites.find(function (s) { return s && s.name === suiteName; });
  if (!suite) {
    throw new Error('suite "' + suiteName + '" not found in manifest (available: ' +
      manifest.suites.map(function (s) { return s && s.name; }).join(', ') + ')');
  }
  return suite;
}

function classify(failedNames, suite) {
  const exclusions = Array.isArray(suite.static_capability_exclusions) ? suite.static_capability_exclusions : [];
  const byCapability = [];
  const byCap = new Map();
  for (const e of exclusions) {
    if (!e || typeof e.capability !== 'string') continue;
    const prefixes = Array.isArray(e.test_prefixes) ? e.test_prefixes : [];
    const entry = {
      capability: e.capability,
      citation: e.citation || null,
      cases: [],
      by_prefix: prefixes.map(function (p) { return { prefix: p, count: 0 }; }),
    };
    byCapability.push(entry);
    byCap.set(e.capability, entry);
  }
  const real = [];
  for (const name of failedNames) {
    let matched = null;
    for (const e of exclusions) {
      if (!e || !Array.isArray(e.test_prefixes)) continue;
      if (e.test_prefixes.some(function (p) { return name.startsWith(p); })) { matched = e; break; }
    }
    if (matched !== null && byCap.has(matched.capability)) {
      const entry = byCap.get(matched.capability);
      entry.cases.push(name);
      const pfx = matched.test_prefixes.find(function (p) { return name.startsWith(p); });
      const slot = entry.by_prefix.find(function (x) { return x.prefix === pfx; });
      if (slot) slot.count += 1;
    } else {
      real.push(name);
    }
  }
  return { byCapability: byCapability, real: real };
}

function buildReport(text, manifest, suiteName) {
  const parsed = parseLog(text);
  if (!parsed.summary) {
    throw new Error('summary line not found: expected "*** <P> tests passed, <K> known bugs, <F> TESTS FAILED ***"');
  }
  const suite = findSuite(manifest, suiteName);
  const inv = suite.invocation && typeof suite.invocation === 'object' ? suite.invocation : {};
  if (!Number.isInteger(inv.claimed_vt_level)) {
    throw new Error('suite "' + suiteName + '" has no integer invocation.claimed_vt_level');
  }
  const cls = classify(parsed.failedNames, suite);
  const excluded = cls.byCapability.reduce(function (a, c) { return a + c.cases.length; }, 0);

  return {
    schema: SCHEMA,
    suite: suiteName,
    pinned_revision: suite.pinned_revision || null,
    claimed_vt_level: inv.claimed_vt_level,
    claimed_vt_level_rule: suite.claimed_vt_level_rule || null,
    passed: parsed.summary.passed,
    known_bug_raw: parsed.summary.known_bug,
    failed_raw: parsed.summary.failed,
    excluded_by_capability: {
      count: excluded,
      by_capability: cls.byCapability.map(function (c) {
        return { capability: c.capability, count: c.cases.length, citation: c.citation, by_prefix: c.by_prefix, cases: c.cases };
      }),
      cases: cls.byCapability.reduce(function (a, c) { return a.concat(c.cases); }, []),
    },
    failed_real: { count: cls.real.length, names: cls.real },
    eligible: parsed.summary.passed + cls.real.length,
    excluded_by_vt_level: null,
    excluded_by_vt_level_note: NOTE_VT_LEVEL,
    caveats: [CAVEAT_KNOWN_BUG],
    parse: {
      summary_line: parsed.summaryLine,
      failed_line_count: parsed.failedNames.length,
      failed_lines_total: parsed.failedLinesTotal,
      failing_list_count: parsed.listCases.length,
      notes: parsed.notes,
    },
  };
}

// A20: resolve invocation.xterm_reverse_wrap into the argument the reconstructed command carries.
// A declared integer is used verbatim. When the field is absent (or not an integer) the flag is
// still emitted with an explicit NOT-DECLARED placeholder, so copy-pasting the printed command
// fails loudly instead of quietly running esctest with its default 0 - which is a different ruler.
function reverseWrapArg(suite) {
  const inv = suite && typeof suite.invocation === 'object' && suite.invocation !== null ? suite.invocation : {};
  const v = inv.xterm_reverse_wrap;
  if (Number.isInteger(v)) {
    return {
      declared: true,
      value: v,
      text: String(v),
      note: REVERSE_WRAP_FIELD + ' is declared; esctest is told the pinned terminal\'s patch level ' +
        'instead of defaulting to 0 (ADR-0030 errata 2).',
    };
  }
  return {
    declared: false,
    value: null,
    text: REVERSE_WRAP_NOT_DECLARED,
    note: REVERSE_WRAP_FIELD + ' is NOT declared in the manifest, so ' + REVERSE_WRAP_FLAG + ' ' +
      REVERSE_WRAP_NOT_DECLARED + ' above is a placeholder and not a value. esctest would default ' +
      'to 0, ReverseWraparound() would return private mode 45, and the expectations would be ' +
      'pre-383 (a different ruler, ADR-0030 errata 2). The printed command is NOT reproducible ' +
      'until the manifest declares the field.',
  };
}

function reconstructEsctestCommand(suite, logAbs) {
  const inv = suite.invocation || {};
  const parts = [
    'python', rel(path.join(ROOT, 'tools', 'conformance', 'upstream', 'esctest_adapter.py')),
    '--esctest', 'target/conformance/esctest2',
    '--out', rel(path.dirname(logAbs)),
    '--',
    '--expected-terminal', String(inv.expected_terminal),
    '--xterm-checksum', String(inv.xterm_checksum),
    '--max-vt-level', String(inv.claimed_vt_level),
    REVERSE_WRAP_FLAG, reverseWrapArg(suite).text,
  ];
  return parts.join(' ');
}

function humanReport(rep) {
  const L = [];
  L.push('=== esctest report: ' + rep.suite + ' ===');
  L.push('log:    ' + rep.inputs.log);
  L.push('suites: ' + rep.inputs.suites + '  (suite "' + rep.suite + '")');
  L.push('claimed_vt_level: ' + rep.claimed_vt_level);
  L.push('');
  L.push('passed:        ' + rep.passed);
  L.push('known_bug_raw: ' + rep.known_bug_raw);
  L.push('failed_raw:    ' + rep.failed_raw);
  L.push('eligible (passed + failed_real): ' + rep.eligible);
  L.push('');
  L.push('excluded_by_capability: ' + rep.excluded_by_capability.count);
  for (const c of rep.excluded_by_capability.by_capability) {
    L.push('  ' + c.capability + ': ' + c.cases.length + (c.citation ? '  [' + c.citation + ']' : ''));
    const pfx = c.by_prefix.filter(function (x) { return x.count > 0; }).map(function (x) { return x.prefix + '=' + x.count; }).join('  ');
    if (pfx) L.push('    by prefix: ' + pfx);
    for (const n of c.cases) L.push('    ' + n);
  }
  L.push('');
  L.push('failed_real: ' + rep.failed_real.count);
  for (const n of rep.failed_real.names) L.push('  ' + n);
  L.push('');
  L.push('excluded_by_vt_level: UNKNOWN (null)');
  L.push('  ' + rep.excluded_by_vt_level_note);
  L.push('');
  L.push('CAVEAT: ' + rep.caveats[0]);
  L.push('');
  L.push('esctest command (reconstructed from suites.json invocation + log location):');
  L.push('  ' + rep.esctest_command_reconstructed);
  L.push('esctest reverse-wrap (A20 / ADR-0030 errata 2): ' +
    (rep.esctest_reverse_wrap.declared
      ? rep.esctest_reverse_wrap.flag + ' ' + rep.esctest_reverse_wrap.rendered
      : 'NOT DECLARED - ' + rep.esctest_reverse_wrap.flag + ' ' + rep.esctest_reverse_wrap.rendered + ' is a placeholder'));
  L.push('  ' + rep.esctest_reverse_wrap.note);
  L.push('report command:');
  L.push('  ' + rep.report_command);
  for (const note of rep.parse.notes) L.push('parse note: ' + note);
  return L.join('\n');
}

function parseArgs(argv) {
  const o = { log: null, suites: DEFAULT_SUITES, suite: DEFAULT_SUITE, json: false, out: null, selftest: false, help: false };
  const need = function (a, v) { if (v === undefined || v.startsWith('--')) throw new Error(a + ' requires a value'); return v; };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--log') o.log = need(a, argv[++i]);
    else if (a === '--suites') o.suites = need(a, argv[++i]);
    else if (a === '--suite') o.suite = need(a, argv[++i]);
    else if (a === '--out') o.out = need(a, argv[++i]);
    else if (a === '--json') o.json = true;
    else if (a === '--selftest') o.selftest = true;
    else if (a === '--help' || a === '-h') o.help = true;
    else throw new Error('unknown argument: ' + a);
  }
  return o;
}

function printHelp() {
  console.log('usage: node tools/conformance/esctest-report.mjs --log <esctest.log> [options]');
  console.log('  --log <path>      esctest log to classify (required unless --selftest)');
  console.log('  --suites <path>   suites manifest (default tools/conformance/suites.json)');
  console.log('  --suite <name>    suite entry to apply (default esctest2)');
  console.log('  --json            emit the report as JSON instead of human lines');
  console.log('  --out <path>      also write the emitted report to this path');
  console.log('  --selftest        prove the classifier can fire (injection + controls)');
}

function syntheticLog(cases, passed) {
  let s = '';
  for (const c of cases) {
    s += 'Run test: ' + c + '\n';
    s += '*** TEST ' + c + ' FAILED:\n';
    s += 'Traceback (most recent call last):\n  (synthetic)\n';
    s += 'esctypes.TestFailure: Test failed\n\n\n';
  }
  s += '*** ' + (passed === undefined ? 5 : passed) + ' tests passed, 10 known bugs, ' + cases.length + ' TESTS FAILED ***\n';
  s += 'Failing tests:\n';
  for (const c of cases) s += c + '\n';
  return s;
}

function selftest() {
  let missed = 0;
  let total = 0;
  const check = function (name, ok, detail) {
    total += 1;
    if (!ok) missed += 1;
    console.log((ok ? '  caught  ' : '  MISSED  ') + name + (detail ? '  (' + detail + ')' : ''));
  };

  let manifest = null;
  try { manifest = JSON.parse(fs.readFileSync(DEFAULT_SUITES, 'utf8')); }
  catch (e) { console.log('  MISSED  fatal: cannot read ' + rel(DEFAULT_SUITES) + ': ' + e.message); console.log('  injected faults caught: 0/1'); return 1; }

  // (a) injection: a statically-absent capability must be excluded, not counted as a real failure.
  const colorCase = 'ChangeColorTests.test_ChangeColor_RGB';
  let inj = null;
  let injErr = null;
  try { inj = buildReport(syntheticLog([colorCase]), manifest, DEFAULT_SUITE); } catch (e) { injErr = e; }
  check('inject: a ChangeColorTests failure lands in excluded_by_capability',
    !!inj && inj.excluded_by_capability.cases.indexOf(colorCase) >= 0,
    injErr ? injErr.message : (inj ? 'excluded=' + inj.excluded_by_capability.count : 'no report'));
  check('inject: that same case is NOT in failed_real',
    !!inj && inj.failed_real.names.indexOf(colorCase) < 0,
    inj ? 'failed_real=' + inj.failed_real.count : 'no report');
  check('inject: the recorded capability is color-query',
    !!inj && inj.excluded_by_capability.by_capability.some(function (c) { return c.capability === 'color-query' && c.cases.indexOf(colorCase) >= 0; }),
    inj ? 'capabilities=' + inj.excluded_by_capability.by_capability.map(function (c) { return c.capability; }).join(',') : 'no report');

  // (b) control: a non-excluded failure must be a real failure and nothing may be excluded.
  const risCase = 'RISTests.test_RIS_Reset';
  let ctl = null;
  let ctlErr = null;
  try { ctl = buildReport(syntheticLog([risCase]), manifest, DEFAULT_SUITE); } catch (e) { ctlErr = e; }
  check('control: a non-excluded failure (RISTests.test_RIS_Reset) lands in failed_real',
    !!ctl && ctl.failed_real.names.indexOf(risCase) >= 0,
    ctlErr ? ctlErr.message : (ctl ? 'failed_real=' + ctl.failed_real.count : 'no report'));
  check('control: a non-excluded failure produces zero excluded_by_capability',
    !!ctl && ctl.excluded_by_capability.count === 0,
    ctl ? 'excluded=' + ctl.excluded_by_capability.count : 'no report');

  // (c) control: the real log, if present.
  let real = null;
  let realErr = null;
  try {
    if (!fs.existsSync(DEFAULT_LOG)) realErr = new Error('not present at ' + rel(DEFAULT_LOG));
    else real = buildReport(fs.readFileSync(DEFAULT_LOG, 'utf8'), manifest, DEFAULT_SUITE);
  } catch (e) { realErr = e; }
  if (realErr && realErr.message.startsWith('not present')) {
    console.log('  skip    control: real log not present (' + rel(DEFAULT_LOG) + ')');
  } else {
    check('control: real log reports claimed_vt_level 1 without throwing',
      !!real && real.claimed_vt_level === 1,
      realErr ? realErr.message : (real ? 'level=' + real.claimed_vt_level : 'no report'));
    check('control: real log eligible = passed + failed_real',
      !!real && real.eligible === real.passed + real.failed_real.count,
      real ? 'eligible=' + real.eligible + ' passed=' + real.passed + ' failed_real=' + real.failed_real.count : 'no report');
  }

  // (d) A20 injection: a manifest that does NOT declare invocation.xterm_reverse_wrap must make the
  // reconstructed command say so explicitly. esctest would then default to 0 and judge pre-383
  // semantics, so a silently shorter command reproduces different numbers than the report prints.
  const selftestLog = path.join(ROOT, 'target', 'conformance', 'selftest', 'esctest.log');
  const bareManifest = JSON.parse(JSON.stringify(manifest));
  let bareCmd = null;
  let bareErr = null;
  try {
    const bareSuite = findSuite(bareManifest, DEFAULT_SUITE);
    delete bareSuite.invocation.xterm_reverse_wrap;
    bareCmd = reconstructEsctestCommand(bareSuite, selftestLog);
  } catch (e) { bareErr = e; }
  check('inject (A20): a manifest without ' + REVERSE_WRAP_FIELD + ' makes the reconstructed command say ' + REVERSE_WRAP_NOT_DECLARED,
    !!bareCmd && bareCmd.indexOf(REVERSE_WRAP_FLAG + ' ' + REVERSE_WRAP_NOT_DECLARED) >= 0,
    bareErr ? bareErr.message : (bareCmd || 'no command'));
  check('inject (A20): that command does not silently look complete (' + REVERSE_WRAP_FLAG + ' is still named, with a placeholder value)',
    !!bareCmd && bareCmd.indexOf(REVERSE_WRAP_FLAG) >= 0,
    bareCmd ? 'flag present, value is the explicit placeholder' : 'no command');

  // (e) A20 control: the real manifest declares the patch level, so the flag must carry that value -
  // and the declared and undeclared reconstructions must differ, or the field is not reaching the
  // command at all (which is exactly what A20 found).
  let declaredRw = null;
  let declaredCmd = null;
  let declaredErr = null;
  try {
    const realSuite = findSuite(manifest, DEFAULT_SUITE);
    declaredRw = realSuite.invocation ? realSuite.invocation.xterm_reverse_wrap : null;
    declaredCmd = reconstructEsctestCommand(realSuite, selftestLog);
  } catch (e) { declaredErr = e; }
  check('control (A20): the manifest declares ' + REVERSE_WRAP_FIELD + ' ' + declaredRw + ' and the command carries ' + REVERSE_WRAP_FLAG + ' ' + declaredRw,
    !!declaredCmd && Number.isInteger(declaredRw) &&
      declaredCmd.indexOf(REVERSE_WRAP_FLAG + ' ' + declaredRw) >= 0 && declaredCmd !== bareCmd,
    declaredErr ? declaredErr.message : (declaredCmd || 'no command'));

  console.log('  injected faults caught: ' + (total - missed) + '/' + total);
  if (missed) { console.log('result: FAIL - ' + missed + ' check(s) did not fire; the classifier is not trustworthy'); return 1; }
  console.log('result: PASS - every injection was caught and the controls hold; the assertions are not always-green');
  return 0;
}

function main() {
  let o;
  try { o = parseArgs(process.argv.slice(2)); }
  catch (e) { console.error('error: ' + e.message); return 2; }

  if (o.selftest) return selftest();
  if (o.help) { printHelp(); return 0; }
  if (!o.log) { console.error('error: --log <esctest.log> is required (or use --selftest)'); return 2; }

  let manifest;
  try { manifest = JSON.parse(fs.readFileSync(path.resolve(o.suites), 'utf8')); }
  catch (e) { console.error('error: cannot read suites manifest ' + rel(o.suites) + ': ' + e.message); return 2; }

  let text;
  try { text = fs.readFileSync(path.resolve(o.log), 'utf8'); }
  catch (e) { console.error('error: cannot read log ' + rel(o.log) + ': ' + e.message); return 2; }

  let rep;
  let suite;
  try {
    suite = findSuite(manifest, o.suite);
    rep = buildReport(text, manifest, o.suite);
  } catch (e) { console.error('error: ' + e.message); return 2; }

  const logAbs = path.resolve(o.log);
  rep.inputs = {
    log: rel(logAbs),
    log_abs: logAbs,
    suites: rel(path.resolve(o.suites)),
    suites_abs: path.resolve(o.suites),
    suite: o.suite,
  };
  rep.report_command = ['node', rel(process.argv[1])].concat(process.argv.slice(2)).join(' ');
  const rw = reverseWrapArg(suite);
  rep.esctest_command_reconstructed = reconstructEsctestCommand(suite, logAbs);
  rep.esctest_reverse_wrap = {
    flag: REVERSE_WRAP_FLAG,
    field: REVERSE_WRAP_FIELD,
    declared: rw.declared,
    value: rw.value,
    rendered: rw.text,
    note: rw.note,
  };
  rep.esctest_command_source = 'reconstructed from suites.json invocation + the log location; the --esctest checkout path is the repository convention target/conformance/esctest2, not a suites.json field; ' + REVERSE_WRAP_FLAG + ' is taken from ' + REVERSE_WRAP_FIELD + ' and is printed as the explicit placeholder ' + REVERSE_WRAP_NOT_DECLARED + ' when the manifest does not declare it, never omitted (A20 / ADR-0030 errata 2)';

  const out = o.json ? JSON.stringify(rep, null, 2) : humanReport(rep);
  console.log(out);
  if (o.out) {
    const abs = path.resolve(o.out);
    try {
      fs.mkdirSync(path.dirname(abs), { recursive: true });
      fs.writeFileSync(abs, out + '\n', 'utf8');
    } catch (e) { console.error('error: cannot write --out ' + rel(abs) + ': ' + e.message); return 2; }
  }
  return 0;
}

process.exit(main());
