#!/usr/bin/env node
// tools/kernel-gates/check.mjs
// TermAI kernel gates (merge-blocking). Zero external dependencies, Node >= 18.
//
//   node tools/kernel-gates/check.mjs              # run K1-K7
//   node tools/kernel-gates/check.mjs --selftest   # prove the gates are not always-green
//   node tools/kernel-gates/check.mjs --json       # machine-readable JSON only
//
// Gate map (AGENTS.md section 4 / docs/spec/07 section 3.4 / ADR-0019):
//   K1 fmt          cargo fmt --all -- --check
//   K2 clippy       cargo clippy --workspace --all-targets -- -D warnings
//   K3 test         cargo test --workspace
//   K4 dep shape    parse crates/*/Cargo.toml + apps/*/Cargo.toml: ADR-0019 D1 allowed edges
//                   (session -> core,ipc; vt/pty/ipc -> core; tokens/core leaf),
//                   no library -> app edge, no upward termai-core edge, GPL/AGPL/SSPL denylist,
//                   refused dependencies (portable-pty, AR-28.3)
//   K5 license      every package.license == "Apache-2.0 OR MIT" (or license.workspace = true),
//                   and [workspace.package].license == that SPDX expression (AR-21)
//   K6 spec-defects docs/plan/m0-spec-defects.md exists and registers SD-01..SD-05
//   K7 codeowners   .github/CODEOWNERS names every tracked top-level directory and every
//                   rule names an owner; spec 07 section 3.1.2 requires 100% coverage and
//                   "no directory without an owner may merge"
//   K8 artifacts    .github/workflows/ci.yml publishes build artifacts (a build job must
//                   output artifacts, ADR-0021 D5), every upload pins retention-days,
//                   upload paths stay inside the D2 whitelist, and every "uses:" is on the
//                   ADR-0021 admission list
//
// A missing tool makes a gate SKIP with an explicit reason; it never silently passes.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(HERE, '..', '..');

const STATUS = { PASS: 'PASS', FAIL: 'FAIL', SKIP: 'SKIP' };
const LICENSE_EXPR = 'Apache-2.0 OR MIT';
const SPEC_DEFECTS = ['SD-01', 'SD-02', 'SD-03', 'SD-04', 'SD-05'];

// ADR-0019 D1: admitted library -> library edges. Anything else (including an edge into an
// app or an unregistered crate) is merge-blocking. Fail-closed by design.
const ALLOWED_LIB_EDGES = {
  'termai-tokens': [],
  'termai-core': [],
  'termai-ipc': ['termai-core'],
  'termai-vt': ['termai-core'],
  'termai-pty': ['termai-core'],
  'termai-session': ['termai-core', 'termai-ipc'],
  // ADR-0024 D1: the first render slice is pure logic on the core DTO; the HARNESS
  // section 4.3 edges to termai-vt / termai-gpu are registered with the shaping and GPU
  // slices, each with its own dependency-admission ADR.
  'termai-render': ['termai-core'],
};

// AR-21 / ADR-0015 D4: strong copyleft + field-of-use licenses are forbidden in the link
// boundary. K4 applies this to normal [dependencies] (in boundary per LB-01/LB-05); dev and
// build dependencies are out of boundary per ADR-0015 LB-02/LB-03 and are intentionally not
// matched by the name denylist.
const GPL_DENY_TOKENS = ['agpl', 'gpl', 'sspl'];

// ADR-0019 D2: dependencies that are explicitly refused even though they are not GPL.
const REFUSED_DEP_NAMES = ['portable-pty'];

// --------------------------------------------------- command helpers

function runCmd(cmd, args, cwd) {
  const r = spawnSync(cmd, args, {
    cwd: cwd,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    shell: false,
    windowsHide: true,
  });
  return {
    error: r.error || null,
    status: typeof r.status === 'number' ? r.status : null,
    signal: r.signal || null,
    stdout: r.stdout || '',
    stderr: r.stderr || '',
  };
}

function commandExists(cmd) {
  const r = runCmd(cmd, ['--version'], process.cwd());
  return !r.error && r.status === 0;
}

function tail(text, max) {
  const t = String(text == null ? '' : text).replace(/[\s]+$/, '');
  if (t.length <= max) return t;
  return '...' + t.slice(t.length - max);
}

function combineOutput(r) {
  return [r.stderr, r.stdout].filter(function (s) { return s && s.trim(); }).join('\n');
}

function gate(id, title, status, detail, notes) {
  return { id: id, title: title, status: status, detail: detail || '', notes: notes || [] };
}

