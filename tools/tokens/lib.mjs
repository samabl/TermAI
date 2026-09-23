// tools/tokens/lib.mjs
// TermAI design-token load / resolve / codegen. Zero external dependencies (ADR-0015).
// Source of truth: tokens/ (DC-09 / AR-22). Consumers: build.mjs, check.mjs.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const TOOL_DIR = path.dirname(fileURLToPath(import.meta.url));
export const DEFAULT_ROOT = path.resolve(TOOL_DIR, '..', '..');

// The seven groups of spec 02 section 3.11. Order = CSS emission order.
export const BASE_GROUPS = ['palette', 'type', 'space', 'radius', 'elevation', 'motion', 'ai-semantic'];

export const MARK_BEGIN = '/* @tokens:begin */';
export const MARK_END = '/* @tokens:end */';

export const PROTOTYPE_REL = 'prototype/termai-ui-terminal-first.html';

export function readJson(p) {
  return JSON.parse(fs.readFileSync(p, 'utf8'));
}

export function loadSource(root, opts) {
  root = root || DEFAULT_ROOT;
  opts = opts || {};
  const baseOverrides = opts.base || {};
  const groups = {};
  const tokens = new Map();
  const order = [];
  for (const g of BASE_GROUPS) {
    const p = path.join(root, 'tokens', 'base', g + '.json');
    if (!fs.existsSync(p)) throw new Error('missing token base file: ' + p);
    const doc = readJson(p);
    groups[g] = doc;
    for (const k of Object.keys(doc.tokens)) {
      if (tokens.has(k)) throw new Error('duplicate token key: ' + k);
      const t = doc.tokens[k];
      const value = Object.prototype.hasOwnProperty.call(baseOverrides, k) ? baseOverrides[k] : t.value;
      tokens.set(k, { key: k, value: value, declaredValue: t.value, type: t.type, group: g, description: t.description, ladder: t.ladder });
      order.push(k);
    }
  }
  const themesDir = path.join(root, 'tokens', 'themes');
  const themeFiles = fs.readdirSync(themesDir).filter(function (f) { return f.endsWith('.json'); }).sort();
  const themes = {};
  for (const f of themeFiles) {
    const doc = readJson(path.join(themesDir, f));
    if (!doc.id) throw new Error('theme file missing id: ' + f);
    themes[doc.id] = doc;
  }
  if (!themes.dark || !themes.light) throw new Error('tokens/themes must define dark and light');
  const contrast = readJson(path.join(root, 'tokens', 'contrast-pairs.json'));
  if (opts.contrastPairs) contrast.pairs = opts.contrastPairs;
  return { root: root, groups: groups, tokens: tokens, order: order, themes: themes, contrast: contrast, baseOverrides: baseOverrides };
}

// resolved(theme) = base merged with the theme override file (rgba-safe: raw strings).
export function resolveTheme(src, id) {
  const theme = src.themes[id];
  if (!theme) throw new Error('unknown theme: ' + id);
  const out = new Map();
  for (const k of src.order) out.set(k, src.tokens.get(k).value);
  for (const k of Object.keys(theme.overrides)) {
    if (!src.tokens.has(k)) throw new Error('theme ' + id + ' overrides unknown token: ' + k);
    out.set(k, theme.overrides[k]);
  }
  return out;
}

export function kebabToCamel(k) {
  return k.split('-').map(function (s, i) {
    return i === 0 ? s : s.charAt(0).toUpperCase() + s.slice(1);
  }).join('');
}

export function kebabToScreaming(k) {
  return k.split('-').join('_').toUpperCase();
}

// ---------------------------------------------------------------- color math

export function parseColor(input) {
  if (typeof input !== 'string') return null;
  const s = input.trim().toLowerCase();
  if (s.charAt(0) === '#') {
    const h = s.slice(1);
    let r, g, b, a = 1;
    if (h.length === 3 || h.length === 4) {
      r = parseInt(h.charAt(0) + h.charAt(0), 16);
      g = parseInt(h.charAt(1) + h.charAt(1), 16);
      b = parseInt(h.charAt(2) + h.charAt(2), 16);
      if (h.length === 4) a = parseInt(h.charAt(3) + h.charAt(3), 16) / 255;
    } else if (h.length === 6 || h.length === 8) {
      r = parseInt(h.slice(0, 2), 16);
      g = parseInt(h.slice(2, 4), 16);
      b = parseInt(h.slice(4, 6), 16);
      if (h.length === 8) a = parseInt(h.slice(6, 8), 16) / 255;
    } else {
      return null;
    }
    if ([r, g, b].some(function (x) { return Number.isNaN(x); })) return null;
    return { r: r, g: g, b: b, a: a };
  }
  const m = s.match(/^rgba?[(]([^)]*)[)]$/);
  if (!m) return null;
  const parts = m[1].split(/[ ,/]+/).filter(function (x) { return x.length > 0; });
  if (parts.length < 3) return null;
  function num(x) { return x.endsWith('%') ? parseFloat(x) / 100 * 255 : parseFloat(x); }
  function alpha(x) { return x.endsWith('%') ? parseFloat(x) / 100 : parseFloat(x); }
  const r = num(parts[0]), g = num(parts[1]), b = num(parts[2]);
  if ([r, g, b].some(function (x) { return Number.isNaN(x); })) return null;
  return { r: r, g: g, b: b, a: parts.length > 3 ? alpha(parts[3]) : 1 };
}

