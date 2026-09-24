//! DECRQM (`CSI Ps $ p` / `CSI ? Ps $ p`) reports a mode's state as
//! `CSI Ps ; Pm $ y`, where Pm is 0 not recognised, 1 set, 2 reset (xterm ctlseqs; oracle = xterm).
//! Reporting 0 for a mode we do not track is the honest answer.

use termai_vt::Terminal;

fn ask(term: &mut Terminal, input: &[u8]) -> Vec<String> {
    term.feed(input);
    term.take_responses()
        .into_iter()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .collect()
}

#[test]
fn decrqm_reports_private_mode_state() {
    let mut term = Terminal::new(80, 24);
    // 25 (cursor visible) is set by default.
    assert_eq!(ask(&mut term, b"\x1b[?25$p"), vec!["\x1b[?25;1$y"]);
    term.feed(b"\x1b[?25l");
    assert_eq!(ask(&mut term, b"\x1b[?25$p"), vec!["\x1b[?25;2$y"]);
    // 2004 (bracketed paste) is reset by default.
    assert_eq!(ask(&mut term, b"\x1b[?2004$p"), vec!["\x1b[?2004;2$y"]);
    term.feed(b"\x1b[?2004h");
    assert_eq!(ask(&mut term, b"\x1b[?2004$p"), vec!["\x1b[?2004;1$y"]);
}

#[test]
fn decrqm_reports_standard_mode_state() {
    let mut term = Terminal::new(80, 24);
    // ANSI mode 4 = insert mode, reset by default.
    assert_eq!(ask(&mut term, b"\x1b[4$p"), vec!["\x1b[4;2$y"]);
    term.feed(b"\x1b[4h");
    assert_eq!(ask(&mut term, b"\x1b[4$p"), vec!["\x1b[4;1$y"]);
}

#[test]
fn decrqm_reports_untracked_modes_as_not_recognised() {
    let mut term = Terminal::new(80, 24);
    assert_eq!(ask(&mut term, b"\x1b[?9999$p"), vec!["\x1b[?9999;0$y"]);
    assert_eq!(ask(&mut term, b"\x1b[9999$p"), vec!["\x1b[9999;0$y"]);
}

#[test]
fn decrqm_does_not_disturb_the_screen() {
    let mut term = Terminal::new(80, 24);
    term.feed(b"hi");
    let before = term.grid().row_text(0);
    let _ = ask(&mut term, b"\x1b[?25$p");
    assert_eq!(term.grid().row_text(0), before);
}
