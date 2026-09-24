# tools/conformance/vttest — V-01 build attempt (honest finding)

Authority: `docs/spec/kernel/01-vt-conformance.md` §3.9 + §5 row **V-01**,
`HARNESS.md` §7 P0 exit, `docs/audit/debt-p0.md` row **A3**.

> ## STATUS: **NO RUNNABLE VTTEST ON THIS HOST.**
> vttest is fetched and its `configure` completes, but **`make` fails at the first
> translation unit**. **No G1 progress is claimed** — "built and ran vttest" would mean
> nothing anyway, and here we did not even get that far.
> Debt row **A3's premise is stale** and must be corrected; its *conclusion* survives.
> See §7 for the exact correction.

This directory contains only the probe and its evidence. There is deliberately **no
`probe.mjs`**: step 4 of this round was gated on step 2 (a successful build) succeeding,
and it did not. Adding an unrunnable driver would be scaffolding with no evidence behind it.

---

## 1. Verdict in one paragraph

A C compiler is **not** the missing piece. This host has **two** working ones — MSVC
`cl.exe` 19.51.36248 (Visual Studio 18.7.2 Community) and the portable **w64devkit
gcc 14.1.0 + GNU Make 4.4.1 + sh** — plus Git-for-Windows' `sh`/`perl` (a shell, not a
compiler). What is missing is a
**POSIX tty header**: vttest's `vttest.h` hard-requires one of `termios.h` / `termio.h` /
`sgtty.h`, and **mingw-w64 ships none of the three**, so the build dies with

```
./vttest.h:57:4: error: #error please fix me
```

MSVC cannot help either — it has no `termios.h` and no `make`. The minimal, *verified*
unblock is an **MSYS2 `msys` toolchain** (not the `mingw64` one), which does ship
`/usr/include/termios.h`. That is a **system install**, ~10–20 min, not a source change.

---

## 2. Pinned inputs (exact)

| item | value |
| --- | --- |
| vttest source | `https://github.com/ThomasDickey/vttest-snapshots` |
| commit | `0229d7171a8574a2bf406c6ce14549f65d810e51` |
| snapshot label | `t20251205` (committer date 2025-12-06, `snapshot of project "vttest", label t20251205`) |
| tarball | `https://github.com/ThomasDickey/vttest-snapshots/archive/0229d7171a8574a2bf406c6ce14549f65d810e51.tar.gz` |
| tarball bytes / sha256 | `239867` / `42A9B6DBD0A2542F43667F6AA4CB76B687FAAB50D5B4F00AB1C659262C1270D9` |
| toolchain | w64devkit `1.23.0` (`https://github.com/skeeto/w64devkit/releases/download/v1.23.0/w64devkit-1.23.0.zip`) |
| toolchain bytes / sha256 | `76798128` / `5C7DCE6762BE3E0DBA648A9317790444C0E2F1EF3E677315C115727D7A549539` |
| host | Windows 10.0.26200 x64, cargo 1.94, node 26 |

`w64devkit` was chosen because it is a **single zip** providing `gcc` + `make` + `sh` —
exactly what a plain HTTPS download can deliver without MSYS2/WSL.

---

## 3. The four attempts, raw

Reproduce all of it with:

```powershell
pwsh -File tools\conformance\vttest\build-vttest.ps1
```

It writes the raw stdout/stderr of every attempt to
`$env:TEMP\termai-vttest\logs\`. The lines below are copied verbatim from those logs and
from the interactive session.

### 3.a MSVC `cl.exe` — compiler runs, header absent

`cl.exe` is **not on `PATH`**; it is reached via
`C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat`.
Compiling a three-line `termios.h` probe (`struct termios t; tcgetattr(0,&t);`) gives:

```
**********************************************************************
** Visual Studio 2026 Developer Command Prompt v18.7.2
** Copyright (c) 2026 Microsoft Corporation
**********************************************************************
[vcvarsall.bat] Environment initialized for: 'x64'
=== cl version ===
用法: cl [ 选项... ] 文件名... [ /link 链接选项... ]
Microsoft (R) C/C++ Optimizing Compiler Version 19.51.36248 for x64
版权所有(C) Microsoft Corporation。保留所有权利。

=== compile termios probe ===
Microsoft (R) C/C++ Optimizing Compiler Version 19.51.36248 for x64
版权所有(C) Microsoft Corporation。保留所有权利。