export function composite(fg, bg) {
  const a = fg.a;
  return { r: a * fg.r + (1 - a) * bg.r, g: a * fg.g + (1 - a) * bg.g, b: a * fg.b + (1 - a) * bg.b, a: 1 };
}

export function srgbToLinear(c) {
  const x = c / 255;
  return x <= 0.04045 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4);
}

export function linearToSrgb(y) {
  const x = y <= 0.0031308 ? y * 12.92 : 1.055 * Math.pow(y, 1 / 2.4) - 0.055;
  return Math.round(Math.min(1, Math.max(0, x)) * 255);
}

export function relativeLuminance(c) {
  return 0.2126 * srgbToLinear(c.r) + 0.7152 * srgbToLinear(c.g) + 0.0722 * srgbToLinear(c.b);
}

export function contrastRatio(a, b) {
  const la = relativeLuminance(a), lb = relativeLuminance(b);
  const hi = Math.max(la, lb), lo = Math.min(la, lb);
  return (hi + 0.05) / (lo + 0.05);
}

// Resolve a token to an opaque color; translucent values composite over baseKey.
export function opaqueColor(resolved, key, baseKey) {
  const raw = resolved.get(key);
  if (raw === undefined) throw new Error('token not found for contrast pair: ' + key);
  const c = parseColor(raw);
  if (!c) throw new Error('not a parseable color: ' + key + ' = ' + raw);
  if (c.a >= 1) return c;
  if (!baseKey) throw new Error('translucent color needs bgBase: ' + key);
  return composite(c, opaqueColor(resolved, baseKey, null));
}

export function pairRatio(resolved, pair) {
  const bg = opaqueColor(resolved, pair.bg, pair.bgBase);
  let fg;
  if (pair.fgBase) {
    fg = opaqueColor(resolved, pair.fg, pair.fgBase);
  } else {
    const c = parseColor(resolved.get(pair.fg));
    if (!c) throw new Error('bad fg color: ' + pair.fg);
    fg = c.a >= 1 ? c : composite(c, bg);
  }
  return contrastRatio(fg, bg);
}

// ---------------------------------------------------------------- codegen

export function renderCssRule(selector, colorScheme, entries) {
  const lines = [];
  lines.push(selector + ' {');
  lines.push('  color-scheme: ' + colorScheme + ';');
  for (const e of entries) lines.push('  --' + e[0] + ': ' + e[1] + ';');
  lines.push('}');
  return lines.join('\n');
}

function camelNames(src) {
  const seen = new Map();
  for (const k of src.order) {
    const c = kebabToCamel(k);
    if (seen.has(c)) throw new Error('camelCase collision: ' + k + ' vs ' + seen.get(c));
    seen.set(c, k);
  }
  return seen;
}

function renderTs(src, dark) {
  camelNames(src);
  const lines = [];
  lines.push('/* AUTO-GENERATED by tools/tokens/build.mjs - DO NOT EDIT. Source: tokens/ (DC-09 / AR-22). */');
  lines.push('');
  lines.push('export const tokens = {');
  for (const k of src.order) lines.push('  ' + kebabToCamel(k) + ': ' + JSON.stringify(dark.get(k)) + ',');
  lines.push('} as const;');
  lines.push('');
  lines.push('export const vars = {');
  for (const k of src.order) lines.push('  ' + kebabToCamel(k) + ': ' + JSON.stringify('--' + k) + ',');
  lines.push('} as const;');
  lines.push('');
  lines.push('export const themes = {');
  for (const id of Object.keys(src.themes).sort()) {
    const resolved = resolveTheme(src, id);
    lines.push('  ' + id + ': {');
    for (const k of src.order) lines.push('    ' + kebabToCamel(k) + ': ' + JSON.stringify(resolved.get(k)) + ',');
    lines.push('  },');
  }
  lines.push('} as const;');
  lines.push('');
  lines.push('export type TokenName = keyof typeof tokens;');
  lines.push('export type TokenValue = (typeof tokens)[TokenName];');
  lines.push('export type ThemeName = keyof typeof themes;');
  lines.push('export type ThemeTokens = (typeof themes)[ThemeName];');
  lines.push("export const DEFAULT_THEME: ThemeName = 'dark';");
  lines.push('');
  lines.push('export function cssVar(name: TokenName): string {');
  lines.push('  return vars[name];');
  lines.push('}');
  return lines.join('\n') + '\n';
}

