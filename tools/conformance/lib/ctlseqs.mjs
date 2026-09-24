// tools/conformance/lib/ctlseqs.mjs
//
// Parser + resolver for the xterm control sequences document (ctlseqs.txt).
//
// The document itself is NOT vendored into this repository (it is third-party
// documentation with its own copyright). What is checked in is the derived entry
// registry (tools/conformance/data/ctlseqs-entries.json): entry pattern, doc line,
// mnemonic and the resolved byte form. Regenerating requires --source <ctlseqs.txt>.
//
// AR-31 item 1 requires "at least one case per ctlseqs entry", so this module is the
// entry side of that mapping.
import { createHash } from 'node:crypto';

export const INTRODUCERS = ['CSI', 'ESC', 'DCS', 'OSC', 'APC', 'PM', 'SOS', 'SS2', 'SS3'];

const INTRO_BYTES = {
  CSI: [0x1b, 0x5b],
  ESC: [0x1b],
  DCS: [0x1b, 0x50],
  OSC: [0x1b, 0x5d],
  APC: [0x1b, 0x5f],
  PM: [0x1b, 0x5e],
  SOS: [0x1b, 0x58],
  SS2: [0x1b, 0x4e],
  SS3: [0x1b, 0x4f],
};

const NAMED = {
  NUL: [0x00],
  SOH: [0x01],
  STX: [0x02],
  ETX: [0x03],
  EOT: [0x04],
  ENQ: [0x05],
  ACK: [0x06],
  BEL: [0x07],
  BS: [0x08],
  HT: [0x09],
  TAB: [0x09],
  LF: [0x0a],
  VT: [0x0b],
  FF: [0x0c],
  CR: [0x0d],
  SO: [0x0e],
  SI: [0x0f],
  DLE: [0x10],
  DC1: [0x11],
  DC2: [0x12],
  DC3: [0x13],
  DC4: [0x14],
  NAK: [0x15],
  SYN: [0x16],
  ETB: [0x17],
  CAN: [0x18],
  EM: [0x19],
  SUB: [0x1a],
  ESC: [0x1b],
  FS: [0x1c],
  GS: [0x1d],
  RS: [0x1e],
  US: [0x1f],
  DEL: [0x7f],
  SP: [0x20],
  ST: [0x1b, 0x5c],
};

// Allowed literal single-character tokens, as code points. Kept numeric so that the
// quote characters, the backslash and the backtick never need escaping in source.
const SINGLE_LITERAL_CODES = [
  0x3b, 0x3f, 0x3a, 0x3e, 0x21, 0x3d, 0x23, 0x25, 0x28, 0x29, 0x2a, 0x2b, 0x2d,
  0x2e, 0x2f, 0x2c, 0x3c, 0x5b, 0x5d, 0x7b, 0x7d, 0x7c, 0x26, 0x5e, 0x7e, 0x24,
  0x40, 0x5f, 0x22, 0x27, 0x5c, 0x60,
];

export function sha256Hex(text) {
  return createHash('sha256').update(Buffer.from(text, 'binary')).digest('hex');
}

function literalBytes(token) {
  if (token.length !== 1) return null;
  const code = token.charCodeAt(0);
  if (SINGLE_LITERAL_CODES.indexOf(code) >= 0) return [code];
  if (/[A-Za-z0-9]/.test(token)) return [code];
  return null;
}

function paramValue(token) {
  // Repeat notation from the document, e.g. "Pr..Pr ST", means "one or more Pr".
  const bare = token.replace(/\.\..*$/, '');
  if (bare === 'Pt') return 'x'; // text parameter
  if (/^P[a-z]$/.test(bare)) return '1'; // Ps / Pm / Pi / Pa / Pv / Pc / ...
  return null;
}

// Trailing prose that leaked into the pattern because the document line omitted the
// usual two-space column separator.
function looksLikeProse(text) {
  if (!text) return false;
  if (/\.$/.test(text)) return true;
  return /^[A-Z][a-z]/.test(text);
}

