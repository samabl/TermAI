// tools/design-gates/check-design.mjs
// TermAI design acceptance gates - STATIC LAYER (S1-S10). Zero external dependencies.
//
// Maps docs/spec/02 section 5 (A17-A35) and section 5.3 (UX-G11-UX-G17) onto executable,
// merge-blocking assertions. The S layer is pure Node and runs in every environment;
// the B layer (browser.mjs) additionally needs Chrome.
//
//   node tools/design-gates/check-design.mjs              # static + browser
//   node tools/design-gates/check-design.mjs --static     # static only
//   node tools/design-gates/check-design.mjs --strict     # S8 hex debt becomes blocking
//   node tools/design-gates/check-design.mjs --selftest   # prove the gates are not always-green
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {
  DEFAULT_ROOT, PROTOTYPE_REL,
  readText, loadTokenScales, fmtScale,
  extractStyle, parseDeclarations, extractLengths, stripCssComments,
  scanTags, findAllTags, hasClass, classList, elementHtml, textOf, describeTag,
  countHardcodedHex, gate, formatReport, STATUS, makeSelftest, copyFileInto,
  extractActionRegistry, collectKeyDeclarations, scanKeyTokens, parseChordSpec, decodeKeyEntities,
  TOKEN_MARK_BEGIN, TOKEN_MARK_END,
} from './lib.mjs';
// Reuse the token pipeline for S9 (AR-22 / DC-09 drift). Never re-implement codegen here.
import { loadSource, generate, extractPrototypeBlock } from '../tokens/lib.mjs';

const CHECK_TITLE = 'design:check (static S1-S10 + browser B1-B10) - spec 02 A17-A35 / UX-G11-UX-G17, AR-22 / AR-23 / AR-29';

// --------------------------------------------------------------- static checks

