# Upstream VT suites (vttest / esctest) — status and raw evidence

This file records **what was actually attempted, what actually ran, and what failed**,
with the raw errors. Nothing here is an aspiration. AR-20 / AGENTS §7 apply: anything not
run is written as not run.

## Summary

| suite | upstream | status on this host | evidence |
| --- | --- | --- | --- |
| esctest2 (test runner) | `ThomasDickey/esctest2` `2798f12149a19c3295e9b4853ab2da4b2eff1b2b` | **ran** through the adapter: full suite `201 passed / 41 known-bug / 325 failed` | `target/conformance/upstream-full/esctest.log` |
| esctest2 (cursor+editing subset) | same revision | **ran**: `63 passed / 0 known-bug / 24 failed` | `target/conformance/upstream/esctest.log` |
| esctest2 natively (no adapter) | same revision | **cannot start on Windows**: `ModuleNotFoundError: No module named 'termios'` | command 2 below |
| vttest | `invisible-island.net` release `vttest-20251205` | **cannot build**: `configure: error: no acceptable cc found in $PATH` | command 3 below |
| kitty suite | — | not attempted (not part of W1-B) | — |

G1 status is unchanged by any of this: **NOT JUDGED**. esctest is a terminal-level suite
and §3.9 requires the 100% gate to be decided on the L0 lane; even the parts that can run
here are not on an RM-A/T0 machine, so the report stays `NON_GATING`.

## 1. Environment facts used below

    Windows host, Node v26.7.0, Python 3.13.14 / 3.14.5, cargo 1.94.0, git 2.52.0
    where.exe cl / gcc / clang / cc / make / nmake  ->  (nothing found)
    wsl.exe -l -v  ->  exit 1, no distribution installed
                       (message is UTF-16LE: "适用于 Linux 的 Windows 子系统没有已安装的分发版"

                       + a https://aka.ms/wslinstall pointer)

## 2. esctest2 natively (failed to start)

    git clone --depth 1 https://github.com/ThomasDickey/esctest2.git
    python esctest/esctest.py --action=list-known-bugs

    Traceback (most recent call last):
      File ".../esctest2/esctest/esctest.py", line 10, in <module>
        import esccmd
      File ".../esctest2/esctest/esccmd.py", line 3, in <module>
        from escutil import AssertVTLevel
      File ".../esctest2/esctest/escutil.py", line 8, in <module>
        import escio
      File ".../esctest2/esctest/escio.py", line 4, in <module>
        import tty
      File "C:\...\Python313\Lib\tty.py", line 5, in <module>
        from termios import *
    ModuleNotFoundError: No module named 'termios'

esctest's only POSIX-only imports are `select` and `tty`, both in `escio.py` (verified
by grepping the whole tree). That is what makes the transport swap in section 4 possible.

## 3. vttest (failed to build)

    Invoke-WebRequest https://invisible-island.net/datafiles/release/vttest.tar.gz
      -> 243249 bytes, sha256
         cd6886f9aefe6a3f6c566fa61271a55710901a71849c630bf5376aa984bf77cc
    tar -xzf vttest.tar.gz          -> vttest-20251205/
    "C:\Program Files\Git\bin\sh.exe" ./configure
      checking build system type... x86_64-pc-mingw64
      checking host system type... x86_64-pc-mingw64
      Configuring for mingw64
      checking target system type... x86_64-pc-mingw64
      (cached) Configuring for mingw64
      checking for gcc... no
      checking for cc... no
      checking for cc... no
      checking for cl... no
      configure: error: no acceptable cc found in $PATH
      configure exit=1

    make  ->  'make' is not recognized as a cmdlet, function, script file, or operable program.

vttest is an autoconf C program and it also needs a real terminal to display its menus;
kernel/01 §3.9 expects it to be driven by a vttest-driver that transcribes the menu
sessions into `.trec` + golden. Neither the build toolchain nor the driver exists here.

## 4. esctest2 through the adapter (ran)

`tools/conformance/upstream/esctest_adapter.py` replaces **only the transport** — the
`escio` module — with a shim that forwards every byte to
`termai-vt-conformance --server` and returns the responses termai-vt actually produced.
`escutil`, `esccmd` and `tests/*` are untouched, so the oracle stays upstream's.
esctest cannot be imported on Windows at all (section 2), so there was no way to run it
against a headless backend without this swap.

Full suite:

    python tools/conformance/upstream/esctest_adapter.py \
      --esctest <esctest2 checkout> --out target/conformance/upstream-full \
      -- --expected-terminal xterm --xterm-checksum 336 --no-print-logs

    adapter: harness=E:\Code\TermAI 2\target\debug\termai-vt-conformance.exe
    adapter: transport = substituted escio (no tty, no select, no X11)
    adapter: feeds=34366 substitutions=687
    *** 201 tests passed, 41 known bugs, 325 TESTS FAILED ***

