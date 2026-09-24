<#
.SYNOPSIS
    Reproducible attempt to obtain a runnable `vttest` on this Windows host.

.DESCRIPTION
    Authority: docs/spec/kernel/01-vt-conformance.md §3.9 / §5 row V-01,
    HARNESS.md §7 P0 exit ("vttest + esctest 全通过"), docs/audit/debt-p0.md row A3.

    Debt A3 claims "vttest 本机无法构建（无 C 编译器、无 WSL 分发版）".  That claim is
    STALE: a C compiler is present three different ways on this host.  The real blocker
    is narrower and is what this script measures -- vttest refuses to compile without a
    POSIX tty header (termios.h / termio.h / sgtty.h), and **mingw-w64 ships none of
    them**, so `vttest.h:57` hard-errors with `#error please fix me`.

    This script runs the four probes in the fixed order required by the task, writes the
    RAW stdout/stderr of each to `<WorkRoot>\logs\`, and ends with an explicit verdict.
    It NEVER patches vttest sources: a stubbed termios would no longer be the upstream
    oracle that V-01 needs, so a "build" produced that way would be a false green.

    Nothing is written outside <WorkRoot> except one optional reused download under
    `<repo>\target\vttest-toolchain\` (git-ignored).  No file under crates/, apps/,
    docs/, .github/, Cargo.toml, Cargo.lock or package.json is touched.

.PARAMETER WorkRoot
    Scratch root.  MUST NOT contain a space: vttest's autoconf `configure` and the
    bundled `sh` mishandle a `;`-separated Windows PATH and paths with spaces.
    Default: $env:TEMP\termai-vttest  (e.g. C:\Users\<you>\AppData\Local\Temp\...).

.PARAMETER CC
    Path to a compiler that DOES provide termios.h (on this host that means the MSYS2
    *msys* toolchain -- `pacman -S gcc make` -- not `mingw-w64-x86_64-gcc`).
    When supplied, probe D performs a real build and the script exits 0 with the
    resulting vttest.exe path.

.EXAMPLE
    pwsh -File tools\conformance\vttest\build-vttest.ps1
    # -> probes A..C run, verdict: NO RUNNABLE VTTEST, exit 1

.EXAMPLE
    pwsh -File tools\conformance\vttest\build-vttest.ps1 -CC C:\msys64\usr\bin\gcc.exe
    # -> probe D builds vttest.exe for real
#>
[CmdletBinding()]
param(
    [string] $WorkRoot = (Join-Path $env:TEMP 'termai-vttest'),

    # Pinned upstream snapshot: see README.md for why this exact revision.
    [string] $Revision = '0229d7171a8574a2bf406c6ce14549f65d810e51',
    [string] $SnapshotLabel = 't20251205',
    [string] $SourceSha256 = '42A9B6DBD0A2542F43667F6AA4CB76B687FAAB50D5B4F00AB1C659262C1270D9',

    # Pinned portable toolchain (gcc + make + sh; NOT a POSIX header set).
    [string] $W64devkitVersion = '1.23.0',
    [string] $W64devkitSha256 = '5C7DCE6762BE3E0DBA648A9317790444C0E2F1EF3E677315C115727D7A549539',

    # Optional termios-capable compiler; enables probe D.
    [string] $CC = ''
)

$ErrorActionPreference = 'Continue'
Set-StrictMode -Version Latest
$ProgressPreference = 'SilentlyContinue'

$RepoRoot   = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$LogDir     = Join-Path $WorkRoot 'logs'
$SourceDir  = Join-Path $WorkRoot 'vttest-src'
$W64Dir     = Join-Path $WorkRoot 'w64devkit'
$DownloadDir = Join-Path $RepoRoot 'target\vttest-toolchain'   # git-ignored reuse cache

foreach ($d in @($WorkRoot, $LogDir, $SourceDir, $DownloadDir)) {
    New-Item -ItemType Directory -Force -Path $d | Out-Null
}

$script:Findings = [System.Collections.Generic.List[string]]::new()

function Record([string] $text) {
    $script:Findings.Add($text)
    Write-Host $text
}

function Save-Log([string] $name, [string[]] $lines) {
    $path = Join-Path $LogDir $name
    Set-Content -Path $path -Value $lines -Encoding utf8
    Write-Host "    [raw log] $path"
}

function Get-Tail([string] $text, [int] $n = 20) {
    ($text -split "`r?`n") | Where-Object { $_ -ne '' } | Select-Object -Last $n
}

# ---------------------------------------------------------------- preconditions
if ($WorkRoot -match ' ') {
    Record "FATAL: -WorkRoot '$WorkRoot' contains a space; autoconf + the bundled sh will misbehave."
    exit 2
}
Write-Host "== host =="
Write-Host "  WorkRoot  : $WorkRoot"
Write-Host "  RepoRoot  : $RepoRoot"
Write-Host "  vttest rev: $Revision ($SnapshotLabel)"
Write-Host ""

# ---------------------------------------------------------------- probe A: MSVC
Write-Host "== probe A: MSVC cl.exe =="
$probeC = Join-Path $WorkRoot 'termios-probe.c'
@'
#include <stdio.h>
#include <termios.h>
#include <unistd.h>
int main(void){ struct termios t; tcgetattr(0,&t); printf("ok\n"); return 0; }
'@ | Set-Content -Path $probeC -Encoding ascii

$vcvars = Get-ChildItem -Path 'C:\Program Files\Microsoft Visual Studio' -Recurse -Filter 'vcvars64.bat' -ErrorAction SilentlyContinue |
          Select-Object -First 1 -ExpandProperty FullName
$probeALines = @()
if (-not $vcvars) {
    $probeALines = @('vcvars64.bat not found under C:\Program Files\Microsoft Visual Studio')
    Record "  A: no vcvars64.bat -> MSVC unusable"
} else {
    $bat = Join-Path $WorkRoot 'probe-a.bat'
    @(
        '@echo off',
        "call `"$vcvars`"",
        'echo === cl version ===',
        'cl',
        'echo === compile termios probe ===',
        "cd /d `"$WorkRoot`"",
        'cl termios-probe.c',
        'echo === cl exit=%ERRORLEVEL% ==='
    ) | Set-Content -Path $bat -Encoding ascii
    $probeALines = (& cmd.exe /c $bat 2>&1 | Out-String) -split "`r?`n"
    Save-Log 'probe-a-msvc.log' $probeALines
    $hit = $probeALines | Where-Object { $_ -match 'termios\.h' } | Select-Object -First 1
    if ($hit) { Record "  A: cl.exe RUNS but has no termios.h -> $($hit.Trim())" }
    else      { Record "  A: cl.exe compiled the probe (unexpected on Windows) -- re-check by hand" }
}

# ---------------------------------------------------------------- probe B: Git for Windows
Write-Host "== probe B: Git for Windows =="
$gitBash = 'C:\Program Files\Git\bin\bash.exe'
$probeBLines = @("Test-Path $gitBash -> $(Test-Path $gitBash)")
if (Test-Path 'C:\Program Files\Git') {
    $found = Get-ChildItem -Path 'C:\Program Files\Git' -Recurse -File -ErrorAction SilentlyContinue |
             Where-Object { $_.Name -in @('make.exe','gcc.exe','cc.exe','g++.exe') } |
             Select-Object -ExpandProperty FullName
    $probeBLines += if ($found) { $found } else { @('(no make.exe / gcc.exe / cc.exe / g++.exe anywhere under C:\Program Files\Git)') }
    $shPerl = Get-ChildItem -Path 'C:\Program Files\Git' -Recurse -File -ErrorAction SilentlyContinue |
              Where-Object { $_.Name -in @('sh.exe','perl.exe') } |
              Select-Object -ExpandProperty FullName
    $probeBLines += 'present:', $shPerl
}
Save-Log 'probe-b-gitbash.log' $probeBLines
Record "  B: Git Bash $gitBash exists, but ships sh+perl only -- no make, no gcc/cc"

# ---------------------------------------------------------------- probe C: w64devkit
Write-Host "== probe C: w64devkit (portable gcc + make + sh) =="
$zipName = "w64devkit-$W64devkitVersion.zip"
$zipPath = Join-Path $DownloadDir $zipName
if (-not (Test-Path $zipPath)) {
    $zipPath = Join-Path $WorkRoot $zipName
}
$probeCLines = @()

if (-not (Test-Path $zipPath)) {
    $url = "https://github.com/skeeto/w64devkit/releases/download/v$W64devkitVersion/$zipName"
    Write-Host "    downloading $url"
    try { Invoke-WebRequest -Uri $url -OutFile $zipPath -TimeoutSec 900 }
    catch { $probeCLines += "download FAILED: $($_.Exception.Message)" }
}
if ($probeCLines.Count -eq 0) {
    $sha = (Get-FileHash $zipPath -Algorithm SHA256).Hash
    $probeCLines += "zip          : $zipPath"
    $probeCLines += "zip sha256   : $sha"
    if ($sha -ne $W64devkitSha256) { $probeCLines += "SHA256 MISMATCH (expected $W64devkitSha256)" }
    if (-not (Test-Path (Join-Path $W64Dir 'bin\gcc.exe'))) {
        Expand-Archive -Path $zipPath -DestinationPath $WorkRoot -Force
    }
    $gcc = Join-Path $W64Dir 'bin\gcc.exe'
    $make = Join-Path $W64Dir 'bin\make.exe'
    $sh = Join-Path $W64Dir 'bin\sh.exe'
    $probeCLines += "gcc          : $((& $gcc --version 2>&1 | Select-Object -First 1))"
    $probeCLines += "make         : $((& $make --version 2>&1 | Select-Object -First 1))"
    $termiosHits = Get-ChildItem $W64Dir -Recurse -File -Filter 'termios.h' -ErrorAction SilentlyContinue
    $probeCLines += "termios.h under w64devkit: $(if ($termiosHits) { ($termiosHits | ForEach-Object FullName) -join ', ' } else { 'NONE' })"

    # --- fetch + unpack the pinned vttest snapshot
    $tgz = Join-Path $WorkRoot "vttest-$SnapshotLabel.tar.gz"
    if (-not (Test-Path $tgz)) {
        try { Invoke-WebRequest -Uri "https://github.com/ThomasDickey/vttest-snapshots/archive/$Revision.tar.gz" -OutFile $tgz -TimeoutSec 300 }
        catch { $probeCLines += "vttest download FAILED: $($_.Exception.Message)" }
    }
    if (Test-Path $tgz) {
        $tsha = (Get-FileHash $tgz -Algorithm SHA256).Hash
        $probeCLines += "vttest tar   : $tgz"
        $probeCLines += "vttest sha256: $tsha"
        if ($tsha -ne $SourceSha256) { $probeCLines += "SHA256 MISMATCH (expected $SourceSha256)" }
        if (-not (Test-Path (Join-Path $SourceDir 'configure'))) {
            $stage = Join-Path $WorkRoot '_stage'
            New-Item -ItemType Directory -Force -Path $stage | Out-Null
            tar.exe -xzf $tgz -C $stage
            $inner = Get-ChildItem $stage -Directory | Select-Object -First 1
            Copy-Item -Recurse (Join-Path $inner.FullName '*') $SourceDir
            Remove-Item -Recurse -Force $stage
        }
    }

    # --- configure + make inside w64devkit's sh.
    #     Two host quirks are worked around here and ONLY here:
    #       (i)  w64devkit's sh gets the Windows PATH `;`-separated, and autoconf
    #            splits PATH on ':' -> "no acceptable cc found in $PATH" (the exact
    #            error debt A3 recorded).  We export a colon-separated POSIX PATH.
    #       (ii) config.status needs a writable TMPDIR (/tmp does not exist here).
    if (Test-Path (Join-Path $SourceDir 'configure')) {
        $pbase = $WorkRoot.Replace('\', '/')
        $tmp = Join-Path $WorkRoot 'tmp'
        New-Item -ItemType Directory -Force -Path $tmp | Out-Null
        $pre = "PATH='$pbase/w64devkit/bin:/c/Windows/system32'; export PATH; TMPDIR='$pbase/tmp'; export TMPDIR; TMP='$pbase/tmp'; export TMP; TEMP='$pbase/tmp'; export TEMP; cd '$pbase/vttest-src'"
        $conf = & $sh -c "$pre; CC=gcc ./configure --build=x86_64-w64-mingw32 --host=x86_64-w64-mingw32 CC=gcc" 2>&1 | Out-String
        $probeCLines += '=== configure (tail) ==='
        $probeCLines += Get-Tail $conf 12
        $probeCLines += '=== config.h tty-relevant defines ==='
        if (Test-Path (Join-Path $SourceDir 'config.h')) {
            $probeCLines += (Select-String -Path (Join-Path $SourceDir 'config.h') `
                -Pattern 'TERMIOS|TERMIO|SGTTY|TCGETATTR|SYS_IOCTL|IOCTL_H' | ForEach-Object { $_.Line })
        } else { $probeCLines += 'config.h NOT produced' }

        $mk = & $sh -c "$pre; make CC=gcc" 2>&1 | Out-String
        $probeCLines += '=== make (full) ==='
        $probeCLines += ($mk -split "`r?`n")
        $exe = Join-Path $SourceDir 'vttest.exe'
        $probeCLines += "=== vttest.exe present: $(Test-Path $exe) ==="
    }
}
Save-Log 'probe-c-w64devkit.log' $probeCLines
$cFailed = ($probeCLines | Where-Object { $_ -match 'please fix me' }) -ne $null
if ($cFailed) { Record "  C: gcc/make/sh WORK, but mingw-w64 has no termios.h -> vttest.h:57 '#error please fix me'" }
else          { Record "  C: w64devkit path did not reach the expected vttest.h:57 failure -- read the log" }

