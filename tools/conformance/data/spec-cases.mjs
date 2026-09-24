// tools/conformance/data/spec-cases.mjs
//
// Curated L0 cases with real, independently derived expectations (oracle: ECMA-48 /
// xterm ctlseqs / kernel/01 sections 3.3-3.4). This is the source of truth for the
// "termai-invariants" and "xterm-ctlseqs-spec" suites; tools/conformance/gen-spec.mjs
// materialises the .trec bodies and the index.jsonl files.
//
// Escaping note: these strings are written into .trec replay scripts, so any escape the
// .trec reader must interpret (backslash-x, backslash-r, backslash-n, ...) has to be
// doubled here. The helpers below avoid writing raw control bytes into the case files,
// which would be a different input than intended (0xFF as a raw byte is UTF-8 encoded
// to C3 BF, i.e. "y with diaeresis" instead of an invalid byte).
export const SUITES = {
  'termai-invariants': {
    suite_version: 'kernel/01 sections 3.3-3.4 (unrecognised-sequence and string-limit contract)',
    oracle: 'kernel-01-spec',
  },
  'xterm-ctlseqs-spec': {
    suite_version: 'xterm ctlseqs (patch #411) + ECMA-48, curated expectations',
    oracle: 'xterm-ctlseqs',
  },
};

const E = String.fromCharCode(27);
const CSI = E + '[';
const SL = String.fromCharCode(92);            // backslash, for .trec escapes
// .trec text for ESC backslash: the reader must see backslash-x-1-b then a doubled
// backslash, otherwise it reports "unknown escape".
const ST = SL + 'x1b' + SL + SL;
const CRLF = SL + 'r' + SL + 'n';              // .trec escape for CR LF
const TAB = SL + 't';                          // .trec escape for HT
const X = function (hex) { return SL + 'x' + hex; };
const BT = String.fromCharCode(96);            // backtick, for HPA

