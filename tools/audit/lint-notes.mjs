// tools/audit/lint-notes.mjs
//
// Candidate lister for the strikethrough convention (plan section 6.3 rule 12): a row that announces a
// correction must strike through what it superseded, rather than only appending the correction under it.
//
// It is deliberately NOT a gate. Round 220 measured its output at 5 candidates over 36 table rows, of which
// 3 were real and 2 were judgement calls. Round 140's precedent is that inferring violations from a pattern
// raises false alarms, and a check that cries wolf teaches people to ignore it. So this prints candidates and
// exits 0 either way; the decision stays with the reader.
import fs from 'node:fs';
import path from 'node:path';

// '已被' was tried and removed: it matched ordinary prose such as '已核实，且已被门禁守住' in the compliance
// table, contributing two of six candidates that were pure lexical noise. The words kept are ones that
// announce a superseded conclusion rather than merely mention one.
const CORRECTION = ['更正', '已推翻', '回滚', '作废', '取代', '不再成立', '已作废'];

export function candidates(text) {
  const out = [];
  for (const line of text.split(String.fromCharCode(10))) {
    if (line.indexOf('| ') !== 0) continue;
    const cells = line.split('|');
    const id = (cells[1] || '').trim();
    if (!id || id === '---' || id.indexOf('---') === 0) continue;
    if (CORRECTION.some(function (w) { return line.indexOf(w) >= 0; }) && line.indexOf('~~') < 0) {
      out.push({ id: id, line: line.slice(0, 100) });
    }
  }
  return out;
}

function selftest() {
  const cases = [
    { name: 'a row announcing a correction with no strikethrough is a candidate',
      text: '| A1 | **x**。**第 9 轮更正：结论改为 y** | T1 |', want: 1 },
    { name: 'control: the same row with a stricken superseded half is not a candidate',
      text: '| A1 | ~~**x**~~ **第 9 轮更正：结论改为 y** | T1 |', want: 0 },
    { name: 'control: a row with no correction language is not a candidate',
      text: '| A1 | **x** 仍成立 | T1 |', want: 0 },
  ];
  let bad = 0;
  for (const c of cases) {
    const got = candidates(c.text).length;
    const ok = got === c.want;
    if (!ok) bad += 1;
    console.log((ok ? 'ok   ' : 'FAIL ') + c.name + ' (got ' + got + ', want ' + c.want + ')');
  }
  if (bad) { console.log('lint-notes selftest: FAIL'); process.exit(1); }
  console.log('lint-notes selftest: PASS - ' + cases.length + ' case(s), planted violation found and both controls clean');
}

const args = process.argv.slice(2);
if (args.indexOf('--selftest') >= 0) { selftest(); process.exit(0); }
const target = args[0] || path.join('docs', 'audit', 'debt-p0.md');
const found = candidates(fs.readFileSync(target, 'utf8'));
console.log('lint-notes: ' + found.length + ' candidate(s) in ' + target);
for (const c of found) console.log('  ' + c.id + '  ' + c.line);
console.log('note: candidates, not violations - around two in five were judgement calls when last measured.');