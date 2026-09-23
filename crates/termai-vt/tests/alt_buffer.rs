//! Alt-buffer (DECSET 47) regression: the sequence esctest's doAltBuftest uses.
//!
//! Written as a probe while triaging the esctest DECSET cluster. It is kept because it
//! pins the behaviour that triage needed to know: entering the alt screen must not
//! disturb the main screen, and leaving it must restore the main screen untouched.

use termai_vt::Terminal;

fn rows(t: &Terminal, n: u16) -> Vec<String> {
    (0..n).map(|r| t.grid().row_text(r)).collect()
}

#[test]
fn mode_47_keeps_main_and_restores_it_on_exit() {
    let mut t = Terminal::new(80, 24);
    t.feed(b"abc\r\nabc");
    assert_eq!(rows(&t, 3), vec!["abc", "abc", ""]);

    // Enter alt, erase it, then write on the second row (esctest's CUP(1,2) = row 2, col 1).
    t.feed(b"\x1b[?47h");
    t.feed(b"\x1b[2J");
    t.feed(b"\x1b[2;1H");
    t.feed(b"def\r\ndef");
    assert_eq!(rows(&t, 3), vec!["", "def", "def"]);

    // Leave alt: the main screen must come back exactly as it was.
    t.feed(b"\x1b[?47l");
    assert_eq!(rows(&t, 3), vec!["abc", "abc", ""]);
}