Cursor + editing subset (the part of the suite whose assertions are pure grid/cursor):

    ... -- --expected-terminal xterm --xterm-checksum 336 \
        --include "test_(CUP|CUU|CUD|CUF|CUB|CHA|VPA|CNL|CPL|ED|EL|ECH|ICH|DCH|REP|DECSC|DECRC)_" \
        --no-print-logs
    *** 63 tests passed, 0 known bugs, 24 TESTS FAILED ***

Largest failure clusters in the full run: `DECRQMTests` 32, `XtermWinopsTests` 19,
`DECSETTests` 16, `ChangeSpecialColorTests` 14, `ChangeColorTests` 13,
`ChangeDynamicColorTests` 13, `DECRQSSTests` 11, `DECDSRTests` 10. Those are
capabilities termai-vt does not have yet (mode reporting, window ops, colour model,
DECRQSS/DECDSR replies, device attributes), not transport artefacts — for a query the
terminal does not answer, the adapter reports "no response" and the test fails, which is
the same outcome a real terminal that stays silent would produce.

### Disclosed substitutions

`substitutions.txt` records every answer the adapter supplied instead of termai-vt:

    CSI 11 t -> CSI 1 t          (window model 80x24)
    CSI 13 t -> CSI 3;0;0 t      (window model 80x24)
    CSI 18 t -> CSI 8;24;80 t    (window model 80x24)
    CSI 19 t -> CSI 9;24;80 t    (window model 80x24)

687 of them, all window/char size reports. `Grid` deliberately does not implement
`CSI t` (`crates/termai-vt/src/grid.rs`: `b't' => {}`), and esctest's `reset()` needs
one before any test can run. Everything else esctest asks for is answered by the real
backend, including DECRQCRA, which the harness computes from the live grid
(`CHECKSUM` command) rather than inventing screen content.
**Read the pass counts with that substitution in mind**: a window-size answer was not
produced by termai-vt.

## 5. What the next increment must be

1. **vttest** needs a machine with a C toolchain and a real terminal, plus the
   `vttest-driver` from kernel/01 §3.9 (expect-style menu drive -> `.trec` + golden).
   Nothing about it can be done on this host; the honest move is an RM-A/RM-B runner.
2. **esctest natively** needs a POSIX host (tty + `select` on a pty). The adapter here is
   the cross-platform fallback; on Linux the upstream `escio` should be used unmodified so
   the transport is not part of the measurement at all.
3. **Close the gap that the run exposed**, in rough size order: device attributes / DSR
   variants, DECRQM/DECRQM-mode reporting, DECRQSS, the colour model (OSC 4/10/11),
   window ops, and left/right margins + origin mode (DECSLRM/DECOM), which alone accounts
   for most of the "RespectsOriginMode"/"StopsAt*Margin" failures.
4. **Register what will not be fixed**: any difference that survives must go through the
   K-04 deviation process (double sign-off + expiry) before it stops being a failure.

## Static capability declarations (ADR-0029 D-2 / ADR-0030)

`tools/conformance/suites.json` is the machine-checked declaration of how each upstream suite is
run and which capabilities are declared absent. `node tools/conformance/check-suites.mjs`
validates it, and `--selftest` proves the checks can fail; a declaration may only reduce a suite's
eligible set as a **static capability precondition** (kernel/01 K-01 `S_cap`), never as a runtime
skip and never as a difference entry.

- **esctest2**, pinned at `2798f12149a19c3295e9b4853ab2da4b2eff1b2b`, runs with
  `--expected-terminal xterm --xterm-checksum 336` and **`--max-vt-level 1`** (ADR-0030 D-1: the
  claimed level is the highest whose xterm DA1 expected set is entirely implemented; level 5 would
  require claiming selective erase, locator, colour and rectangular editing, which this VT does not
  implement).
- **`color-query`** (OSC 4 / 10 / 11 / 12) is declared **statically absent**: 47 located failing
  cases (ChangeColorTests 13, ChangeDynamicColorTests 13, ChangeSpecialColorTests 14, ResetColorTests
  2, ResetSpecialColorTests 5) are excluded by that precondition. AR-20 forbids answering with a
  palette before colour rendering exists, so "not answered" is the honest capability state.
- **Quote the level with the number** (ADR-0030 D-1): level 1 is **103 passed / 378 known-bug /
  86 failed** of 189 raw eligible after the device-attribute work (**99 / 378 / 90** before it), and
  level 5 is 267 / 41 / **259** of 526. Applying the `color-query` exclusion moves 47 cases out of
  the judging set, so **gate-eligible = 189 - 47 = 142 and failed_real = 39**; do that with
  `node tools/conformance/esctest-report.mjs --log <log>`, never by hand. Known bugs that are
  really "not run because the level is too low" are reported as `excluded_by_vt_level: unknown`,
  because the log carries no per-case level attribution and inventing a split would be a lie.
