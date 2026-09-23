// tools/design-gates/lib.mjs
// TermAI design-gate shared utilities (zero external dependencies, ADR-0015).
// Consumers: check-design.mjs (static layer S1-S10), browser.mjs (browser layer B1-B8),
// and both --selftest harnesses. Pure Node standard library only.
//
// This module deliberately does NOT own the design-token truth source: token rulers and
// the prototype inline token block are read from tokens/ and tools/tokens/lib.mjs, so the
// gates cannot drift from the token pipeline (DC-09 / AR-22).
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import zlib from 'node:zlib';
import { createHash } from 'node:crypto';

export const TOOL_DIR = path.dirname(fileURLToPath(import.meta.url));
export const DEFAULT_ROOT = path.resolve(TOOL_DIR, '..', '..');

// The single implemented artifact of this phase (AR-23 reference prototype).
export const PROTOTYPE_REL = 'prototype/termai-ui-terminal-first.html';
export const BASELINE_DIR_REL = 'prototype/baseline';

// Re-exported so callers do not hardcode the markers (they belong to the token pipeline).
export const TOKEN_MARK_BEGIN = '/* @tokens:begin */';
export const TOKEN_MARK_END = '/* @tokens:end */';

// ------------------------------------------------------------------ fs / text

export function readText(p) {
  return fs.readFileSync(p, 'utf8');
}

export function readTextOrNull(p) {
  try {
    return fs.readFileSync(p, 'utf8');
  } catch (err) {
    return null;
  }
}

export function readJson(p) {
  return JSON.parse(readText(p));
}

export function sha256(buf) {
  return createHash('sha256').update(buf).digest('hex');
}

// ------------------------------------------------------------------ token rulers
// The legal value sets are DERIVED from tokens/base/*.json (AR-22: "ruler value or
// nothing"). Nothing here is hardcoded, so changing a ruler in tokens/ moves the gate.

function pxNumber(v) {
  const m = /^(-?[0-9]+(?:\.[0-9]+)?)px$/.exec(String(v).trim());
  return m ? parseFloat(m[1]) : null;
}

function msNumber(v) {
  const m = /^(-?[0-9]+(?:\.[0-9]+)?)ms$/.exec(String(v).trim());
  return m ? parseFloat(m[1]) : null;
}

export function loadTokenScales(root) {
  root = root || DEFAULT_ROOT;
  const space = readJson(path.join(root, 'tokens', 'base', 'space.json'));
  const radius = readJson(path.join(root, 'tokens', 'base', 'radius.json'));
  const type = readJson(path.join(root, 'tokens', 'base', 'type.json'));
  const motion = readJson(path.join(root, 'tokens', 'base', 'motion.json'));

  const spacing = [];
  for (const [k, t] of Object.entries(space.tokens)) {
    if (!/^sp-[0-9]+$/.test(k)) continue;
    const n = pxNumber(t.value);
    if (n !== null) spacing.push(n);
  }
  const radii = [];
  for (const [k, t] of Object.entries(radius.tokens)) {
    if (!/^r-[a-z0-9-]+$/.test(k)) continue;
    const n = pxNumber(t.value);
    if (n !== null) radii.push(n);
  }
  const fontSizes = [];
  for (const t of Object.values(type.tokens)) {
    if (t.type !== 'fontSize') continue;
    const n = pxNumber(t.value);
    if (n !== null) fontSizes.push(n);
  }
  const durations = [];
  for (const t of Object.values(motion.tokens)) {
    if (t.type !== 'duration') continue;
    const n = msNumber(t.value);
    if (n !== null) durations.push(n);
  }
  const uniq = (a) => Array.from(new Set(a)).sort((x, y) => x - y);
  return {
    spacing: uniq(spacing),
    radius: uniq(radii),
    fontSize: uniq(fontSizes),
    duration: uniq(durations),
    // Documented hairline / structural exemptions from the prototype rule table
    // (prototype header [1](a): 1px borders, 1px dots, 1.5px glyphs, 2px guides).
    spacingExempt: [0, 1, 1.5, 2],
    radiusExempt: [0, 1, 1.5],
    radiusPercentExempt: [50],
    // The terminal cursor breathing is fixed at 200ms by docs/spec/02 §3.15 and is not a
    // UI transition; every other duration must come from the motion ruler.
    durationExempt: [0, 200],
  };
}