// --------------------------------------------------- manifest parsing (minimal TOML)

function unquote(v) {
  const s = String(v == null ? '' : v).trim();
  if (s.length >= 2) {
    const q = s.charAt(0);
    if ((q === '"' || q === "'") && s.charAt(s.length - 1) === q) return s.slice(1, -1);
  }
  return s;
}

// Normal [dependencies] and target-scoped [target.'...'.dependencies] (in boundary).
function isDepSection(section) {
  if (!section) return false;
  return /(^|\.)dependencies$/.test(section);
}

function parseManifest(text) {
  const out = { pkg: {}, deps: [], allDeps: [], wsLicense: null };
  const lines = String(text).split(/\r?\n/);
  let section = null;
  for (let i = 0; i < lines.length; i++) {
    const t = lines[i].replace(/^\uFEFF/, '').trim();
    if (!t || t.charAt(0) === '#') continue;
    const sec = /^\[([^\]]+)\]/.exec(t);
    if (sec) { section = sec[1].trim(); continue; }
    const m = /^([A-Za-z0-9_.\-]+)\s*=\s*(.*)$/.exec(t);
    if (!m) continue;
    const key = unquote(m[1]);
    const val = m[2];
    if (section === 'package') {
      if (key === 'name') out.pkg.name = unquote(val);
      else if (key === 'license') out.pkg.license = unquote(val);
      else if (key === 'license.workspace') out.pkg.licenseWorkspace = /^true\b/.test(val.trim());
    } else if (section === 'workspace.package' && key === 'license') {
      out.wsLicense = unquote(val);
    }
    if (section && /dependencies$/.test(section)) out.allDeps.push({ name: key, section: section });
    if (isDepSection(section)) out.deps.push({ name: key, section: section });
  }
  return out;
}

function loadManifests(root) {
  const out = [];
  const kinds = [['crates', 'lib'], ['apps', 'app']];
  for (const kind of kinds) {
    const dirName = kind[0];
    const base = path.join(root, dirName);
    if (!fs.existsSync(base)) continue;
    const entries = fs.readdirSync(base).sort();
    for (const name of entries) {
      const mf = path.join(base, name, 'Cargo.toml');
      if (!fs.existsSync(mf)) continue;
      const parsed = parseManifest(fs.readFileSync(mf, 'utf8'));
      out.push({
        rel: dirName + '/' + name + '/Cargo.toml',
        kind: kind[1],
        name: parsed.pkg.name || name,
        pkg: parsed.pkg,
        deps: parsed.deps,
        allDeps: parsed.allDeps,
      });
    }
  }
  return out;
}

function isDeniedName(name) {
  const n = String(name).toLowerCase();
  for (const t of GPL_DENY_TOKENS) if (n.indexOf(t) >= 0) return true;
  return false;
}

function findCycles(edges) {
  const adj = {};
  for (const e of edges) {
    if (!adj[e[0]]) adj[e[0]] = [];
    adj[e[0]].push(e[1]);
  }
  const color = {};
  const stack = [];
  const cycles = [];
  function dfs(node) {
    color[node] = 1;
    stack.push(node);
    const nexts = adj[node] || [];
    for (const n of nexts) {
      if (color[n] === 1) {
        const idx = stack.indexOf(n);
        cycles.push(stack.slice(idx).concat([n]));
      } else if (!color[n]) {
        dfs(n);
      }
    }
    stack.pop();
    color[node] = 2;
  }
  for (const node of Object.keys(adj)) if (!color[node]) dfs(node);
  return cycles;
}

// --------------------------------------------------- gates

function cargoGate(id, title, args, ctx) {
  if (!ctx.cargo) {
    return gate(id, title, STATUS.SKIP, 'cargo not found on PATH; this gate cannot run here (explicit skip, not a pass)');
  }
  const r = runCmd('cargo', args, ctx.root);
  if (r.error) {
    if (r.error.code === 'ENOENT') {
      return gate(id, title, STATUS.SKIP, 'cargo not found on PATH; this gate cannot run here (explicit skip, not a pass)');
    }
    return gate(id, title, STATUS.SKIP, 'cargo could not start: ' + (r.error.message || String(r.error)));
  }
  const outText = combineOutput(r);
  if (r.status === 0) {
    return gate(id, title, STATUS.PASS, 'cargo ' + args[0] + ' clean', outText ? ['output tail: ' + tail(outText, 400)] : []);
  }
  return gate(id, title, STATUS.FAIL, 'cargo ' + args[0] + ' exited ' + r.status, outText ? ['output tail: ' + tail(outText, 2000)] : []);
}