function renderJson(src) {
  const obj = {
    $generatedBy: 'tools/tokens/build.mjs',
    version: '1.0.0',
    defaultTheme: 'dark',
    tokenCount: src.order.length,
    sourceOfTruth: 'tokens/',
    themes: {}
  };
  for (const id of Object.keys(src.themes).sort()) {
    const resolved = resolveTheme(src, id);
    const flat = {};
    for (const k of src.order) flat[k] = resolved.get(k);
    obj.themes[id] = flat;
  }
  return JSON.stringify(obj, null, 2) + '\n';
}

function renderThemeSchema(src) {
  const keys = src.order.slice().sort();
  return {
    $schema: 'https://json-schema.org/draft/2020-12/schema',
    $id: 'https://termai.dev/schemas/theme.schema.json',
    title: 'TermAI third-party theme file',
    description: 'Theme = pure data (DC-12): no scripts, no CSS injection. overrides may only set official token names generated by tools/tokens/build.mjs; keys are restricted to the official token set.',
    type: 'object',
    required: ['$schema', 'id', 'label', 'colorScheme', 'overrides'],
    additionalProperties: false,
    properties: {
      $schema: { type: 'string', minLength: 1 },
      id: { type: 'string', pattern: '^[a-z][a-z0-9]*(-[a-z0-9]+)*$' },
      label: { type: 'string', minLength: 1 },
      colorScheme: { enum: ['dark', 'light'] },
      extends: { enum: ['dark', 'light'] },
      version: { type: 'string', pattern: '^[0-9]+[.][0-9]+[.][0-9]+$' },
      author: { type: 'string' },
      signature: { type: 'string' },
      overrides: {
        type: 'object',
        minProperties: 1,
        propertyNames: { enum: keys },
        additionalProperties: { type: 'string', minLength: 1 }
      }
    }
  };
}

function renderRust(src, dark) {
  const lines = [];
  lines.push('//! AUTO-GENERATED by tools/tokens/build.mjs - DO NOT EDIT.');
  lines.push('//! TermAI design tokens (DC-09 / AR-22). Leaf crate: no dependencies.');
  lines.push('#![no_std]');
  lines.push('#![forbid(unsafe_code)]');
  lines.push('// Generated file: keep rustfmt out of it (regenerate instead of reformatting).');
  lines.push('#![cfg_attr(rustfmt, rustfmt::skip)]');
  lines.push('');
  lines.push('/// Active theme. Default is follow-system, falling back to dark (AR-23 item 7).');
  lines.push('#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]');
  lines.push('pub enum Theme {');
  lines.push('    Dark,');
  lines.push('    Light,');
  lines.push('}');
  lines.push('');
  lines.push('impl Theme {');
  lines.push('    pub const ALL: [Theme; 2] = [Theme::Dark, Theme::Light];');
  lines.push('');
  lines.push('    pub const fn id(self) -> &\'static str {');
  lines.push('        match self {');
  lines.push('            Theme::Dark => "dark",');
  lines.push('            Theme::Light => "light",');
  lines.push('        }');
  lines.push('    }');
  lines.push('');
  lines.push('    /// System preference could not be determined -> dark.');
  lines.push('    pub const fn fallback() -> Theme {');
  lines.push('        Theme::Dark');
  lines.push('    }');
  lines.push('}');
  lines.push('');
  lines.push('/// A token: kebab-case name (equal to the CSS custom property without the leading --).');
  lines.push('#[derive(Debug, Clone, Copy, PartialEq, Eq)]');
  lines.push('pub struct Token {');
  lines.push('    pub name: &\'static str,');
  lines.push('    pub value: &\'static str,');
  lines.push('}');
  lines.push('');
  for (const id of ['dark', 'light']) {
    const resolved = resolveTheme(src, id);
    lines.push('/// Resolved ' + id + ' theme values.');
    lines.push('pub const ' + id.toUpperCase() + ': &[Token] = &[');
    for (const k of src.order) lines.push('    Token { name: "' + k + '", value: ' + JSON.stringify(resolved.get(k)) + ' },');
    lines.push('];');
    lines.push('');
  }
  lines.push('/// Resolved token list for a theme.');
  lines.push('pub fn tokens(theme: Theme) -> &\'static [Token] {');
  lines.push('    match theme {');
  lines.push('        Theme::Dark => DARK,');
  lines.push('        Theme::Light => LIGHT,');
  lines.push('    }');
  lines.push('}');
  lines.push('');
  lines.push('/// Look up a token value by kebab-case name.');
  lines.push('pub fn value(theme: Theme, name: &str) -> Option<&\'static str> {');
  lines.push('    for t in tokens(theme) {');
  lines.push('        if t.name == name {');
  lines.push('            return Some(t.value);');
  lines.push('        }');
  lines.push('    }');
  lines.push('    None');
  lines.push('}');
  lines.push('');
  for (const id of ['dark', 'light']) {
    const resolved = resolveTheme(src, id);
    lines.push('/// Per-token constants: kebab-case name -> SCREAMING_SNAKE_CASE (' + id + ' theme).');
    lines.push('pub mod ' + id + ' {');
    for (const k of src.order) lines.push('    pub const ' + kebabToScreaming(k) + ': &str = ' + JSON.stringify(resolved.get(k)) + ';');
    lines.push('}');
    lines.push('');
  }
  while (lines.length > 0 && lines[lines.length - 1] === '') lines.pop();
  return lines.join('\n') + '\n';
}

