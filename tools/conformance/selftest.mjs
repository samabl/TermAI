// Fault-injection selftest for the G1 conformance runner.
//
// Round 133 asked which CI-wired gates have been shown to report an error, and conformance L0 had not been.
// gen-spec --check is a drift check, not a demonstration that a case can fail. This injects one.
//
// Round 135 corrected the approach: --root is the repository root (cases come from
// <root>/tools/conformance/cases, git and cargo build run there), so a scratch root would need the whole
// workspace. The injection is therefore in place with a guaranteed restore, and it targets a GENERATED
// case file so that gen-spec.mjs can rebuild it if the restore never runs.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const NL = String.fromCharCode(10);
const target = path.join('tools', 'conformance', 'cases', 'xterm-ctlseqs-spec', 'spec-bs.trec');
const original = fs.readFileSync(target, 'utf8');

const lines = original.split(NL);
let corrupted = false;
for (let i = lines.length - 1; i >= 0; i -= 1) {
  if (lines[i].startsWith('ASSERT CURSOR ')) {
    lines[i] = 'ASSERT CURSOR 99 99';
    corrupted = true;
    break;
  }
}
if (!corrupted) {
  console.error('selftest: no ASSERT CURSOR line found to corrupt; the target changed');
  process.exit(2);
}

const run = () => spawnSync('node', ['tools/conformance/run.mjs', '--quiet', '--no-artifacts'], { encoding: 'utf8' });

let caught = false;
try {
  fs.writeFileSync(target, lines.join(NL));
  const bad = run();
  caught = bad.status !== 0;
  if (caught) console.log('caught  injection: a corrupted expectation made the runner exit ' + bad.status);
  else console.error('MISSED  injection: the runner exited 0 despite a corrupted expectation');
} finally {
  fs.writeFileSync(target, original);
}

const good = run();
const control = good.status === 0;
if (control) console.log('caught  control: the restored corpus runs clean again');
else console.error('MISSED  control: the restored corpus exits ' + good.status);

if (!caught || !control) {
  console.error('conformance selftest: FAIL');
  process.exit(1);
}
console.log('conformance selftest: PASS - the runner reports a corrupted expectation, and the control holds');