export function runStaticChecks(opts) {
  opts = opts || {};
  const root = opts.root || DEFAULT_ROOT;
  const strictHex = !!opts.strictHex;
  const protoPath = path.join(root, opts.prototypeRel || PROTOTYPE_REL);
  const html = readText(protoPath);

  const tags = scanTags(html);
  const style = extractStyle(html);
  const decls = parseDeclarations(style);
  const scales = loadTokenScales(root);
  const gates = [];

  // ---- S1: fold sections default-expanded <= 2 (A21 / UX-G11)
  {
    const folds = findAllTags(tags, (t) => t.tag === 'details' && hasClass(t.attrs, 'fold'));
    const open = [];
    for (let i = 0; i < folds.length; i++) {
      const t = tags[folds[i].open];
      const hasData = Object.prototype.hasOwnProperty.call(t.attrs, 'data-open');
      const isOpen = hasData ? t.attrs['data-open'] === 'true' : Object.prototype.hasOwnProperty.call(t.attrs, 'open');
      if (isOpen) open.push(i + 1);
    }
    const ok = folds.length > 0 && open.length <= 2;
    gates.push(gate('S1',
      'fold sections default-expanded <= 2',
      ok ? STATUS.PASS : STATUS.FAIL,
      'default-expanded ' + open.length + ' / ' + folds.length + ' (limit 2)',
      ok ? ['expanded fold ordinals: ' + (open.join(', ') || 'none')]
         : ['expanded fold ordinals: ' + open.join(', ') + ' - set data-open="false" (and drop the open attribute) on the extra ones']));
  }

  // ---- S2: top bar has no "connect host / new workspace" (A31 / UX-G14)
  {
    const mainWin = findAllTags(tags, (t) => t.attrs.id === 'mainWin')[0];
    const scope = mainWin ? { open: mainWin.open, close: mainWin.close } : { open: 0, close: tags.length - 1 };
    const bars = findAllTags(tags, (t) => (hasClass(t.attrs, 'toolbar') || hasClass(t.attrs, 'titlebar')))
      .filter((b) => b.open > scope.open && b.open < scope.close);
    const forbidden = ['连接主机', '新建工作区'];
    const hits = [];
    for (const b of bars) {
      const txt = textOf(elementHtml(html, tags, b.open, b.close));
      for (const f of forbidden) if (txt.indexOf(f) >= 0) hits.push(describeTag(tags[b.open]) + ' contains "' + f + '"');
    }
    const ok = bars.length > 0 && hits.length === 0;
    gates.push(gate('S2',
      'top bar has no "连接主机 / 新建工作区" entry',
      ok ? STATUS.PASS : STATUS.FAIL,
      bars.length + ' top-bar containers scanned, ' + hits.length + ' forbidden label(s)',
      ok ? ['new-terminal entry lives on the tab "+" picker only (A31 / AR-23 item 4)']
         : hits.concat(['AR-23 item 4: the tab "+" picker is the only new-terminal entry']))); 
  }

  // ---- S3: every icon CONTROL has an accessible name (A18 / UX-G12 / AR-22 item 3)
  //
  // Scope rule (deliberately conservative: over-report rather than miss an icon control):
  //   (a) any element carrying a known icon-control class, on ANY tag (button or not);
  //   (b) any ICON-ONLY element (its visible text is empty) that carries an interactive
  //       signal: an interactive ARIA role, data-act, data-win, data-tip, tabindex, or a
  //       CSS rule with cursor:pointer that matches it.
  // Text-bearing controls are NOT reported: their accessible name legitimately comes from
  // their visible text, so an extra aria-label is not required. An icon-only control is
  // compliant when aria-label is non-empty.
  {
    const ICON_CLASSES = ['icon-btn', 'rail-btn', 'ttab-add', 'send', 'wc-dot', 'wc-btn', 't-x'];
    const INTERACTIVE_ROLES = ['button', 'tab', 'menuitem', 'menuitemcheckbox', 'menuitemradio', 'option', 'checkbox', 'radio', 'switch', 'link', 'treeitem', 'gridcell', 'slider', 'spinbutton'];

    // Conservative cursor:pointer detection: collect every id/class/tag token that appears
    // in any selector of a rule whose body mentions cursor + pointer, then treat an element
    // as a candidate when it carries one of those tokens. Over-matching is intentional here
    // and is narrowed by the "icon-only" condition below.
    const cursorTokens = new Set();
    {
      const css = stripCssComments(style);
      const ruleRe = /([^{}]+)\{([^{}]*)\}/g;
      let m;
      while ((m = ruleRe.exec(css)) !== null) {
        const body = m[2] || '';
        if (body.indexOf('cursor') < 0 || body.indexOf('pointer') < 0) continue;
        const toks = (m[1] || '').match(/\.([A-Za-z_][-A-Za-z0-9_]*)|([A-Za-z][-A-Za-z0-9_]*)/g) || [];
        for (const raw of toks) cursorTokens.add(raw.charAt(0) === '.' ? raw.slice(1) : raw);
      }
    }
    const dashed = (t) => describeTag(t) + (Object.prototype.hasOwnProperty.call(t.attrs, 'aria-hidden') ? '[aria-hidden=' + t.attrs['aria-hidden'] + ']' : '');
    const iconish = [];
    for (const e of findAllTags(tags, () => true)) {
      if (e.close < 0) continue;
      const t = tags[e.open];
      const cls = classList(t.attrs);
      let reason = null;
      if (ICON_CLASSES.some((c) => cls.indexOf(c) >= 0)) reason = 'icon class';
      else if (textOf(elementHtml(html, tags, e.open, e.close)) === '') {
        const role = String(t.attrs.role || '');
        if (INTERACTIVE_ROLES.indexOf(role) >= 0) reason = 'role=' + role;
        else if (Object.prototype.hasOwnProperty.call(t.attrs, 'data-act')) reason = 'data-act';
        else if (Object.prototype.hasOwnProperty.call(t.attrs, 'data-win')) reason = 'data-win';
        else if (Object.prototype.hasOwnProperty.call(t.attrs, 'data-tip')) reason = 'data-tip';
        else if (Object.prototype.hasOwnProperty.call(t.attrs, 'tabindex')) reason = 'tabindex';
        else if (cursorTokens.has(t.tag) || cls.some((c) => cursorTokens.has(c))) reason = 'cursor:pointer';
      }
      if (reason) iconish.push({ e, t, reason });
    }
    const missing = [];
    for (const x of iconish) {
      const label = String(x.t.attrs['aria-label'] || '').trim();
      if (!label) missing.push(dashed(x.t) + ' has no aria-label (matched by ' + x.reason + ')');
    }
    const ok = iconish.length > 0 && missing.length === 0;
    gates.push(gate('S3',
      'icon controls 100% have an accessible name (aria-label)',
      ok ? STATUS.PASS : STATUS.FAIL,
      iconish.length + ' icon controls checked, ' + missing.length + ' missing aria-label',
      ok ? [
        'scope: icon classes (' + ICON_CLASSES.join(' | ') + ') on any tag, plus icon-only elements with an interactive signal (role / data-act / data-win / data-tip / tabindex / cursor:pointer)',
        'text-bearing controls are excluded: their name comes from visible text',
      ] : missing.slice(0, 10)));
  }

  // ---- S4: window controls are real <button> with aria-label (A33 / UX-G16)
  {
    const ctrls = findAllTags(tags, (t) => Object.prototype.hasOwnProperty.call(t.attrs, 'data-win'));
    const bad = [];
    for (const e of ctrls) {
      const t = tags[e.open];
      if (t.tag !== 'button') bad.push(describeTag(t) + ' is <' + t.tag + '>, expected <button>');
      if (!String(t.attrs['aria-label'] || '').trim()) bad.push(describeTag(t) + ' has no aria-label');
      if (String(t.attrs['aria-hidden'] || '') === 'true') bad.push(describeTag(t) + ' is aria-hidden (decorative)');
    }
    const ok = ctrls.length > 0 && bad.length === 0;
    gates.push(gate('S4',
      'window controls are real <button> with aria-label',
      ok ? STATUS.PASS : STATUS.FAIL,
      ctrls.length + ' [data-win] controls checked (main + settings window), ' + bad.length + ' problem(s)',
      ok ? ['main window: mac dots + win flat buttons; settings window: own title bar controls (A32/A33)'] : bad));
  }

  // ---- S5: main area has exactly one view and it is the terminal (A25 / UX-G14)
  {
    const viewsHost = findAllTags(tags, (t) => hasClass(t.attrs, 'views'))[0];
    let views = [];
    if (viewsHost && viewsHost.close > viewsHost.open) {
      views = tags.filter((t, i) => i > viewsHost.open && i < viewsHost.close && !t.closing && hasClass(t.attrs, 'view'));
    }
    const active = views.filter((t) => hasClass(t.attrs, 'is-active'));
    const terminal = views.length === 1 && (hasClass(views[0].attrs, 'term-view') || views[0].attrs.id === 'viewTerm');
    const switcher = findAllTags(tags, (t) => Object.prototype.hasOwnProperty.call(t.attrs, 'data-view'));
    const ok = views.length === 1 && active.length === 1 && terminal && switcher.length === 0;
    const notes = [];
    notes.push('views in main area: ' + views.length + ' (' + views.map(describeTag).join(', ') + ')');
    notes.push('active views: ' + active.length + '; view switchers (data-view): ' + switcher.length);
    if (!terminal && views.length >= 1) notes.push('the single view must be the terminal (term-view / #viewTerm)');
    if (views.length !== 1) notes.push('AR-23 item 1: no "terminal / workspace" view switch may exist');
    gates.push(gate('S5',
      'main area has exactly one view = terminal',
      ok ? STATUS.PASS : STATUS.FAIL,
      views.length + ' view(s), ' + active.length + ' active, terminal=' + terminal,
      notes));
  }

  // ---- S6: rail has exactly 7 sections, each labelled (A26 / UX-G14)
  {
    const rail = findAllTags(tags, (t) => t.attrs.id === 'rail')[0];
    const expected = ['workspaces', 'sessions', 'agent', 'snippets', 'themes', 'plugins', 'audit'];
    let tabs = [];
    if (rail && rail.close > rail.open) {
      tabs = tags.filter((t, i) => i > rail.open && i < rail.close && !t.closing && t.tag === 'button' && String(t.attrs.role || '') === 'tab');
    }
    const secs = tabs.map((t) => t.attrs['data-sec'] || '');
    const missingLabel = tabs.filter((t) => !String(t.attrs['aria-label'] || '').trim() || !(String(t.attrs.title || '').trim() || String(t.attrs['data-tip'] || '').trim()));
    const setOk = secs.length === 7 && expected.every((s) => secs.indexOf(s) >= 0);
    const ok = setOk && missingLabel.length === 0;
    gates.push(gate('S6',
      'sidebar rail has 7 labelled sections',
      ok ? STATUS.PASS : STATUS.FAIL,
      tabs.length + ' rail tabs: ' + secs.join(', '),
      missingLabel.length
        ? ['missing aria-label or title/data-tip: ' + missingLabel.map(describeTag).join(', ')]
        : ['fixed set: ' + expected.join(' / ') + ' (AR-23 item 2); Settings and collapse live in the rail foot']));
  }

  // ---- S7: spacing / radius / font-size / motion duration all on the token ruler (A23 / UX-G13)
  {
    const SPACING = /^(gap|row-gap|column-gap|padding|padding-top|padding-right|padding-bottom|padding-left|margin|margin-top|margin-right|margin-bottom|margin-left)$/;
    const RADIUS = /^(border-radius|border-top-left-radius|border-top-right-radius|border-bottom-left-radius|border-bottom-right-radius)$/;
    const FONT = /^font-size$/;
    const MOTION = /^(transition|transition-duration|animation|animation-duration)$/;
    const spAllowed = scales.spacing.concat(scales.spacingExempt);
    const rAllowed = scales.radius.concat(scales.radiusExempt);
    const violations = [];
    let checked = 0;
    for (const d of decls) {
      if (d.prop.indexOf('--') === 0) continue;
      const lens = extractLengths(d.value);
      if (SPACING.test(d.prop)) {
        checked++;
        for (const l of lens) {
          if (l.unit === 'px') {
            const v = Math.abs(l.num);
            if (spAllowed.indexOf(v) < 0) violations.push('SPACING ' + d.prop + ': ' + d.value + ' (' + l.num + 'px not in ' + fmtScale(scales.spacing, '') + ' px ruler)');
          } else if (l.unit === '' && l.num !== 0 && !/auto/.test(d.value)) {
            violations.push('SPACING ' + d.prop + ': ' + d.value + ' (unitless ' + l.num + ')');
          }
        }
      } else if (RADIUS.test(d.prop)) {
        checked++;
        for (const l of lens) {
          if (l.unit === 'px') {
            const v = Math.abs(l.num);
            if (rAllowed.indexOf(v) < 0) violations.push('RADIUS ' + d.prop + ': ' + d.value + ' (' + l.num + 'px not in ' + fmtScale(scales.radius, '') + ' px ruler)');
          } else if (l.unit === '%' && scales.radiusPercentExempt.indexOf(l.num) < 0) {
            violations.push('RADIUS ' + d.prop + ': ' + d.value + ' (' + l.num + '% is not a token radius)');
          } else if (l.unit === '' && l.num !== 0) {
            violations.push('RADIUS ' + d.prop + ': ' + d.value + ' (unitless ' + l.num + ')');
          }
        }
      } else if (FONT.test(d.prop)) {
        checked++;
        for (const l of lens) {
          if (l.unit === 'px') {
            if (scales.fontSize.indexOf(l.num) < 0) violations.push('FONT_SIZE ' + d.prop + ': ' + d.value + ' (not in ' + fmtScale(scales.fontSize, '') + ' px ladder)');
          } else if (l.unit === 'em' || l.unit === 'rem' || l.unit === '%') {
            violations.push('FONT_SIZE ' + d.prop + ': ' + d.value + ' (literal ' + l.unit + ' bypasses the px ladder)');
          }
        }
      } else if (MOTION.test(d.prop)) {
        checked++;
        const isCaret = d.value.indexOf('caret') >= 0;
        const allowed = scales.duration.concat(scales.durationExempt.filter((x) => x === 0 || isCaret));
        for (const l of lens) {
          if (l.unit === 'ms' || l.unit === 's') {
            const v = l.unit === 's' ? l.num * 1000 : l.num;
            if (allowed.indexOf(v) < 0) violations.push('MOTION ' + d.prop + ': ' + d.value + ' (' + v + 'ms not in ' + fmtScale(scales.duration, 'ms') + ')');
          }
        }
      }
    }
    const uniq = Array.from(new Set(violations));
    const ok = uniq.length === 0;
    gates.push(gate('S7',
      'CSS spacing / radius / font-size / motion duration are on the token ruler',
      ok ? STATUS.PASS : STATUS.FAIL,
      decls.length + ' declarations parsed, ' + checked + ' ruler-relevant, ' + uniq.length + ' off-ruler value(s)',
      ok
        ? ['spacing ' + fmtScale(scales.spacing, '') + ' px; radius ' + fmtScale(scales.radius, '') + ' px; font-size ' + fmtScale(scales.fontSize, '') + ' px; motion ' + fmtScale(scales.duration, 'ms') + ' (+0 / caret 200ms)']
        : uniq.slice(0, 20).concat(uniq.length > 20 ? ['... ' + (uniq.length - 20) + ' more'] : [])));
  }

  // ---- S8: hardcoded hex outside the token block (AR-22 target = 0). BLOCKING by default.
  // The 28-value / 20-distinct debt was cleared (every literal is now a semantic token, or an
  // HTML entity in display text); the gate is therefore promoted from WARN to FAIL. --strict
  // is still accepted for compatibility but is now a no-op (this is the strict behaviour).
  {
    const hex = countHardcodedHex(html);
    const detail = hex.total + ' hardcoded hex colors outside the token block (' + hex.distinct + ' distinct)';
    const notes = hex.colors.map((c) => c[0] + ' x' + c[1]);
    if (hex.total === 0) {
      gates.push(gate('S8', 'no hardcoded hex color outside the token block', STATUS.PASS, '0 hardcoded hex colors',
        ['AR-22 target = 0, UX-G2 (spec 02); debt cleared, gate is blocking by default' + (strictHex ? ' (--strict accepted)' : '')]));
    } else {
      gates.push(gate('S8', 'no hardcoded hex color outside the token block', STATUS.FAIL, detail,
        notes.concat(violationsHint())));
    }
  }

  // ---- S9: prototype inline token block matches generated output (DC-09 / AR-22 drift)
  {
    let status = STATUS.PASS;
    let detail = '';
    const notes = [];
    try {
      const src = loadSource(root);
      const out = generate(src);
      let block = null;
      try { block = extractPrototypeBlock(html); } catch (err) { block = null; }
      if (block === null) {
        status = STATUS.FAIL;
        detail = 'prototype token markers (' + TOKEN_MARK_BEGIN + ' / ' + TOKEN_MARK_END + ') are missing';
      } else if (block !== out.prototypeBlock) {
        status = STATUS.FAIL;
        let i = 0;
        while (i < block.length && i < out.prototypeBlock.length && block.charAt(i) === out.prototypeBlock.charAt(i)) i++;
        detail = 'prototype inline token block differs from generated CSS at byte ' + i;
        notes.push('regenerate with: node tools/tokens/build.mjs');
      } else {
        detail = 'inline block matches generated CSS byte-for-byte (' + block.length + ' bytes, ' + src.order.length + ' tokens)';
        notes.push('reuses tools/tokens/lib.mjs generate()/extractPrototypeBlock(); no codegen logic duplicated');
      }
    } catch (err) {
      status = STATUS.FAIL;
      detail = 'token pipeline error: ' + err.message;
    }
    gates.push(gate('S9', 'prototype inline token block matches tokens/ codegen', status, detail, notes));
  }

  // ---- S10: keymap expands to three platforms with zero conflicts (AR-29 item 7)
  //
  // Truth source: the embedded Action Registry (<script type="application/json"
  // id="actionRegistry">). Every bound action must carry its stable id, the platform-agnostic
  // `display` form and an EXPLICIT macOS / Windows / Linux expansion (or a platformExclusive
  // marker with a single platform field). The human-facing registration (settings shortcut
  // table, command-palette .kb, popup .pi-tag) and every Mod+... prose mention are parsed back
  // and must resolve to a registry entry, so the registry cannot drift from what users see.
  {
    const PLATFORMS = ['mac', 'win', 'linux'];
    const notes = [];
    let ok = true;
    const fail = (msg) => {
      ok = false;
      if (notes.length < 40) notes.push(msg);
      else if (notes.length === 40) notes.push('... more problem(s) suppressed');
    };

    const reg = extractActionRegistry(html);
    const entries = reg.entries || [];
    if (reg.error) fail(reg.error);

    const byId = new Map();
    const chords = { mac: new Map(), win: new Map(), linux: new Map() };
    let bound = 0;
    for (const e of entries) {
      if (!e || typeof e !== 'object' || Array.isArray(e)) { fail('registry entry is not an object: ' + JSON.stringify(e)); continue; }
      const id = typeof e.id === 'string' ? e.id.trim() : '';
      if (!id) { fail('registry entry without a string id'); continue; }
      if (byId.has(id)) fail('duplicate action id "' + id + '" in the registry');
      byId.set(id, e);
      const exclusive = typeof e.platformExclusive === 'string' && e.platformExclusive ? e.platformExclusive : null;
      if (exclusive !== null && PLATFORMS.indexOf(exclusive) < 0) { fail(id + ': platformExclusive must be one of mac / win / linux'); continue; }
      const required = exclusive ? [exclusive] : PLATFORMS;
      for (const p of PLATFORMS) {
        const v = typeof e[p] === 'string' ? e[p].trim() : '';
        if (required.indexOf(p) >= 0) {
          if (!v) { fail(id + ': missing explicit ' + p + ' expansion' + (exclusive ? ' (declared platformExclusive=' + exclusive + ')' : '')); continue; }
          bound++;
          const parsed = parseChordSpec(v, p);
          if (!parsed.ok) { fail(id + ' ' + p + '="' + v + '": ' + parsed.problems.join('; ')); continue; }
          if (chords[p].has(parsed.chord)) fail('zero-conflict violation on ' + p + ': "' + id + '" and "' + chords[p].get(parsed.chord) + '" both expand to ' + v);
          else chords[p].set(parsed.chord, id);
        } else if (v) {
          fail(id + ': platformExclusive=' + exclusive + ' must not also declare ' + p + '="' + v + '"');
        }
      }
    }

    const sites = collectKeyDeclarations(html, tags);
    const declared = new Set();
    let structured = 0;
    for (const s of sites) {
      if (!s.token || s.token === '—') continue;
      structured++;
      declared.add(s.token);
      if (!s.action) continue;
      const e = byId.get(s.action);
      if (!e) fail(s.kind + ' [' + s.action + '] declares "' + s.token + '" but the action has no registry entry');
      else if (e.display !== s.token) fail(s.kind + ' [' + s.action + '] displays "' + s.token + '" but the registry display is "' + (e.display || '') + '"');
    }
    const prose = decodeKeyEntities(html.split(reg.raw).join(''));
    const tokens = scanKeyTokens(prose);
    for (const tk of tokens.keys()) declared.add(tk);
    for (const tk of declared) {
      let found = false;
      for (const e of entries) if (e && e.display === tk) { found = true; break; }
      if (!found) fail('key declaration "' + tk + '" has no three-platform registry entry (AR-29 item 7)');
    }
    for (const entry of byId) {
      const id = entry[0];
      const e = entry[1];
      if (e && e.display && !declared.has(e.display)) fail('registry entry "' + id + '" declares display "' + e.display + '" which appears nowhere in the prototype');
    }

    gates.push(gate('S10',
      'keymap expands to three platforms with zero conflicts',
      ok ? STATUS.PASS : STATUS.FAIL,
      entries.length + ' registered action(s) / ' + bound + ' platform binding(s); ' + structured + ' structured key site(s); ' + tokens.size + ' distinct Mod token(s) in prose',
      ok ? [
        'truth source: <script type="application/json" id="actionRegistry"> = stable id + display + explicit mac / win / linux expansion',
        'AR-29 item 7: Mod = Cmd on macOS, Ctrl+Shift as a whole on Windows/Linux; Mod+Shift+X is reassigned to Ctrl+Alt+X, never Ctrl+Shift+Shift+X',
        'cross-checked against keys-table rows (data-action), palette .kb (data-run), menu .pi-tag (data-act) and every Mod+... prose mention',
      ] : notes));
  }

  return { gates, root, protoPath, html, style, decls, scales, tags };
}

