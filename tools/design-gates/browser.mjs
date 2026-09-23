// tools/design-gates/browser.mjs
// TermAI design acceptance gates - BROWSER LAYER (B1-B10). Zero external dependencies:
// Chrome headless is driven over the DevTools Protocol using Node's built-in WebSocket
// (no puppeteer / playwright).
//
// Visual-regression methodology (docs/spec/07, AR-22 / AR-23):
//   * every render passes --force-prefers-reduced-motion, so the prototype's only
//     animation (the 200ms caret breathing) is disabled and screenshots are stable;
//   * each state is rendered twice and the PNG hashes MUST match (animation-freeze
//     self-check) before any pixel comparison happens;
//   * the comparison against prototype/baseline is exact by default, or bounded by
//     --max-diff-pct with the diff bounding box reported.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import {
  DEFAULT_ROOT, PROTOTYPE_REL, BASELINE_DIR_REL,
  sha256, decodePng, diffImages, gate, STATUS,
} from './lib.mjs';

// ------------------------------------------------------------------ chrome discovery

export function findChrome() {
  const env = process.env.CHROME_PATH || process.env.CHROME_BIN || process.env.GOOGLE_CHROME_BIN;
  if (env && fs.existsSync(env)) return env;
  const cands = [];
  if (process.platform === 'win32') {
    const pf = process.env['ProgramFiles'] || 'C:/Program Files';
    const pf86 = process.env['ProgramFiles(x86)'] || 'C:/Program Files (x86)';
    const local = process.env['LOCALAPPDATA'] || '';
    cands.push(
      pf + '/Google/Chrome/Application/chrome.exe',
      pf86 + '/Google/Chrome/Application/chrome.exe',
      local + '/Google/Chrome/Application/chrome.exe',
      pf + '/Microsoft/Edge/Application/msedge.exe',
      pf86 + '/Microsoft/Edge/Application/msedge.exe'
    );
  } else if (process.platform === 'darwin') {
    cands.push(
      '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      '/Applications/Chromium.app/Contents/MacOS/Chromium',
      '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge'
    );
  } else {
    cands.push('/usr/bin/google-chrome', '/usr/bin/google-chrome-stable', '/usr/bin/chromium', '/usr/bin/chromium-browser', '/snap/bin/chromium');
  }
  for (const c of cands) if (c && fs.existsSync(c)) return c;
  const names = process.platform === 'win32' ? ['chrome.exe', 'msedge.exe'] : ['google-chrome', 'google-chrome-stable', 'chromium', 'chromium-browser'];
  for (const dir of String(process.env.PATH || '').split(path.delimiter)) {
    if (!dir) continue;
    for (const n of names) {
      const p = path.join(dir, n);
      if (fs.existsSync(p)) return p;
    }
  }
  return null;
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

export async function launchChrome(chromePath) {
  const userDataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'termai-cdp-'));
  const args = [
    '--headless=new',
    '--remote-debugging-port=0',
    '--user-data-dir=' + userDataDir,
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-extensions',
    '--disable-background-networking',
    '--disable-component-update',
    '--disable-sync',
    '--disable-features=Translate,OptimizationHints',
    '--disable-dev-shm-usage',
    '--no-sandbox',
    '--disable-gpu',
    '--force-prefers-reduced-motion',
    '--window-size=1600,1000',
    'about:blank',
  ];
  const proc = spawn(chromePath, args, { stdio: 'ignore' });
  const portFile = path.join(userDataDir, 'DevToolsActivePort');
  const deadline = Date.now() + 25000;
  while (!fs.existsSync(portFile)) {
    if (proc.exitCode !== null) {
      fs.rmSync(userDataDir, { recursive: true, force: true });
      throw new Error('Chrome exited immediately (code ' + proc.exitCode + '): ' + chromePath);
    }
    if (Date.now() > deadline) {
      proc.kill();
      fs.rmSync(userDataDir, { recursive: true, force: true });
      throw new Error('Chrome did not expose a DevTools port within 25s: ' + chromePath);
    }
    await sleep(120);
  }
  const text = fs.readFileSync(portFile, 'utf8').split(/\r?\n/).filter(Boolean);
  const port = text[0];
  const wsPath = text[1] || '/devtools/browser';
  const wsUrl = 'ws://127.0.0.1:' + port + wsPath;
  return { proc, userDataDir, wsUrl, dispose: () => { try { proc.kill(); } catch (e) {} try { fs.rmSync(userDataDir, { recursive: true, force: true }); } catch (e) {} } };
}

// ------------------------------------------------------------------ minimal CDP client