termios-probe.c
termios-probe.c(2): fatal error C1083: 无法打开包括文件: “termios.h”: No such file or directory
=== cl exit=2 ===
```

Raw log: `logs\probe-a-msvc.log` (verbatim, above).
**Result: FAIL** — no `termios.h`, and MSVC has no `make` for vttest's
autoconf-generated `makefile` either.

### 3.b Git for Windows — `sh` and `perl`, no build tools

```
Test-Path C:\Program Files\Git\bin\bash.exe -> True
(no make.exe / gcc.exe / cc.exe / g++.exe anywhere under C:\Program Files\Git)
present:
C:\Program Files\Git\bin\sh.exe C:\Program Files\Git\usr\bin\perl.exe C:\Program Files\Git\usr\bin\sh.exe
```

```
PS> where.exe make ; where.exe gcc ; where.exe cc ; where.exe perl
INFO: Could not find files for the given pattern(s).
INFO: Could not find files for the given pattern(s).
INFO: Could not find files for the given pattern(s).
INFO: Could not find files for the given pattern(s).
```

Raw log: `logs\probe-b-gitbash.log` (first block, verbatim; the `where.exe` block is the
interactive session that preceded the script).
**Result: FAIL** — Git for Windows embeds the MSYS2 *runtime*, not a toolchain.

### 3.c w64devkit — gcc/make/sh all work; the header is still missing

```
zip          : ...\w64devkit-1.23.0.zip
zip sha256   : 5C7DCE6762BE3E0DBA648A9317790444C0E2F1EF3E677315C115727D7A549539
gcc          : gcc.exe (GCC) 14.1.0
make         : GNU Make 4.4.1
termios.h under w64devkit: NONE
```

The toolchain **runs**. Two host quirks had to be worked around first, both inside the
script and only there:

1. **`configure: error: no acceptable cc found in $PATH`** — *this is the exact error debt
   A3 recorded.* It is **not** "no compiler installed". w64devkit's `sh` inherits the
   Windows `PATH` with `;` separators; autoconf splits `PATH` on `:`, so the whole variable
   is one bogus entry and every compiler probe returns `no`:

   ```
   checking for x86_64-w64-mingw32-gcc... no
   checking for gcc... no
   checking for x86_64-w64-mingw32-cc... no
   checking for cc... no
   checking for cc... no
   checking for x86_64-w64-mingw32-cl... no
   checking for cl... no
   configure: error: no acceptable cc found in $PATH
   ```

   `sh -c 'which gcc'` finds it, and `gcc --version` prints `gcc (GCC) 14.1.0`, in the very
   same shell — the toolchain is there, the search is broken. Exporting a colon-separated
   POSIX `PATH` **and** passing `CC=gcc` fixes it.

2. `config.status: cannot create a temporary directory in /tmp` — `/tmp` does not exist on
   this host; `TMPDIR` must be set to a real directory.

   A third quirk: `config.guess` cannot identify `Windows_NT` from busybox `uname`, so
   `--build=x86_64-w64-mingw32 --host=x86_64-w64-mingw32` must be given explicitly.
   Working inside a **space-free** path (`$env:TEMP\...`, not `E:\Code\TermAI 2`) is also
   required.

With all four handled, `configure` **succeeds** and produces `config.h` — and that is where
the real blocker becomes visible:

```
checking for ioctl.h... no
checking for sgtty.h... no
checking for sys/filio.h... no
checking for sys/ioctl.h... no
checking for termio.h... no
checking for termios.h... no
checking for alarm... yes
checking for rdchk... no
checking for tcgetattr... no
checking for usleep... yes
...
config.status: creating makefile
config.status: creating config.h
```

```c
/* config.h */
/* #undef HAVE_IOCTL_H */
/* #undef HAVE_SGTTY_H */
/* #undef HAVE_SYS_IOCTL_H */
/* #undef HAVE_TCGETATTR */
/* #undef HAVE_TERMIOS_H */
/* #undef HAVE_TERMIO_H */
```

`make` then fails immediately and **identically on every source file**:

```
gcc -c -DHAVE_CONFIG_H -I. -I.  -g -O2  charsets.c
In file included from charsets.c:6:
./vttest.h:57:4: error: #error please fix me
   57 | #  error please fix me
      |    ^~~~~
make: *** [makefile:53: charsets.o] Error 1