function renderCargo() {
  return [
    '# AUTO-GENERATED by tools/tokens/build.mjs - DO NOT EDIT.',
    '[package]',
    'name = "termai-tokens"',
    'version = "0.1.0"',
    'edition = "2021"',
    'rust-version = "1.75"',
    'license = "Apache-2.0 OR MIT"',
    'description = "TermAI design tokens (generated). Leaf crate, zero dependencies (DC-09 / ADR-0015)."',
    'publish = false',
    '',
    '[lib]',
    'path = "src/lib.rs"',
    '',
    '[dependencies]',
    '',
    '[lints.rust]',
    'unsafe_code = "forbid"',
    ''
  ].join('\n');
}

export function generate(src) {
  const dark = resolveTheme(src, 'dark');
  const darkTheme = src.themes.dark;
  const lightTheme = src.themes.light;
  const darkEntries = src.order.map(function (k) { return [k, dark.get(k)]; });
  const lightEntries = Object.keys(lightTheme.overrides).map(function (k) { return [k, lightTheme.overrides[k]]; });
  const rootRule = renderCssRule(':root', darkTheme.colorScheme, darkEntries);
  const lightRule = renderCssRule('[data-theme="light"]', lightTheme.colorScheme, lightEntries);
  const cssHeader = [
    '/* AUTO-GENERATED by tools/tokens/build.mjs - DO NOT EDIT.',
    ' * Source of truth: tokens/ (DC-09 / AR-22). Regenerate: node tools/tokens/build.mjs',
    ' * Theme model: :root = dark (base); [data-theme="light"] = light overrides. */',
    ''
  ].join('\n');
  const files = {};
  files['packages/tokens/dist/tokens.css'] = cssHeader + rootRule + '\n' + lightRule + '\n';
  files['packages/tokens/dist/tokens.json'] = renderJson(src);
  files['packages/tokens/dist/tokens.ts'] = renderTs(src, dark);
  files['packages/tokens/dist/theme.schema.json'] = JSON.stringify(renderThemeSchema(src), null, 2) + '\n';
  files['crates/termai-tokens/src/lib.rs'] = renderRust(src, dark);
  files['crates/termai-tokens/Cargo.toml'] = renderCargo();
  const prototypeBlock = [
    MARK_BEGIN,
    '/* AUTO-GENERATED by tools/tokens/build.mjs from tokens/ - DO NOT EDIT (DC-09 / AR-22). */',
    rootRule + '\n' + lightRule,
    MARK_END
  ].join('\n');
  return { files: files, prototypeBlock: prototypeBlock, rootRule: rootRule, lightRule: lightRule, dark: dark, lightResolved: resolveTheme(src, 'light') };
}

export function splicePrototype(html, block) {
  const i = html.indexOf(MARK_BEGIN);
  const j = html.indexOf(MARK_END);
  if (i < 0 || j < 0 || j < i) {
    throw new Error('prototype token markers not found: ' + MARK_BEGIN + ' / ' + MARK_END);
  }
  return html.slice(0, i) + block + html.slice(j + MARK_END.length);
}

export function extractPrototypeBlock(html) {
  const i = html.indexOf(MARK_BEGIN);
  const j = html.indexOf(MARK_END);
  if (i < 0 || j < 0 || j < i) throw new Error('prototype token markers not found');
  return html.slice(i, j + MARK_END.length);
}