function gateK4(ctx) {
  const TITLE = 'dep shape: crates/apps edges + GPL/AGPL/SSPL denylist (ADR-0019 D1/D2 / AR-21)';
  const manifests = loadManifests(ctx.root);
  const libs = manifests.filter(function (m) { return m.kind === 'lib'; });
  const apps = manifests.filter(function (m) { return m.kind === 'app'; });
  if (libs.length + apps.length === 0) {
    return gate('K4', TITLE, STATUS.FAIL, 'no crates/*/Cargo.toml or apps/*/Cargo.toml found under ' + ctx.root);
  }
  const libNames = {};
  libs.forEach(function (m) { libNames[m.name] = true; });
  const appNames = {};
  apps.forEach(function (m) { appNames[m.name] = true; });

  const failures = [];
  const edges = [];

  for (const m of manifests) {
    const termaiDeps = [];
    const appDeps = [];
    for (const d of m.deps) {
      if (appNames[d.name]) appDeps.push(d.name);
      else if (d.name.indexOf('termai-') === 0 || libNames[d.name]) termaiDeps.push(d.name);
      if (isDeniedName(d.name)) {
        failures.push(m.rel + ': dependency "' + d.name + '" matches the GPL/AGPL/SSPL denylist (AR-21)');
      }
      if (REFUSED_DEP_NAMES.indexOf(d.name) >= 0) {
        failures.push(m.rel + ': dependency "' + d.name + '" is refused by ADR-0019 D2 / AR-28 3rd decision');
      }
    }
    if (m.kind === 'lib') {
      for (const a of appDeps) {
        failures.push(m.rel + ': library crate "' + m.name + '" depends on app "' + a + '" (apps/* must never be depended upon)');
      }
      const allowed = ALLOWED_LIB_EDGES[m.name];
      if (allowed === undefined) {
        failures.push(m.rel + ': library crate "' + m.name + '" is not registered in the ADR-0019 D1 edge map (fail-closed)');
      } else {
        for (const d of termaiDeps) {
          if (allowed.indexOf(d) < 0) {
            failures.push(m.rel + ': edge ' + m.name + ' -> ' + d + ' is not an admitted downward edge (ADR-0019 D1)');
          }
        }
      }
      for (const d of termaiDeps) edges.push([m.name, d]);
    } else {
      for (const a of appDeps) {
        failures.push(m.rel + ': app "' + m.name + '" depends on app "' + a + '" (apps are leaf consumers)');
      }
      for (const d of termaiDeps) {
        if (!libNames[d]) failures.push(m.rel + ': app "' + m.name + '" depends on unknown library crate "' + d + '"');
        edges.push([m.name, d]);
      }
    }
  }

  for (const c of findCycles(edges)) {
    failures.push('dependency cycle detected: ' + c.join(' -> '));
  }

  if (failures.length) {
    return gate('K4', TITLE, STATUS.FAIL, failures.length + ' dependency-shape violation(s)',
      failures.slice(0, 20).concat(failures.length > 20 ? ['... ' + (failures.length - 20) + ' more'] : []));
  }
  return gate('K4', TITLE, STATUS.PASS,
    libs.length + ' library crate(s) + ' + apps.length + ' app(s); edges and denylist clean',
    ['allowed downward edges: ' + Object.keys(ALLOWED_LIB_EDGES).map(function (k) { return k + ' -> [' + ALLOWED_LIB_EDGES[k].join(', ') + ']'; }).join('; ')]);
}

function gateK5(ctx) {
  const TITLE = 'license: package.license + workspace SPDX expression (AR-21)';
  const failures = [];
  const notes = [];
  const rootManifestPath = path.join(ctx.root, 'Cargo.toml');
  if (!fs.existsSync(rootManifestPath)) {
    failures.push('Cargo.toml (workspace root) is missing');
  } else {
    const rootManifest = parseManifest(fs.readFileSync(rootManifestPath, 'utf8'));
    if (rootManifest.wsLicense !== LICENSE_EXPR) {
      failures.push('Cargo.toml: [workspace.package].license = ' + JSON.stringify(rootManifest.wsLicense) + ' but AR-21 requires exactly "' + LICENSE_EXPR + '"');
    } else {
      notes.push('[workspace.package].license = "' + LICENSE_EXPR + '"');
    }
  }
  const manifests = loadManifests(ctx.root);
  for (const m of manifests) {
    if (m.pkg.licenseWorkspace === true) continue;
    if (typeof m.pkg.license === 'string' && m.pkg.license.length > 0) {
      if (m.pkg.license !== LICENSE_EXPR) {
        failures.push(m.rel + ': package.license = ' + JSON.stringify(m.pkg.license) + ' (expected "' + LICENSE_EXPR + '")');
      }
    } else {
      failures.push(m.rel + ': package.license is missing and does not inherit [workspace.package].license');
    }
  }
  if (failures.length) {
    return gate('K5', TITLE, STATUS.FAIL, failures.length + ' license violation(s)', failures.concat(notes));
  }
  return gate('K5', TITLE, STATUS.PASS, manifests.length + ' package manifest(s) inherit or declare "' + LICENSE_EXPR + '"', notes);
}

