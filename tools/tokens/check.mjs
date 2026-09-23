// tools/tokens/check.mjs
// TermAI design-token validator (zero external dependencies). Fails (exit 1) on any gate.
// Gates: (1) schema (2) AR-22 rulers (3) WCAG contrast (4) codegen drift (5) hex debt (warn only).
import fs from 'node:fs';
import path from 'node:path';
import {
  DEFAULT_ROOT, loadSource, resolveTheme, generate, extractPrototypeBlock,
  pairRatio, parseColor, relativeLuminance, linearToSrgb,
  BASE_GROUPS, PROTOTYPE_REL
} from './lib.mjs';

// AR-22 item 1 + spec 02 section 3.11.
const SPACE_SCALE = [4, 8, 12, 16, 24, 32, 48];
const RADIUS_SCALE = [8, 10, 14, 999];
const ICON_SIZES = [16, 20, 24];
const MOTION_DURATIONS = [120, 180, 240];
// Two explicit font-size ladders (spec 02 section 3.11 + the AR-23 reference prototype):
//   ui        12 / 13 / 14 / 15 / 16 / 18 / 20   (15 retained: block titles are an accepted
//             decision of the AR-23 reference implementation, kept for backward compatibility)
//   terminal  12 / 12.5 / 14 / 16 / 18 / 20      (terminal size is independently adjustable)
// Every fontSize token declares which ladder it belongs to in tokens/base/type.json
// ("ladder": "ui" | "terminal"); the validator rejects a value that is off its own ladder.
const FONT_SIZE_LADDERS = {
  ui: [12, 13, 14, 15, 16, 18, 20],
  terminal: [12, 12.5, 14, 16, 18, 20],
};
// Union of both ladders, for consumers/UI that only ask "is this value on a font ladder".
const FONT_SIZE_UNION = [12, 12.5, 13, 14, 15, 16, 18, 20];

function pxNumber(v) {
  const m = /^([0-9]+(?:[.][0-9]+)?)px$/.exec(String(v).trim());
  return m ? parseFloat(m[1]) : null;
}
function msNumber(v) {
  const m = /^([0-9]+(?:[.][0-9]+)?)ms$/.exec(String(v).trim());
  return m ? parseFloat(m[1]) : null;
}
function inScale(list, n) { return n !== null && list.indexOf(n) >= 0; }
function firstDiff(a, b) {
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++) if (a.charAt(i) !== b.charAt(i)) return i;
  return n;
}

