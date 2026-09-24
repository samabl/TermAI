// tools/audit/check-claims.mjs
//
// Checks a claim that has drifted twice in this session: the number of discipline rules in the plan's
// section 6.3, as stated elsewhere in the documents. Rule 12 says to change a number and the places that
// state it together, and rule 17 says to search for them afterwards; this is that search, run mechanically.
//
// It reports mismatches and exits non-zero when the stated count differs from the counted one, because here,
//
// Measured control (round 233), on the real documents rather than a fixture: with section 6.3 holding
//
// Scope, and one thing tried and reverted (round 234). Range claims look like the same class - HARNESS states
// ADR-0001 to ADR-NNNN and an SD range, and both drifted this session - so an extension was written to check
// them. It reported three mismatches and all three were false: A14's own row quotes the stale ADR range in
// order to record it, and the delivery plan's line describes what W1-B registered at the time. A mechanical
// pattern cannot tell an assertion from a quotation, which is the difference between a stale claim and a
// record of one, so the extension was reverted rather than shipped crying wolf.
//
// The scope is therefore stated counts, where the pattern has no such ambiguity, and not ranges.
//
// Round 242 adds a second exclusion, for the same reason as the first. A completeness check was run by hand -
// does each decision in the brief carry its blocked cases, its options, a recommendation and the line saying
// what the owner must supply - and it reported D-1 as having no options. It has them: the section calls them
// 'two readings' and puts them in a table. The pattern looked for one word and the section used another.
//
// So structural completeness cannot be mechanized here either, though for a different cause than ranges: not
// because a pattern cannot tell an assertion from a quotation, but because the phrasing space is open. Counts
// and the gate count stay in scope, because there the phrasing is closed.
// eighteen rules and the register's entry stating seventeen, it prints
//   MISMATCH docs/audit/debt-p0.md: states 十七 (17), section 6.3 has 18
// and exits 1. Restoring the entry returns it to 'every stated count agrees' and exit 0. That is the exact
// failure this session made twice, in rounds 171 and 212, reproduced and caught.
// unlike the strikethrough linter, there is no judgement in the answer.
import fs from 'node:fs';
import path from 'node:path';

const NL = String.fromCharCode(10);

export function countRules(text) {
  const lines = text.split(NL);
  const s = lines.findIndex(function (l) { return l.indexOf('### 6.3') === 0; });
  if (s < 0) return -1;
  let e = lines.findIndex(function (l, i) { return i > s && l.indexOf('## ') === 0; });
  if (e < 0) e = lines.length;
  let n = 0;
  for (let i = s; i < e; i += 1) if (/^[0-9]+\. /.test(lines[i])) n += 1;
  return n;
}

export function statedCounts(text) {
  const out = [];
  const re = /§6\.3（\*\*(\S+?)条\*\*）/g;
  let m;
  while ((m = re.exec(text)) !== null) out.push({ raw: m[0], stated: m[1] });
  return out;
}

const CN = { '一': 1, '二': 2, '三': 3, '四': 4, '五': 5, '六': 6, '七': 7, '八': 8, '九': 9, '十': 10 };
// --- gate-count claim (added round 240): 'N gates in each job' against the GATE_PAIRS rows ---
// Countable from source, with no judgement in the answer, so it belongs here rather than in a candidate
// lister. The risk is missing a rephrased claim, not a false report.
export function gatePairCount(text) {
  const lines = text.split(NL);
  const s = lines.findIndex(function (l) { return l.indexOf('const GATE_PAIRS = [') >= 0; });
  if (s < 0) return -1;
  let n = 0;
  for (let i = s; i < lines.length; i += 1) {
    if (i > s && lines[i].trim() === '];') break;
    if (/^    \['/.test(lines[i])) n += 1;
  }
  return n;
}

export function statedGateCounts(text) {
  const out = [];
  const re = /各(\S+?)道门禁/g;
  let m;
  while ((m = re.exec(text)) !== null) out.push(m[1]);
  return out;
}
export function cnToNum(s) {
  if (/^[0-9]+$/.test(s)) return parseInt(s, 10);
  if (s === '十') return 10;
  if (s.indexOf('十') === 0) return 10 + (CN[s.slice(1)] || 0);
  if (s.indexOf('十') > 0) {
    const parts = s.split('十');
    return (CN[parts[0]] || 0) * 10 + (parts[1] ? (CN[parts[1]] || 0) : 0);
  }
  return CN[s] || -1;
}

function walk(dir, acc) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, acc);
    else if (e.name.endsWith('.md')) acc.push(p);
  }
  return acc;
}

function selftest() {
  const cases = [
    { name: 'a stated count matching the rules is clean', text: '### 6.3 x' + NL + '1. a' + NL + '2. b' + NL + '## next' + NL + '§6.3（**二条**）', want: 0 },
    { name: 'a stated count that differs is reported', text: '### 6.3 x' + NL + '1. a' + NL + '2. b' + NL + '## next' + NL + '§6.3（**三条**）', want: 1 },
  ];
  let bad = 0;
  for (const c of cases) {
    const n = countRules(c.text);
    const st = statedCounts(c.text).map(function (s) { return cnToNum(s.stated); });
    const mismatches = st.filter(function (v) { return v !== n; }).length;
    const ok = mismatches === c.want;
    if (!ok) bad += 1;
    console.log((ok ? 'ok   ' : 'FAIL ') + c.name + ' (counted ' + n + ', mismatches ' + mismatches + ', want ' + c.want + ')');
  }
  if (bad) { console.log('check-claims selftest: FAIL'); process.exit(1); }
  console.log('check-claims selftest: PASS - ' + cases.length + ' case(s), a matching count clean and a differing one reported');
}

const args = process.argv.slice(2);
if (args.indexOf('--selftest') >= 0) { selftest(); process.exit(0); }
const plan = fs.readFileSync(path.join('docs', 'plan', 'p0-delivery-plan.md'), 'utf8');
const n = countRules(plan);
const files = walk('docs', []);
let bad = 0;
for (const f of files) {
  for (const s of statedCounts(fs.readFileSync(f, 'utf8'))) {
    const v = cnToNum(s.stated);
    if (v !== n) { bad += 1; console.log('MISMATCH ' + f + ': states ' + s.stated + ' (' + v + '), section 6.3 has ' + n); }
  }
}
const realGates = gatePairCount(fs.readFileSync(path.join('tools', 'kernel-gates', 'check.mjs'), 'utf8'));
for (const f of files) {
  for (const s of statedGateCounts(fs.readFileSync(f, 'utf8'))) {
    const v = cnToNum(s);
    if (v !== realGates) { bad += 1; console.log('MISMATCH ' + f + ': states ' + s + ' (' + v + ') gates, GATE_PAIRS has ' + realGates); }
  }
}
console.log('check-claims: GATE_PAIRS lists ' + realGates + ' pair(s); ' + (bad ? bad + ' mismatch(es)' : 'every stated count and gate count agrees'));
process.exit(bad ? 1 : 0);