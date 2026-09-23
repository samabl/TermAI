#!/usr/bin/env node
// tools/conformance/gen-spec.mjs
//
// Materialise the curated suites ("termai-invariants", "xterm-ctlseqs-spec") from
// tools/conformance/data/spec-cases.mjs.
//
//   node tools/conformance/gen-spec.mjs [--check]
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { CASES, SUITES } from './data/spec-cases.mjs';
import { canonicalJson } from './lib/report.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const CASES_DIR = path.join(HERE, 'cases');
const REGISTRY = path.join(HERE, 'data', 'ctlseqs-entries.json');

function knowledgeOfEntry() {
  const known = new Set();
  if (!fs.existsSync(REGISTRY)) return known;
  const registry = JSON.parse(fs.readFileSync(REGISTRY, 'utf8'));
  for (const entry of registry.entries) {
    if (entry.resolved) known.add(entry.resolved_pattern);
  }
  return known;
}

function build() {
  const known = knowledgeOfEntry();
  const problems = [];
  const seen = new Set();
  const bySuite = new Map();
  for (const suite of Object.keys(SUITES)) bySuite.set(suite, { bodies: new Map(), index: [] });
  for (const c of CASES) {
    if (!c.id || !/^[a-z0-9][a-z0-9._-]*$/.test(c.id)) problems.push('bad case id ' + JSON.stringify(c.id));
    if (seen.has(c.id)) problems.push('duplicate case id ' + c.id);
    seen.add(c.id);
    if (!SUITES[c.suite]) { problems.push(c.id + ': unknown suite ' + c.suite); continue; }
    if (!Array.isArray(c.body) || c.body.length === 0) problems.push(c.id + ': empty body');
    const oracle = c.oracle || SUITES[c.suite].oracle;
    const gating = c.gating === undefined ? true : c.gating === true;
    const lane = c.lane === undefined ? 'L0' : c.lane;
    if (oracle === 'invariant' && gating) problems.push(c.id + ': invariant oracle cannot gate');
    if (gating && lane !== 'L0') problems.push(c.id + ': only L0 cases may gate');
    const entry = c.entry && known.has(c.entry) ? c.entry : '';
    if (c.entry && !entry) problems.push(c.id + ': ctlseqs entry not in the registry: ' + c.entry);
    const slot = bySuite.get(c.suite);
    slot.bodies.set(c.id + '.trec', c.body.join('\n') + '\n');
    slot.index.push({
      id: c.id,
      suite: c.suite,
      lane: lane,
      oracle: oracle,
      gating: gating,
      cols: c.cols === undefined ? 80 : c.cols,
      rows: c.rows === undefined ? 24 : c.rows,
      ctlseqs_entry: entry,
      ctlseqs_mnemonic: c.mnemonic || '',
      documented: c.documented || '',
      real_corpus: c.real_corpus === true,
      requires: c.requires || [],
      expect_responses: c.expect_responses || undefined,
      note: c.note || undefined,
    });
  }
  for (const slot of bySuite.values()) slot.index.sort(function (a, b) { return a.id < b.id ? -1 : 1; });
  return { problems: problems, bySuite: bySuite };
}

function main() {
  const check = process.argv.indexOf('--check') >= 0;
  const built = build();
  if (built.problems.length) {
    console.error('gen-spec: case table is invalid:');
    for (const p of built.problems) console.error('  - ' + p);
    process.exit(2);
  }
  const wanted = [];
  for (const [suite, slot] of built.bySuite) {
    wanted.push({ suite: suite, dir: path.join(CASES_DIR, suite), bodies: slot.bodies,
      index: slot.index.map(function (o) { return JSON.stringify(o); }).join('\n') + '\n',
      suiteJson: canonicalJson(Object.assign({ suite: suite }, SUITES[suite])) });
  }
  if (check) {
    const problems = [];
    for (const s of wanted) {
      const indexPath = path.join(s.dir, 'index.jsonl');
      if (!fs.existsSync(indexPath)) { problems.push('missing ' + indexPath); continue; }
      if (fs.readFileSync(indexPath, 'utf8') !== s.index) problems.push(s.suite + '/index.jsonl is stale');
      for (const [name, body] of s.bodies) {
        const p = path.join(s.dir, name);
        if (!fs.existsSync(p) || fs.readFileSync(p, 'utf8') !== body) problems.push(s.suite + '/' + name + ' is stale or missing');
      }
    }
    if (problems.length) {
      console.log('gen-spec --check FAIL');
      for (const p of problems) console.log('  - ' + p);
      process.exit(1);
    }
    console.log('gen-spec --check PASS (' + CASES.length + " curated cases match the table)");
    return;
  }
  let written = 0;
  for (const s of wanted) {
    fs.mkdirSync(s.dir, { recursive: true });
    fs.writeFileSync(path.join(s.dir, 'index.jsonl'), s.index);
    fs.writeFileSync(path.join(s.dir, 'suite.json'), s.suiteJson);
    const stale = fs.readdirSync(s.dir).filter(function (f) {
      return f.endsWith('.trec') && !s.bodies.has(f);
    });
    for (const f of stale) fs.unlinkSync(path.join(s.dir, f));
    for (const [name, body] of s.bodies) fs.writeFileSync(path.join(s.dir, name), body);
    written += s.bodies.size;
    console.log('gen-spec: ' + s.suite + ' -> ' + s.bodies.size + ' case(s)' + (stale.length ? ', removed ' + stale.length + ' stale' : ''));
  }
  console.log('gen-spec: ' + written + ' curated case(s) total');
}

main();
