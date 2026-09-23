//! Test 6: SGR, cursor movement, erase, scroll region and alternate screen.

use termai_core::grid::{Color, ATTR_BOLD, ATTR_UNDERLINE};
use termai_vt::grid::{MODE_ALT_SCREEN, MODE_AUTOWRAP, MODE_BRACKETED_PASTE};
use termai_vt::Terminal;

#[test]
fn sgr_basic_attributes_and_colors() {
    let mut t = Terminal::new(10, 3);
    t.feed(b"\x1b[1;4;31;44mX");
    let cell = *t.grid().cell(0, 0).expect("cell");
    assert_eq!(cell.ch, 'X');
    assert_eq!(cell.fg, Color::Indexed(1));
    assert_eq!(cell.bg, Color::Indexed(4));
    assert_ne!(cell.attrs & ATTR_BOLD, 0);
    assert_ne!(cell.attrs & ATTR_UNDERLINE, 0);
    t.feed(b"\x1b[0mY");
    let cell = *t.grid().cell(0, 1).expect("cell");
    assert_eq!(cell.fg, Color::Default);
    assert_eq!(cell.attrs, 0);
}

#[test]
fn sgr_truecolor_and_256() {
    let mut t = Terminal::new(10, 3);
    t.feed(b"\x1b[38;2;10;20;30mR\x1b[48;5;200mB");
    assert_eq!(
        t.grid().cell(0, 0).map(|c| c.fg),
        Some(Color::Rgb(10, 20, 30))
    );
    assert_eq!(t.grid().cell(0, 1).map(|c| c.bg), Some(Color::Indexed(200)));
}

#[test]
fn sgr_bright_and_resets() {
    let mut t = Terminal::new(10, 3);
    t.feed(b"\x1b[91;102mZ");
    assert_eq!(t.grid().cell(0, 0).map(|c| c.fg), Some(Color::Indexed(9)));
    assert_eq!(t.grid().cell(0, 0).map(|c| c.bg), Some(Color::Indexed(10)));
    t.feed(b"\x1b[39;49mY");
    assert_eq!(t.grid().cell(0, 1).map(|c| c.fg), Some(Color::Default));
    assert_eq!(t.grid().cell(0, 1).map(|c| c.bg), Some(Color::Default));
}

#[test]
fn cursor_movement() {
    let mut t = Terminal::new(10, 6);
    t.feed(b"abc");
    assert_eq!(t.grid().cursor(), (0, 3));
    t.feed(b"\x1b[2;3H");
    assert_eq!(t.grid().cursor(), (1, 2));
    t.feed(b"\x1b[A");
    assert_eq!(t.grid().cursor(), (0, 2));
    t.feed(b"\x1b[2C");
    assert_eq!(t.grid().cursor(), (0, 4));
    t.feed(b"\x1b[9D");
    assert_eq!(t.grid().cursor(), (0, 0));
    t.feed(b"\x1b[E");
    assert_eq!(t.grid().cursor(), (1, 0));
    t.feed(b"\x1b[F");
    assert_eq!(t.grid().cursor(), (0, 0));
    t.feed(b"\x1b[3G");
    assert_eq!(t.grid().cursor(), (0, 2));
    t.feed(b"\x1b[4d");
    assert_eq!(t.grid().cursor(), (3, 2));
    t.feed(b"\x1b[9e");
    assert_eq!(t.grid().cursor(), (5, 2));
    t.feed(b"\x1b[f");
    assert_eq!(t.grid().cursor(), (0, 0));
}

#[test]
fn erase_in_line_and_display() {
    let mut t = Terminal::new(10, 4);
    t.feed(b"hello\r");
    assert_eq!(t.grid().cursor(), (0, 0));
    t.feed(b"\x1b[1K");
    assert_eq!(t.grid().row_text(0), " ello");
    t.feed(b"\x1b[2K");
    assert_eq!(t.grid().row_text(0), "");

    let mut t = Terminal::new(10, 4);
    t.feed(b"hello\r\nworld\r");
    t.feed(b"\x1b[0J");
    assert_eq!(t.grid().row_text(0), "hello");
    assert_eq!(t.grid().row_text(1), "");
    t.feed(b"\x1b[2J");
    assert_eq!(t.grid().row_text(0), "");
}

#[test]
fn insert_and_delete_chars() {
    let mut t = Terminal::new(10, 2);
    t.feed(b"abcde\x1b[1;1H");
    t.feed(b"\x1b[2@");
    assert_eq!(t.grid().row_text(0), "  abcde");
    t.feed(b"\x1b[2P");
    assert_eq!(t.grid().row_text(0), "abcde");
    t.feed(b"\x1b[3X");
    assert_eq!(t.grid().row_text(0), "   de");
}

#[test]
fn scroll_region_and_scroll_ops() {
    let mut t = Terminal::new(10, 5);
    t.feed(b"AAAA\r\nBBBB\r\nCCCC\r\nDDDD\r\nEEEE");
    t.feed(b"\x1b[2;4r");
    assert_eq!(t.grid().scroll_region(), (1, 3));
    t.feed(b"\x1b[S");
    assert_eq!(t.grid().row_text(0), "AAAA");
    assert_eq!(t.grid().row_text(1), "CCCC");
    assert_eq!(t.grid().row_text(2), "DDDD");
    assert_eq!(t.grid().row_text(3), "");
    assert_eq!(t.grid().row_text(4), "EEEE");
    t.feed(b"\x1b[T");
    assert_eq!(t.grid().row_text(1), "");
    assert_eq!(t.grid().row_text(2), "CCCC");
    assert_eq!(t.grid().row_text(3), "DDDD");
}