=== vttest.exe present: False ===
```

Raw log: `logs\probe-c-w64devkit.log`.
**Result: FAIL** — not for want of a compiler, but for want of a header.

### 3.d A termios-capable compiler — not attempted within budget

`build-vttest.ps1 -CC <path>` implements this probe and is ready to run; it was not run
because installing such a compiler is a **system change**, and the 25-minute feasibility
budget was spent on 3.a–3.c. See §5 for the evidence-backed target.

---

## 4. Why it fails, mechanistically

`vttest.h:49-58` (upstream, unmodified):

```c
#if defined(HAVE_TERMIOS_H) && defined(HAVE_TCGETATTR)
#  define USE_POSIX_TERMIOS 1
#elif defined(HAVE_TERMIO_H)
#  define USE_TERMIO 1
#elif defined(HAVE_SGTTY_H)
#  define USE_SGTTY 1
#  define USE_FIONREAD 1
#elif !defined(VMS)
#  error please fix me
#endif
```

There is no fourth branch. vttest has exactly **two** I/O back-ends: `unix_io.c` and
`vms_io.c`. Windows is not a supported target of the upstream snapshot — grepping the whole
tree for `WIN32|MINGW|_WIN32|MSDOS|WINNT` matches **only `config.guess`**. Line 57 is
upstream's deliberate "no usable tty interface found" tripwire, and it fires.

**Why we did not patch around it.** `ttymodes.c:23` declares `static TTY old_modes, new_modes;`
unconditionally, and every real tty operation in the file (`tcgetattr`, `cfgetospeed`,
`tcsetattr`, `NCCS`, `VMIN`, `ICANON`, …) is inside `#ifdef UNIX`, which `vttest.h:14-16`
defines as soon as `HAVE_CONFIG_H` is set. So a Windows build would require supplying a
**working** `termios.h` **and** a raw-mode implementation backed by the Win32 console API,
plus a replacement for the `/dev/tty` reopen at `ttymodes.c:211-215`. That is a port, not a
build. A stubbed termios would leave a binary that prints escape sequences but cannot be put
in raw mode, and **V-01's whole point is to drive the real upstream program** — a shimmed
vttest would be a false green exactly like the "green because it cannot run" pattern
`kernel/01` K-01 D forbids. If a port is ever wanted it belongs in a registered deviation
entry, not in a silent build script.

---

## 5. Minimal system change that unblocks it (verified, not assumed)

**Install MSYS2 and use its `msys` (Cygwin-derived) toolchain — `pacman -S gcc make` —
not `mingw-w64-x86_64-gcc`.**

Verified from the MSYS2 package file lists:

| package | repo | ships `termios.h`? |
| --- | --- | --- |
| `msys2-runtime-devel` | `msys` | **yes** — `/usr/include/termios.h`, `/usr/include/sys/termios.h`, `/usr/include/machine/termios.h` |
| `mingw-w64-x86_64-headers-git` | `mingw64` | **no** |

That second row is the trap worth recording: **installing MSYS2 and then building with its
`mingw64` gcc changes nothing** — the header comes from the `msys` runtime layer, so `make`
and `gcc` must be the `msys` ones (`/usr/bin/gcc`, not `/mingw64/bin/gcc`).

* Method: `msys2-base-x86_64-*.sfx.exe -y` then `pacman -Sy --noconfirm gcc make`
  (non-interactive, scriptable, no GUI).
* Then: `pwsh -File tools\conformance\vttest\build-vttest.ps1 -CC C:\msys64\usr\bin\gcc.exe`.
* **Estimate: ~10–20 minutes**, dominated by the MSYS2 download + install, not by vttest
  (measured download rate on this host: 76.8 MB in 42.6 s ≈ 1.8 MB/s; vttest's own
  configure+make is ≈ 20 s once the header exists).
* **Caveat:** an `msys` gcc links against `msys-2.0.dll`, so the produced `vttest.exe` only
  runs with MSYS2's `usr\bin` on `PATH`. Any future `probe.mjs` must spawn it accordingly.
* Alternatives: Cygwin (`setup-x86_64.exe -q -P gcc-core,make`, same ~15 min) or a **WSL2
  distro** (minutes to install, but a distro install + reboot is a heavier change than MSYS2).

**Not** required: a C compiler. Three already exist here; only a POSIX tty environment is
missing. Whatever is chosen, **the choice is a system/CI change owned by T5**, not something
this probe may decide unilaterally.

---

## 6. What V-01 still needs *after* a compiler appears

Even with a runnable `vttest.exe`, V-01 (`kernel/01` §5) is **not** satisfiable on this host.
`vttest` is an interactive, human-driven program; V-01 asks for
"vttest-driver 驱动菜单 → 转录 `.trec` 回放 → 网格比对 **100%**". The gap decomposes into
three independent missing pieces:

1. **A verdict needs an oracle, and there is none here.** Our L0 lane can only *record* what
   vttest emits. Turning that into a pass/fail needs the expected grids from a **pinned xterm
   under Xvfb** (`kernel/01` §3.9, K-03) — a Linux oracle host this Windows box is not.
   Without it a driver can produce a transcript and a digest, and no more.