export function fmtScale(list, unit) {
  return list.map((n) => n + (unit || '')).join('/');
}

// ------------------------------------------------------------------ CSS parsing

export function extractStyle(html) {
  const m = /<style[^>]*>([\s\S]*?)<\/style>/i.exec(html);
  return m ? m[1] : '';
}

export function stripCssComments(css) {
  return css.replace(/\/\*[\s\S]*?\*\//g, '');
}

// Returns flat declarations: { prop, value, index }. Selectors/at-rules are not modelled;
// we only need property values for the ruler check.
export function parseDeclarations(css) {
  const src = stripCssComments(css);
  const out = [];
  const re = /(^|[;{]\s*)(--[a-zA-Z0-9-]+|[a-zA-Z-][a-zA-Z0-9-]*)\s*:\s*([^;{}]+)/g;
  let m;
  while ((m = re.exec(src)) !== null) {
    out.push({ prop: m[2].toLowerCase(), value: m[3].trim(), index: m.index });
  }
  return out;
}

// Extract numeric lengths with their unit. "0" has no unit and is reported as unit ''.
export function extractLengths(value) {
  // var() references resolve to token values, which tools/tokens/check.mjs already keeps
  // on the ruler; remove them so identifiers such as "--sp-3" are not misread as numbers.
  const cleaned = String(value).replace(/var\([^)]*\)/g, ' ');
  const out = [];
  const re = /(?<![\w.%#-])(-?[0-9]+(?:\.[0-9]+)?)(px|%|em|rem|vh|vw|ms|s)?(?![\w%])/g;
  let m;
  while ((m = re.exec(cleaned)) !== null) {
    out.push({ num: parseFloat(m[1]), unit: m[2] || '' });
  }
  return out;
}

// ------------------------------------------------------------------ HTML scanning
// The prototype is a single well-formed HTML file; a light tag scanner is enough to run
// the static gates without a browser (S layer must run everywhere).

const VOID_TAGS = new Set(['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'param', 'source', 'track', 'wbr']);

export function parseAttrs(attrStr) {
  const attrs = {};
  const re = /([a-zA-Z_:][-a-zA-Z0-9_:.]*)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>]+)))?/g;
  let m;
  while ((m = re.exec(attrStr)) !== null) {
    const v = m[2] !== undefined ? m[2] : m[3] !== undefined ? m[3] : m[4] !== undefined ? m[4] : '';
    attrs[m[1].toLowerCase()] = v;
  }
  return attrs;
}

export function hasClass(attrs, cls) {
  return String(attrs.class || '').split(/\s+/).indexOf(cls) >= 0;
}

export function classList(attrs) {
  return String(attrs.class || '').split(/\s+/).filter(Boolean);
}

export function describeTag(t) {
  const cls = t.attrs.class ? '.' + t.attrs.class.split(/\s+/).join('.') : '';
  const id = t.attrs.id ? '#' + t.attrs.id : '';
  return '<' + t.tag + id + cls + '>';
}

// Only the markup region (before <script>) is scanned: JS string literals contain SVG
// markup that must not be mistaken for real elements.
export function scanTags(html) {
  const scriptAt = html.search(/<script\b/i);
  const region = scriptAt >= 0 ? html.slice(0, scriptAt) : html;
  const re = /<(\/?)([a-zA-Z][a-zA-Z0-9:-]*)((?:"[^"]*"|'[^']*'|[^>"'])*?)(\/?)>/g;
  const tags = [];
  let m;
  while ((m = re.exec(region)) !== null) {
    tags.push({
      index: m.index,
      end: re.lastIndex,
      closing: m[1] === '/',
      tag: m[2].toLowerCase(),
      attrStr: m[3] || '',
      selfClose: m[4] === '/',
      attrs: parseAttrs(m[3] || ''),
      raw: m[0],
      region,
    });
  }
  return tags;
}

export function matchCloseTag(tags, openIdx) {
  const open = tags[openIdx];
  if (open.selfClose || VOID_TAGS.has(open.tag)) return openIdx;
  let depth = 0;
  for (let i = openIdx + 1; i < tags.length; i++) {
    const t = tags[i];
    if (t.tag !== open.tag) continue;
    if (t.closing) {
      if (depth === 0) return i;
      depth--;
    } else if (!t.selfClose && !VOID_TAGS.has(t.tag)) {
      depth++;
    }
  }
  return -1;
}

// All tags matching a predicate, each with its matching close index.
export function findAllTags(tags, predicate) {
  const out = [];
  for (let i = 0; i < tags.length; i++) {
    if (tags[i].closing) continue;
    if (predicate(tags[i])) out.push({ open: i, close: matchCloseTag(tags, i) });
  }
  return out;
}

export function elementHtml(html, tags, openIdx, closeIdx) {
  if (closeIdx < 0) return '';
  return html.slice(tags[openIdx].index, tags[closeIdx].end);
}

// Visible text of an element: strip tags, decode the few entities the prototype uses.
export function textOf(html) {
  return html
    .replace(/<[^>]*>/g, ' ')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#92;/g, '\\')
    .replace(/&nbsp;/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

// ------------------------------------------------------------------ hex debt (AR-22 target = 0)
// Same scan contract as tools/tokens/check.mjs [5]: hex colors outside the token block,
// with the token block and the two generated theme rules removed first.
export function stripTokenBlock(html) {
  const i = html.indexOf(TOKEN_MARK_BEGIN);
  const j = html.indexOf(TOKEN_MARK_END);
  if (i < 0 || j < 0 || j < i) return { stripped: html, block: null };
  return { stripped: html.slice(0, i) + html.slice(j + TOKEN_MARK_END.length), block: html.slice(i, j + TOKEN_MARK_END.length) };
}

export function countHardcodedHex(html) {
  const { stripped } = stripTokenBlock(html);
  let s = stripped.replace(/[/][*] @tokens:begin [*][/][\s\S]*?[/][*] @tokens:end [*][/]/g, '');
  s = s.replace(/html\[data-theme="?[a-z]+"?\]\s*{[^}]*}/g, '');
  s = s.replace(/:root\s*{[^}]*}/g, '');
  const hist = new Map();
  const re = /#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{4}|[0-9a-fA-F]{3})(?![0-9A-Za-z_])/g;
  let m;
  while ((m = re.exec(s)) !== null) {
    const key = m[0].toUpperCase();
    hist.set(key, (hist.get(key) || 0) + 1);
  }
  const colors = Array.from(hist.entries()).sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1));
  let total = 0;
  for (const c of colors) total += c[1];
  return { total, distinct: colors.length, colors };
}