function gateK6(ctx) {
  const TITLE = 'spec-defects registry: docs/plan/m0-spec-defects.md (AGENTS section 5)';
  const rel = 'docs/plan/m0-spec-defects.md';
  const abs = path.join(ctx.root, rel);
  if (!fs.existsSync(abs)) {
    return gate('K6', TITLE, STATUS.FAIL, rel + ' is missing');
  }
  const text = fs.readFileSync(abs, 'utf8');
  const missing = SPEC_DEFECTS.filter(function (id) { return text.indexOf(id) < 0; });
  if (missing.length) {
    return gate('K6', TITLE, STATUS.FAIL, rel + ' does not register: ' + missing.join(', '));
  }
  return gate('K6', TITLE, STATUS.PASS, rel + ' registers ' + SPEC_DEFECTS[0] + '..' + SPEC_DEFECTS[SPEC_DEFECTS.length - 1]);
}

// Top-level directories excluded from the ownership check: build output, VCS metadata
// and local runtime state. All of them are covered by .gitignore and are never tracked.
const NON_REPO_DIRS = ['.git', 'target', 'node_modules', 'dist', '.termai'];
const CODEOWNERS_REL = '.github/CODEOWNERS';

function codeownersRules(text) {
  return text
    .split(/\r?\n/)
    .map(function (l) { return l.trim(); })
    .filter(function (l) { return l.length > 0 && l.charAt(0) !== '#'; });
}

function topLevelDirs(root) {
  return fs.readdirSync(root, { withFileTypes: true })
    .filter(function (e) { return e.isDirectory() && NON_REPO_DIRS.indexOf(e.name) < 0; })
    .map(function (e) { return e.name; })
    .sort();
}

// spec 07 section 3.1.2: coverage must be 100% and no directory may merge without an
// owner. The blanket "*" rule prevents a directory from being *unowned*, but it gives it
// no team, so this gate additionally requires every tracked top-level directory to be
// named by a rule of its own.
function gateK7(ctx) {
  const TITLE = 'CODEOWNERS coverage: every tracked top-level directory is named by a rule (spec 07 section 3.1.2)';
  const abs = path.join(ctx.root, CODEOWNERS_REL);
  if (!fs.existsSync(abs)) {
    return gate('K7', TITLE, STATUS.FAIL, CODEOWNERS_REL + ' is missing');
  }
  const rules = codeownersRules(fs.readFileSync(abs, 'utf8'));
  if (!rules.length) {
    return gate('K7', TITLE, STATUS.FAIL, CODEOWNERS_REL + ' defines no rules');
  }
  const noOwner = rules.filter(function (l) { return l.split(/\s+/).length < 2; });
  if (noOwner.length) {
    return gate('K7', TITLE, STATUS.FAIL, noOwner.length + ' rule(s) name no owner', noOwner);
  }
  const dirs = topLevelDirs(ctx.root);
  const unowned = dirs.filter(function (d) {
    return !rules.some(function (l) {
      const pattern = l.split(/\s+/)[0];
      return pattern.indexOf('/' + d + '/') === 0 || pattern === '/' + d;
    });
  });
  if (unowned.length) {
    return gate(
      'K7',
      TITLE,
      STATUS.FAIL,
      unowned.length + ' top-level director(y/ies) have no explicit rule: ' + unowned.join(', '),
      unowned
    );
  }
  return gate('K7', TITLE, STATUS.PASS, dirs.length + ' top-level director(y/ies) named by explicit rules');
}

// ADR-0021 admits exactly these actions. Adding another is a supply-chain change and
// requires its own ADR, so this list is deliberately short and explicit.
const ADMITTED_ACTIONS = ['actions/checkout', 'actions/setup-node', 'actions/upload-artifact'];
// Paths that must never be published: build intermediates, runtime session logs, log
// files, and whole-repository globs. Artifacts of a public repository are downloadable by
// anyone, so the content whitelist is enforced here (ADR-0021 D2, AR-11, section 6.3).
const ARTIFACT_PATH_DENY = ['target/debug', '.termai', '*.log', 'target/**', 'path: .', '**/*'];