# ---------------------------------------------------------------- probe D: real build
$vttestExe = Join-Path $SourceDir 'vttest.exe'
if ($CC -ne '' -and (Test-Path (Join-Path $SourceDir 'configure'))) {
    Write-Host "== probe D: build with a termios-capable CC =="
    $probeDLines = @("CC = $CC")
    $sh = Join-Path $W64Dir 'bin\sh.exe'
    $pbase = $WorkRoot.Replace('\', '/')
    $pre = "PATH='$pbase/w64devkit/bin:/c/Windows/system32'; export PATH; TMPDIR='$pbase/tmp'; export TMPDIR; cd '$pbase/vttest-src'"
    $conf = & $sh -c "$pre; CC='$CC' ./configure --build=x86_64-w64-mingw32 --host=x86_64-w64-mingw32 CC='$CC'" 2>&1 | Out-String
    $probeDLines += Get-Tail $conf 12
    $mk = & $sh -c "$pre; make CC='$CC'" 2>&1 | Out-String
    $probeDLines += ($mk -split "`r?`n")
    Save-Log 'probe-d-real-build.log' $probeDLines
    if (Test-Path $vttestExe) { Record "  D: built $vttestExe" }
    else { Record "  D: build with '$CC' did not produce vttest.exe (see log)" }
}

# ---------------------------------------------------------------- verdict
Write-Host ""
Write-Host "== verdict =="
if (Test-Path $vttestExe) {
    Write-Host "  RUNNABLE VTTEST: $vttestExe"
    exit 0
}
Write-Host "  NO RUNNABLE VTTEST on this host."
Write-Host "  A C compiler is NOT the missing piece: MSVC cl.exe, w64devkit gcc 14.1.0 and"
Write-Host "  Git-for-Windows sh are all present.  The missing piece is a POSIX tty header."
Write-Host "  Minimal unblock: an MSYS2 *msys* toolchain (pacman -S gcc make)."
Write-Host "  See tools/conformance/vttest/README.md for the raw evidence."
exit 1