// ------------------------------------------------------------------ PNG decode + image diff
// Chrome emits non-interlaced 8-bit PNG. Decoding in-process keeps the baseline check
// dependency-free (no pixelmatch / pngjs).

export function decodePng(buf) {
  if (buf.length < 8 || buf.readUInt32BE(0) !== 0x89504e47) throw new Error('not a PNG');
  let pos = 8;
  let width = 0, height = 0, bitDepth = 0, colorType = 0, interlace = 0;
  const idat = [];
  while (pos + 8 <= buf.length) {
    const len = buf.readUInt32BE(pos);
    const type = buf.toString('latin1', pos + 4, pos + 8);
    const data = buf.subarray(pos + 8, pos + 8 + len);
    if (type === 'IHDR') {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
      interlace = data[12];
    } else if (type === 'IDAT') {
      idat.push(data);
    } else if (type === 'IEND') {
      break;
    }
    pos += 12 + len;
  }
  if (bitDepth !== 8) throw new Error('unsupported PNG bit depth ' + bitDepth);
  if (interlace !== 0) throw new Error('interlaced PNG not supported');
  const channelsByType = { 0: 1, 2: 3, 4: 2, 6: 4 };
  const channels = channelsByType[colorType];
  if (!channels) throw new Error('unsupported PNG color type ' + colorType);
  const raw = zlib.inflateSync(Buffer.concat(idat));
  const stride = width * channels;
  const out = new Uint8Array(width * height * 4);
  let prev = Buffer.alloc(stride);
  let rp = 0;
  for (let y = 0; y < height; y++) {
    const ft = raw[rp++];
    const line = raw.subarray(rp, rp + stride);
    rp += stride;
    const cur = Buffer.alloc(stride);
    for (let i = 0; i < stride; i++) {
      const a = i >= channels ? cur[i - channels] : 0;
      const b = prev[i];
      const c = i >= channels ? prev[i - channels] : 0;
      const x = line[i];
      let v;
      switch (ft) {
        case 0: v = x; break;
        case 1: v = x + a; break;
        case 2: v = x + b; break;
        case 3: v = x + ((a + b) >> 1); break;
        case 4: {
          const p = a + b - c;
          const pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
          v = x + (pa <= pb && pa <= pc ? a : pb <= pc ? b : c);
          break;
        }
        default: throw new Error('bad PNG filter ' + ft);
      }
      cur[i] = v & 0xff;
    }
    for (let x = 0; x < width; x++) {
      const o = (y * width + x) * 4;
      const s = x * channels;
      if (channels >= 3) {
        out[o] = cur[s];
        out[o + 1] = cur[s + 1];
        out[o + 2] = cur[s + 2];
        out[o + 3] = channels === 4 ? cur[s + 3] : 255;
      } else if (channels === 1) {
        out[o] = out[o + 1] = out[o + 2] = cur[s];
        out[o + 3] = 255;
      } else {
        out[o] = out[o + 1] = out[o + 2] = cur[s];
        out[o + 3] = cur[s + 1];
      }
    }
    prev = cur;
  }
  return { width, height, data: out };
}