export class Cdp {
  constructor(url) {
    this.url = url;
    this.id = 0;
    this.pending = new Map();
    this.listeners = new Map();
  }
  connect() {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(this.url);
      this.ws = ws;
      const to = setTimeout(() => reject(new Error('CDP websocket connect timeout')), 15000);
      ws.addEventListener('open', () => { clearTimeout(to); resolve(); });
      ws.addEventListener('error', () => { clearTimeout(to); reject(new Error('CDP websocket error')); });
      ws.addEventListener('close', () => {
        for (const p of this.pending.values()) p.reject(new Error('CDP connection closed'));
        this.pending.clear();
      });
      ws.addEventListener('message', (ev) => {
        let msg;
        try {
          msg = JSON.parse(typeof ev.data === 'string' ? ev.data : Buffer.from(ev.data).toString('utf8'));
        } catch (err) { return; }
        if (msg.id !== undefined && msg.id !== null) {
          const p = this.pending.get(msg.id);
          if (!p) return;
          this.pending.delete(msg.id);
          if (msg.error) p.reject(new Error(msg.error.message)); else p.resolve(msg.result || {});
        } else if (msg.method) {
          const ls = (this.listeners.get(msg.method) || []).slice();
          for (const fn of ls) { try { fn(msg.params || {}, msg.sessionId); } catch (err) { /* listener errors must not kill the client */ } }
        }
      });
    });
  }
  send(method, params, sessionId) {
    const id = ++this.id;
    const payload = { id, method, params: params || {} };
    if (sessionId) payload.sessionId = sessionId;
    return new Promise((resolve, reject) => {
      const to = setTimeout(() => { this.pending.delete(id); reject(new Error('CDP timeout: ' + method)); }, 30000);
      this.pending.set(id, {
        resolve: (v) => { clearTimeout(to); resolve(v); },
        reject: (e) => { clearTimeout(to); reject(e); },
      });
      try { this.ws.send(JSON.stringify(payload)); } catch (err) { clearTimeout(to); this.pending.delete(id); reject(err); }
    });
  }
  on(method, fn) {
    const arr = this.listeners.get(method) || [];
    arr.push(fn);
    this.listeners.set(method, arr);
    return () => { const a = this.listeners.get(method) || []; const i = a.indexOf(fn); if (i >= 0) a.splice(i, 1); };
  }
  once(method, timeoutMs) {
    return new Promise((resolve, reject) => {
      const off = this.on(method, (params) => { off(); clearTimeout(to); resolve(params); });
      const to = setTimeout(() => { off(); reject(new Error('CDP event timeout: ' + method)); }, timeoutMs || 20000);
    });
  }
  close() { try { this.ws.close(); } catch (err) {} }
}

// ------------------------------------------------------------------ page probes
// Each probe is a self-contained function serialised with toString() and evaluated in the
// page, so no template literal / quoting gymnastics are needed here.

function probeOverflow() {
  var de = document.documentElement;
  var offenders = [];
  var all = document.querySelectorAll('body *');
  for (var i = 0; i < all.length; i++) {
    var el = all[i];
    var cs = getComputedStyle(el);
    if (cs.display === 'none' || cs.visibility === 'hidden') continue;
    if (cs.overflowX !== 'visible') continue;
    if (el.scrollWidth > el.clientWidth + 1) {
      offenders.push({
        el: el.tagName.toLowerCase() + (el.id ? '#' + el.id : '') + (typeof el.className === 'string' && el.className.trim() ? '.' + el.className.trim().split(/\s+/).join('.') : ''),
        sw: el.scrollWidth,
        cw: el.clientWidth,
      });
      if (offenders.length >= 8) break;
    }
  }
  var term = document.querySelector('#term');
  var termCS = term ? getComputedStyle(term) : null;
  return {
    docSW: de.scrollWidth,
    docCW: de.clientWidth,
    bodySW: document.body.scrollWidth,
    bodyCW: document.body.clientWidth,
    termOverflowX: termCS ? termCS.overflowX : null,
    termSW: term ? term.scrollWidth : null,
    termCW: term ? term.clientWidth : null,
    offenders: offenders,
  };
}

// Page-level vertical overflow. Internal scroll containers (overflow-y:auto/hidden) are
// excluded by design; only elements whose own overflow-y is visible - i.e. overflow that
// propagates to the page - are offenders. (A22 / UX-G11 / AR-22 item 6.)
function probeVerticalOverflow() {
  var de = document.documentElement;
  var body = document.body;
  var offenders = [];
  var all = document.querySelectorAll('body *');
  for (var i = 0; i < all.length; i++) {
    var el = all[i];
    var cs = getComputedStyle(el);
    if (cs.display === 'none' || cs.visibility === 'hidden') continue;
    if (cs.overflowY !== 'visible') continue;
    if (el.scrollHeight > el.clientHeight + 1) {
      offenders.push({
        el: el.tagName.toLowerCase() + (el.id ? '#' + el.id : '') + (typeof el.className === 'string' && el.className.trim() ? '.' + el.className.trim().split(/\s+/).join('.') : ''),
        sh: el.scrollHeight,
        ch: el.clientHeight,
      });
      if (offenders.length >= 8) break;
    }
  }
  return {
    docSH: de.scrollHeight,
    docCH: de.clientHeight,
    bodySH: body.scrollHeight,
    bodyCH: body.clientHeight,
    winH: window.innerHeight,
    offenders: offenders,
  };
}

function probeTooltip() {
  var tip = document.querySelector('.tip');
  var btn = document.querySelector('#mainWin .toolbar .icon-btn[data-tip]') || document.querySelector('[data-tip]');
  var out = { trigger: null, visible: false, text: '', hiddenAfterEsc: false, ariaDescribedBy: false };
  if (btn) {
    out.trigger = btn.tagName.toLowerCase() + (btn.id ? '#' + btn.id : '');
    btn.focus();
  }
  var cs = tip ? getComputedStyle(tip) : null;
  out.visible = !!(tip && tip.classList.contains('is-on') && cs.display !== 'none' && cs.visibility !== 'hidden' && parseFloat(cs.opacity) > 0.5);
  out.text = tip ? tip.textContent.slice(0, 80) : '';
  out.ariaDescribedBy = !!(btn && btn.getAttribute('aria-describedby') === 'uiTip');
  document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  out.hiddenAfterEsc = !!(tip && !tip.classList.contains('is-on'));
  return out;
}