function gateK8(ctx) {
  const TITLE = 'build artifacts: a build job publishes artifacts with a retention policy (ADR-0021)';
  const rel = '.github/workflows/ci.yml';
  const abs = path.join(ctx.root, rel);
  if (!fs.existsSync(abs)) {
    return gate('K8', TITLE, STATUS.FAIL, rel + ' is missing');
  }
  const text = fs.readFileSync(abs, 'utf8');
  const lines = text.split(/\r?\n/);

  const used = [];
  const re = /uses:\s*([^\s#]+)/g;
  let m;
  while ((m = re.exec(text)) !== null) used.push(m[1]);
  // "uses:" values carry a version suffix (actions/checkout@v4); admission is by action
  // name, and version pinning is a separate policy (ADR-0021 D1, SHA pinning is a P1 item).
  const unadmitted = used.filter(function (u) { return ADMITTED_ACTIONS.indexOf(u.split('@')[0]) < 0; });
  if (unadmitted.length) {
    return gate('K8', TITLE, STATUS.FAIL, 'action(s) not admitted by ADR-0021: ' + unadmitted.join(', '), unadmitted);
  }

  const uploads = [];
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].indexOf('uses: actions/upload-artifact') >= 0) uploads.push(i);
  }
  if (!uploads.length) {
    return gate('K8', TITLE, STATUS.FAIL, 'no artifact upload step: a build job must publish its artifacts (ADR-0021 D5)');
  }

  const problems = [];
  for (const i of uploads) {
    const block = lines.slice(i, Math.min(i + 30, lines.length)).join('\n');
    if (block.indexOf('retention-days:') < 0) {
      problems.push('upload step at line ' + (i + 1) + ' does not set retention-days (ADR-0021 D3)');
    }
    if (block.indexOf('if-no-files-found: error') < 0) {
      problems.push('upload step at line ' + (i + 1) + ' does not set if-no-files-found: error (ADR-0021 D3)');
    }
    for (const d of ARTIFACT_PATH_DENY) {
      if (block.indexOf(d) >= 0) {
        problems.push('upload step at line ' + (i + 1) + ' publishes a denylisted path: ' + d + ' (ADR-0021 D2)');
      }
    }
  }
  if (problems.length) {
    return gate('K8', TITLE, STATUS.FAIL, problems.length + ' artifact policy violation(s)', problems);
  }
  return gate(
    'K8',
    TITLE,
    STATUS.PASS,
    uploads.length + ' upload step(s) with retention + whitelist, ' + used.length + ' action use(s) all admitted'
  );
}

// --------------------------------------------------- reporting

function summarize(gates) {
  const c = { PASS: 0, FAIL: 0, SKIP: 0 };
  for (const g of gates) c[g.status] = (c[g.status] || 0) + 1;
  return c;
}

function buildReport(ctx, gates) {
  const c = summarize(gates);
  const failed = gates.filter(function (g) { return g.status === STATUS.FAIL; });
  return {
    tool: 'kernel-gates',
    schema_version: 1,
    root: ctx.root,
    authority: 'AGENTS.md section 4 / docs/spec/07 section 3.4 / ADR-0019',
    result: failed.length === 0 ? 'PASS' : 'FAIL',
    counts: c,
    gates: gates,
  };
}

function formatHuman(report) {
  const lines = [];
  lines.push('=== kernel-gates (AGENTS.md section 4 / docs/spec/07 G1-G8 / ADR-0019) ===');
  for (const g of report.gates) {
    lines.push('[' + g.id + '] ' + g.status.padEnd(4) + ' ' + g.title + (g.detail ? ' - ' + g.detail : ''));
    for (const n of g.notes) lines.push('      ' + n);
  }
  lines.push('summary: ' + report.counts.PASS + ' PASS / ' + report.counts.FAIL + ' FAIL / ' + report.counts.SKIP + ' SKIP  (' + report.gates.length + ' gates)');
  lines.push('result: ' + report.result + (report.result === 'FAIL' ? ' (' + report.counts.FAIL + ' blocking)' : ''));
  if (report.counts.FAIL > 0) {
    lines.push('blocking failures:');
    for (const g of report.gates) {
      if (g.status === STATUS.FAIL) lines.push('  - [' + g.id + '] ' + g.title + ' - ' + g.detail);
    }
  }
  return lines.join('\n');
}

// --------------------------------------------------- selftest helpers