export function diffImages(a, b, channelTolerance) {
  const tol = channelTolerance || 0;
  if (a.width !== b.width || a.height !== b.height) {
    return { sizeMismatch: true, a: { w: a.width, h: a.height }, b: { w: b.width, h: b.height }, diffPixels: -1, total: 0, diffPct: 100, bbox: null };
  }
  const total = a.width * a.height;
  let diff = 0;
  let x0 = Infinity, y0 = Infinity, x1 = -1, y1 = -1;
  for (let i = 0; i < total; i++) {
    const o = i * 4;
    if (
      Math.abs(a.data[o] - b.data[o]) > tol ||
      Math.abs(a.data[o + 1] - b.data[o + 1]) > tol ||
      Math.abs(a.data[o + 2] - b.data[o + 2]) > tol ||
      Math.abs(a.data[o + 3] - b.data[o + 3]) > tol
    ) {
      diff++;
      const x = i % a.width;
      const y = (i / a.width) | 0;
      if (x < x0) x0 = x;
      if (y < y0) y0 = y;
      if (x > x1) x1 = x;
      if (y > y1) y1 = y;
    }
  }
  return {
    sizeMismatch: false,
    diffPixels: diff,
    total,
    diffPct: total ? (diff / total) * 100 : 0,
    bbox: diff ? { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 } : null,
  };
}

// ------------------------------------------------------------------ report model

export const STATUS = { PASS: 'PASS', FAIL: 'FAIL', WARN: 'WARN', SKIP: 'SKIP' };

export function gate(id, title, status, detail, notes) {
  return { id, title, status, detail: detail || '', notes: notes || [] };
}

export function summarize(gates) {
  const counts = { PASS: 0, FAIL: 0, WARN: 0, SKIP: 0 };
  for (const g of gates) counts[g.status] = (counts[g.status] || 0) + 1;
  return counts;
}

export function formatReport(title, sections, opts) {
  opts = opts || {};
  const lines = [];
  lines.push('=== ' + title + ' ===');
  const all = [];
  for (const sec of sections) {
    lines.push(sec.name + ':');
    for (const g of sec.gates) {
      all.push(g);
      lines.push('  [' + g.id + '] ' + g.status.padEnd(4) + ' ' + g.title + (g.detail ? ' - ' + g.detail : ''));
      for (const n of g.notes) lines.push('        ' + n);
    }
  }
  const c = summarize(all);
  lines.push('summary: ' + c.PASS + ' PASS / ' + c.FAIL + ' FAIL / ' + c.WARN + ' WARN / ' + c.SKIP + ' SKIP  (' + all.length + ' gates)');
  const failed = all.filter((g) => g.status === STATUS.FAIL);
  const result = failed.length === 0 ? 'PASS' : 'FAIL';
  lines.push('result: ' + result + (failed.length ? ' (' + failed.length + ' blocking)' : ''));
  if (failed.length) {
    lines.push('blocking failures:');
    for (const g of failed) lines.push('  - [' + g.id + '] ' + g.title + (g.detail ? ' - ' + g.detail : ''));
  }
  if (opts.footer) lines.push(opts.footer);
  return { text: lines.join('\n'), counts: c, failed, result };
}

// ------------------------------------------------------------------ selftest helper