#[test]
fn insert_and_delete_lines() {
    let mut t = Terminal::new(10, 5);
    t.feed(b"A\r\nB\r\nC\r\nD\r\nE");
    t.feed(b"\x1b[2;1H\x1b[L");
    assert_eq!(t.grid().row_text(1), "");
    assert_eq!(t.grid().row_text(2), "B");
    assert_eq!(t.grid().row_text(3), "C");
    t.feed(b"\x1b[M");
    assert_eq!(t.grid().row_text(1), "B");
    assert_eq!(t.grid().row_text(2), "C");
    assert_eq!(t.grid().row_text(3), "D");
    assert_eq!(t.grid().row_text(4), "");
}

#[test]
fn alternate_screen_1049() {
    let mut t = Terminal::new(10, 3);
    t.feed(b"main");
    t.feed(b"\x1b[?1049h\x1b[H");
    assert!(t.grid().snapshot().alt);
    assert_ne!(t.grid().modes() & MODE_ALT_SCREEN, 0);
    assert_eq!(t.grid().row_text(0), "");
    t.feed(b"alt");
    assert_eq!(t.grid().row_text(0), "alt");
    t.feed(b"\x1b[?1049l");
    assert_eq!(t.grid().row_text(0), "main");
    assert_eq!(t.grid().cursor(), (0, 4));
    assert_eq!(t.grid().modes() & MODE_ALT_SCREEN, 0);
}

#[test]
fn autowrap_on_and_off() {
    let mut t = Terminal::new(5, 3);
    t.feed(b"abcde");
    assert!(t.grid().wrap_pending());
    assert_ne!(t.grid().modes() & MODE_AUTOWRAP, 0);
    t.feed(b"f");
    assert_eq!(t.grid().row_text(0), "abcde");
    assert_eq!(t.grid().row_text(1), "f");

    let mut t = Terminal::new(5, 3);
    t.feed(b"\x1b[?7l12345");
    assert_eq!(t.grid().modes() & MODE_AUTOWRAP, 0);
    t.feed(b"X");
    assert_eq!(t.grid().row_text(0), "1234X");
}

#[test]
fn insert_mode() {
    let mut t = Terminal::new(10, 2);
    t.feed(b"abc\r\x1b[4hX");
    assert_eq!(t.grid().row_text(0), "Xabc");
}

#[test]
fn origin_mode_homes_inside_region() {
    let mut t = Terminal::new(10, 6);
    t.feed(b"\x1b[?6h\x1b[2;4r");
    assert_eq!(t.grid().cursor(), (1, 0));
    t.feed(b"\x1b[H");
    assert_eq!(t.grid().cursor(), (1, 0));
    t.feed(b"\x1b[9;1H");
    assert_eq!(t.grid().cursor(), (3, 0));
}

#[test]
fn save_and_restore_cursor() {
    let mut t = Terminal::new(10, 6);
    t.feed(b"\x1b[2;3H\x1b7\x1b[5;5H\x1b8");
    assert_eq!(t.grid().cursor(), (1, 2));
}

#[test]
fn private_modes_bitmap() {
    let mut t = Terminal::new(10, 3);
    t.feed(b"\x1b[?25l");
    assert!(!t.grid().cursor_visible());
    t.feed(b"\x1b[?25h");
    assert!(t.grid().cursor_visible());
    t.feed(b"\x1b[?2004h");
    assert_ne!(t.grid().modes() & MODE_BRACKETED_PASTE, 0);
    t.feed(b"\x1b[?2004l");
    assert_eq!(t.grid().modes() & MODE_BRACKETED_PASTE, 0);
}

#[test]
fn full_reset_clears_everything() {
    let mut t = Terminal::new(10, 3);
    t.feed(b"\x1b[31mabc\x1b[2;1H\x1b[?25l\x1bc");
    assert_eq!(t.grid().row_text(0), "");
    assert_eq!(t.grid().cursor(), (0, 0));
    assert!(t.grid().cursor_visible());
    assert_ne!(t.grid().modes() & MODE_AUTOWRAP, 0);
}

#[test]
fn tab_stops_and_backspace() {
    let mut t = Terminal::new(20, 2);
    t.feed(b"\t");
    assert_eq!(t.grid().cursor(), (0, 8));
    t.feed(b"\t");
    assert_eq!(t.grid().cursor(), (0, 16));
    t.feed(b"\x1b[3g\t");
    assert_eq!(t.grid().cursor(), (0, 19));

    let mut t = Terminal::new(20, 2);
    t.feed(b"abc\x08X");
    assert_eq!(t.grid().row_text(0), "abX");
}

#[test]
fn scrollback_counts_lines_pushed_off_the_top() {
    let mut t = Terminal::new(5, 2);
    t.feed(b"a\r\nb\r\nc");
    assert_eq!(t.grid().row_text(0), "b");
    assert_eq!(t.grid().row_text(1), "c");
    assert_eq!(t.grid().scrollback_len(), 1);
    t.feed(b"\x1b[3J");
    assert_eq!(t.grid().scrollback_len(), 0);
}

#[test]
fn resize_preserves_overlap() {
    let mut t = Terminal::new(10, 3);
    t.feed(b"hello");
    t.resize(20, 5);
    assert_eq!(t.grid().cols(), 20);
    assert_eq!(t.grid().rows(), 5);
    assert_eq!(t.grid().row_text(0), "hello");
    t.resize(3, 2);
    assert_eq!(t.grid().row_text(0), "hel");
}