function makeSelftest() {
  const rows = [];
  let missed = 0;
  return {
    check: function (name, condition, detail) {
      const ok = !!condition;
      if (!ok) missed++;
      rows.push({ name: name, ok: ok, detail: detail || '' });
      return ok;
    },
    skip: function (name, detail) { rows.push({ name: name, ok: null, detail: detail || '' }); },
    report: function (title) {
      const lines = ['=== ' + title + ' ==='];
      for (const r of rows) {
        const tag = r.ok === null ? 'SKIP' : r.ok ? 'caught' : 'MISSED';
        lines.push('  ' + tag.padEnd(7) + ' ' + r.name + (r.detail ? '  (' + r.detail + ')' : ''));
      }
      const skipped = rows.filter(function (r) { return r.ok === null; }).length;
      const executed = rows.length - skipped;
      const caught = rows.filter(function (r) { return r.ok === true; }).length;
      lines.push('  injected faults caught: ' + caught + '/' + executed + (skipped ? ' (' + skipped + ' skipped)' : ''));
      const ok = missed === 0;
      lines.push('result: ' + (ok ? 'PASS - every executed injection was caught; the gates are not always-green' : 'FAIL - ' + missed + ' injection(s) were not caught'));
      return { text: lines.join('\n'), ok: ok, missed: missed, caught: caught, skipped: skipped, total: rows.length };
    },
  };
}

function tmpDir(prefix) { return fs.mkdtempSync(path.join(os.tmpdir(), prefix)); }

function makeMiniWorkspace(badFmt) {
  const dest = tmpDir('kernel-gates-fmt-');
  fs.mkdirSync(path.join(dest, 'm', 'src'), { recursive: true });
  fs.writeFileSync(path.join(dest, 'Cargo.toml'), '[workspace]\nresolver = "2"\nmembers = ["m"]\n');
  fs.writeFileSync(path.join(dest, 'm', 'Cargo.toml'), '[package]\nname = "kg-selftest"\nversion = "0.0.0"\nedition = "2021"\n');
  const src = badFmt ? 'pub fn f( )->u32{1}\n' : 'pub fn f() -> u32 {\n    1\n}\n';
  fs.writeFileSync(path.join(dest, 'm', 'src', 'lib.rs'), src);
  return dest;
}

function makeManifestRoot() {
  const dest = tmpDir('kernel-gates-manifests-');
  fs.copyFileSync(path.join(DEFAULT_ROOT, 'Cargo.toml'), path.join(dest, 'Cargo.toml'));
  const kinds = ['crates', 'apps'];
  for (const kind of kinds) {
    const base = path.join(DEFAULT_ROOT, kind);
    if (!fs.existsSync(base)) continue;
    for (const name of fs.readdirSync(base)) {
      const src = path.join(base, name, 'Cargo.toml');
      if (!fs.existsSync(src)) continue;
      const d = path.join(dest, kind, name);
      fs.mkdirSync(d, { recursive: true });
      fs.copyFileSync(src, path.join(d, 'Cargo.toml'));
    }
  }
  const sd = path.join(DEFAULT_ROOT, 'docs', 'plan', 'm0-spec-defects.md');
  fs.mkdirSync(path.join(dest, 'docs', 'plan'), { recursive: true });
  if (fs.existsSync(sd)) fs.copyFileSync(sd, path.join(dest, 'docs', 'plan', 'm0-spec-defects.md'));
  return dest;
}

function mutateFile(root, rel, fn) {
  const p = path.join(root, rel);
  const after = fn(fs.readFileSync(p, 'utf8'));
  fs.writeFileSync(p, after);
}