// --------------------------------------------------- minimal JSON Schema engine
function isPlainObject(v) { return v !== null && typeof v === 'object' && !Array.isArray(v); }
function instanceType(v) {
  if (v === null) return 'null';
  if (Array.isArray(v)) return 'array';
  return typeof v;
}
function resolvePointer(rootDoc, ref) {
  if (ref.charAt(0) !== '#') throw new Error('unsupported $ref: ' + ref);
  const parts = ref.slice(1).split('/').filter(function (p) { return p.length > 0; });
  let cur = rootDoc;
  for (const p of parts) {
    const key = p.replace(/~1/g, '/').replace(/~0/g, '~');
    cur = cur[key];
    if (cur === undefined) throw new Error('cannot resolve $ref: ' + ref);
  }
  return cur;
}
function validateInstance(instance, schema, rootDoc, loc) {
  const errors = [];
  walk(instance, schema, rootDoc, loc, errors);
  return errors;
}
function walk(instance, schema, rootDoc, loc, errors) {
  if (schema === true) return;
  if (schema === false) { errors.push({ loc: loc, msg: 'schema is false' }); return; }
  if (schema.$ref) { walk(instance, resolvePointer(rootDoc, schema.$ref), rootDoc, loc, errors); return; }
  if (schema.oneOf) {
    let count = 0;
    for (const s of schema.oneOf) if (validateInstance(instance, s, rootDoc, loc).length === 0) count++;
    if (count !== 1) errors.push({ loc: loc, msg: 'oneOf: expected exactly 1 matching schema, got ' + count });
  }
  if (schema.anyOf) {
    let ok = false;
    for (const s of schema.anyOf) if (validateInstance(instance, s, rootDoc, loc).length === 0) { ok = true; break; }
    if (!ok) errors.push({ loc: loc, msg: 'anyOf: no matching schema' });
  }
  if (schema.allOf) for (const s of schema.allOf) walk(instance, s, rootDoc, loc, errors);
  if (schema.not && validateInstance(instance, schema.not, rootDoc, loc).length === 0) errors.push({ loc: loc, msg: 'not: schema unexpectedly matched' });
  if (schema.const !== undefined && instance !== schema.const) errors.push({ loc: loc, msg: 'const: expected ' + JSON.stringify(schema.const) });
  if (schema.enum && schema.enum.indexOf(instance) < 0) errors.push({ loc: loc, msg: 'enum: ' + JSON.stringify(instance) + ' not in ' + JSON.stringify(schema.enum) });
  if (schema.type) {
    const types = Array.isArray(schema.type) ? schema.type : [schema.type];
    const actual = instanceType(instance);
    const ok = types.some(function (t) {
      if (t === actual) return true;
      if (t === 'integer') return actual === 'number' && Number.isInteger(instance);
      if (t === 'number') return actual === 'number';
      return false;
    });
    if (!ok) { errors.push({ loc: loc, msg: 'type: expected ' + types.join('|') + ', got ' + actual }); return; }
  }
  if (typeof instance === 'string') {
    if (schema.minLength !== undefined && instance.length < schema.minLength) errors.push({ loc: loc, msg: 'minLength ' + schema.minLength });
    if (schema.maxLength !== undefined && instance.length > schema.maxLength) errors.push({ loc: loc, msg: 'maxLength ' + schema.maxLength });
    if (schema.pattern && !(new RegExp(schema.pattern)).test(instance)) errors.push({ loc: loc, msg: 'pattern ' + schema.pattern });
  }
  if (typeof instance === 'number') {
    if (schema.minimum !== undefined && instance < schema.minimum) errors.push({ loc: loc, msg: 'minimum ' + schema.minimum });
    if (schema.maximum !== undefined && instance > schema.maximum) errors.push({ loc: loc, msg: 'maximum ' + schema.maximum });
  }
  if (Array.isArray(instance)) {
    if (schema.minItems !== undefined && instance.length < schema.minItems) errors.push({ loc: loc, msg: 'minItems ' + schema.minItems });
    if (schema.items) for (let i = 0; i < instance.length; i++) walk(instance[i], schema.items, rootDoc, loc + '/' + i, errors);
  }
  if (isPlainObject(instance)) {
    if (schema.minProperties !== undefined && Object.keys(instance).length < schema.minProperties) errors.push({ loc: loc, msg: 'minProperties ' + schema.minProperties });
    if (schema.required) for (const r of schema.required) if (!(r in instance)) errors.push({ loc: loc, msg: 'missing required property: ' + r });
    const props = schema.properties || {};
    const patProps = schema.patternProperties || {};
    for (const key of Object.keys(instance)) {
      const childLoc = loc + '/' + key;
      let matched = false;
      if (Object.prototype.hasOwnProperty.call(props, key)) { walk(instance[key], props[key], rootDoc, childLoc, errors); matched = true; }
      for (const pat of Object.keys(patProps)) if ((new RegExp(pat)).test(key)) { walk(instance[key], patProps[pat], rootDoc, childLoc, errors); matched = true; }
      if (!matched) {
        if (schema.additionalProperties === false) errors.push({ loc: childLoc, msg: 'additional property not allowed' });
        else if (isPlainObject(schema.additionalProperties)) walk(instance[key], schema.additionalProperties, rootDoc, childLoc, errors);
      }
      if (schema.propertyNames) walk(key, schema.propertyNames, rootDoc, childLoc, errors);
    }
  }
}