export function bytesToHex(bytes) {
  let out = '';
  for (const byte of bytes) out += byte.toString(16).padStart(2, '0');
  return out;
}

// Resolve as many leading tokens as possible into bytes. Returns the resolved token
// list, the byte array, and (when the doc line carries extra prose) the leftover text.
export function resolvePattern(pattern) {
  const tokens = String(pattern).split(/\s+/).filter(Boolean);
  const bytes = [];
  const used = [];
  let index = 0;
  for (; index < tokens.length; index += 1) {
    const token = tokens[index];
    if (index === 0 && INTRO_BYTES[token]) {
      bytes.push.apply(bytes, INTRO_BYTES[token]);
      used.push(token);
      continue;
    }
    if (Object.prototype.hasOwnProperty.call(NAMED, token)) {
      bytes.push.apply(bytes, NAMED[token]);
      used.push(token);
      continue;
    }
    const value = paramValue(token);
    if (value !== null) {
      for (const ch of value) bytes.push(ch.charCodeAt(0));
      used.push(token);
      continue;
    }
    const literal = literalBytes(token);
    if (literal !== null) {
      bytes.push.apply(bytes, literal);
      used.push(token);
      continue;
    }
    break;
  }
  return {
    pattern: used.join(' '),
    bytes: bytes,
    hex: bytesToHex(bytes),
    complete: index >= tokens.length,
    leftover: tokens.slice(index).join(' '),
  };
}

function findMnemonic(text) {
  const m = /\(([A-Z][A-Z0-9-]{1,14})\)/.exec(text || '');
  return m ? m[1] : '';
}

export function slug(text) {
  return String(text)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+/, '')
    .replace(/-+$/, '');
}

export function parseCtlseqs(text) {
  const lines = text.split(/\r?\n/);
  const out = { patch: null, updated: null, entries: [], raw_entry_lines: 0 };
  const seenPatterns = new Set();
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    const version = /updated for XTerm Patch #(\d+)\s*\(([^)]*)\)/.exec(line);
    if (version) {
      out.patch = version[1];
      out.updated = version[2];
    }
    const m = /^([A-Z]{2,6})\s+(.*)$/.exec(line);
    if (!m) continue;
    if (INTRODUCERS.indexOf(m[1]) < 0) continue;
    const body = m[2];
    const dbl = body.search(/  /);
    const left = (dbl >= 0 ? body.slice(0, dbl) : body).trim();
    const desc = dbl >= 0 ? body.slice(dbl + 2).trim() : '';
    if (!left) continue;
    out.raw_entry_lines += 1;
    const resolved = resolvePattern(m[1] + ' ' + left);
    const proseTail = looksLikeProse(resolved.leftover);
    const complete = resolved.complete || proseTail;
    const mnemonic =
      findMnemonic(desc) ||
      findMnemonic(lines[i + 1]) ||
      findMnemonic(lines[i + 2]) ||
      findMnemonic(lines[i + 3]);
    const entry = {
      pattern: m[1] + ' ' + left,
      resolved_pattern: resolved.pattern,
      complete: complete,
      prose_tail: proseTail,
      leftover: proseTail ? '' : resolved.leftover,
      seq_hex: resolved.hex,
      mnemonic: mnemonic,
      doc_line: i + 1,
    };
    const key = entry.complete ? entry.resolved_pattern : entry.pattern;
    if (seenPatterns.has(key)) continue;
    seenPatterns.add(key);
    out.entries.push(entry);
  }
  out.entries.sort(function (a, b) {
    if (a.resolved_pattern === b.resolved_pattern) return a.doc_line - b.doc_line;
    return a.resolved_pattern < b.resolved_pattern ? -1 : 1;
  });
  return out;
}

export function entryId(entry) {
  const base = entry.mnemonic ? slug(entry.mnemonic) : slug(entry.resolved_pattern);
  return base + '-' + entry.doc_line;
}