function runSelftest() {
  const st = makeSelftest();
  const temps = [];
  try {
    // injection 1: bad formatting in a temporary workspace (control also proves the gate can pass).
    if (commandExists('cargo')) {
      const clean = makeMiniWorkspace(false); temps.push(clean);
      const bad = makeMiniWorkspace(true); temps.push(bad);
      const cleanRun = runCmd('cargo', ['fmt', '--all', '--', '--check'], clean);
      const badRun = runCmd('cargo', ['fmt', '--all', '--', '--check'], bad);
      st.check('K1 control: well-formatted temp workspace passes', cleanRun.status === 0, 'cargo fmt exit ' + cleanRun.status);
      st.check('K1: badly formatted Rust in a temp workspace is caught', badRun.status !== 0, 'cargo fmt exit ' + badRun.status);
    } else {
      st.skip('K1: badly formatted Rust in a temp workspace', 'cargo not found on PATH; fmt injection cannot run here');
    }

    // injection 2: forbidden upward edge termai-core -> termai-vt.
    {
      const root = makeManifestRoot(); temps.push(root);
      mutateFile(root, 'crates/termai-core/Cargo.toml', function (t) {
        return t.replace('[dependencies]', '[dependencies]\ntermai-vt = { workspace = true }');
      });
      const g = gateK4({ root: root });
      st.check('K4: termai-core -> termai-vt (upward edge) is caught', g.status === STATUS.FAIL, g.detail);
    }

    // injection 3: forbidden library -> app reverse edge.
    {
      const root = makeManifestRoot(); temps.push(root);
      mutateFile(root, 'crates/termai-session/Cargo.toml', function (t) {
        return t.replace('[dependencies]', '[dependencies]\nsessiond = { path = "../../apps/sessiond" }');
      });
      const g = gateK4({ root: root });
      st.check('K4: library -> app reverse edge is caught', g.status === STATUS.FAIL, g.detail);
    }

    // injection 4: GPL/AGPL/SSPL denylisted dependency name.
    {
      const root = makeManifestRoot(); temps.push(root);
      mutateFile(root, 'crates/termai-vt/Cargo.toml', function (t) {
        return t.replace('[dependencies]', '[dependencies]\nsome-gpl-thing = "1"');
      });
      const g = gateK4({ root: root });
      st.check('K4: GPL-named dependency is caught', g.status === STATUS.FAIL, g.detail);
    }

    // injection 5: wrong crate license.
    {
      const root = makeManifestRoot(); temps.push(root);
      mutateFile(root, 'crates/termai-vt/Cargo.toml', function (t) {
        return t.replace('license.workspace = true', 'license = "GPL-3.0-only"');
      });
      const g = gateK5({ root: root });
      st.check('K5: wrong crate license (GPL-3.0-only) is caught', g.status === STATUS.FAIL, g.detail);
    }

    // injection 6: wrong workspace license expression.
    {
      const root = makeManifestRoot(); temps.push(root);
      mutateFile(root, 'Cargo.toml', function (t) {
        return t.replace(/license = "Apache-2\.0 OR MIT"/, 'license = "MIT"');
      });
      const g = gateK5({ root: root });
      st.check('K5: wrong [workspace.package].license is caught', g.status === STATUS.FAIL, g.detail);
    }

    // injection 7: dropped spec-defect registration.
    {
      const root = makeManifestRoot(); temps.push(root);
      mutateFile(root, 'docs/plan/m0-spec-defects.md', function (t) {
        return t.split('SD-05').join('SD-06');
      });
      const g = gateK6({ root: root });
      st.check('K6: dropped SD-05 registration is caught', g.status === STATUS.FAIL, g.detail);
    }

    // injection 8: CODEOWNERS that leaves a top-level directory without its own rule.
    {
      const root = tmpDir('kg-k7-unowned-');
      temps.push(root);
      fs.mkdirSync(path.join(root, '.github'), { recursive: true });
      fs.mkdirSync(path.join(root, 'crates'), { recursive: true });
      fs.mkdirSync(path.join(root, 'docs'), { recursive: true });
      fs.writeFileSync(
        path.join(root, '.github', 'CODEOWNERS'),
        '*  @samabl\ncrates/termai-core/  @termai/core-kernel\n'
      );
      const g = gateK7({ root: root });
      st.check('K7: a top-level directory with only the blanket rule is caught', g.status === STATUS.FAIL, g.detail);
    }

    // injection 9: CODEOWNERS rule that names no owner.
    {
      const root = tmpDir('kg-k7-noowner-');
      temps.push(root);
      fs.mkdirSync(path.join(root, '.github'), { recursive: true });
      fs.mkdirSync(path.join(root, 'crates'), { recursive: true });
      fs.writeFileSync(path.join(root, '.github', 'CODEOWNERS'), '/crates/\n');
      const g = gateK7({ root: root });
      st.check('K7: a rule that names no owner is caught', g.status === STATUS.FAIL, g.detail);
    }

    // injection 10: a workflow that never publishes artifacts.
    {
      const root = tmpDir('kg-k8-noupload-');
      temps.push(root);
      fs.mkdirSync(path.join(root, '.github', 'workflows'), { recursive: true });
      fs.writeFileSync(
        path.join(root, '.github', 'workflows', 'ci.yml'),
        'jobs:\n  b:\n    runs-on: windows-latest\n    steps:\n      - uses: actions/checkout@v4\n'
      );
      const g = gateK8({ root: root });
      st.check('K8: a build workflow that publishes nothing is caught', g.status === STATUS.FAIL, g.detail);
    }

    // injection 11: an action outside the ADR-0021 admission list.
    {
      const root = tmpDir('kg-k8-action-');
      temps.push(root);
      fs.mkdirSync(path.join(root, '.github', 'workflows'), { recursive: true });
      fs.writeFileSync(
        path.join(root, '.github', 'workflows', 'ci.yml'),
        'jobs:\n  b:\n    steps:\n      - uses: actions/checkout@v4\n' +
          '      - uses: actions/upload-artifact@v4\n        with:\n' +
          '          retention-days: 14\n          if-no-files-found: error\n' +
          '      - uses: some/third-party-action@v1\n'
      );
      const g = gateK8({ root: root });
      st.check('K8: an action outside the ADR-0021 admission list is caught', g.status === STATUS.FAIL, g.detail);
    }

    // injection 12: an upload that publishes build intermediates or runtime session logs.
    {
      const root = tmpDir('kg-k8-path-');
      temps.push(root);
      fs.mkdirSync(path.join(root, '.github', 'workflows'), { recursive: true });
      fs.writeFileSync(
        path.join(root, '.github', 'workflows', 'ci.yml'),
        'jobs:\n  b:\n    steps:\n      - uses: actions/checkout@v4\n' +
          '      - uses: actions/upload-artifact@v4\n        with:\n' +
          '          retention-days: 14\n          if-no-files-found: error\n' +
          '          path: |\n            target/debug/**\n'
      );
      const g = gateK8({ root: root });
      st.check('K8: publishing a denylisted path is caught', g.status === STATUS.FAIL, g.detail);
    }

    // baseline: the file-based gates must be green on the real tree.
    {
      const g4 = gateK4({ root: DEFAULT_ROOT });
      const g5 = gateK5({ root: DEFAULT_ROOT });
      const g6 = gateK6({ root: DEFAULT_ROOT });
      const g7 = gateK7({ root: DEFAULT_ROOT });
      const g8 = gateK8({ root: DEFAULT_ROOT });
      st.check('baseline K4 on the real tree passes', g4.status === STATUS.PASS, g4.detail);
      st.check('baseline K5 on the real tree passes', g5.status === STATUS.PASS, g5.detail);
      st.check('baseline K6 on the real tree passes', g6.status === STATUS.PASS, g6.detail);
      st.check('baseline K7 on the real tree passes', g7.status === STATUS.PASS, g7.detail);
      st.check('baseline K8 on the real tree passes', g8.status === STATUS.PASS, g8.detail);
    }
  } finally {
    for (const t of temps) {
      try { fs.rmSync(t, { recursive: true, force: true }); } catch (err) { /* best effort */ }
    }
  }
  const rep = st.report('kernel-gates --selftest (injections run on temporary copies only)');
  console.log(rep.text);
  console.log('note: the real workspace was not modified; temp copies lived under ' + os.tmpdir());
  process.exit(rep.ok ? 0 : 1);
}

