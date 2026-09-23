//! Test 5: wide chars, WIDE_CONTINUATION and exact row_text / copy round-trip.

use termai_core::grid::Cell;
use termai_vt::{Grid, Terminal};

#[test]
fn wide_char_occupies_two_columns() {
    let mut t = Terminal::new(20, 4);
    t.feed("A\u{6c49}B".as_bytes());
    assert_eq!(t.grid().row_text(0), "A\u{6c49}B");
    assert_eq!(t.grid().cell(0, 0).map(|c| c.ch), Some('A'));
    assert_eq!(t.grid().cell(0, 1).map(|c| c.ch), Some('\u{6c49}'));
    assert!(t
        .grid()
        .cell(0, 2)
        .map(|c| c.is_wide_continuation())
        .unwrap_or(false));
    assert_eq!(t.grid().cell(0, 3).map(|c| c.ch), Some('B'));
    assert_eq!(t.grid().cursor(), (0, 4));
}

#[test]
fn wide_right_half_is_wide_continuation() {
    let mut t = Terminal::new(20, 4);
    t.feed("\u{6c49}".as_bytes());
    let cell = t.grid().cell(0, 1).expect("continuation cell");
    assert_eq!(*cell, Cell::WIDE_CONTINUATION);
    assert!(cell.is_wide_continuation());
}

#[test]
fn combining_char_appends_without_allocating_a_cell() {
    let grid = {
        let mut g = Grid::new(20, 4);
        g.print('e');
        g.print('\u{301}');
        g
    };
    assert_eq!(grid.row_text(0), "e\u{301}");
    assert_eq!(grid.cell(0, 0).map(|c| c.ch), Some('e'));
    // The next cell must remain blank: no cell was allocated for the combining mark.
    assert_eq!(grid.cell(0, 1).map(|c| c.ch), Some(' '));
}

#[test]
fn wide_char_at_last_column_wraps() {
    let mut t = Terminal::new(4, 3);
    t.feed(b"abc");
    t.feed("\u{6c49}".as_bytes());
    assert_eq!(t.grid().row_text(0), "abc");
    assert_eq!(t.grid().row_text(1), "\u{6c49}");
    assert_eq!(t.grid().cursor(), (1, 2));
}

#[test]
fn row_text_copy_round_trips_across_widths() {
    let mut t = Terminal::new(30, 4);
    t.feed("a\u{6c49}b\u{ff21}c".as_bytes());
    let text = t.grid().row_text(0);
    assert_eq!(text, "a\u{6c49}b\u{ff21}c");
    // Re-feeding the same logical text into a fresh grid reproduces it byte for byte.
    let mut u = Terminal::new(30, 4);
    u.feed(text.as_bytes());
    assert_eq!(u.grid().row_text(0), text);
}
