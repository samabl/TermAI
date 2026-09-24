// Reading check for ci-cost.json (spec 06 A-PM-12 / ADR-0014 decision 6).
// Validates schema completeness, recomputes every per-pipeline cost and the monthly total, and applies
// A-PM-12's thresholds: warn at 80% of the cap, require downsampling at 100%.
// No dependencies. Exits non-zero only when the cap is reached or exceeded, or the manifest is
// internally inconsistent - a reading under the cap must not turn the build red.
//
// --selftest injects the two faults this check exists to catch and asserts both are reported. A gate
// whose failure paths have never been exercised proves nothing, and until this existed neither branch
// had been shown to work in CI.
import fs from 'node:fs';

const PATH = 'ci-cost.json';

function validate(c) {
  const fail = [];
  const need = ['schema_version', 'currency', 'unit_prices_usd_per_minute', 'caps_usd_per_month', 'thresholds', 'pipelines', 'estimated_monthly_usd'];
  for (const k of need) if (!(k in c)) fail.push('missing top-level field: ' + k);

  const fields = ['pipeline', 'runner_class', 'minutes_per_run', 'runs_per_month', 'unit_usd_per_min', 'est_usd_month'];
  for (const [i, pl] of (c.pipelines || []).entries()) {
    for (const k of fields) if (!(k in pl)) fail.push('pipeline[' + i + '] missing ' + k);
    if (!pl.pipeline || !pl.runner_class) continue;
    const expect = Number((pl.minutes_per_run * pl.runs_per_month * pl.unit_usd_per_min).toFixed(4));
    if (Math.abs(expect - pl.est_usd_month) > 0.01) {
      fail.push(pl.pipeline + ': est_usd_month ' + pl.est_usd_month + ' != minutes x runs x price = ' + expect);
    }
  }

  const sum = Number((c.pipelines || []).reduce((a, x) => a + x.est_usd_month, 0).toFixed(1));
  if (Math.abs(sum - c.estimated_monthly_usd) > 0.05) {
    fail.push('declared monthly ' + c.estimated_monthly_usd + ' != summed per-pipeline ' + sum);
  }

  const caps = c.caps_usd_per_month || {};
  const warnAt = (c.thresholds && c.thresholds.warn_at_fraction) || 0.8;
  const hardAt = (c.thresholds && c.thresholds.downsample_at_fraction) || 1.0;
  const frac = sum / caps.cloud;
  const state = frac >= hardAt ? 'AT_OR_OVER_CAP' : frac >= warnAt ? 'WARN' : 'UNDER_WARN';
  if (state === 'AT_OR_OVER_CAP') {
    fail.push('monthly estimate ' + sum + ' reaches the cloud cap ' + caps.cloud + '; ADR-0014 line 235 requires automatic downsampling');
  }
  return { fail: fail, sum: sum, frac: frac, state: state, warnAt: warnAt };
}

function report(c) {
  const r = validate(c);
  console.log('ci-cost: pipelines=' + (c.pipelines || []).length + ' monthly=$' + r.sum +
    ' cloud_cap=$' + (c.caps_usd_per_month || {}).cloud + ' fraction=' + r.frac.toFixed(3) + ' state=' + r.state);
  if (r.state === 'WARN') console.log('ci-cost: WARNING - past ' + (r.warnAt * 100) + '% of the cloud cap; ADR-0014 requires an alert (A-PM-12)');
  for (const m of r.fail) console.error('ci-cost FAIL: ' + m);
  return r.fail.length === 0;
}

if (process.argv.includes('--selftest')) {
  const real = JSON.parse(fs.readFileSync(PATH, 'utf8'));
  let caught = 0;
  let missed = 0;

  const corruptTotal = JSON.parse(JSON.stringify(real));
  corruptTotal.estimated_monthly_usd = 9999;
  if (validate(corruptTotal).fail.length > 0) { caught += 1; console.log('caught  arithmetic: a declared total that contradicts the per-pipeline sum'); }
  else { missed += 1; console.error('MISSED  arithmetic: a declared total that contradicts the per-pipeline sum'); }

  const breachCap = JSON.parse(JSON.stringify(real));
  breachCap.caps_usd_per_month.cloud = 1;
  if (validate(breachCap).state === 'AT_OR_OVER_CAP' && validate(breachCap).fail.length > 0) { caught += 1; console.log('caught  cap breach: an estimate at or over the cloud cap'); }
  else { missed += 1; console.error('MISSED  cap breach: an estimate at or over the cloud cap'); }

  const control = JSON.parse(JSON.stringify(real));
  if (validate(control).fail.length === 0) { caught += 1; console.log('caught  control: the real manifest still validates, so the injections are what failed'); }
  else { missed += 1; console.error('MISSED  control: the real manifest must still validate'); }

  if (missed > 0) {
    console.error('ci-cost selftest: FAIL - ' + missed + ' injection(s) not caught');
    process.exit(1);
  }
  console.log('ci-cost selftest: PASS - ' + caught + ' branch(es) behaved as documented');
  process.exit(0);
}

const manifest = JSON.parse(fs.readFileSync(PATH, 'utf8'));
if (!report(manifest)) process.exit(1);
console.log('ci-cost: OK - schema complete, arithmetic consistent, under the cap');