// --------------------------------------------------- entry

function parseArgs(argv) {
  const out = { selftest: false, json: false, root: null };
  for (const a of argv) {
    if (a === '--selftest') out.selftest = true;
    else if (a === '--json') out.json = true;
    else if (a.indexOf('--root=') === 0) out.root = a.slice('--root='.length);
  }
  return out;
}

async function main(argv) {
  const args = parseArgs(argv);
  const root = args.root ? path.resolve(args.root) : DEFAULT_ROOT;
  const ctx = { root: root, cargo: commandExists('cargo') };
  const gates = [
    cargoGate('K1', 'cargo fmt --all -- --check', ['fmt', '--all', '--', '--check'], ctx),
    cargoGate('K2', 'cargo clippy --workspace --all-targets -- -D warnings', ['clippy', '--workspace', '--all-targets', '--', '-D', 'warnings'], ctx),
    cargoGate('K3', 'cargo test --workspace', ['test', '--workspace'], ctx),
    gateK4(ctx),
    gateK5(ctx),
    gateK6(ctx),
    gateK7(ctx),
    gateK8(ctx),
  ];
  const report = buildReport(ctx, gates);
  if (args.json) {
    process.stdout.write(JSON.stringify(report, null, 2) + '\n');
  } else {
    console.log(formatHuman(report));
    console.log('json: ' + JSON.stringify(report));
  }
  return report.result === 'PASS' ? 0 : 1;
}

const args = parseArgs(process.argv.slice(2));
if (args.selftest) {
  runSelftest();
} else {
  main(process.argv.slice(2)).then(function (code) { process.exit(code); }).catch(function (err) {
    console.error('kernel-gates ERROR: ' + (err && err.stack ? err.stack : err));
    process.exit(1);
  });
}
