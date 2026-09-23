//! Test 4: invalid UTF-8 becomes U+FFFD and never swallows the rest of the segment.

use termai_vt::{ParseErrorKind, Terminal};

#[test]
fn invalid_byte_becomes_replacement() {
    let mut t = Terminal::new(40, 4);
    t.feed(b"a\xffb");
    assert_eq!(t.grid().row_text(0), "a\u{FFFD}b");
    assert_eq!(t.counters().get(ParseErrorKind::InvalidUtf8), 1);
}

#[test]
fn invalid_continuation_does_not_eat_the_rest() {
    let mut t = Terminal::new(40, 4);
    t.feed(b"ab\xe2\x28cd");
    assert_eq!(t.grid().row_text(0), "ab\u{FFFD}(cd");
}

#[test]
fn truncated_sequence_then_text_is_not_dropped() {
    let mut t = Terminal::new(40, 4);
    t.feed(b"\xc3\x28ok");
    assert_eq!(t.grid().row_text(0), "\u{FFFD}(ok");
}

#[test]
fn raw_c1_in_utf8_mode_is_replacement_not_control() {
    let mut t = Terminal::new(40, 4);
    t.feed(&[0x9b, b'x']);
    assert_eq!(t.grid().row_text(0), "\u{FFFD}x");
    assert_eq!(t.counters().get(ParseErrorKind::C1EightBitInUtf8), 1);
}

#[test]
fn replacement_survives_across_segments() {
    let mut t = Terminal::new(40, 4);
    t.feed(b"\xe2");
    t.feed(b"\x28tail");
    assert_eq!(t.grid().row_text(0), "\u{FFFD}(tail");
}