function probeSettingsCycle() {
  var win = document.querySelector('#settingsWin');
  var railSettings = document.querySelector('#rail [data-act="settings"]');
  var topSettings = document.querySelector('#mainWin [data-act="settings"]');
  var out = { initialHidden: win.hidden, openHidden: null, openNotClosed: null, closedHidden: null, closedClass: null, reopenHidden: null, reopenedFromRail: false };
  if (!railSettings) return out;
  railSettings.click();
  out.openHidden = win.hidden;
  out.openNotClosed = !win.classList.contains('is-closed');
  var closeBtn = win.querySelector('.wc-flat [data-win="close"]') || win.querySelector('.wc-mac [data-win="close"]');
  if (closeBtn) closeBtn.click();
  out.closedHidden = win.hidden;
  out.closedClass = win.classList.contains('is-closed');
  railSettings.click();
  out.reopenHidden = win.hidden;
  out.reopenedFromRail = !win.hidden;
  return out;
}

function probeThemeLinkage() {
  var html = document.documentElement;
  var themeBtn = document.querySelector('#themeBtn');
  var settingsLight = document.querySelector('#segTheme button[data-theme-set="light"]');
  var settingsDark = document.querySelector('#segTheme button[data-theme-set="dark"]');
  var out = { start: html.getAttribute('data-theme') };
  if (settingsLight) settingsLight.click();
  out.afterSettingsLight = html.getAttribute('data-theme');
  out.settingsSelectedLight = !!(settingsLight && settingsLight.getAttribute('aria-selected') === 'true');
  out.mainBtnPressedAfterLight = themeBtn ? themeBtn.getAttribute('aria-pressed') : null;
  out.mainBtnLabelAfterLight = themeBtn ? themeBtn.getAttribute('aria-label') : null;
  if (themeBtn) themeBtn.click();
  out.afterMainToggle = html.getAttribute('data-theme');
  out.settingsSelectedDark = !!(settingsDark && settingsDark.getAttribute('aria-selected') === 'true');
  if (settingsDark) settingsDark.click();
  return out;
}

function probePicker() {
  var out = {};
  var tabsBefore = document.querySelectorAll('#termTabs .ttab').length;
  var add = document.querySelector('#addTabBtn');
  var pop = document.querySelector('#newTermPop');
  var inp = document.querySelector('#npInput');
  out.tabsBefore = tabsBefore;
  if (add) add.click();
  out.opened = pop ? !pop.hidden : false;
  if (inp) {
    inp.value = 'ubuntu';
    inp.dispatchEvent(new Event('input', { bubbles: true }));
    inp.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    inp.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  }
  out.tabsAfter = document.querySelectorAll('#termTabs .ttab').length;
  out.closedAfterEnter = pop ? pop.hidden : null;
  var active = document.querySelector('#termTabs .ttab[aria-selected="true"]');
  out.activeLabel = active ? active.textContent.trim() : null;
  out.created = out.tabsAfter === tabsBefore + 1;
  if (add) add.click();
  if (inp) inp.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  out.escClosed = pop ? pop.hidden : null;
  out.tabsAfterEsc = document.querySelectorAll('#termTabs .ttab').length;
  out.escNoSideEffect = out.tabsAfterEsc === out.tabsAfter;
  return out;
}

// B10: read-only observation of the state a real key event is supposed to change. The gate
// never writes DOM state itself; it only reads what the page's own keydown handling did.
function probeKeyState() {
  var ov = document.querySelector('#overlay');
  return {
    pal: !!(ov && ov.classList.contains('is-open')),
    theme: document.documentElement.getAttribute('data-theme'),
    plat: document.documentElement.getAttribute('data-plat'),
  };
}

function probeWindowForms(plat) {
  var res = { plat: plat, macVisible: null, flatVisible: null, states: {} };
  var segBtn = document.querySelector('#segPlat button[data-plat-set="' + plat + '"]');
  if (segBtn) segBtn.click();
  var macGroup = document.querySelector('#mainWin .titlebar .wc-mac');
  var flatGroup = document.querySelector('#mainWin .titlebar .wc-flat');
  var vis = function (el) { return !!el && getComputedStyle(el).display !== 'none'; };
  res.macVisible = vis(macGroup);
  res.flatVisible = vis(flatGroup);
  var main = document.querySelector('#mainWin');
  var group = (plat === 'mac') ? macGroup : flatGroup;
  var ctrl = function (action) { return group ? group.querySelector('[data-win="' + action + '"]') : null; };
  var state = function () {
    return { max: main.classList.contains('is-max'), min: main.classList.contains('is-min'), closed: main.classList.contains('is-closed') };
  };
  var maxBtn = ctrl('max');
  if (maxBtn) { maxBtn.click(); res.states.maxOn = state().max; maxBtn.click(); res.states.maxOff = !state().max; }
  var minBtn = ctrl('min');
  if (minBtn) { minBtn.click(); res.states.minOn = state().min; minBtn.click(); res.states.minOff = !state().min; }
  var closeBtn = ctrl('close');
  if (closeBtn) {
    closeBtn.click();
    res.states.closedOn = state().closed;
    var reopen = document.querySelector('#reopenMain');
    res.states.reopenExists = !!reopen;
    if (reopen) reopen.click();
    res.states.closedOff = !state().closed;
  }
  return res;
}