2. **A non-interactive driver must answer queries, not just feed keystrokes.** vttest blocks
   on `inchar()`/`readnl()` (`unix_io.c:31,245`) and *expects replies* to DA / DECRQM / DSR
   before continuing. A fixed stdin script is not enough; the driver must be
   `expect`-style and bidirectional, per `kernel/01` §3.9.
3. **A concrete hazard for whoever writes that driver.** Reading `unix_io.c:260-266`,
   `readnl()` is `do { if (read(0,&b,1) < 0) break; else ch = b; } while (ch != '\n' && !brkrd);`
   — on a **pipe at EOF, `read()` returns `0`, not `-1`**, so `ch` becomes `0`, which is never
   `'\n'`, and the loop **spins forever**. `inputline()` (`unix_io.c:196-201`) has the same
   shape via `getchar()`. Piping a finite stdin script into vttest and walking away will
   therefore **hang, not exit**. The driver must keep stdin open and never let it hit EOF, or
   it must enforce its own timeout — the in-process guard is `alarm(60)` under
   `#ifdef HAVE_ALARM` (`unix_io.c:42-45`), and *whether mingw-w64's `alarm()` is a real timer
   or a no-op stub was **not** verified here, because nothing was built.* Treat the timeout as
   the driver's responsibility.

None of the three is a compiler problem, so **none of them is on this host's critical path
today** — but all three remain after the compiler problem is solved.

---

## 7. Correction to `docs/audit/debt-p0.md` row A3

Recorded here because the probe's most valuable output is a corrected *premise*; this round
may not edit `docs/**`.

| | A3 says | measured |
| --- | --- | --- |
| cause | "无 C 编译器" | **stale.** MSVC `cl` 19.51.36248, w64devkit `gcc` 14.1.0 + `make` 4.4.1, Git-for-Windows `sh` all present and running. |
| symptom | `configure` 报 `no acceptable cc found in $PATH` | **reproduced verbatim**, but its cause is a `;`-vs-`:` `PATH` bug in the autoconf/`sh` handoff, **not** a missing compiler. `CC=gcc` + a colon `PATH` makes `configure` succeed. |
| blocker | 需 RM-A/RM-B + §3.9 driver | the **immediate** blocker is narrower: no `termios.h`/`termio.h`/`sgtty.h` on mingw-w64 → `vttest.h:57: #error please fix me`. Minimal fix = MSYS2 `msys` gcc+make (§5, ~10–20 min). |
| conclusion | vttest 本机无法构建 | **still true** — and remains true for a second, independent reason (§6): no xterm+Xvfb oracle. |

Suggested A3 rewrite (for the owner of `docs/`):

> **A3** | **vttest 本机无法构建**（mingw-w64 无 `termios.h`；`vttest.h:57` `#error please fix me`） | T5 + 平台 | 编译器**不缺**（MSVC 19.51 / w64devkit gcc 14.1 均可用）；缺 POSIX tty 头。最小解法 = MSYS2 **msys** gcc+make（`packages.msys2.org` 证实 `msys2-runtime-devel` 含 `/usr/include/termios.h`，`mingw-w64-x86_64-headers-git` 不含），约 10–20 分钟；即便构建成功，V-01 仍缺「钉定 xterm + Xvfb oracle」与双向 expect 驱动器（见 `tools/conformance/vttest/README.md` §6） | §8.1-1、OQ-VT-12

---

## 8. Files

| path | role |
| --- | --- |
| `tools/conformance/vttest/README.md` | this file — the honest finding |
| `tools/conformance/vttest/build-vttest.ps1` | reproducible four-probe script; exits `1` with a verdict when no runnable vttest is produced |

**Nothing is vendored.** Fetched sources, the toolchain and all build output live under
`target/vttest-toolchain/`, `target/vttest-src/` (git-ignored via `.gitignore:16 target/`)
or `$env:TEMP\termai-vttest\`. No `crates/`, `apps/`, `docs/`, `.github/`, `Cargo.toml`,
`Cargo.lock` or `package.json` file was touched.

### Reproduce

```powershell
# full four-probe run; raw logs land in $env:TEMP\termai-vttest\logs\
pwsh -File tools\conformance\vttest\build-vttest.ps1
# expected: probes A/B/C report FAIL, "NO RUNNABLE VTTEST on this host.", exit 1

# after installing an MSYS2 msys toolchain (§5):
pwsh -File tools\conformance\vttest\build-vttest.ps1 -CC C:\msys64\usr\bin\gcc.exe
```

The script re-uses an already-downloaded
`target\vttest-toolchain\w64devkit-1.23.0.zip` when present and re-verifies its SHA-256 on
every run; a mismatch is reported, never silently accepted.
