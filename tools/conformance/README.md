# tools/conformance — G1 VT consistency runner (W1-B)

Authority: `docs/spec/kernel/01-vt-conformance.md` §3.1/§3.4/§3.7/§3.8/§3.9/§5,
HARNESS §2 **AR-25**, HARNESS §7 P0 exit, HARNESS §8.1-1, and the **AR-31 item 1** floor
(xterm cases >= 2000, >= 1 case per ctlseqs entry, real captures >= 20%).

**G1 is NOT judged here.** This runner measures what it runs and says so: every report
carries `g1_status: "NOT_JUDGED"` plus the reason.

## What this is, and what it is not

* It is an **L0 parser lane** runner (AR-25 item 1): byte-level cases are fed straight
  into `termai-vt` with no PTY, no renderer and no UI.
* It is **not** an xterm oracle. Most of the case set is *coverage scaffolding*: one
  generated case per ctlseqs entry that asserts structural invariants (listed below) and
  nothing about xterm behaviour. Only cases whose `oracle` is not `invariant` carry
  real, independently derived expectations, and only those count toward the L0 pass rate.
* It is **not** wired into `tools/kernel-gates/check.mjs`. The measured L0 rate is below
  100%, so wiring it in would only make the merge gate red without adding information.
  The intended CI hookup is described at the end of this file.

## Layout

    tools/conformance/
      run.mjs                     the runner (Node >= 22, no npm dependencies)
      gen-spec.mjs                materialises the curated suites from data/spec-cases.mjs
      verify-selftest.mjs         proves gen-spec.mjs --check reports a drift (the CI drift gate)
      check-suites.mjs            validates suites.json: static capability declarations must cite an ADR and kernel/01
      suites.json                 the declaration: pinned revisions, claimed VT level, static capability exclusions
      gen-ctlseqs.mjs             derives the xterm ctlseqs suite + entry registry
      lib/harness.mjs             wrapper around the Rust harness binary
      lib/ctlseqs.mjs             ctlseqs.txt parser + entry -> bytes resolver
      lib/report.mjs              deterministic JSON (sorted keys) + sha256
      data/spec-cases.mjs         curated case table (source of truth for two suites)
      data/ctlseqs-entries.json   derived ctlseqs entry registry (checked in)
      cases/<suite>/index.jsonl   case metadata, one JSON object per line
      cases/<suite>/<id>.trec     case body, a kernel/01 §3.8 replay script
      upstream/                   esctest/vttest integration + the raw evidence

The Rust side is `crates/termai-vt/src/bin/termai-vt-conformance.rs`: an auto-discovered
bin target (no Cargo.toml change, no new dependency) that calls only the public API —
`parse_trec`, `run`, `write_golden`, `Terminal`, `lane_verdict`. It never changes
VT semantics.

`vttest/` holds the V-01 build probe (`build-vttest.ps1` + its raw logs). This round
established that **no runnable vttest exists on this host**, and that the missing piece is
**not** a C compiler — MSVC `cl` 19.51 and w64devkit `gcc` 14.1 + `make` 4.4.1 both run —
but a POSIX tty header: mingw-w64 ships no `termios.h`, so upstream `vttest.h:57` hard-errors
with `#error please fix me`. See `vttest/README.md` for the four probes' raw output, the
verified minimal unblock (MSYS2 **msys** gcc+make, ~10–20 min), what V-01 still needs beyond
a compiler, and a correction owed to `docs/audit/debt-p0.md` row A3.

## Case format

One case = one `.trec` body plus one metadata line. Bodies use the implemented
`termai_vt::parse_trec` dialect (`KEY`, `FEED`, `PTY_OUT`, `RESIZE`, `ASSERT ROW`,
`ASSERT COUNTER`, `ASSERT TITLE`, `ASSERT CURSOR`) rather than the §3.8 spelling
`ASSERT_COUNTER key == value`; that divergence is registered under known gaps.

`cases/<suite>/index.jsonl` fields:

