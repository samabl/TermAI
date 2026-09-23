// Per-test triage for esctest failures (the method rounds 76-83 established).
//
// Round 82's lesson is why this is per test and not per class: isolating a class only rules out
// contamination from other classes, never from an earlier test inside the same class. So this runs
// each failing test alone and classifies it:
//   PASSES ALONE -> contamination (the case is fine; something before it is not)
//   FAILS ALONE  -> a real failure (defect or out-of-scope sequence; see the classification table)
// Usage: node tools/conformance/triage-single.mjs <log> <ClassPrefix> [outDirRoot]
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const NL = String.fromCharCode(10);
const CR = String.fromCharCode(13);
const strip = (l) => (l.endsWith(CR) ? l.slice(0, -1) : l);

const logPath = process.argv[2];
const classPrefix = process.argv[3];
if (!logPath || !classPrefix) {
  console.error('usage: triage-single.mjs <log> <ClassPrefix> [outDirRoot]');
  process.exit(2);
}
const outRoot = process.argv[4] || ('target/conformance/triage-' + classPrefix);

const lines = fs.readFileSync(logPath, 'utf8').split(NL).map(strip);
const names = [];
for (const line of lines) {
  if (!line.startsWith('*** TEST ') || !line.endsWith(' FAILED:')) continue;
  const id = line.slice('*** TEST '.length, line.length - ' FAILED:'.length);
  if (id.startsWith(classPrefix)) names.push(id);
}

console.log('triage: ' + names.length + ' failing test(s) matching ' + classPrefix);
let contaminated = 0;
let real = 0;
for (const id of names) {
  const safe = id.split('.').pop().replace(/[^A-Za-z0-9]/g, '_');
  const out = path.join(outRoot, safe);
  const r = spawnSync('python', [
    'tools/conformance/upstream/esctest_adapter.py',
    '--esctest', 'C:/Users/z5075/AppData/Local/Temp/termai-conformance-upstream/esctest2',
    '--out', out,
    '--', '--expected-terminal', 'xterm', '--xterm-checksum', '336', '--include', id,
  ], { encoding: 'utf8' });
  const text = (r.stdout || '') + (r.stderr || '');
  // esctest writes singular forms for one test: "1 test passed" and "1 TEST FAILED". Matching
  // only the plural spellings made every single-test run look clean, which nearly shipped this
  // tool reporting 5 of 6 DECDCTests as contamination when all six fail alone.
  const passed = /(\d+) tests? passed/.exec(text);
  const failed = /(\d+) TESTS? FAILED/.exec(text);
  const nFailed = failed ? Number(failed[1]) : 0;
  if (passed && nFailed === 0) {
    contaminated += 1;
    console.log('CONTAMINATED  ' + id + '  (passes alone)');
  } else {
    real += 1;
    console.log('REAL          ' + id + '  (still fails alone)');
  }
}
console.log('triage: real=' + real + ' contaminated=' + contaminated + ' of ' + names.length);
