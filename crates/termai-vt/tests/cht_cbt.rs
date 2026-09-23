//! ECMA-48 CHT (CSI Ps I) / CBT (CSI Ps Z) cursor tabulation.
//!
//! Assertions mirror esctest2 tests/cht.py and tests/cbt.py: one tab stop by
//! default, an explicit parameter, and the CBT left-edge clamp.

use termai_vt::{ParseErrorKind, Terminal};

#[test]
fn cht_one_tab_stop_by_default() {
    // esctest: CHT() -> cursor.x == 9 (1-based), i.e. 0-based col 8.
    let mut t = Terminal::new(80, 4);
    t.feed(b"\x1b[I");
    assert_eq!(t.grid().cursor(), (0, 8));
}

#[test]
fn cht_explicit_parameter() {
    // esctest: CHT(2) -> cursor.x == 17 (1-based), i.e. 0-based col 16.
    let mut t = Terminal::new(80, 4);
    t.feed(b"\x1b[2I");
    assert_eq!(t.grid().cursor(), (0, 16));
}

#[test]
fn cht_zero_parameter_behaves_as_one_and_moves() {
    // Negative control: CSI 0 I must equal CSI I and must actually move the
    // cursor, so the assertion cannot pass with a no-op implementation.
    let mut explicit = Terminal::new(80, 4);
    explicit.feed(b"\x1b[0I");
    let mut implicit = Terminal::new(80, 4);
    implicit.feed(b"\x1b[I");
    assert_eq!(explicit.grid().cursor(), (0, 8));
    assert_eq!(explicit.grid().cursor(), implicit.grid().cursor());
    assert_ne!(explicit.grid().cursor(), (0, 0));
    assert_eq!(explicit.counters().get(ParseErrorKind::CsiUnknown), 0);
}

#[test]
fn cht_stops_at_right_edge() {
    // Forward boundary guard (upstream's third CHT test is the margin case, not
    // this): CHT must clamp at the last column, never wrap or exceed the grid.
    let mut t = Terminal::new(80, 4);
    t.feed(b"\x1b[1;73H\x1b[5I");
    assert_eq!(t.grid().cursor(), (0, 79));
}

#[test]
fn cbt_one_tab_stop_by_default() {
    // esctest: CUP(17,1); CBT() -> cursor.x == 9 (1-based), i.e. 0-based col 8.
    let mut t = Terminal::new(80, 4);
    t.feed(b"\x1b[1;17H\x1b[Z");
    assert_eq!(t.grid().cursor(), (0, 8));
}

#[test]
fn cbt_explicit_parameter() {
    // esctest: CUP(25,1); CBT(2) -> cursor.x == 9 (1-based), i.e. 0-based col 8.
    let mut t = Terminal::new(80, 4);
    t.feed(b"\x1b[1;25H\x1b[2Z");
    assert_eq!(t.grid().cursor(), (0, 8));
}

#[test]
fn cbt_stops_at_left_edge() {
    // esctest: CUP(25,2); CBT(5) -> x == 1, y == 2 and must not wrap or underflow.
    let mut t = Terminal::new(80, 4);
    t.feed(b"\x1b[2;25H\x1b[5Z");
    assert_eq!(t.grid().cursor(), (1, 0));
}

#[test]
fn cbt_zero_parameter_behaves_as_one_and_moves() {
    // Negative control: CSI 0 Z must equal CSI Z and must actually move the
    // cursor, so the assertion cannot pass with a no-op implementation.
    let mut explicit = Terminal::new(80, 4);
    explicit.feed(b"\x1b[1;17H\x1b[0Z");
    let mut implicit = Terminal::new(80, 4);
    implicit.feed(b"\x1b[1;17H\x1b[Z");
    assert_eq!(explicit.grid().cursor(), (0, 8));
    assert_eq!(explicit.grid().cursor(), implicit.grid().cursor());
    assert_ne!(explicit.grid().cursor(), (0, 16));
    assert_eq!(explicit.counters().get(ParseErrorKind::CsiUnknown), 0);
}