function violationsHint() {
  return [
    'AR-22 target = 0: replace the literal with a semantic token in tokens/base/palette.json, then run "npm run tokens:build";',
    'hex shown in display TEXT must be written with the HTML entity &#35; (renders "#", is not a CSS color literal);',
    'see tools/design-gates/README.md section "S8" for the rule.',
  ];
}

// --------------------------------------------------------------- CLI

function parseArgs(argv) {
  const a = { static: false, strict: false, selftest: false, noBrowser: false, json: false, quiet: false, updateBaseline: false, maxDiffPct: null };
  for (const x of argv) {
    if (x === '--static') a.static = true;
    else if (x === '--strict') a.strict = true;
    else if (x === '--selftest') a.selftest = true;
    else if (x === '--no-browser') a.noBrowser = true;
    else if (x === '--json') a.json = true;
    else if (x === '--update-baseline') a.updateBaseline = true;
    else if (x.indexOf('--max-diff-pct=') === 0) a.maxDiffPct = parseFloat(x.slice('--max-diff-pct='.length));
  }
  return a;
}

async function main(args) {
  const sections = [];
  const stat = runStaticChecks({ root: DEFAULT_ROOT, strictHex: args.strict });
  sections.push({ name: 'static layer (S1-S10; runs in any environment, no browser)', gates: stat.gates });

  let browserGates = [];
  if (!args.static && !args.noBrowser) {
    try {
      const mod = await import('./browser.mjs');
      browserGates = await mod.runBrowserGates({ root: DEFAULT_ROOT, updateBaseline: args.updateBaseline, maxDiffPct: args.maxDiffPct });
      sections.push({ name: 'browser layer (B1-B10; Chrome headless + CDP, --force-prefers-reduced-motion)', gates: browserGates });
    } catch (err) {
      browserGates = browserSkipGates(err.message || String(err));
      sections.push({ name: 'browser layer (B1-B10; Chrome headless + CDP)', gates: browserGates });
    }
  } else if (args.static || args.noBrowser) {
    browserGates = browserSkipGates('--static / --no-browser requested: browser layer not executed');
    sections.push({ name: 'browser layer (B1-B10; skipped by flag)', gates: browserGates });
  }

  const footer = 'baseline dir: prototype/baseline  |  S8 blocks by default (AR-22 target = 0)  |  browser layer needs Chrome (CHROME_PATH)';
  const rep = formatReport(CHECK_TITLE, sections, { footer });
  if (args.json) console.log(JSON.stringify({ sections, counts: rep.counts, result: rep.result }, null, 2));
  else console.log(rep.text);
  process.exit(rep.failed.length === 0 ? 0 : 1);
}