// ------------------------------------------------------------------ driver

const STATES = [
  { name: 'dark-workspaces-1600x1000', w: 1600, h: 1000, hash: '' },
  { name: 'light-workspaces-1600x1000', w: 1600, h: 1000, hash: '#theme=light' },
  { name: 'dark-agent-1600x1000', w: 1600, h: 1000, hash: '#sec=agent' },
  { name: 'dark-collapsed-1600x1000', w: 1600, h: 1000, hash: '#sidebar=collapsed' },
  { name: 'dark-settings-1600x1000', w: 1600, h: 1000, hash: '#settings=1' },
  { name: 'dark-nowrap-1600x1000', w: 1600, h: 1000, hash: '#wrap=0' },
  { name: 'dark-workspaces-1280x800', w: 1280, h: 800, hash: '' },
];

function platformTag() {
  return process.platform + '-' + process.arch;
}

async function evaluate(cdp, sessionId, expression, awaitPromise) {
  const r = await cdp.send('Runtime.evaluate', { expression: expression, returnByValue: true, awaitPromise: !!awaitPromise }, sessionId);
  if (r.exceptionDetails) {
    const ex = r.exceptionDetails.exception || {};
    throw new Error('page exception: ' + (ex.description || r.exceptionDetails.text || 'unknown'));
  }
  return r.result ? r.result.value : undefined;
}

function callProbe(fn, arg) {
  return '(' + fn.toString() + ')(' + (arg === undefined ? '' : JSON.stringify(arg)) + ')';
}

// Real key events over CDP (Input.dispatchKeyEvent) - B10 must not satisfy itself by reading
// DOM attributes. CDP modifier bitmask: Alt=1, Ctrl=2, Meta=4, Shift=8.
const KEY_MODIFIER_BITS = { alt: 1, ctrl: 2, meta: 4, shift: 8 };

async function pressKey(cdp, sessionId, spec) {
  const modifiers = (spec.modifiers || []).reduce((a, m) => a | (KEY_MODIFIER_BITS[m] || 0), 0);
  const base = {
    modifiers: modifiers,
    key: spec.key,
    code: spec.code,
    windowsVirtualKeyCode: spec.vk,
    nativeVirtualKeyCode: spec.vk,
  };
  await cdp.send('Input.dispatchKeyEvent', Object.assign({ type: 'keyDown' }, base), sessionId);
  await cdp.send('Input.dispatchKeyEvent', Object.assign({ type: 'keyUp' }, base), sessionId);
  await sleep(90);
}

async function newPage(cdp, w, h) {
  const created = await cdp.send('Target.createTarget', { url: 'about:blank' });
  const attached = await cdp.send('Target.attachToTarget', { targetId: created.targetId, flatten: true });
  const sessionId = attached.sessionId;
  await cdp.send('Page.enable', {}, sessionId);
  await cdp.send('Runtime.enable', {}, sessionId);
  await cdp.send('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: 1, mobile: false }, sessionId);
  await cdp.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] }, sessionId);
  return { targetId: created.targetId, sessionId };
}

let navSeq = 0;

async function navigate(cdp, page, url, state, settleMs) {
  navSeq++;
  await cdp.send('Emulation.setDeviceMetricsOverride', { width: state.w, height: state.h, deviceScaleFactor: 1, mobile: false }, page.sessionId);
  const loaded = cdp.once('Page.loadEventFired', 20000);
  await cdp.send('Page.navigate', { url: url + '?s=' + navSeq + (state.hash || '') }, page.sessionId);
  await loaded.catch(() => {});
  await evaluate(cdp, page.sessionId,
    'new Promise(function(r){function go(){setTimeout(function(){r(1);},140);}if(document.fonts&&document.fonts.ready){document.fonts.ready.then(go,go);}else{go();}})',
    true);
  // Two animation frames after the font/layout timers, then a generous settle: the
  // prototype runs foldFit()/sbFit() from fonts.ready and from 60ms/120ms timers, and a
  // screenshot taken between those runs is a different (though self-consistent) frame.
  await evaluate(cdp, page.sessionId,
    'new Promise(function(r){requestAnimationFrame(function(){requestAnimationFrame(function(){r(1);});});})',
    true);
  await sleep(settleMs === undefined ? 600 : settleMs);
}

async function capturePng(cdp, sessionId) {
  const r = await cdp.send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false, fromSurface: true }, sessionId);
  return Buffer.from(r.data, 'base64');
}

// ------------------------------------------------------------------ gates

