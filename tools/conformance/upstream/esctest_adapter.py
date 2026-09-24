#!/usr/bin/env python3
# tools/conformance/upstream/esctest_adapter.py
#
# Runs the upstream esctest2 suite (kernel/01 section 5 V-02) against termai-vt's
# headless L0 parser lane, with no real terminal and no X11.
#
# Why an adapter is needed
# ------------------------
# esctest drives "the terminal" over stdin/stdout: it writes escape sequences and reads
# the terminal's responses. Its escio.py does that with a POSIX tty plus select(), so on
# Windows it cannot even be imported:
#
#     ModuleNotFoundError: No module named 'termios'      (escio.py line 4: import tty)
#
# This adapter replaces ONLY the transport (the escio module) with a shim that forwards
# every byte to "termai-vt-conformance --server" and returns the responses termai-vt
# actually produced. escutil / esccmd / tests are untouched, so the oracle (the upstream
# expectations) stays upstream's.
#
# Honest substitutions
# --------------------
# termai-vt's L0 grid now answers the character-cell XTWINOPS reports itself (grid.rs:
# 11 t / 13 t / 18 t / 19 t) and esctest's reset() needs one before any test can run. The
# adapter therefore does NOT re-answer those four: a second answer for one request would
# put TWO responses on the wire and desynchronise every later read.
# The pixel-class reports (14 t / 15 t / 16 t) stay synthesised from a fixed 80x24 window
# model, because the VT layer holds no font metrics (AR-14 keeps pixels out of this
# layer); each one is recorded as ADAPTER_SUBSTITUTION in substitutions.txt, so no reader
# can mistake it for a termai-vt capability.
# DECRQCRA is NOT synthesised: it is computed by the harness from the real grid
# (CHECKSUM command), i.e. the terminal answers from its own screen state.
#
# Usage
# -----
#   python tools/conformance/upstream/esctest_adapter.py --esctest <esctest2 checkout> \
#       [--harness target/debug/termai-vt-conformance] [--cols 80] [--rows 24] \
#       [--out target/conformance/upstream] -- <esctest args>
#
# Example:
#   ... -- --expected-terminal xterm --xterm-checksum 336 --include 'CUP_.*' --no-print-logs
import argparse
import os
import subprocess
import sys
import types

ESC = '\x1b'
BEL = '\x07'
CELL_W = 8
CELL_H = 16


