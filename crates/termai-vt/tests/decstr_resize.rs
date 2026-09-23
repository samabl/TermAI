//! Regression tests for the round-70 pair: DECSTR (CSI ! p) and the same-size resize no-op.
//!
//! Together they took esctest from 224 to 239 passing. The property that matters is that a
//! same-size RESIZE - which the conformance adapter sends after every feed - no longer destroys
//! the scrolling region, and that DECSTR (which esctest sends before every case) returns it to the
//! full screen without erasing what is on screen.

use termai_vt::Terminal;

fn screen_region(t: &Terminal) -> (u16, u16) {
    t.grid().scroll_region()
}

#[test]
fn decstr_returns_the_region_to_full_screen_and_keeps_the_contents() {
    let mut t = Terminal::new(80, 24);
    t.feed(b"abc");
    t.feed(b"\x1b[2;4r"); // DECSTBM(2,4)
    assert_eq!(screen_region(&t), (1, 3));

    t.feed(b"\x1b[!p"); // DECSTR
    assert_eq!(
        screen_region(&t),
        (0, 23),
        "DECSTR must restore the full-screen region"
    );
    assert_eq!(
        t.grid().row_text(0),
        "abc",
        "DECSTR must not erase the screen"
    );
}

#[test]
fn decstr_homes_the_cursor() {
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b[3;5H");
    assert_eq!(t.grid().cursor(), (2, 4));
    t.feed(b"\x1b[!p");
    assert_eq!(t.grid().cursor(), (0, 0));
}

#[test]
fn same_size_resize_does_not_disturb_the_scroll_region() {
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b[2;4r");
    assert_eq!(screen_region(&t), (1, 3));
    t.resize(80, 24); // the adapter sends this after every feed
    assert_eq!(
        screen_region(&t),
        (1, 3),
        "a no-op resize must not clear the region"
    );
}

#[test]
fn a_real_resize_still_applies() {
    // Negative control: the guard must not make resize a no-op in general.
    let mut t = Terminal::new(80, 24);
    t.resize(40, 12);
    assert_eq!(t.grid().cols(), 40);
    assert_eq!(t.grid().rows(), 12);
    assert_eq!(screen_region(&t), (0, 11));
}
