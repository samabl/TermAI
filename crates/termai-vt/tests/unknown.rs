//! Test 3: unrecognised sequences are consumed, never echoed, and never change the grid.

use termai_vt::{ParseErrorKind, Terminal};

fn screen(term: &Terminal) -> String {
    let mut out = String::new();
    for row in 0..term.grid().rows() {
        out.push_str(&term.grid().row_text(row));
        out.push('\n');
    }
    out
}

#[test]
fn unknown_csi_is_fully_consumed() {
    let mut t = Terminal::new(40, 4);
    t.feed(b"\x1b[1;2;3;99zhello");
    assert_eq!(t.grid().row_text(0), "hello");
    assert_eq!(t.counters().get(ParseErrorKind::CsiUnknown), 1);
    let text = screen(&t);
    assert!(!text.contains("99z"));
    assert!(!text.contains("[1;2;3"));
}

#[test]
fn unknown_osc_is_ignored_and_not_echoed() {
    let mut t = Terminal::new(40, 4);
    t.feed(b"\x1b]999;SECRET-PAYLOAD\x07visible");
    assert_eq!(t.grid().row_text(0), "visible");
    assert_eq!(t.counters().get_key("osc_unknown[999]"), Some(1));
    let text = screen(&t);
    assert!(!text.contains("SECRET"));
    assert!(!text.contains("999"));
}

#[test]
fn unknown_dcs_is_ignored_and_not_echoed() {
    let mut t = Terminal::new(40, 4);
    t.feed(b"\x1bPzSECRET-DCS\x1b\\visible");
    assert_eq!(t.grid().row_text(0), "visible");
    assert_eq!(t.counters().get(ParseErrorKind::DcsUnknown), 1);
    let text = screen(&t);
    assert!(!text.contains("SECRET"));
    assert!(!text.contains("zSECRET"));
}

#[test]
fn unknown_apc_is_ignored_and_not_echoed() {
    let mut t = Terminal::new(40, 4);
    t.feed(b"\x1b_G SECRET-APC\x1b\\visible");
    assert_eq!(t.grid().row_text(0), "visible");
    let text = screen(&t);
    assert!(!text.contains("SECRET"));
}

#[test]
fn well_formed_unknown_has_first_error() {
    let mut t = Terminal::new(40, 4);
    let report = t.feed(b"\x1b[99z");
    let error = report
        .first_error
        .expect("unknown CSI should report a first error");
    assert_eq!(error.kind, ParseErrorKind::CsiUnknown);
    assert_eq!(error.final_byte, b'z');
    assert_eq!(error.offset, 0);
}