export function makeSelftest() {
  const rows = [];
  let failed = 0;
  return {
    check(name, condition, detail) {
      const ok = !!condition;
      if (!ok) failed++;
      rows.push({ name, ok, detail: detail || '' });
      return ok;
    },
    skip(name, detail) {
      rows.push({ name, ok: null, detail: detail || '' });
    },
    report(title) {
      const lines = ['=== ' + title + ' ==='];
      for (const r of rows) {
        const tag = r.ok === null ? 'SKIP' : r.ok ? 'caught' : 'MISSED';
        lines.push('  ' + tag.padEnd(7) + ' ' + r.name + (r.detail ? '  (' + r.detail + ')' : ''));
      }
      const caught = rows.filter((r) => r.ok === true).length;
      const skipped = rows.filter((r) => r.ok === null).length;
      lines.push('  injected faults caught: ' + caught + '/' + (rows.length - skipped) + (skipped ? ' (' + skipped + ' skipped)' : ''));
      const ok = failed === 0;
      lines.push('result: ' + (ok ? 'PASS - every executed injection was caught; the gates are not always-green' : 'FAIL - ' + failed + ' injection(s) were not caught'));
      return { text: lines.join('\n'), ok, failed, caught, skipped, total: rows.length };
    },
  };
}

// Copy one file, preserving its relative path, into another root.
export function copyFileInto(root, rel, destRoot) {
  const src = path.join(root, rel);
  const dst = path.join(destRoot, rel);
  fs.mkdirSync(path.dirname(dst), { recursive: true });
  fs.copyFileSync(src, dst);
  return dst;
}

// ------------------------------------------------------------------ keymap (AR-29 item 7 / S10)
// The prototype keeps keyboard bindings in two layers, and S10 cross-checks them:
//
//   1. ACTION REGISTRY (truth source) - an embedded JSON block
//      <script type="application/json" id="actionRegistry">[ { id, display, mac, win, linux } ]</script>
//      Every bound action declares its STABLE action id, the platform-agnostic `display` form
//      used in the UI, and an EXPLICIT expansion for macOS / Windows / Linux (AR-29 item 7).
//      An action that cannot exist on a platform may instead carry `platformExclusive` and a
//      single platform field.
//
//   2. DISPLAY SITES (must agree with the registry) - the settings shortcut table rows
//      (<tr data-action="..."> key cell), the command-palette items (.pal-item[data-run] > .kb)
//      and the popup-menu items (.pop-item[data-act] > .pi-tag); plus every `Mod+...` prose
//      mention (data-tip / toast / picker data). These are parsed back and required to resolve
//      to a registry entry, so a new shortcut cannot be added without a three-platform
//      declaration.
//
// The helpers below only parse; the assertions live in check-design.mjs (S10).

export const ACTION_REGISTRY_ID = 'actionRegistry';