| field | required | meaning |
| --- | --- | --- |
| `id` | yes | `[a-z0-9][a-z0-9._-]*`, unique across all suites |
| `suite` | yes | must equal the directory name |
| `lane` | no (L0) | `L0` / `L1` / `L2`; only L0 may gate (AR-25 item 1, enforced fail-closed) |
| `oracle` | yes | `ecma48` / `xterm-ctlseqs` / `kernel-01-spec` / `termai-corpus` / `invariant` |
| `gating` | no (true) | counts toward the L0 rate; `oracle: invariant` may not gate |
| `cols` / `rows` | no (80/24) | initial grid |
| `path` | no | body path, defaults to `cases/<suite>/<id>.trec` |
| `requires` | no | static capability preconditions (K-01 `S_cap`); any non-empty list excludes the case from the gate denominator |
| `expect_responses` | no | expected terminal responses as lowercase hex, compared exactly |
| `ctlseqs_entry` | no | the ctlseqs pattern this case maps to (coverage accounting) |
| `documented` | recommended | where the expectation comes from (clause or ctlseqs line) |
| `real_corpus` | no | true only for genuine real-world captures (AR-31 item 1) |

Structural invariants applied to **every** case, by the runner:

1. `parity_ok` — the harness replay loop agrees with `termai_vt::run`.
2. `all_consumed` — `consumed == input` for every feed (kernel/01 §3.3 invariant 2).
3. `dims_ok` — the grid stayed at the declared size.
4. `cursor_in_bounds`.
5. `no_control_byte_in_grid` — no C0/C1 byte ever reached the grid (K-06 anti-echo;
   wide-char continuation cells, which are U+0000 placeholders, are excluded).

## Usage

    node tools/conformance/run.mjs --determinism-check     # run everything, write the report
    node tools/conformance/run.mjs --quiet                 # exit code only
    node tools/conformance/gen-spec.mjs [--check]          # regenerate / verify curated suites
    node tools/conformance/verify-selftest.mjs             # inject a drift; prove the check reports it
    node tools/conformance/check-suites.mjs [--selftest]   # validate the static suite declarations
    node tools/conformance/check-suites.mjs --file <p>     # validate another declaration file
    node tools/conformance/gen-ctlseqs.mjs --source <ctlseqs.txt> [--check]

`--source` is required for the ctlseqs generator: the document itself is **not** vendored
(third-party documentation), only the derived registry and the case bodies are checked in.
Regenerating needs `https://invisible-island.net/xterm/ctlseqs/ctlseqs.txt`
(sha256 `364c1c1987c85b1c1135e57a93e9008054f46d09a338de8dcc66ea9a7c613709`).

Defaults: report to `target/conformance/conformance-report.json`, failure repro
directories under `target/conformance/<suite>/<case>/` (both inside the gitignored
`target/` tree).

Exit code is 1 when any gating L0 case fails or any structural invariant fails. That is
deliberate: an unregistered deviation is a failure, not something to hide (AR-25 item 3,
K-01 D). `npm run conformance` is therefore **expected to be red** until the findings
below are fixed or registered.

## Three-lane closure (AR-25 item 1 / index B-2)

The runner asks `termai_vt::lane_verdict` itself (harness `--server LANE`) and records
the answers in `lane_policy.probe`:

| probe | verdict |
| --- | --- |
| L0 2/2, L0 2/3 | `pass`, `fail` |
| L1 2/2, L1 2/3 | `not_applicable`, `registered` |
| L2 2/2, L2 2/3 | `not_applicable`, `registered` |

L1/L2 case verdicts are rendered as `NOT_APPLICABLE` / `REGISTERED` and can never be
`PASS` / `FAIL`; `lane_policy.violations` must stay empty. Two fixtures
(`policy-fixture-l1-registered`, `policy-fixture-l2-not-applicable`) exercise that path
in real report data.

## Report schema

`conformance-report.json` follows kernel/01 §3.9: top level `schema`, `g1_status`,
`environment`, `commit`, `lane_policy`, `determinism`, `suites[]` (each with
`suite/suite_version/oracle/lane/backend/gpu_tier/machine_fingerprint/cases{x}/x_registered/r_strict/r_gate/gate/registry_digest/artifacts/commit`),
`totals`, `coverage`, `findings`, `known_gaps` and the per-case `cases[]`.

* `gate` is always `NON_GATING`: this is not an RM-A/T0 reference machine, so no
  compatibility verdict may be emitted (kernel/01 §3.9, spec 07 §3.8.3).