// --------------------------------------------------- gates
export function runChecks(opts) {
  opts = opts || {};
  const root = opts.root || DEFAULT_ROOT;
  const failures = [];
  const warnings = [];
  function fail(code, message) { failures.push({ code: code, message: message }); }

  const src = loadSource(root, opts);
  const report = { schema: null, rulers: null, contrast: null, drift: null, hex: null };

  // [1] schema
  const schemaDoc = JSON.parse(fs.readFileSync(path.join(root, 'tokens', 'tokens.schema.json'), 'utf8'));
  const schemaTargets = BASE_GROUPS.map(function (g) { return 'tokens/base/' + g + '.json'; });
  for (const f of fs.readdirSync(path.join(root, 'tokens', 'themes')).filter(function (f) { return f.endsWith('.json'); }).sort()) schemaTargets.push('tokens/themes/' + f);
  const schemaBad = [];
  for (const rel of schemaTargets) {
    const doc = JSON.parse(fs.readFileSync(path.join(root, rel), 'utf8'));
    const errs = validateInstance(doc, schemaDoc, schemaDoc, rel);
    if (errs.length) for (const e of errs.slice(0, 8)) { schemaBad.push(rel + ' ' + e.loc + ': ' + e.msg); fail('SCHEMA', rel + ' ' + e.loc + ': ' + e.msg); }
  }
  report.schema = { files: schemaTargets.length, bad: schemaBad };

  // [2] AR-22 rulers
  const rulerBad = [];
  for (const k of src.order) {
    const t = src.tokens.get(k);
    if (/^sp-[0-9]+$/.test(k)) {
      const n = pxNumber(t.value);
      if (!inScale(SPACE_SCALE, n)) rulerBad.push('RULER_SPACE ' + k + ' = ' + t.value + ' (allowed: ' + SPACE_SCALE.join('/') + ')');
    }
    if (/^r-[a-z0-9-]+$/.test(k)) {
      const n = pxNumber(t.value);
      if (!inScale(RADIUS_SCALE, n)) rulerBad.push('RULER_RADIUS ' + k + ' = ' + t.value + ' (allowed: ' + RADIUS_SCALE.join('/') + ')');
    }
    if (/^m-[a-z0-9-]+$/.test(k)) {
      const n = msNumber(t.value);
      if (!inScale(MOTION_DURATIONS, n)) rulerBad.push('RULER_MOTION ' + k + ' = ' + t.value + ' (allowed: ' + MOTION_DURATIONS.join('/') + 'ms)');
    }
    if (t.type === 'fontSize') {
      const n = pxNumber(t.value);
      const ladder = t.ladder;
      if (!ladder) {
        rulerBad.push('RULER_FONTSIZE ' + k + ' = ' + t.value + ' has no "ladder" (allowed: ' + Object.keys(FONT_SIZE_LADDERS).join('/') + ')');
      } else if (!FONT_SIZE_LADDERS[ladder]) {
        rulerBad.push('RULER_FONTSIZE ' + k + ' = ' + t.value + ' declares unknown ladder "' + ladder + '" (allowed: ' + Object.keys(FONT_SIZE_LADDERS).join('/') + ')');
      } else if (!inScale(FONT_SIZE_LADDERS[ladder], n)) {
        rulerBad.push('RULER_FONTSIZE ' + k + ' = ' + t.value + ' is off the ' + ladder + ' ladder (allowed: ' + FONT_SIZE_LADDERS[ladder].join('/') + ')');
      }
    }
  }
  for (const ladderName of Object.keys(FONT_SIZE_LADDERS)) {
    const members = src.order.filter(function (k) { return src.tokens.get(k).type === 'fontSize' && src.tokens.get(k).ladder === ladderName; });
    if (members.length === 0) rulerBad.push('RULER_FONTSIZE ladder "' + ladderName + '" has no token');
  }
  const protoHtml = Object.prototype.hasOwnProperty.call(opts, 'prototypeHtml') ? opts.prototypeHtml : fs.readFileSync(path.join(root, PROTOTYPE_REL), 'utf8');
  const iconSizes = new Set();
  const szRe = /data-sz="([0-9]+)"/g;
  let szM;
  while ((szM = szRe.exec(protoHtml)) !== null) iconSizes.add(parseInt(szM[1], 10));
  for (const s of Array.from(iconSizes).sort(function (a, b) { return a - b; })) {
    if (ICON_SIZES.indexOf(s) < 0) rulerBad.push('RULER_ICON prototype data-sz=' + s + ' (allowed: ' + ICON_SIZES.join('/') + ')');
  }
  for (const r of rulerBad) fail(r.split(' ')[0], r);
  report.rulers = {
    checked: src.order.length,
    iconSizes: Array.from(iconSizes).sort(function (a, b) { return a - b; }),
    fontLadders: FONT_SIZE_LADDERS,
    fontUnion: FONT_SIZE_UNION,
    bad: rulerBad,
  };

  // [3] WCAG contrast (real computation)
  const thresholds = src.contrast.thresholds || { text: 4.5, ui: 3.0 };
  const contrastReport = {};
  for (const id of ['dark', 'light']) {
    const resolved = resolveTheme(src, id);
    const rows = [];
    for (const pair of src.contrast.pairs) {
      let ratio;
      try { ratio = pairRatio(resolved, pair); }
      catch (e) { fail('CONTRAST_ERROR', id + ' ' + pair.id + ': ' + e.message); continue; }
      const threshold = thresholds[pair.kind];
      rows.push({ id: pair.id, category: pair.category, kind: pair.kind, ratio: ratio, threshold: threshold });
      if (ratio + 1e-9 < threshold) {
        fail(pair.kind === 'ui' ? 'CONTRAST_UI' : 'CONTRAST_TEXT',
          id + ' ' + pair.id + ' [' + pair.category + '] ' + ratio.toFixed(2) + ':1 < ' + threshold + ':1');
      }
    }
    rows.sort(function (a, b) { return a.ratio - b.ratio; });
    contrastReport[id] = { min: rows[0], tightest: rows.slice(0, 5), count: rows.length };
  }
  report.contrast = contrastReport;

  // [4] drift: regenerate in memory and compare byte-for-byte
  const cleanSrc = loadSource(root);
  const cleanOut = generate(cleanSrc);
  const artifactOverrides = opts.artifacts || {};
  const driftBad = [];
  for (const rel of Object.keys(cleanOut.files).sort()) {
    const generated = cleanOut.files[rel];
    let disk = null;
    if (Object.prototype.hasOwnProperty.call(artifactOverrides, rel)) disk = artifactOverrides[rel];
    else if (fs.existsSync(path.join(root, rel))) disk = fs.readFileSync(path.join(root, rel), 'utf8');
    if (disk === null) driftBad.push('DRIFT_ARTIFACT ' + rel + ' is missing on disk (run: npm run tokens:build)');
    else if (disk !== generated) driftBad.push('DRIFT_ARTIFACT ' + rel + ' differs from generated output at byte ' + firstDiff(disk, generated) + ' (run: npm run tokens:build)');
  }
  const protoDisk = Object.prototype.hasOwnProperty.call(opts, 'prototypeHtml') ? opts.prototypeHtml : fs.readFileSync(path.join(root, PROTOTYPE_REL), 'utf8');
  let block = null;
  try { block = extractPrototypeBlock(protoDisk); } catch (e) { block = null; }
  if (block === null) driftBad.push('DRIFT_PROTOTYPE prototype token markers (' + '@tokens:begin/' + '@tokens:end' + ') missing');
  else if (block !== cleanOut.prototypeBlock) driftBad.push('DRIFT_PROTOTYPE prototype inline token block differs from generated CSS at byte ' + firstDiff(block, cleanOut.prototypeBlock) + ' (run: npm run tokens:build)');
  for (const d of driftBad) fail(d.split(' ')[0], d);
  report.drift = { artifacts: Object.keys(cleanOut.files).length, bad: driftBad };

  // [5] hex debt scan (warning only, AR-22 zero-hardcoded-color goal)
  const protoDir = path.join(root, 'prototype');
  const hexFiles = [];
  for (const f of fs.readdirSync(protoDir).filter(function (f) { return f.endsWith('.html'); }).sort()) {
    let html = fs.readFileSync(path.join(protoDir, f), 'utf8');
    if (f === path.basename(PROTOTYPE_REL)) html = html.replace(/[/][*] @tokens:begin [*][/][\s\S]*?[/][*] @tokens:end [*][/]/g, '');
    html = html.replace(/html\[data-theme="?[a-z]+"?\]\s*{[^}]*}/g, '');
    html = html.replace(/:root\s*{[^}]*}/g, '');
    const hist = new Map();
    const hexRe = /#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{4}|[0-9a-fA-F]{3})(?![0-9A-Za-z_])/g;
    let m;
    while ((m = hexRe.exec(html)) !== null) {
      const key = m[0].toUpperCase();
      hist.set(key, (hist.get(key) || 0) + 1);
    }
    let total = 0;
    for (const v of hist.values()) total += v;
    const sorted = Array.from(hist.entries()).sort(function (a, b) { return b[1] - a[1] || (a[0] < b[0] ? -1 : 1); });
    hexFiles.push({ file: f, total: total, colors: sorted });
    if (total > 0) warnings.push('HEX_DEBT ' + f + ': ' + total + ' hardcoded hex colors outside the token block (' + sorted.length + ' distinct)');
  }
  report.hex = hexFiles;

  return { failures: failures, warnings: warnings, report: report, src: src };
}

