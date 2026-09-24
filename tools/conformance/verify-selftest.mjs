// Fault-injection selftest for the generator/corpus drift check.
//
// debt-p0 A18 records that `npm run conformance:verify` (= `gen-spec.mjs --check`) caught a real
// divergence - the generator table said ESC #8 while the committed .trec said ESC #9 - and that no CI
// step ran it, so a fresh drift would have gone unnoticed. The check now runs in both CI jobs and is
// registered in K8's GATE_PAIRS.
//
// This proves the check can report a drift and returns to clean afterwards. It follows
// tools/conformance/selftest.mjs: --root is the repository root, so the injection is in place with a
// guaranteed restore, and it targets a GENERATED case file so that gen-spec.mjs can rebuild it if the
// restore never runs. The injection is the historical ESC #9 -> ESC #8 divergence itself, so the
// property under test is the one A18 was about.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const ESC = String.fromCharCode(27);
const suite = 'termai-invariants';
const name = 'inv-esc-intermediate-has-no-side-effect.trec';
const target = path.join('tools', 'conformance', 'cases', suite, name);
const original = fs.readFileSync(target, 'utf8');

const drifted = original.replace(ESC + '#9', ESC + '#8');
if (drifted === original) {
  console.error('selftest: no ESC #9 found in ' + suite + '/' + name + '; the target changed');
  process.exit(2);
}

const run = () => spawnSync('node', ['tools/conformance/gen-spec.mjs', '--check'], { encoding: 'utf8' });

let caught = false;
try {
  fs.writeFileSync(target, drifted);
  const bad = run();
  const named = (bad.stdout || '').indexOf(suite + '/' + name) >= 0;
  caught = bad.status !== 0 && named;
  if (caught) console.log('caught  injection: a drifted case made gen-spec --check exit ' + bad.status + ' and name ' + suite + '/' + name);
  else if (bad.status === 0) console.error('MISSED  injection: gen-spec --check exited 0 despite a drifted case');
  else console.error('MISSED  injection: gen-spec --check exited ' + bad.status + ' but did not name ' + suite + '/' + name);
} finally {
  fs.writeFileSync(target, original);
}

const good = run();
const control = good.status === 0;
if (control) console.log('caught  control: the restored corpus matches the generator again');
else console.error('MISSED  control: the restored corpus still fails gen-spec --check with exit ' + good.status);

if (!caught || !control) {
  console.error('conformance verify selftest: FAIL');
  process.exit(1);
}
console.log('conformance verify selftest: PASS - the drift check reports a drifted case, and the control holds');