export async function runBrowserGates(opts) {
  opts = opts || {};
  const root = opts.root || DEFAULT_ROOT;
  const skipBaseline = !!opts.skipBaseline;
  const updateBaseline = !!opts.updateBaseline;
  const maxDiffPct = (opts.maxDiffPct === undefined || opts.maxDiffPct === null) ? 0 : opts.maxDiffPct;
  // Per-channel +/-2 absorbs sub-pixel/compositor rounding (observed: 218 pixels, max delta 2
  // on the rounded accent borders); every pixel beyond that still fails.
  const pixelTolerance = (opts.pixelTolerance === undefined || opts.pixelTolerance === null) ? 2 : opts.pixelTolerance;
  const filter = opts.gateFilter || null;
  const want = (id) => !filter || filter.indexOf(id) >= 0;
  const protoUrl = pathToFileURL(path.join(root, PROTOTYPE_REL)).href;
  const baselineDir = path.join(root, BASELINE_DIR_REL);
  const tag = platformTag();

  const chromePath = findChrome();
  if (!chromePath) throw new Error('Chrome not found (set CHROME_PATH, or install google-chrome / chromium)');

  const chrome = await launchChrome(chromePath);
  const cdp = new Cdp(chrome.wsUrl);
  const gates = [];
  try {
    await cdp.connect();
    const page = await newPage(cdp, 1600, 1000);

    // ---- B1: page-level horizontal scroll = 0 (A17 / UX-G11)
    if (want('B1')) {
      try {
        const rows = [];
        let ok = true;
        for (const vp of [{ w: 1600, h: 1000 }, { w: 1280, h: 800 }]) {
          await navigate(cdp, page, protoUrl, { w: vp.w, h: vp.h, hash: '' });
          const r = await evaluate(cdp, page.sessionId, callProbe(probeOverflow));
          const pageOk = r.docSW === r.docCW && r.bodySW <= r.bodyCW + 1 && r.offenders.length === 0;
          if (!pageOk) ok = false;
          rows.push({ vp: vp.w + 'x' + vp.h, docSW: r.docSW, docCW: r.docCW, offenders: r.offenders });
        }
        gates.push(gate('B1', 'page-level horizontal scroll = 0', ok ? STATUS.PASS : STATUS.FAIL,
          rows.map((x) => x.vp + ' doc.scrollWidth=' + x.docSW + '/clientWidth=' + x.docCW + ' offending=' + x.offenders.length).join('; '),
          ok ? ['checked at 1600x1000 and 1280x800; overflow-x:auto/hidden containers are excluded by design (internal scroll containers)']
             : rows.filter((x) => x.offenders.length).map((x) => x.vp + ': ' + x.offenders.map((o) => o.el + ' sw=' + o.sw + ' cw=' + o.cw).join(', '))));
      } catch (err) {
        gates.push(gate('B1', 'page-level horizontal scroll = 0', STATUS.FAIL, 'browser error: ' + err.message));
      }
    }

    // ---- B2: soft wrap off (#wrap=0) still has no horizontal scroll (A34 / UX-G17)
    if (want('B2')) {
      try {
        const rows = [];
        let ok = true;
        for (const vp of [{ w: 1600, h: 1000 }, { w: 1280, h: 800 }]) {
          await navigate(cdp, page, protoUrl, { w: vp.w, h: vp.h, hash: '#wrap=0' });
          const r = await evaluate(cdp, page.sessionId, callProbe(probeOverflow));
          const noWrap = await evaluate(cdp, page.sessionId, 'document.body.classList.contains("no-wrap")');
          const termOk = r.termOverflowX !== 'scroll' && r.termOverflowX !== 'auto' && r.termSW <= r.termCW + 1;
          const pageOk = r.docSW === r.docCW && r.offenders.length === 0;
          if (!pageOk || !termOk || !noWrap) ok = false;
          rows.push({ vp: vp.w + 'x' + vp.h, noWrap: noWrap, termOverflowX: r.termOverflowX, termSW: r.termSW, termCW: r.termCW, docSW: r.docSW, docCW: r.docCW });
        }
        gates.push(gate('B2', 'no horizontal scroll with soft wrap off (#wrap=0)', ok ? STATUS.PASS : STATUS.FAIL,
          rows.map((x) => x.vp + ' no-wrap=' + x.noWrap + ' #term overflow-x=' + x.termOverflowX + ' sw=' + x.termSW + '/cw=' + x.termCW).join('; '),
          ok ? ['visual clipping: #term keeps overflow-x:hidden and its scrollWidth does not exceed clientWidth (AR-23 item 6)']
             : ['terminal grid must never become a horizontal scroll container (AR-23 item 6 / A34)']));
      } catch (err) {
        gates.push(gate('B2', 'no horizontal scroll with soft wrap off (#wrap=0)', STATUS.FAIL, 'browser error: ' + err.message));
      }
    }

    // ---- B3: tooltip keyboard reachable (A19 / UX-G12)
    if (want('B3')) {
      try {
        await navigate(cdp, page, protoUrl, { w: 1600, h: 1000, hash: '' });
        const r = await evaluate(cdp, page.sessionId, callProbe(probeTooltip));
        const ok = !!(r.visible && r.text && r.hiddenAfterEsc);
        gates.push(gate('B3', 'tooltip is keyboard reachable (focus shows it, Esc hides it)', ok ? STATUS.PASS : STATUS.FAIL,
          'trigger=' + r.trigger + ' visible=' + r.visible + ' escHides=' + r.hiddenAfterEsc + ' aria-describedby=' + r.ariaDescribedBy,
          [ok ? 'tooltip text: ' + r.text : 'AR-22 item 3: focus-visible tooltips + Esc close are part of A19']));
      } catch (err) {
        gates.push(gate('B3', 'tooltip is keyboard reachable (focus shows it, Esc hides it)', STATUS.FAIL, 'browser error: ' + err.message));
      }
    }

    // ---- B5: settings window open / close / reopen (A28 / UX-G15)
    if (want('B5')) {
      try {
        await navigate(cdp, page, protoUrl, { w: 1600, h: 1000, hash: '' });
        const r = await evaluate(cdp, page.sessionId, callProbe(probeSettingsCycle));
        const ok = r.initialHidden === true && r.openHidden === false && r.openNotClosed === true && r.closedHidden === true && r.reopenHidden === false;
        gates.push(gate('B5', 'settings window can be opened, closed, and reopened from the sidebar', ok ? STATUS.PASS : STATUS.FAIL,
          'initialHidden=' + r.initialHidden + ' openHidden=' + r.openHidden + ' closedHidden=' + r.closedHidden + ' reopenHidden=' + r.reopenHidden,
          [ok ? 'reopened from #rail [data-act=settings]' : 'AR-23 item 3 / A28: closing must not dead-end the settings window']));
      } catch (err) {
        gates.push(gate('B5', 'settings window can be opened, closed, and reopened from the sidebar', STATUS.FAIL, 'browser error: ' + err.message));
      }
    }

    // ---- B6: theme two-way cross-window linkage (A29 / UX-G15)
    if (want('B6')) {
      try {
        await navigate(cdp, page, protoUrl, { w: 1600, h: 1000, hash: '#settings=1' });
        const r = await evaluate(cdp, page.sessionId, callProbe(probeThemeLinkage));
        const ok = r.afterSettingsLight === 'light' && r.settingsSelectedLight === true && r.afterMainToggle === 'dark' && r.settingsSelectedDark === true;
        gates.push(gate('B6', 'theme is linked two-way between main and settings window', ok ? STATUS.PASS : STATUS.FAIL,
          'start=' + r.start + ' settings->light=' + r.afterSettingsLight + ' main->= ' + r.afterMainToggle + ' settingsSelectedDark=' + r.settingsSelectedDark,
          [ok ? 'both directions observed in the same document (main window <-> settings window)'
             : 'AR-23 item 3 / A29: any window change must reflect in the other without restart']));
      } catch (err) {
        gates.push(gate('B6', 'theme is linked two-way between main and settings window', STATUS.FAIL, 'browser error: ' + err.message));
      }
    }

    // ---- B7: tab "+" picker keyboard add + activate (A30 / UX-G15)
    if (want('B7')) {
      try {
        await navigate(cdp, page, protoUrl, { w: 1600, h: 1000, hash: '' });
        const r = await evaluate(cdp, page.sessionId, callProbe(probePicker));
        const ok = r.opened === true && r.created === true && r.closedAfterEnter === true && r.escClosed === true && r.escNoSideEffect === true;
        gates.push(gate('B7', 'tab "+" picker adds and activates a tab by keyboard', ok ? STATUS.PASS : STATUS.FAIL,
          'opened=' + r.opened + ' tabs ' + r.tabsBefore + '->' + r.tabsAfter + ' active="' + r.activeLabel + '" escClosed=' + r.escClosed + ' escNoSideEffect=' + r.escNoSideEffect,
          [ok ? 'keyboard path: type -> ArrowDown -> Enter; Esc cancels with no side effect'
             : 'AR-23 item 4 / A30: the picker must support search + arrow keys + Enter and Esc must be side-effect free']));
      } catch (err) {
        gates.push(gate('B7', 'tab "+" picker adds and activates a tab by keyboard', STATUS.FAIL, 'browser error: ' + err.message));
      }
    }

    // ---- B8: window controls in mac / win form, three states restorable (A32 / UX-G16)
    if (want('B8')) {
      try {
        await navigate(cdp, page, protoUrl, { w: 1600, h: 1000, hash: '' });
        const mac = await evaluate(cdp, page.sessionId, callProbe(probeWindowForms, 'mac'));
        const win = await evaluate(cdp, page.sessionId, callProbe(probeWindowForms, 'win'));
        const formOk = (x) => (x.plat === 'mac' ? (x.macVisible && !x.flatVisible) : (!x.macVisible && x.flatVisible));
        const statesOk = (x) => x.states.maxOn === true && x.states.maxOff === true && x.states.minOn === true && x.states.minOff === true && x.states.closedOn === true && x.states.closedOff === true;
        const ok = formOk(mac) && formOk(win) && statesOk(mac) && statesOk(win);
        gates.push(gate('B8', 'window controls: mac / win form, max/min/close all restorable', ok ? STATUS.PASS : STATUS.FAIL,
          'mac formVisible=' + mac.macVisible + ' states=' + JSON.stringify(mac.states) + ' | win flatVisible=' + win.flatVisible + ' states=' + JSON.stringify(win.states),
          [ok ? 'both forms expose the visible control group and all three states round-trip'
             : 'AR-23 item 5 / A32: only position and shape follow the platform; the three states must be real']));
      } catch (err) {
        gates.push(gate('B8', 'window controls: mac / win form, max/min/close all restorable', STATUS.FAIL, 'browser error: ' + err.message));
      }
    }

    // ---- B9: page-level vertical scroll = 0 with the sidebar panel / settings window open (A22 / UX-G11)
    if (want('B9')) {
      try {
        const rows = [];
        let ok = true;
        for (const vp of [{ w: 1600, h: 1000 }, { w: 1280, h: 800 }]) {
          for (const sc of [{ label: 'sidebar', hash: '' }, { label: 'settings', hash: '#settings=1' }]) {
            await navigate(cdp, page, protoUrl, { w: vp.w, h: vp.h, hash: sc.hash });
            const r = await evaluate(cdp, page.sessionId, callProbe(probeVerticalOverflow));
            const pageOk = r.docSH <= r.docCH + 1 && r.bodySH <= r.bodyCH + 1 && r.offenders.length === 0;
            if (!pageOk) ok = false;
            rows.push({ vp: vp.w + 'x' + vp.h + '/' + sc.label, docSH: r.docSH, docCH: r.docCH, bodySH: r.bodySH, bodyCH: r.bodyCH, offenders: r.offenders });
          }
        }
        const bad = rows.filter((x) => x.docSH > x.docCH + 1 || x.bodySH > x.bodyCH + 1 || x.offenders.length);
        gates.push(gate('B9', 'page-level vertical scroll = 0 with the sidebar / settings window open', ok ? STATUS.PASS : STATUS.FAIL,
          rows.map((x) => x.vp + ' doc.scrollHeight=' + x.docSH + '/clientHeight=' + x.docCH + ' body=' + x.bodySH + '/' + x.bodyCH + ' offending=' + x.offenders.length).join('; '),
          ok ? ['checked at 1600x1000 and 1280x800 with the sidebar panel open and with the settings window open',
                'overflow-y:auto/hidden containers are excluded by design (internal scroll containers: .panel-scroll / .ai-scroll / .term-body)']
             : bad.map((x) => x.vp + ': ' + (x.offenders.length ? x.offenders.map((o) => o.el + ' sh=' + o.sh + ' ch=' + o.ch).join(', ') : 'doc/body scrollHeight exceeds clientHeight'))));
      } catch (err) {
        gates.push(gate('B9', 'page-level vertical scroll = 0 with the sidebar / settings window open', STATUS.FAIL, 'browser error: ' + err.message));
      }
    }

    // ---- B10: keymap BEHAVIOUR matches the Action Registry (AR-29 item 7)
    //
    // S10 proves the registry is internally consistent; B10 proves the runtime keydown path
    // actually resolves chords through it. Every assertion below comes from a real CDP key
    // event (modifier bitmask) and only reads the resulting page state.
    if (want('B10')) {
      try {
        const N = 1600;
        const state = () => evaluate(cdp, page.sessionId, callProbe(probeKeyState));
        const press = (spec) => pressKey(cdp, page.sessionId, spec);
        const K = { key: 'K', code: 'KeyK', vk: 75 };
        const L = { key: 'L', code: 'KeyL', vk: 76 };
        const ESC = { key: 'Escape', code: 'Escape', vk: 27 };

        // --- Windows / Linux form: Mod = Ctrl+Shift as a whole; Mod+Shift+X -> Ctrl+Alt+X.
        await navigate(cdp, page, protoUrl, { w: N, h: 1000, hash: '#plat=win' });
        await cdp.send('Page.bringToFront', {}, page.sessionId);
        const winStart = await state();
        await press(Object.assign({ modifiers: ['ctrl', 'shift'] }, K));
        const afterModK = await state();
        await press(ESC);
        const afterEsc = await state();
        await press(Object.assign({ modifiers: ['ctrl'] }, K));
        const afterBareCtrlK = await state();
        await press(Object.assign({ modifiers: ['ctrl', 'alt'] }, L)); // theme = Mod+Shift+L
        const afterCtrlAltL = await state();
        await press(Object.assign({ modifiers: ['ctrl', 'shift'] }, L));
        const afterCtrlShiftL = await state();

        // --- macOS form: Mod = Cmd; bare Ctrl+K must stay with the terminal.
        await navigate(cdp, page, protoUrl, { w: N, h: 1000, hash: '#plat=mac' });
        await cdp.send('Page.bringToFront', {}, page.sessionId);
        const macStart = await state();
        await press(Object.assign({ modifiers: ['meta'] }, K));
        const afterCmdK = await state();
        await press(ESC);
        const macAfterEsc = await state();
        await press(Object.assign({ modifiers: ['ctrl'] }, K));
        const macAfterCtrlK = await state();

        const checks = [
          ['win #plat=win resolved', winStart.plat === 'win'],
          ['win Ctrl+Shift+K opens the palette (Mod = Ctrl+Shift)', afterModK.pal === true],
          ['win Esc closes the palette', afterEsc.pal === false],
          ['win bare Ctrl+K does NOT open the palette (Ctrl alone belongs to the terminal)', afterBareCtrlK.pal === false],
          ['win Ctrl+Alt+L toggles the theme (Mod+Shift+L -> Ctrl+Alt+L)', winStart.theme === 'dark' && afterCtrlAltL.theme === 'light'],
          ['win Ctrl+Shift+L does NOT toggle the theme (never a mechanical shift stack)', afterCtrlShiftL.theme === 'light'],
          ['mac #plat=mac resolved', macStart.plat === 'mac'],
          ['mac Cmd+K opens the palette (Mod = Cmd)', afterCmdK.pal === true],
          ['mac Esc closes the palette', macAfterEsc.pal === false],
          ['mac Ctrl+K does NOT open the palette', macAfterCtrlK.pal === false],
        ];
        const bad = checks.filter((c) => !c[1]).map((c) => c[0]);
        gates.push(gate('B10',
          'keymap behaviour matches the Action Registry (AR-29 item 7)',
          bad.length ? STATUS.FAIL : STATUS.PASS,
          'win: Ctrl+Shift+K palette=' + afterModK.pal + ', bare Ctrl+K palette=' + afterBareCtrlK.pal +
            ', Ctrl+Alt+L theme ' + winStart.theme + '->' + afterCtrlAltL.theme + ', Ctrl+Shift+L theme=' + afterCtrlShiftL.theme +
            ' | mac: Cmd+K palette=' + afterCmdK.pal + ', Ctrl+K palette=' + macAfterCtrlK.pal,
          bad.length ? bad.map((x) => 'FAILED: ' + x) : [
            'real CDP Input.dispatchKeyEvent (keyDown/keyUp + modifier bitmask), never a DOM attribute read',
            'truth source: #actionRegistry mac / win / linux fields; the page keeps no second Mod -> modifier mapping table',
            'win/linux Mod = Ctrl+Shift as a whole; Mod+Shift+X is reassigned to Ctrl+Alt+X (AR-29 item 7)',
          ]));
      } catch (err) {
        gates.push(gate('B10', 'keymap behaviour matches the Action Registry (AR-29 item 7)', STATUS.FAIL, 'browser error: ' + err.message));
      }
    }

    // ---- B4: visual regression under frozen animation (A17/G3/UX-G17; spec 07 methodology)
    if (want('B4')) {
      try {
        const rows = [];
        let hashFail = false;
        let diffFail = false;
        let missing = 0;
        if (updateBaseline) fs.mkdirSync(baselineDir, { recursive: true });
        for (const st of STATES) {
          await navigate(cdp, page, protoUrl, st);
          await capturePng(cdp, page.sessionId); // warm-up raster, discarded
          const first = await capturePng(cdp, page.sessionId);
          const second = await capturePng(cdp, page.sessionId);
          const h1 = sha256(first);
          const h2 = sha256(second);
          const frozen = h1 === h2;
          if (!frozen) hashFail = true;
          const baseRel = BASELINE_DIR_REL + '/' + tag + '-' + st.name + '.png';
          const basePath = path.join(root, baseRel);
          let line = st.name + ' hash=' + h1.slice(0, 12) + (frozen ? ' (frozen)' : ' != ' + h2.slice(0, 12) + ' (ANIMATION NOT FROZEN)');
          if (updateBaseline) {
            fs.writeFileSync(basePath, first);
            line += ' baseline written';
          } else if (skipBaseline) {
            line += ' baseline check skipped (--selftest)';
          } else if (!fs.existsSync(basePath)) {
            missing++;
            line += ' no baseline for ' + tag + ' (run: npm run design:baseline)';
          } else {
            const base = decodePng(fs.readFileSync(basePath));
            const cur = decodePng(first);
            const d = diffImages(base, cur, pixelTolerance);
            const within = !d.sizeMismatch && (maxDiffPct > 0 ? d.diffPct <= maxDiffPct : d.diffPixels === 0);
            if (!within) {
              diffFail = true;
              if (process.env.DESIGN_GATE_DEBUG) fs.writeFileSync(basePath + '.current.png', first);
            }
            line += ' diff=' + d.diffPixels + '/' + d.total + ' (' + d.diffPct.toFixed(4) + '%)' + (d.bbox ? ' bbox=' + JSON.stringify(d.bbox) : '') + (within ? ' OK' : ' MISMATCH');
          }
          rows.push(line);
        }
        let status;
        let detail;
        if (hashFail) {
          status = STATUS.FAIL;
          detail = 'same state rendered twice produced different hashes: animation is not frozen';
        } else if (diffFail) {
          status = STATUS.FAIL;
          detail = 'baseline pixel diff exceeds tolerance (maxDiffPct=' + maxDiffPct + ')';
        } else if (missing > 0 && !updateBaseline && !skipBaseline) {
          status = STATUS.SKIP;
          detail = 'animation-freeze self-check passed; ' + missing + '/' + STATES.length + ' states have no ' + tag + ' baseline yet';
        } else if (skipBaseline && !updateBaseline) {
          status = STATUS.PASS;
          detail = 'animation-freeze self-check passed for ' + STATES.length + ' states (baseline comparison skipped)';
        } else {
          status = STATUS.PASS;
          detail = STATES.length + ' states rendered twice with identical hashes and matched the baseline';
        }
        gates.push(gate('B4', 'visual regression under frozen animation vs baseline', status, detail,
          rows.concat([
            'method: --force-prefers-reduced-motion + Emulation.setEmulatedMedia(prefers-reduced-motion: reduce) + --m-*:0s',
            'tolerance: per-channel +/-' + pixelTolerance + ' (anti-aliasing) and ' + (maxDiffPct > 0 ? '<= ' + maxDiffPct + '% differing pixels' : '0 pixels beyond that') + '; override with --max-diff-pct=0.1',
            'baseline files: ' + BASELINE_DIR_REL + '/' + tag + '-*.png',
          ])));
      } catch (err) {
        gates.push(gate('B4', 'visual regression under frozen animation vs baseline', STATUS.FAIL, 'browser error: ' + err.message));
      }
    }
  } finally {
    try { cdp.close(); } catch (err) {}
    chrome.dispose();
  }
  // Keep a stable report order even when a filter is active.
  const order = ['B1', 'B2', 'B3', 'B4', 'B5', 'B6', 'B7', 'B8', 'B9', 'B10'];
  gates.sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id));
  return gates;
}
