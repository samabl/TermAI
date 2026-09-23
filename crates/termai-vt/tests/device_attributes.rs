//! Device-attribute replies: ADR-0030 D-2 (DA1 / DA2) and their counter parity.
//!
//! The exact response bytes are the contract:
//!   DA1 (CSI c, CSI 0 c)   -> ESC [ ? 1 ; 2 c
//!   DA2 (CSI > c, CSI > 0 c) -> ESC [ > 0 ; 3 1 4 ; 0 c
//! The "314" is TermAI's own self-reported version (esctest accepts 314..=999); it is
//! never an xterm version number.

use termai_vt::{ParseErrorKind, Terminal};

/// ESC [ ? 1 ; 2 c
const DA1_RESPONSE: &[u8] = b"\x1b[?1;2c";
/// ESC [ > 0 ; 314 ; 0 c
const DA2_RESPONSE: &[u8] = b"\x1b[>0;314;0c";

fn responses(input: &[u8]) -> Vec<Vec<u8>> {
    let mut term = Terminal::new(10, 3);
    term.feed(input);
    term.take_responses()
}

#[test]
fn da1_answers_csi_c_and_csi_0_c_with_the_level_one_identity() {
    assert_eq!(responses(b"\x1b[c"), vec![DA1_RESPONSE.to_vec()]);
    assert_eq!(responses(b"\x1b[0c"), vec![DA1_RESPONSE.to_vec()]);
}

#[test]
fn da2_answers_csi_gt_c_and_csi_gt_0_c_with_the_self_reported_version() {
    assert_eq!(responses(b"\x1b[>c"), vec![DA2_RESPONSE.to_vec()]);
    assert_eq!(responses(b"\x1b[>0c"), vec![DA2_RESPONSE.to_vec()]);
}

#[test]
fn device_attributes_are_not_counted_as_unknown_csi() {
    let mut term = Terminal::new(10, 3);
    term.feed(b"\x1b[c\x1b[0c\x1b[>c\x1b[>0c");
    assert_eq!(term.counters().get(ParseErrorKind::CsiUnknown), 0);
    assert_eq!(term.take_responses().len(), 4);
}

#[test]
fn unhandled_csi_still_bumps_the_unknown_counter() {
    let mut term = Terminal::new(10, 3);
    term.feed(b"\x1b[99z");
    assert_eq!(term.counters().get(ParseErrorKind::CsiUnknown), 1);
    assert!(term.take_responses().is_empty());
}

#[test]
fn private_csi_question_c_is_not_da1_and_stays_unknown() {
    let mut term = Terminal::new(10, 3);
    term.feed(b"\x1b[?c");
    assert!(term.take_responses().is_empty());
    assert_eq!(term.counters().get(ParseErrorKind::CsiUnknown), 1);
}
