//! Mode/state cluster: LNM, DECSET 1048 cursor save/restore, xterm DEC-private-mode
//! save/restore, and the OSC/title reports that `CSI 20 t` / `CSI 21 t` answer.
//!
//! These pin the behaviour the esctest mode/state cases assert (sm.py test_SM_LNM,
//! decset.py test_DECSET_SaveRestoreCursor, decset_tite_inhibit.py
//! test_SaveRestoreCursor_*, xterm_save.py, ris.py test_RIS_ResetTitleMode).

use termai_vt::grid::MODE_AUTOWRAP;
use termai_vt::Terminal;

#[test]
fn lnm_makes_lf_return_to_column_one() {
    let mut term = Terminal::new(20, 4);
    // LNM reset: LF moves down and leaves the column alone.
    term.feed(b"\x1b[1;5H\x0a");
    assert_eq!(term.grid().cursor(), (1, 4));
    // LNM set: LF, VT and FF also do a carriage return.
    term.feed(b"\x1b[20h");
    term.feed(b"\x1b[1;5H\x0a");
    assert_eq!(term.grid().cursor(), (1, 0));
    term.feed(b"\x1b[1;5H\x0b");
    assert_eq!(term.grid().cursor(), (1, 0));
    term.feed(b"\x1b[1;5H\x0c");
    assert_eq!(term.grid().cursor(), (1, 0));
    // LNM reset again: the carriage return is gone.
    term.feed(b"\x1b[20l");
    term.feed(b"\x1b[1;5H\x0c");
    assert_eq!(term.grid().cursor(), (1, 4));
}

#[test]
fn decset_1048_saves_and_restores_cursor() {
    let mut term = Terminal::new(10, 6);
    term.feed(b"\x1b[2;3H\x1b[?1048h\x1b[5;5H\x1b[?1048l");
    assert_eq!(term.grid().cursor(), (1, 2));
}

#[test]
fn xterm_save_restore_saves_per_private_mode() {
    let mut term = Terminal::new(20, 3);
    // Save autowrap while set, reset it, restore it.
    term.feed(b"\x1b[?7h\x1b[?7s\x1b[?7l");
    assert_eq!(term.grid().modes() & MODE_AUTOWRAP, 0);
    term.feed(b"\x1b[?7r");
    assert_ne!(term.grid().modes() & MODE_AUTOWRAP, 0);
    // And the other direction: save it reset, set it, restore it.
    term.feed(b"\x1b[?7l\x1b[?7s\x1b[?7h");
    assert_ne!(term.grid().modes() & MODE_AUTOWRAP, 0);
    term.feed(b"\x1b[?7r");
    assert_eq!(term.grid().modes() & MODE_AUTOWRAP, 0);
}

#[test]
fn icon_and_window_titles_are_independent_and_reported() {
    let mut term = Terminal::new(20, 3);
    term.feed(b"\x1b]2;win\x07");
    term.feed(b"\x1b]1;ico\x07");
    assert_eq!(term.grid().title(), "win");

    // CSI 21 t reports the window title as OSC l; CSI 20 t the icon title as OSC L.
    term.feed(b"\x1b[21t\x1b[20t");
    let responses = term.take_responses();
    assert_eq!(
        responses,
        vec![b"\x1b]lwin\x1b\\".to_vec(), b"\x1b]Lico\x1b\\".to_vec()]
    );

    // OSC 0 sets both titles.
    term.feed(b"\x1b]0;both\x07\x1b[20t");
    assert_eq!(term.grid().title(), "both");
    assert_eq!(term.take_responses(), vec![b"\x1b]Lboth\x1b\\".to_vec()]);
}
