// tools/tokens/build.mjs
// TermAI design-token code generator (zero external dependencies, ADR-0015).
// Inputs: tokens/base/*.json (seven groups) + tokens/themes/*.json (overrides).
// Outputs (committed): packages/tokens/dist/* and crates/termai-tokens/*, plus the
// inlined token block of prototype/termai-ui-terminal-first.html (single-file, file://).
import fs from 'node:fs';
import path from 'node:path';
import { DEFAULT_ROOT, loadSource, generate, splicePrototype, PROTOTYPE_REL } from './lib.mjs';

const root = DEFAULT_ROOT;
const src = loadSource(root);
const out = generate(src);

const results = [];
function writeRel(rel, content) {
  const p = path.join(root, rel);
  fs.mkdirSync(path.dirname(p), { recursive: true });
  const existed = fs.existsSync(p);
  const before = existed ? fs.readFileSync(p, 'utf8') : null;
  fs.writeFileSync(p, content, 'utf8');
  results.push({ rel: rel, bytes: Buffer.byteLength(content), status: !existed ? 'created' : (before === content ? 'unchanged' : 'updated') });
}

for (const rel of Object.keys(out.files).sort()) writeRel(rel, out.files[rel]);

const protoPath = path.join(root, PROTOTYPE_REL);
const html = fs.readFileSync(protoPath, 'utf8');
const next = splicePrototype(html, out.prototypeBlock);
if (next !== html) fs.writeFileSync(protoPath, next, 'utf8');
results.push({ rel: PROTOTYPE_REL + ' [@tokens block]', bytes: Buffer.byteLength(out.prototypeBlock), status: next === html ? 'unchanged' : 'updated' });

const byGroup = {};
for (const k of src.order) {
  const g = src.tokens.get(k).group;
  byGroup[g] = (byGroup[g] || 0) + 1;
}
console.log('tokens:build - ' + src.order.length + ' tokens across ' + Object.keys(byGroup).length + ' groups, ' + Object.keys(src.themes).length + ' themes');
console.log('  groups: ' + Object.keys(byGroup).map(function (g) { return g + '=' + byGroup[g]; }).join(', '));
console.log('  outputs (' + results.length + '):');
for (const r of results) console.log('    ' + r.status.padEnd(9) + ' ' + r.rel + '  ' + r.bytes + ' B');