// The prototype writes a literal backslash as the HTML entity &#92; so the source carries no
// ambiguous escape; decode it before comparing display strings.
export function decodeKeyEntities(s) {
  return String(s).replace(/&#92;/g, '\\');
}

export function extractActionRegistry(html) {
  const re = new RegExp(
    '<script\\b[^>]*\\bid\\s*=\\s*["\']' + ACTION_REGISTRY_ID + '["\'][^>]*>([\\s\\S]*?)<\\/script>',
    'i'
  );
  const m = re.exec(html);
  if (!m) {
    return { found: false, raw: '', entries: [], error: 'structured keymap missing: no <script id="' + ACTION_REGISTRY_ID + '"> block' };
  }
  const raw = m[0];
  let parsed;
  try {
    parsed = JSON.parse(m[1]);
  } catch (err) {
    return { found: true, raw, entries: [], error: 'action registry is not valid JSON (' + err.message + ')' };
  }
  if (!Array.isArray(parsed)) {
    return { found: true, raw, entries: [], error: 'action registry must be a JSON array of actions' };
  }
  return { found: true, raw, entries: parsed, error: null };
}

const MODIFIER_ALIASES = {
  mod: 'mod', cmd: 'cmd', command: 'cmd', '\u2318': 'cmd', meta: 'cmd', super: 'cmd', win: 'cmd',
  ctrl: 'ctrl', control: 'ctrl', '\u2303': 'ctrl',
  shift: 'shift', '\u21e7': 'shift',
  alt: 'alt', opt: 'alt', option: 'alt', '\u2325': 'alt',
};

// Parse a chord like "Ctrl+Shift+K", "Ctrl+Alt+\\" or "?" into a canonical, platform-checked
// token. Returns { ok, chord, mods, key, problems }; chord is stable for conflict detection.
export function parseChordSpec(spec, platform) {
  const raw = String(spec == null ? '' : spec).trim();
  if (!raw) return { ok: false, problems: ['empty binding'], chord: null, mods: [], key: null };
  const parts = raw.split('+').map((x) => x.trim()).filter(Boolean);
  const mods = [];
  const problems = [];
  let key = null;
  for (const p of parts) {
    const canon = MODIFIER_ALIASES[p.toLowerCase()];
    if (canon) {
      if (mods.indexOf(canon) >= 0) problems.push('duplicate modifier "' + p + '"');
      mods.push(canon);
    } else {
      if (key !== null) problems.push('multiple key tokens ("' + key + '", "' + p + '")');
      key = p;
    }
  }
  if (key === null) problems.push('no key token (modifiers only)');
  if (mods.indexOf('mod') >= 0) problems.push('unresolved "Mod" in a platform expansion');
  if (platform === 'mac') {
    if (mods.indexOf('ctrl') >= 0) problems.push('macOS expansion must not use Ctrl (Mod = Cmd on macOS)');
  } else if (mods.indexOf('cmd') >= 0) {
    problems.push(platform + ' expansion must not use Cmd/Meta (Mod = Ctrl+Shift on Windows/Linux)');
  }
  const chord = (mods.slice().sort().join('+') || '-') + '|' + (key === null ? '?' : String(key).toLowerCase());
  return { ok: problems.length === 0, problems, chord, mods, key };
}

// Every `Mod+...` token in a text (prose hints, toasts, picker data), with occurrence counts.
export function scanKeyTokens(text) {
  const out = new Map();
  const re = /Mod\+(?:Shift\+)?[A-Za-z0-9,?\\/]+/g;
  let m;
  while ((m = re.exec(String(text))) !== null) {
    // "Mod+Shift" (a modifier-only mention such as prose) is not a key declaration.
    if (!parseChordSpec(m[0], 'mac').key) continue;
    out.set(m[0], (out.get(m[0]) || 0) + 1);
  }
  return out;
}

function childSpan(tags, parent, cls) {
  for (let j = parent.open + 1; j < parent.close; j++) {
    const t = tags[j];
    if (!t.closing && t.tag === 'span' && hasClass(t.attrs, cls)) return j;
  }
  return -1;
}

// Structured key-display sites, each carrying the action id it belongs to (when the markup
// provides one via data-action / data-run / data-act).
export function collectKeyDeclarations(html, tags) {
  const sites = [];
  const textOfRange = (openIdx, closeIdx) =>
    decodeKeyEntities(textOf(closeIdx >= 0 ? html.slice(tags[openIdx].end, tags[closeIdx].index) : ''));

  // (a) settings shortcut table
  const table = findAllTags(tags, (t) => t.attrs.id === 'keysTable')[0];
  if (table && table.close > table.open) {
    for (let i = table.open + 1; i < table.close; i++) {
      const t = tags[i];
      if (t.closing || t.tag !== 'tr') continue;
      let td = -1;
      for (let j = i + 1; j < table.close; j++) {
        if (tags[j].closing) continue;
        if (tags[j].tag === 'td') { td = j; break; }
        if (tags[j].tag === 'tr') break;
      }
      if (td < 0) continue;
      sites.push({ kind: 'keys-table-row', action: t.attrs['data-action'] || null, token: textOfRange(td, matchCloseTag(tags, td)) });
    }
  }
  // (b) command palette
  for (const it of findAllTags(tags, (t) => hasClass(t.attrs, 'pal-item'))) {
    const kb = childSpan(tags, it, 'kb');
    if (kb < 0) continue;
    sites.push({ kind: 'palette-item', action: tags[it.open].attrs['data-run'] || null, token: textOfRange(kb, matchCloseTag(tags, kb)) });
  }
  // (c) popup menus: only ACTION items carry a key; scope radios use data-scope and a badge
  for (const it of findAllTags(tags, (t) => hasClass(t.attrs, 'pop-item') && !!t.attrs['data-act'])) {
    const tag = childSpan(tags, it, 'pi-tag');
    if (tag < 0) continue;
    sites.push({ kind: 'menu-item', action: tags[it.open].attrs['data-act'] || null, token: textOfRange(tag, matchCloseTag(tags, tag)) });
  }
  return sites;
}