class VtLink(object):
    """Line protocol client for termai-vt-conformance --server."""

    def __init__(self, bin_path, cols, rows):
        self.bin_path = bin_path
        self.cols = cols
        self.rows = rows
        self.proc = subprocess.Popen(
            [bin_path, '--server'],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self.queue = bytearray()
        self.substitutions = []
        self.feeds = 0

    def _command(self, line):
        self.proc.stdin.write((line + '\n').encode('ascii'))
        self.proc.stdin.flush()
        out = []
        while True:
            raw = self.proc.stdout.readline()
            if not raw:
                raise RuntimeError('harness closed the connection')
            text = raw.decode('utf-8', 'replace').rstrip('\r\n')
            if text == 'OK':
                return out
            if text == 'BYE':
                raise RuntimeError('harness said BYE')
            if text.startswith('ERR '):
                raise RuntimeError('harness error: ' + bytes.fromhex(text[4:]).decode('utf-8', 'replace'))
            if text.startswith('RESP '):
                out.append(bytes.fromhex(text[5:]))
            else:
                out.append(text)

    def feed(self, data):
        self.feeds += 1
        for response in self._command('FEED ' + data.hex()):
            self.queue.extend(response)
        return self._command('RESIZE %d %d' % (self.cols, self.rows))

    def checksum(self, top, left, bottom, right):
        out = self._command('CHECKSUM %d %d %d %d' % (top, left, bottom, right))
        return int(out[0].split(' ')[1])

    def take(self, n):
        if len(self.queue) < n:
            return None
        chunk = bytes(self.queue[:n])
        del self.queue[:n]
        return chunk

    def close(self):
        try:
            self.proc.stdin.write(b'QUIT\n')
            self.proc.stdin.flush()
        except Exception:
            pass
        try:
            self.proc.wait(timeout=5)
        except Exception:
            self.proc.kill()


def build_escio(link):
    mod = types.ModuleType('escio')
    mod.stdin_fd = None
    mod.stdout_fd = None
    mod.gSideChannel = None
    mod.use8BitControls = False
    mod.substitutions = link.substitutions

    def substitute(kind, detail):
        mod.substitutions.append(kind + ': ' + detail)

    def CmdChar(c):
        if mod.use8BitControls:
            return chr(c)
        return ESC + chr(c - 0x40)

    mod.CmdChar = CmdChar

    def Init():
        pass

    def Shutdown():
        pass

    def SetSideChannel(filename):
        mod.gSideChannel = filename

    def Write(s, sideChannelOk=True):
        link.feed(s.encode('latin-1'))

    def WriteRaw(data):
        link.feed(data)

    def WriteAPC(params, bel=False, requestsReport=False):
        str_params = list(map(str, params))
        terminator = BEL if bel else ST()
        Write(APC() + ''.join(str_params) + terminator, sideChannelOk=not requestsReport)

    def WriteOSC(params, bel=False, requestsReport=False):
        str_params = list(map(str, params))
        joined_params = ';'.join(str_params)
        terminator = BEL if bel else ST()
        sequence = OSC() + joined_params + terminator
        Write(sequence, sideChannelOk=not requestsReport)

    def WriteDCS(introducer, params):
        Write(DCS() + introducer + params + ST())

    def WriteCSI(prefix='', params=[], intermediate='', final='', requestsReport=False):
        if len(final) == 0:
            raise RuntimeError('final must not be empty')

        def stringify(p):
            return '' if p is None else str(p)

        str_params = list(map(stringify, params))
        while len(str_params) > 0 and str_params[-1] == '':
            str_params = str_params[:-1]
        joined_params = ';'.join(str_params)
        sequence = CmdChar(0x9b) + prefix + joined_params + intermediate + final
        Write(sequence, sideChannelOk=not requestsReport)
        if not requestsReport:
            return
        if final == 't' and intermediate == '' and prefix == '' and len(str_params) == 1:
            synthesize_winop(str_params[0])
        elif final == 'y' and intermediate == '*':
            synthesize_checksum(str_params)

    def synthesize_winop(code):
        # Only the pixel-class reports (14/15/16 t) are missing from termai-vt. The
        # character-cell reports (11/13/18/19 t) are answered by the terminal itself
        # (grid.rs window_op), so the adapter must stay silent for them: answering too
        # would put TWO responses on the wire for one request and desynchronise every
        # later read.
        try:
            code = int(code)
        except ValueError:
            return
        h = link.rows
        w = link.cols
        if code == 14:
            response = '4;%d;%d' % (h * CELL_H, w * CELL_W)
        elif code == 15:
            response = '5;%d;%d' % (h * CELL_H, w * CELL_W)
        elif code == 16:
            response = '6;%d;%d' % (CELL_H, CELL_W)
        else:
            return
        substitute('ADAPTER_SUBSTITUTION', 'CSI %d t -> CSI %s t (pixel-class window model: %dx%d cells x %dx%d px; termai-vt VT layer has no font metrics)' % (code, response, w, h, CELL_W, CELL_H))
        link.queue.extend((ESC + '[' + response + 't').encode('latin-1'))

    def synthesize_checksum(str_params):
        if len(str_params) < 5:
            return
        pid = str_params[0]
        tail = [p for p in str_params[-4:]]
        try:
            top, left, bottom, right = [int(p) for p in tail]
        except ValueError:
            return
        value = link.checksum(top, left, bottom, right)
        link.queue.extend((ESC + 'P' + pid + '!~%04X' % value + ESC + '\\').encode('latin-1'))

    def read(n):
        while True:
            chunk = link.take(n)
            if chunk is not None:
                return chunk.decode('latin-1')
            raise RuntimeError('Timeout waiting to read: termai-vt produced no response for the last request')

    def ReadOrDie(e):
        c = read(1)
        AssertCharsEqual(c, e)

    def AssertCharsEqual(c, e):
        if c != e:
            raise RuntimeError('Read %r, expected %r' % (c, e))

    def ReadOSC(expected_prefix):
        ReadOrDie(ESC)
        ReadOrDie(']')
        for c in expected_prefix:
            ReadOrDie(c)
        s = ''
        while not s.endswith(ST()):
            s += read(1)
        return s[:-2]

    def ReadCSI(expected_final, expected_prefix=None):
        c = read(1)
        if c == ESC:
            ReadOrDie('[')
        elif ord(c) != 0x9b:
            raise RuntimeError('Read %r, expected CSI' % c)
        params = []
        current_param = ''
        c = read(1)
        if not c.isdigit() and c != ';':
            if c == expected_prefix:
                c = read(1)
            else:
                raise RuntimeError('Unexpected character 0x%02x' % ord(c))
        while True:
            if c == ';':
                params.append(int(current_param))
                current_param = ''
            elif '0' <= c <= '9':
                current_param += c
            else:
                while True:
                    AssertCharsEqual(c, expected_final[0])
                    expected_final = expected_final[1:]
                    if len(expected_final) > 0:
                        c = read(1)
                    else:
                        break
                if current_param == '':
                    params.append(None)
                else:
                    params.append(int(current_param))
                break
            c = read(1)
        return params

    def ReadDCS():
        p = read(1)
        if p == ESC:
            ReadOrDie('P')
        elif ord(p) != 0x90:
            raise RuntimeError('Read %r, expected DCS' % p)
        result = ''
        while not result.endswith(ST()):
            result += read(1)
        if result.endswith(chr(0x9c)):
            return result[:-1]
        return result[:-2]

    def Is7BitControl(c):
        return 1 if len(c) == 2 and c.startswith(ESC) else 0

    def Is8BitControl(c):
        return 1 if len(c) == 1 and 0x80 <= ord(c) <= 0x9f else 0

    # C1 control helpers
    def IND():
        return CmdChar(0x84)

    def NEL():
        return CmdChar(0x85)

    def HTS():
        return CmdChar(0x88)

    def RI():
        return CmdChar(0x8d)

    def SS2():
        return CmdChar(0x8e)

    def SS3():
        return CmdChar(0x8f)

    def DCS():
        return CmdChar(0x90)

    def SPA():
        return CmdChar(0x96)

    def EPA():
        return CmdChar(0x97)

    def SOS():
        return CmdChar(0x98)

    def DECID():
        return CmdChar(0x9a)

    def CSI():
        return CmdChar(0x9b)

    def ST():
        return CmdChar(0x9c)

    def OSC():
        return CmdChar(0x9d)

    def PM():
        return CmdChar(0x9e)

    def APC():
        return CmdChar(0x9f)

    for name, value in list(locals().items()):
        if callable(value) or name in ('stdin_fd', 'stdout_fd', 'gSideChannel', 'use8BitControls', 'substitutions'):
            setattr(mod, name, value)
    return mod


def main():
    argv = sys.argv[1:]
    if '--' in argv:
        cut = argv.index('--')
        adapter_args, esctest_args = argv[:cut], argv[cut + 1:]
    else:
        adapter_args, esctest_args = argv, []

    ap = argparse.ArgumentParser()
    ap.add_argument('--esctest', required=True)
    ap.add_argument('--harness', default=None)
    ap.add_argument('--cols', type=int, default=80)
    ap.add_argument('--rows', type=int, default=24)
    ap.add_argument('--out', default=None)
    args = ap.parse_args(adapter_args)

    here = os.path.dirname(os.path.abspath(__file__))
    root = os.path.abspath(os.path.join(here, '..', '..', '..'))
    harness = args.harness or os.path.join(root, 'target', 'debug', 'termai-vt-conformance.exe')
    if not os.path.exists(harness):
        harness = os.path.join(root, 'target', 'debug', 'termai-vt-conformance')
    if not os.path.exists(harness):
        sys.stderr.write('adapter: harness binary not found; run cargo build -p termai-vt --bin termai-vt-conformance\n')
        return 2

    pkg_dir = os.path.join(os.path.abspath(args.esctest), 'esctest')
    if not os.path.isdir(pkg_dir):
        sys.stderr.write('adapter: %s is not an esctest checkout (no esctest/ package)\n' % args.esctest)
        return 2

    out_dir = args.out or os.path.join(root, 'target', 'conformance', 'upstream')
    os.makedirs(out_dir, exist_ok=True)
    logfile = os.path.join(out_dir, 'esctest.log')
    if '--logfile' not in esctest_args:
        esctest_args = esctest_args + ['--logfile', logfile]

    link = VtLink(harness, args.cols, args.rows)
    sys.path.insert(0, pkg_dir)
    sys.modules['escio'] = build_escio(link)

    sys.stderr.write('adapter: harness=%s\n' % harness)
    sys.stderr.write('adapter: esctest=%s\n' % pkg_dir)
    sys.stderr.write('adapter: transport = substituted escio (no tty, no select, no X11)\n')

    # escargs.parser.parse_args() reads sys.argv[1:], so hand it only the esctest args.
    sys.argv = ['esctest.py'] + esctest_args

    status = 0
    try:
        import runpy
        runpy.run_path(os.path.join(pkg_dir, 'esctest.py'), run_name='__main__')
    except SystemExit as exc:
        status = exc.code if isinstance(exc.code, int) else 1
    except Exception as exc:
        sys.stderr.write('adapter: esctest raised %s: %s\n' % (type(exc).__name__, exc))
        status = 1
    finally:
        sub_path = os.path.join(out_dir, 'substitutions.txt')
        with open(sub_path, 'w', encoding='utf-8') as fh:
            fh.write('\n'.join(link.substitutions) + ('\n' if link.substitutions else ''))
        sys.stderr.write('adapter: feeds=%d substitutions=%d (see %s)\n' % (link.feeds, len(link.substitutions), sub_path))
        link.close()
    return status


if __name__ == '__main__':
    sys.exit(main())