function browserSkipGates(reason) {
  const ids = [
    ['B1', 'page-level horizontal scroll = 0'],
    ['B2', 'no horizontal scroll with soft wrap off (#wrap=0)'],
    ['B3', 'tooltip is keyboard reachable (focus shows it)'],
    ['B4', 'visual regression under frozen animation vs baseline'],
    ['B5', 'settings window open / close / reopen from the sidebar'],
    ['B6', 'theme two-way cross-window linkage'],
    ['B7', 'tab "+" picker keyboard add + activate'],
    ['B8', 'window controls restorable in mac and win form'],
    ['B9', 'page-level vertical scroll = 0 with the sidebar / settings window open'],
    ['B10', 'keymap behaviour matches the Action Registry (AR-29 item 7)'],
  ];
  return ids.map((x) => gate(x[0], x[1], STATUS.SKIP, 'not executed: ' + reason));
}

// --------------------------------------------------------------- selftest

function writeMutation(tmp, mutate) {
  const p = path.join(tmp, PROTOTYPE_REL);
  const before = fs.readFileSync(p, 'utf8');
  const after = mutate(before);
  if (after === before) throw new Error('selftest injection did not change the prototype copy');
  fs.writeFileSync(p, after, 'utf8');
  return after;
}

function makeTempRoot() {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'termai-design-selftest-'));
  fs.cpSync(path.join(DEFAULT_ROOT, 'tokens'), path.join(tmp, 'tokens'), { recursive: true });
  copyFileInto(DEFAULT_ROOT, PROTOTYPE_REL, tmp);
  return tmp;
}