* `r_strict` and `r_gate` use the K-01 double-number rule over gating L0 cases after
  static capability exclusion.
* Determinism: object keys are sorted, arrays are ordered, no timestamps, no absolute
  paths. `--determinism-check` runs the harness twice and compares stdout bytes.

## Measured results in this workspace (Windows x64, vte-0.15.0)

| suite | total | executed | gating passed | coverage-only | excluded (cap) | R_strict |
| --- | --- | --- | --- | --- | --- | --- |
| `termai-corpus` | 8 | 8 | 8/8 | 0 | 0 | 1.000 |
| `termai-invariants` | 14 | 13 | 10/11 | 2 | 1 | 0.909 |
| `xterm-ctlseqs-spec` | 45 | 45 | 41/45 | 0 | 0 | 0.911 |
| `xterm-ctlseqs` (generated) | 208 | 208 | 0/0 | 208 | 0 | n/a |
| **total** | **275** | **274** | **59/64** | **210** | **1** | **0.921875** |

* ctlseqs coverage: **208/208** resolved entries have >= 1 case (100% of the entry
  registry); 26 entries have a case with real expectations (12.5%).
* real-world captures: **0/275 (0%)** — AR-31 item 1 requires >= 20%.
* harness determinism: `byte-identical` over two runs.

### Unregistered findings (5)

| case | measured | expected | root cause (code evidence) |
| --- | --- | --- | --- |
| `inv-esc-intermediate-has-no-side-effect` | cursor becomes (4,4) | cursor stays (0,0) | `Grid::esc_dispatch` takes `_intermediates` and matches only the final byte (`crates/termai-vt/src/grid.rs:845`), so `ESC # 8` (DECALN) executes `b'8' => restore_cursor()` (`:851`). kernel/01 §3.2 carries `intermediates` precisely to separate them; §3.4 requires an unrecognised sequence to change nothing. |
| `spec-decaln` | row 0 empty, `esc_unknown = 1` | 80 x `E` | DECALN (`ESC # 8`) is not implemented (same line as above). |
| `spec-hpa-basic` | cursor unchanged, `csi_unknown = 1` | (4,2) | HPA (`CSI Ps <backtick>`) is absent from the CSI dispatch table (`grid.rs:1474-1534`). |
| `spec-hpr-basic` | cursor unchanged, `csi_unknown = 1` | (4,6) | HPR (`CSI Ps a`) is absent from the CSI dispatch table. |
| `spec-rep` | one `a`, `csi_unknown = 1` | `aaaa` | REP (`CSI Ps b`) is absent from the CSI dispatch table. |

All five carry `registration: "NOT_REGISTERED"` in the report. kernel/01 K-04 requires a
double-signed deviation entry with an expiry before a difference may be treated as known;
that process has not been started, so these are failures, not exemptions.

## Progress toward AR-31 item 1

* Mechanism: **done**. Entry -> case mapping, coverage accounting, capability
  preconditions, lane closure and the deterministic report exist and run.
* Case count: **275 of the required >= 2000** (13.8%).
* ctlseqs 1:1: **208/208** entries have a case; only 26 have real expectations, so 182
  entries still need hand-derived cases.
* Real captures: **0%** of the required >= 20%. `real_corpus` exists so that adding them
  moves the number honestly.
* Smallest useful next increments:
  1. Add a gating case with real expectations for each of the 182 entries that lack one,
     extending the `data/spec-cases.mjs` table.
  2. Record real sessions (vim / htop / less / tmux) through the PTY recorder and add them
     as `real_corpus: true` cases to lift the real-capture ratio.
  3. Fix or register the five findings above.

## CI hookup (not enabled)

`tools/kernel-gates/check.mjs` is untouched. The intended wiring, once the L0 rate is
100% (or every miss is registered under K-04), is a PR-S3 job running
`npm run conformance:check`, treating a non-zero exit as merge-blocking and archiving the
report plus `target/conformance/<suite>/<case>/` repro directories on failure. Adding it
to `check.mjs` before then would only produce a red merge gate with no actionable path,
which is the "green because it cannot run" pattern K-01 D forbids.