// --------------------------------------------------- reporting
export function printReport(res) {
  const r = res.report;
  const L = [];
  L.push('=== tokens:check (DC-09 / AR-22) ===');
  L.push('[1/5] schema: ' + (r.schema.bad.length === 0 ? 'OK' : 'FAIL') + ' - ' + r.schema.files + ' files validated against tokens/tokens.schema.json');
  if (r.schema.bad.length) for (const b of r.schema.bad.slice(0, 6)) L.push('        - ' + b);
  L.push('[2/5] rulers: ' + (r.rulers.bad.length === 0 ? 'OK' : 'FAIL') + ' - ' + r.rulers.checked + ' tokens; icon sizes in prototype: ' + r.rulers.iconSizes.join('/'));
  if (r.rulers.fontLadders) {
    for (const name of Object.keys(r.rulers.fontLadders)) {
      L.push('        font ladder ' + name.padEnd(8) + ': ' + r.rulers.fontLadders[name].join(' / ') + ' px');
    }
  }
  for (const b of r.rulers.bad) L.push('        - ' + b);
  L.push('[3/5] contrast (WCAG 2.x, real formula):');
  for (const id of ['dark', 'light']) {
    const c = r.contrast[id];
    L.push('        ' + id.padEnd(5) + ' min=' + c.min.ratio.toFixed(2) + ':1 (' + c.min.id + ')  ' + c.count + ' pairs');
    for (const t of c.tightest) L.push('          ' + t.ratio.toFixed(2) + ':1  ' + (t.kind === 'ui' ? '>=3.0' : '>=4.5') + '  ' + t.id + '  [' + t.category + ']');
  }
  L.push('[4/5] drift: ' + (r.drift.bad.length === 0 ? 'OK' : 'FAIL') + ' - ' + r.drift.artifacts + ' generated artifacts + prototype inline block match byte-for-byte');
  for (const b of r.drift.bad) L.push('        - ' + b);
  L.push('[5/5] hex debt (warn only, AR-22 target = 0):');
  for (const f of r.hex) L.push('        ' + f.file + ': ' + f.total + ' hex colors outside token block (' + f.colors.length + ' distinct)' + (f.colors.length ? ': ' + f.colors.map(function (c) { return c[0] + 'x' + c[1]; }).join(', ') : ''));
  if (res.warnings.length) { L.push('warnings:'); for (const w of res.warnings) L.push('  - ' + w); }
  L.push('result: ' + (res.failures.length === 0 ? 'PASS' : 'FAIL (' + res.failures.length + ')'));
  if (res.failures.length) { L.push('failures:'); for (const f of res.failures) L.push('  - [' + f.code + '] ' + f.message); }
  console.log(L.join('\n'));
}