export const CASES = [
  // ---------------------------------------------------------------- cursor motion
  { id: 'spec-cup-basic', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps ; Ps H',
    documented: 'ECMA-48 8.3.21 / ctlseqs.txt:609 (CUP, 1-based row;column)', body: ['KEY "' + CSI + '2;3H"', 'ASSERT CURSOR 1 2'] },
  { id: 'spec-cup-default', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps ; Ps H',
    documented: 'ctlseqs.txt:609 (both parameters default to 1)', body: ['KEY "' + CSI + '5;5H' + CSI + 'H"', 'ASSERT CURSOR 0 0'] },
  { id: 'spec-cuu-basic', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps A',
    documented: 'ctlseqs.txt:592 (CUU); 5;5H puts the cursor on 0-based row 4',
    body: ['KEY "' + CSI + '5;5H' + CSI + '3A"', 'ASSERT CURSOR 1 4'] },
  { id: 'spec-cuu-clamp', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps A',
    documented: 'ctlseqs.txt:592 (CUU stops at the top margin)', body: ['KEY "' + CSI + '1;1H' + CSI + '9A"', 'ASSERT CURSOR 0 0'] },
  { id: 'spec-cud-basic', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps B',
    documented: 'ctlseqs.txt:597 (CUD)', body: ['KEY "' + CSI + '2B"', 'ASSERT CURSOR 2 0'] },
  { id: 'spec-cuf-basic', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps C',
    documented: 'ctlseqs.txt:599 (CUF)', body: ['KEY "' + CSI + '3C"', 'ASSERT CURSOR 0 3'] },
  { id: 'spec-cub-basic', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps D',
    documented: 'ctlseqs.txt:601 (CUB)', body: ['KEY "' + CSI + '5C' + CSI + '2D"', 'ASSERT CURSOR 0 3'] },
  { id: 'spec-cub-clamp', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps D',
    documented: 'ctlseqs.txt:601 (CUB stops at column 1)', body: ['KEY "' + CSI + '5C' + CSI + '9D"', 'ASSERT CURSOR 0 0'] },
  { id: 'spec-cnl-basic', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps E',
    documented: 'ctlseqs.txt:603 (CNL): 5;5H is 0-based row 4, CNL 2 moves to row 6, column 0',
    body: ['KEY "' + CSI + '5;5H' + CSI + '2E"', 'ASSERT CURSOR 6 0'] },
  { id: 'spec-cpl-basic', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps F',
    documented: 'ctlseqs.txt:605 (CPL): 0-based row 4 minus 2 is row 2, column 0',
    body: ['KEY "' + CSI + '5;5H' + CSI + '2F"', 'ASSERT CURSOR 2 0'] },
  { id: 'spec-cha-basic', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps G',
    documented: 'ctlseqs.txt:607 (CHA is a 1-based absolute column)', body: ['KEY "' + CSI + '5;5H' + CSI + '2G"', 'ASSERT CURSOR 4 1'] },
  { id: 'spec-vpa-basic', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps d',
    documented: 'ctlseqs.txt:841 (VPA is a 1-based absolute row)', body: ['KEY "' + CSI + '5;5H' + CSI + '3d"', 'ASSERT CURSOR 2 4'] },
  { id: 'spec-hpa-basic', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps ' + BT,
    documented: 'ctlseqs.txt:761 (HPA is a 1-based absolute column)',
    body: ['KEY "' + CSI + '5;5H' + CSI + '3' + BT + '"', 'ASSERT CURSOR 4 2'] },
  { id: 'spec-hpr-basic', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps a',
    documented: 'ctlseqs.txt:764 (HPR: forward 2 columns from column 4)', body: ['KEY "' + CSI + '5;5H' + CSI + '2a"', 'ASSERT CURSOR 4 6'] },
  { id: 'spec-cuf-clamp', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps C',
    documented: 'ctlseqs.txt:599 (CUF stops at the last column)', body: ['KEY "' + CSI + '100C"', 'ASSERT CURSOR 0 79'] },
  { id: 'spec-cud-clamp', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps B',
    documented: 'ctlseqs.txt:597 (CUD stops at the bottom margin)', body: ['KEY "' + CSI + '100B"', 'ASSERT CURSOR 23 0'] },

  // ------------------------------------------------------------ save / restore
  { id: 'spec-decsc-decrc', suite: 'xterm-ctlseqs-spec', entry: 'ESC 7',
    documented: 'ctlseqs.txt:399/401 (DECSC saves, DECRC restores)',
    body: ['KEY "' + CSI + '5;5H"', 'KEY "' + E + '7"', 'KEY "' + CSI + '1;1H"', 'KEY "' + E + '8"', 'ASSERT CURSOR 4 4'] },

  // --------------------------------------------------------------- editing
  { id: 'spec-ed2-clear', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps J',
    documented: 'ctlseqs.txt:614 (ED 2 clears the whole screen)', body: ['KEY "abc"', 'KEY "' + CSI + '2J"', 'ASSERT ROW 0 =', 'ASSERT CURSOR 0 3'] },
  { id: 'spec-el0', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps K',
    documented: 'ctlseqs.txt:627 (EL 0 erases from the cursor to the end)', body: ['KEY "hello"', 'KEY "' + CSI + '1;3H' + CSI + '0K"', 'ASSERT ROW 0 = he'] },
  { id: 'spec-el1', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps K',
    documented: 'ctlseqs.txt:627 (EL 1 erases from the start to the cursor, inclusive)', body: ['KEY "hello"', 'KEY "' + CSI + '1;3H' + CSI + '1K"', 'ASSERT ROW 0 =    lo'] },
  { id: 'spec-el2', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps K',
    documented: 'ctlseqs.txt:627 (EL 2 erases the whole line)', body: ['KEY "hello"', 'KEY "' + CSI + '2K"', 'ASSERT ROW 0 ='] },
  { id: 'spec-ech', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps X',
    documented: 'ctlseqs.txt:753 (ECH erases in place, no shift)', body: ['KEY "hello"', 'KEY "' + CSI + '1;2H' + CSI + '2X"', 'ASSERT ROW 0 = h  lo'] },
  { id: 'spec-ich', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps @',
    documented: 'ctlseqs.txt:587 (ICH inserts blanks and shifts right)', body: ['KEY "hello"', 'KEY "' + CSI + '1;2H' + CSI + '2@"', 'ASSERT ROW 0 = h  ello'] },
  { id: 'spec-dch', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps P',
    documented: 'ctlseqs.txt:642 (DCH deletes and shifts left)', body: ['KEY "hello"', 'KEY "' + CSI + '1;2H' + CSI + '2P"', 'ASSERT ROW 0 = hlo'] },
  { id: 'spec-il', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps L',
    documented: 'ctlseqs.txt:638 (IL inserts blank lines at the cursor row)',
    body: ['KEY "a' + CRLF + 'b"', 'KEY "' + CSI + '1;1H' + CSI + '1L"', 'ASSERT ROW 0 =', 'ASSERT ROW 1 = a', 'ASSERT ROW 2 = b'] },
  { id: 'spec-dl', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps M',
    documented: 'ctlseqs.txt:640 (DL deletes lines at the cursor row)',
    body: ['KEY "a' + CRLF + 'b"', 'KEY "' + CSI + '1;1H' + CSI + '1M"', 'ASSERT ROW 0 = b'] },
  { id: 'spec-su', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps S',
    documented: 'ctlseqs.txt:662 (SU scrolls the page up)',
    body: ['KEY "a' + CRLF + 'b"', 'KEY "' + CSI + '1;1H' + CSI + '1S"', 'ASSERT ROW 0 = b'] },
  { id: 'spec-sd', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps T',
    documented: 'ctlseqs.txt:726 (SD scrolls the page down)',
    body: ['KEY "a' + CRLF + 'b"', 'KEY "' + CSI + '1;1H' + CSI + '1T"', 'ASSERT ROW 1 = a'] },
  { id: 'spec-rep', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps b',
    documented: 'ctlseqs.txt:767 (REP repeats the preceding graphic character)', body: ['KEY "a"', 'KEY "' + CSI + '3b"', 'ASSERT ROW 0 = aaaa'] },

  // ------------------------------------------------------------------- C0 controls
  { id: 'spec-ht-tab', suite: 'xterm-ctlseqs-spec',
    documented: 'ECMA-48 8.3.20 / default tab stops every 8 columns',
    body: ['KEY "a' + TAB + 'b"', 'ASSERT ROW 0 = a       b'] },
  { id: 'spec-cr', suite: 'xterm-ctlseqs-spec', documented: 'ECMA-48 8.3.15 (CR)',
    body: ['KEY "abc' + SL + 'r"', 'ASSERT CURSOR 0 0'] },
  { id: 'spec-bs', suite: 'xterm-ctlseqs-spec', documented: 'ECMA-48 8.3.6 (BS)',
    body: ['KEY "abc' + X('08') + '"', 'ASSERT CURSOR 0 2'] },
  { id: 'spec-nel', suite: 'xterm-ctlseqs-spec', documented: 'ctlseqs.txt:166 / ECMA-48 8.3.49 (NEL)',
    body: ['KEY "' + CSI + '1;5H' + E + 'E"', 'ASSERT CURSOR 1 0'] },
  { id: 'spec-ind', suite: 'xterm-ctlseqs-spec', documented: 'ctlseqs.txt:163 / ECMA-48 8.3.35 (IND)',
    body: ['KEY "' + E + 'D"', 'ASSERT CURSOR 1 0'] },
  { id: 'spec-ri', suite: 'xterm-ctlseqs-spec', documented: 'ctlseqs.txt:172 / ECMA-48 8.3.69 (RI)',
    body: ['KEY "' + CSI + '2;1H' + E + 'M"', 'ASSERT CURSOR 0 0'] },
  { id: 'spec-decaln', suite: 'xterm-ctlseqs-spec', entry: 'ESC # 8',
    documented: 'ctlseqs.txt:310 (DECALN fills the screen with E)',
    body: ['KEY "' + E + '#8"', 'ASSERT ROW 0 = ' + 'E'.repeat(80), 'ASSERT ROW 23 = ' + 'E'.repeat(80)] },

  // ----------------------------------------------------------------- autowrap
  { id: 'spec-decawm-wrap-on', suite: 'xterm-ctlseqs-spec',
    documented: 'DECAWM (default set): printing past the last column wraps to the next row',
    body: ['KEY "' + CSI + '1;80H"', 'KEY "a"', 'KEY "b"', 'ASSERT CURSOR 1 1', 'ASSERT ROW 1 = b'] },
  { id: 'spec-decawm-off', suite: 'xterm-ctlseqs-spec',
    documented: 'DECAWM reset (CSI ?7l): the cursor stays on the last column',
    body: ['KEY "' + CSI + '?7l' + CSI + '1;80H"', 'KEY "a"', 'KEY "b"', 'ASSERT CURSOR 0 79'] },

  // ---------------------------------------------------------- OSC title + unknown
  { id: 'spec-osc0-title', suite: 'xterm-ctlseqs-spec', entry: 'OSC Ps ; Pt BEL',
    documented: 'ctlseqs.txt:2010 (OSC 0 sets icon name and window title)',
    body: ['KEY "' + E + ']0;hello' + X('07') + '"', 'ASSERT TITLE hello'] },
  { id: 'spec-osc2-title', suite: 'xterm-ctlseqs-spec', entry: 'OSC Ps ; Pt ST',
    documented: 'ctlseqs.txt:2012 (OSC 2 sets the window title)',
    body: ['KEY "' + E + ']2;world' + ST + '"', 'ASSERT TITLE world'] },
  { id: 'spec-csi-unknown-not-echoed', suite: 'xterm-ctlseqs-spec',
    documented: 'kernel/01 section 3.4: an unimplemented CSI is consumed and never echoed',
    body: ['KEY "' + CSI + '99z"', 'KEY "x"', 'ASSERT ROW 0 = x', 'ASSERT COUNTER csi_unknown = 1'] },
  { id: 'spec-osc-unknown-not-echoed', suite: 'xterm-ctlseqs-spec',
    documented: 'kernel/01 section 3.4: an unknown OSC is counted, not echoed',
    body: ['KEY "' + E + ']999;junk' + X('07') + 'y"', 'ASSERT ROW 0 = y', 'ASSERT COUNTER osc_unknown[999] = 1'] },
  { id: 'spec-dcs-unknown-not-echoed', suite: 'xterm-ctlseqs-spec',
    documented: 'kernel/01 section 3.4: an unknown DCS is dropped, never shown',
    body: ['KEY "' + E + 'Pzpayload' + ST + 'z"', 'ASSERT ROW 0 = z', 'ASSERT COUNTER dcs_unknown = 1'] },

  // ---------------------------------------------------------------- wide + DSR
  { id: 'spec-utf8-wide-cell', suite: 'xterm-ctlseqs-spec',
    documented: 'kernel/01 section 3.7 (wide continuation cell) / UAX #11',
    body: ['KEY "あ"', 'ASSERT ROW 0 = あ', 'ASSERT CURSOR 0 2'] },
  { id: 'spec-dsr-cursor-position', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps n',
    documented: 'ctlseqs.txt (DSR 6: cursor position report, 1-based)',
    expect_responses: ['1b5b323b3352'],
    body: ['KEY "' + CSI + '2;3H' + CSI + '6n"'] },

  // ---------------------------------------------------------- device attributes
  // ADR-0030 D-2. DA1 answers `CSI c` / `CSI 0 c` with VT100 + AVO (`?1;2`); DA2
  // answers `CSI > c` / `CSI > 0 c` with Pp=0, Pv=314, Pc=0. The 314 is TermAI's own
  // self-reported version (the lower bound of the range esctest accepts, 314..=999), NOT an
  // xterm version number. expect_responses asserts the exact emitted bytes.
  { id: 'spec-da1-primary-attributes', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps c',
    documented: 'ADR-0030 D-2: CSI c -> ESC [ ? 1 ; 2 c (VT100 + Advanced Video Option)',
    expect_responses: ['1b5b3f313b3263'],
    body: ['KEY "' + CSI + 'c"'] },
  { id: 'spec-da1-primary-attributes-ps0', suite: 'xterm-ctlseqs-spec', entry: 'CSI Ps c',
    documented: 'ADR-0030 D-2: CSI 0 c -> ESC [ ? 1 ; 2 c',
    expect_responses: ['1b5b3f313b3263'],
    body: ['KEY "' + CSI + '0c"'] },
  { id: 'spec-da2-secondary-attributes', suite: 'xterm-ctlseqs-spec', entry: 'CSI > Ps c',
    documented: 'ADR-0030 D-2: CSI > c -> ESC [ > 0 ; 314 ; 0 c (Pv is TermAI self-reported, not xterm)',
    expect_responses: ['1b5b3e303b3331343b3063'],
    body: ['KEY "' + CSI + '>c"'] },
  { id: 'spec-da2-secondary-attributes-ps0', suite: 'xterm-ctlseqs-spec', entry: 'CSI > Ps c',
    documented: 'ADR-0030 D-2: CSI > 0 c -> ESC [ > 0 ; 314 ; 0 c',
    expect_responses: ['1b5b3e303b3331343b3063'],
    body: ['KEY "' + CSI + '>0c"'] },

  // ------------------------------------------------- kernel/01 section 3.3 / 3.4
  { id: 'inv-osc-aborted', suite: 'termai-invariants',
    documented: 'kernel/01 section 3.3: ESC + non-ST aborts an OSC and is re-processed as ESC',
    body: ['KEY "' + E + ']0;abc' + CSI + '31mok"', 'ASSERT ROW 0 = ok', 'ASSERT COUNTER osc_aborted = 1', 'ASSERT TITLE ='] },
  { id: 'inv-osc-cancelled', suite: 'termai-invariants',
    documented: 'kernel/01 section 3.3: CAN/SUB aborts any string state and returns to Ground',
    body: ['KEY "' + E + ']0;abc' + X('18') + 'z"', 'ASSERT ROW 0 = z', 'ASSERT COUNTER string_cancelled = 1'] },
  { id: 'inv-dcs-cancelled', suite: 'termai-invariants',
    documented: 'kernel/01 section 3.3: CAN inside a DCS payload aborts it',
    body: ['KEY "' + E + 'Pabc' + X('18') + 'z"', 'ASSERT ROW 0 = z', 'ASSERT COUNTER string_cancelled = 1'] },
  { id: 'inv-st-stray', suite: 'termai-invariants',
    documented: 'kernel/01 section 3.3: a stray ST is ignored and counted',
    body: ['KEY "' + ST + 'q"', 'ASSERT ROW 0 = q', 'ASSERT COUNTER st_stray = 1'] },
  { id: 'inv-dcs-bel-in-data', suite: 'termai-invariants',
    documented: 'kernel/01 section 3.3: BEL inside DCS passthrough is data, not a terminator',
    body: ['KEY "' + E + 'Pab' + X('07') + 'cd' + ST + 'ok"', 'ASSERT ROW 0 = ok', 'ASSERT COUNTER dcs_bel_in_data = 1'] },
  { id: 'inv-c1-8bit-not-recognised-in-utf8', suite: 'termai-invariants',
    documented: 'OQ-VT-14: in UTF-8 mode an 8-bit C1 byte is counted, not acted on',
    body: ['KEY "' + X('9b') + 'm"', 'ASSERT ROW 0 = \uFFFDm', 'ASSERT COUNTER c1_8bit_in_utf8 = 1'] },
  { id: 'inv-invalid-utf8-replaced', suite: 'termai-invariants',
    documented: 'kernel/01 section 3.3: an invalid UTF-8 sequence becomes U+FFFD, not dropped',
    body: ['KEY "' + X('ff') + 'a"', 'ASSERT ROW 0 = \uFFFDa', 'ASSERT COUNTER invalid_utf8 = 1'] },
  { id: 'inv-unknown-osc-then-ground-continues', suite: 'termai-invariants',
    documented: 'kernel/01 section 3.3 invariant 3: ground bytes after an ignored sequence still print',
    body: ['KEY "' + E + ']999;x' + X('07') + 'ok"', 'ASSERT ROW 0 = ok', 'ASSERT COUNTER osc_unknown[999] = 1'] },
  { id: 'inv-unterminated-osc-then-esc', suite: 'termai-invariants',
    documented: 'kernel/01 section 3.3: an unterminated OSC must not swallow the rest of the stream',
    body: ['KEY "' + E + ']0;unterminated' + CSI + '1;1Hdone"', 'ASSERT ROW 0 = done', 'ASSERT COUNTER osc_aborted = 1'] },
  { id: 'inv-dcs-unterminated-then-ground', suite: 'termai-invariants',
    documented: 'kernel/01 section 3.3: an unterminated DCS returns to Ground on the next ESC',
    body: ['KEY "' + E + 'Punterminated' + CSI + '31mred"', 'ASSERT ROW 0 = red'] },

  { id: 'inv-esc-intermediate-has-no-side-effect', suite: 'termai-invariants',
    documented: 'kernel/01 section 3.4: an unrecognised sequence is consumed with zero grid/state change; '
      + 'kernel/01 section 3.2 exists to distinguish ESC # 8 (DECALN) from ESC 8 (DECRC)',
    // The sequence must be one that is NOT defined, otherwise it cannot express "an unrecognised
    // sequence has no side effect": ESC # 8 is DECALN (a defined sequence that fills the screen).
    // ESC # 9 is undefined. ADR-0029 does not touch this; the committed .trec already said #9 while
    // this generator still said #8, which is what tools/conformance/gen-spec.mjs --check caught.
    body: ['KEY "' + CSI + '5;5H"', 'KEY "' + E + '7"', 'KEY "' + CSI + '1;1H"', 'KEY "' + E + '#9"',
      'ASSERT CURSOR 0 0', 'ASSERT COUNTER esc_unknown = 1'] },

  // ------------------------------------------------------------- lane fixtures
  { id: 'policy-fixture-l2-not-applicable', suite: 'termai-invariants', lane: 'L2', gating: false,
    documented: 'AR-25 item 1 closure fixture: L2 must emit NOT_APPLICABLE, never PASS',
    body: ['KEY "hi"', 'ASSERT ROW 0 = hi'] },
  { id: 'policy-fixture-l1-registered', suite: 'termai-invariants', lane: 'L1', gating: false,
    documented: 'AR-25 item 1 closure fixture: an L1 difference is REGISTERED, never FAIL (assertion is intentionally unsatisfied)',
    body: ['KEY "hi"', 'ASSERT ROW 0 = deliberately-unsatisfied'] },
  { id: 'policy-fixture-capability-excluded', suite: 'termai-invariants', gating: true,
    requires: ['graphics.sixel'],
    documented: 'K-01: a static capability precondition excludes the case from the gate denominator',
    body: ['KEY "' + E + 'Pq"#0;2;0;0;0#1;2;100;0;0#0~~~' + ST + '"'] },
];