function gateById(res, id) {
  return res.gates.filter((g) => g.id === id)[0];
}

async function runSelftest(args) {
  const st = makeSelftest();
  const base = runStaticChecks({ root: DEFAULT_ROOT });
  const baseFails = base.gates.filter((g) => g.status === STATUS.FAIL);
  process.stdout.write('baseline (static): ' + (baseFails.length === 0 ? 'PASS' : 'FAIL') + '\n');
  if (baseFails.length) {
    for (const g of baseFails) console.log('  baseline failure [' + g.id + '] ' + g.title + ' - ' + g.detail);
    console.log('SELFTEST ABORTED: the baseline must pass before injected faults can be proven detectable.');
    process.exit(1);
  }

  const tmpRoots = [];
  function tmp() { const t = makeTempRoot(); tmpRoots.push(t); return t; }

  try {
    // S1 injection: make a third fold open by default
    {
      const root = tmp();
      writeMutation(root, (h) => h.replace('data-open="false"', 'data-open="true" open'));
      const res = runStaticChecks({ root });
      const g = gateById(res, 'S1');
      st.check('S1: 3rd fold section set to default-open', g.status === STATUS.FAIL, g.detail);
    }
    // S2 injection: put a forbidden entry back in the top bar
    {
      const root = tmp();
      writeMutation(root, (h) => h.replace('<div class="toolbar">', '<div class="toolbar"><span>连接主机</span>'));
      const g = gateById(runStaticChecks({ root }), 'S2');
      st.check('S2: "连接主机" injected into the top bar', g.status === STATUS.FAIL, g.detail);
    }
    // S3 injection: strip aria-label from an icon button
    {
      const root = tmp();
      writeMutation(root, (h) => h.replace(/(<button class="icon-btn"[^>]*?)\s+aria-label="[^"]*"/, '$1'));
      const g = gateById(runStaticChecks({ root }), 'S3');
      st.check('S3: aria-label removed from an .icon-btn', g.status === STATUS.FAIL, g.detail);
    }
    // S3b injection: an icon-only element with data-act that is NOT a <button> and has no
    // aria-label - exactly the class of gap (.t-x used to be) the widened scope must catch.
    {
      const root = tmp();
      writeMutation(root, (h) => h.replace('<div class="views">', '<div class="views"><span class="inj-ctl" data-act="injected-ctl" data-tip="注入控件"></span>'));
      const g = gateById(runStaticChecks({ root }), 'S3');
      st.check('S3: icon-only [data-act] non-button element without aria-label', g.status === STATUS.FAIL, g.detail);
    }
    // S4 injection: strip aria-label from a window control
    {
      const root = tmp();
      writeMutation(root, (h) => h.replace(/(<button class="wc-dot wc-close"[^>]*?)\s+aria-label="[^"]*"/, '$1'));
      const g = gateById(runStaticChecks({ root }), 'S4');
      st.check('S4: aria-label removed from a window control', g.status === STATUS.FAIL, g.detail);
    }
    // S5 injection: add a second view
    {
      const root = tmp();
      writeMutation(root, (h) => h.replace('<div class="views">', '<div class="views"><section class="view" id="viewInjected" aria-label="注入视图"></section>'));
      const g = gateById(runStaticChecks({ root }), 'S5');
      st.check('S5: a second .view injected into the main area', g.status === STATUS.FAIL, g.detail);
    }
    // S6 injection: strip a rail label
    {
      const root = tmp();
      writeMutation(root, (h) => h.replace(/(<button class="rail-btn"[^>]*?)\s+aria-label="[^"]*"/, '$1'));
      const g = gateById(runStaticChecks({ root }), 'S6');
      st.check('S6: aria-label removed from a rail section', g.status === STATUS.FAIL, g.detail);
    }
    // S7 injection: an off-ruler spacing value
    {
      const root = tmp();
      writeMutation(root, (h) => h.replace('gap:var(--sp-1)', 'gap:13px'));
      const g = gateById(runStaticChecks({ root }), 'S7');
      st.check('S7: spacing changed to 13px', g.status === STATUS.FAIL, g.detail);
    }
    // S7b injection: off-ruler radius + font size + duration in one go
    {
      const root = tmp();
      writeMutation(root, (h) => h.replace('</style>', '.inj-ruler{border-radius:11px;font-size:13.5px;transition:opacity 133ms linear}</style>'));
      const g = gateById(runStaticChecks({ root }), 'S7');
      st.check('S7: radius 11px + font-size 13.5px + transition 133ms', g.status === STATUS.FAIL, g.detail);
    }
    // S8 injection: hardcoded hex outside the token block, checked with the DEFAULT options
    // (S8 is blocking by default now; --strict is a compatibility no-op).
    {
      const root = tmp();
      writeMutation(root, (h) => h.replace('</head>', '<style>.inj-hex{color:#ABCDEF}</style></head>'));
      const g = gateById(runStaticChecks({ root }), 'S8');
      st.check('S8: hardcoded #ABCDEF outside the token block (default = blocking)', g.status === STATUS.FAIL, g.detail);
    }
    // S9 injection: one byte inside the token block
    {
      const root = tmp();
      writeMutation(root, (h) => h.replace('--canvas: #08090C;', '--canvas: #08090D;'));
      const g = gateById(runStaticChecks({ root }), 'S9');
      st.check('S9: token block changed by one byte', g.status === STATUS.FAIL, g.detail);
    }
    // S10 injection: rebind a second action onto a chord already taken on one platform.
    {
      const root = tmp();
      writeMutation(root, (h) => h.replace(/(\{ "id": "quit",[^\n]*?"win": ")[^"]*"/, '$1Ctrl+Shift+K"'));
      const g = gateById(runStaticChecks({ root }), 'S10');
      st.check('S10: two actions bound to the same Windows chord (Ctrl+Shift+K)', g.status === STATUS.FAIL, g.detail);
    }
    // S10 injection: the mechanical Mod+Shift expansion, i.e. an invalid Ctrl+Shift+Shift chord.
    {
      const root = tmp();
      writeMutation(root, (h) => h.replace(/(\{ "id": "quit",[^\n]*?"win": ")[^"]*"/, '$1Ctrl+Shift+Shift+K"'));
      const g = gateById(runStaticChecks({ root }), 'S10');
      st.check('S10: Windows expansion becomes Ctrl+Shift+Shift+K (invalid)', g.status === STATUS.FAIL, g.detail);
    }
    // S10 injection: drop one platform field without marking the action platformExclusive.
    {
      const root = tmp();
      writeMutation(root, (h) => h.replace(/(\{ "id": "quit",[^\n]*?), "linux": "[^"]*"/, '$1'));
      const g = gateById(runStaticChecks({ root }), 'S10');
      st.check('S10: linux expansion missing with no platformExclusive marker', g.status === STATUS.FAIL, g.detail);
    }
    // S10 injection: a palette item advertises a key that is not in the registry.
    {
      const root = tmp();
      writeMutation(root, (h) => h.replace('<span class="kb">Mod+Q</span>', '<span class="kb">Mod+Shift+Z</span>'));
      const g = gateById(runStaticChecks({ root }), 'S10');
      st.check('S10: palette advertises an unregistered key (Mod+Shift+Z)', g.status === STATUS.FAIL, g.detail);
    }

    // Browser-layer injections (only when Chrome is available).
    let browserMod = null;
    let browserErr = null;
    try { browserMod = await import('./browser.mjs'); } catch (err) { browserErr = err.message; }
    const chrome = browserMod ? browserMod.findChrome() : null;
    if (browserMod && chrome) {
      const bRoot1 = tmp();
      writeMutation(bRoot1, (h) => h.replace('</style>', '.term-body{overflow-x:scroll !important}</style>'));
      const res1 = await browserMod.runBrowserGates({ root: bRoot1, skipBaseline: true, gateFilter: ['B1', 'B2'] });
      const b1 = res1.filter((g) => g.id === 'B1' || g.id === 'B2').filter((g) => g.status === STATUS.FAIL);
      st.check('B1/B2: overflow-x:scroll forced on the terminal grid', b1.length > 0, b1.length ? b1[0].detail : 'no browser gate failed');

      const bRoot2 = tmp();
      writeMutation(bRoot2, (h) => h.replace('</style>', '.tip{display:none !important}</style>'));
      const res2 = await browserMod.runBrowserGates({ root: bRoot2, skipBaseline: true, gateFilter: ['B3'] });
      const b3 = res2.filter((g) => g.id === 'B3' && g.status === STATUS.FAIL);
      st.check('B3: tooltip hidden with display:none', b3.length > 0, b3.length ? b3[0].detail : 'no browser gate failed');

      // B9 injection: force a page-level vertical overflow (release the body clip + a 3000px block).
      const bRoot3 = tmp();
      writeMutation(bRoot3, (h) => h.replace('</body>', '<style>html,body{height:auto !important; overflow-y:visible !important}</style><div class="inj-tall" aria-hidden="true" style="display:block;height:3000px"></div></body>'));
      const res3 = await browserMod.runBrowserGates({ root: bRoot3, skipBaseline: true, gateFilter: ['B9'] });
      const b9 = res3.filter((g) => g.id === 'B9' && g.status === STATUS.FAIL);
      st.check('B9: 3000px block injected, page-level vertical overflow forced', b9.length > 0, b9.length ? b9[0].detail : 'no browser gate failed');

      // B10 injection: degrade the platform expansion back to the legacy "metaKey || ctrlKey"
      // rule - Shift is rejected instead of being part of the Windows Mod. The registry stays
      // internally consistent (S10 still passes), so only B10 can catch registry/behaviour drift.
      const bRoot4 = tmp();
      writeMutation(bRoot4, (h) => h.replace('if (!!e.shiftKey !== c.shift) return false;', 'if (e.shiftKey) return false;'));
      const res4 = await browserMod.runBrowserGates({ root: bRoot4, skipBaseline: true, gateFilter: ['B10'] });
      const b10 = res4.filter((g) => g.id === 'B10' && g.status === STATUS.FAIL);
      st.check('B10: win Mod expansion degenerates to bare Ctrl (Shift rejected)', b10.length > 0, b10.length ? b10[0].detail : 'no browser gate failed');
    } else {
      st.skip('B1/B2: overflow-x:scroll forced on the terminal grid', 'Chrome not found' + (browserErr ? ' (' + browserErr + ')' : '') + '; browser assertions cannot run here');
      st.skip('B3: tooltip hidden with display:none', 'Chrome not found; browser assertions cannot run here');
      st.skip('B9: 3000px block injected, page-level vertical overflow forced', 'Chrome not found; browser assertions cannot run here');
      st.skip('B10: win Mod expansion degenerates to bare Ctrl (Shift rejected)', 'Chrome not found; browser assertions cannot run here');
    }
  } finally {
    for (const t of tmpRoots) {
      try { fs.rmSync(t, { recursive: true, force: true }); } catch (err) { /* best effort */ }
    }
  }

  const rep = st.report('design:check --selftest (injections run on temporary copies only)');
  console.log(rep.text);
  if (rep.skipped) {
    console.log('note: ' + rep.skipped + ' browser-layer injection(s) skipped because Chrome is unavailable;');
    console.log('      re-run on a machine with Chrome to execute the full injection set.');
  }
  console.log('prototype source untouched: injections were applied to temp copies under ' + os.tmpdir());
  process.exit(rep.ok ? 0 : 1);
}

// --------------------------------------------------------------- entry

const args = parseArgs(process.argv.slice(2));
if (args.selftest) {
  runSelftest(args).catch((err) => { console.error('SELFTEST ERROR: ' + (err && err.stack ? err.stack : err)); process.exit(1); });
} else {
  main(args).catch((err) => { console.error('design:check ERROR: ' + (err && err.stack ? err.stack : err)); process.exit(1); });
}