// --------------------------------------------------- selftest
function assert(cond, msg) {
  if (!cond) { console.error('SELFTEST FAIL: ' + msg); process.exit(1); }
}
function hasCode(res, code) { return res.failures.some(function (f) { return f.code === code; }); }
function grayWithRatio(bg, ratio) {
  const lb = relativeLuminance(bg);
  const target = ratio * (lb + 0.05) - 0.05;
  const c = linearToSrgb(Math.min(1, Math.max(0, target)));
  return '#' + [c, c, c].map(function (x) { return x.toString(16).padStart(2, '0'); }).join('');
}
function runSelftest() {
  const root = DEFAULT_ROOT;
  const lines = [];
  const base = runChecks({});
  if (base.failures.length) { printReport(base); assert(false, 'baseline must pass before selftest can prove failure detection'); }
  lines.push('baseline: PASS (' + base.report.contrast.dark.count + ' contrast pairs x 2 themes, 0 failures)');

  // injection 1: a space token off the 4/8/12/16/24/32/48 ruler
  const r1 = runChecks({ base: { 'sp-1': '13px' } });
  assert(hasCode(r1, 'RULER_SPACE'), 'space=13px must be caught by RULER_SPACE');
  lines.push('inject space sp-1=13px      -> caught [' + r1.failures.filter(function (f) { return f.code === 'RULER_SPACE'; }).length + ' x RULER_SPACE]');

  // injection 1b: a value that IS on the UI ladder but NOT on the terminal ladder.
  // This proves the two ladders are actually distinct (13 is legal for UI, illegal for terminal).
  const r1b = runChecks({ base: { 'term-fs': '13px' } });
  assert(hasCode(r1b, 'RULER_FONTSIZE'), 'term-fs=13px must be caught by RULER_FONTSIZE (13 is UI-only)');
  lines.push('inject term-fs=13px (UI-only) -> caught [' + r1b.failures.filter(function (f) { return f.code === 'RULER_FONTSIZE'; }).length + ' x RULER_FONTSIZE]');

  // injection 2: squash a contrast pair to ~2:1
  const src = loadSource(root);
  const canvas = parseColor(resolveTheme(src, 'dark').get('canvas'));
  const gray = grayWithRatio(canvas, 2.0);
  const r2 = runChecks({ base: { 'text': gray } });
  assert(hasCode(r2, 'CONTRAST_TEXT'), 'text squeezed to ~2:1 must be caught by CONTRAST_TEXT');
  const ratio = pairRatio(resolveTheme(loadSource(root, { base: { 'text': gray } }), 'dark'), { fg: 'text', bg: 'canvas', kind: 'text' });
  lines.push('inject text=' + gray + ' (~2:1) -> caught [' + r2.failures.filter(function (f) { return f.code === 'CONTRAST_TEXT'; }).length + ' x CONTRAST_TEXT], measured ' + ratio.toFixed(2) + ':1');

  // injection 3: change one byte of a generated artifact
  const cssRel = 'packages/tokens/dist/tokens.css';
  const cssDisk = fs.readFileSync(path.join(root, cssRel), 'utf8');
  const corrupt = cssDisk.slice(0, cssDisk.length - 1) + (cssDisk.slice(-1) === '\n' ? ' ' : 'X');
  const artifacts = {};
  artifacts[cssRel] = corrupt;
  const r3 = runChecks({ artifacts: artifacts });
  assert(hasCode(r3, 'DRIFT_ARTIFACT'), 'one-byte artifact change must be caught by DRIFT_ARTIFACT');
  lines.push('inject ' + cssRel + ' (1 byte) -> caught [' + r3.failures.filter(function (f) { return f.code === 'DRIFT_ARTIFACT'; }).length + ' x DRIFT_ARTIFACT]');

  // injection 4: change one byte of the prototype inline token block
  const protoPath = path.join(root, PROTOTYPE_REL);
  const protoHtml = fs.readFileSync(protoPath, 'utf8');
  const tampered = protoHtml.replace('--canvas: #08090C;', '--canvas: #08090D;');
  assert(tampered !== protoHtml, 'prototype tamper target must exist');
  const r4 = runChecks({ prototypeHtml: tampered });
  assert(hasCode(r4, 'DRIFT_PROTOTYPE'), 'one-byte prototype token change must be caught by DRIFT_PROTOTYPE');
  lines.push('inject prototype --canvas (1 byte) -> caught [' + r4.failures.filter(function (f) { return f.code === 'DRIFT_PROTOTYPE'; }).length + ' x DRIFT_PROTOTYPE]');

  console.log('=== tokens:check --selftest ===');
  for (const l of lines) console.log('  ' + l);
  console.log('result: PASS - validator rejects injected faults and is not always-green');
  process.exit(0);
}

// --------------------------------------------------- entry
if (process.argv.indexOf('--selftest') >= 0) {
  runSelftest();
} else {
  const res = runChecks({});
  printReport(res);
  process.exit(res.failures.length === 0 ? 0 : 1);
}
