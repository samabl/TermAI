use termai_core::grid::{
    Cell, CellPos, CursorState, Damage, GridDelta, GridSnapshot, RowPayload, ScrollOp, LINE_WRAPPED,
};
use termai_render::mirror::{Mirror, MirrorError};

fn snapshot(cols: u16, rows: u16) -> GridSnapshot {
    GridSnapshot::new(cols, rows)
}

fn set_row(grid: &mut GridSnapshot, row: u16, text: &str) {
    let cols = usize::from(grid.cols);
    for (i, ch) in text.chars().enumerate() {
        if i >= cols {
            break;
        }
        grid.cells[usize::from(row) * cols + i].ch = ch;
    }
}

fn row_cells(text: &str, cols: u16) -> Vec<Cell> {
    let mut cells = vec![Cell::BLANK; usize::from(cols)];
    for (i, ch) in text.chars().enumerate() {
        if i < cells.len() {
            cells[i].ch = ch;
        }
    }
    cells
}

fn delta(rev: u32, rows: Vec<RowPayload>) -> GridDelta {
    GridDelta {
        rev,
        scroll: None,
        damage: Damage::default(),
        rows,
        cursor: CursorState::default(),
    }
}

fn text(mirror: &Mirror, row: u16) -> String {
    mirror.grid().expect("a grid").row_text(row)
}

#[test]
fn a_delta_before_a_snapshot_is_uninitialised() {
    let mut mirror = Mirror::new();
    assert!(!mirror.is_valid());
    assert_eq!(
        mirror.apply_delta(&delta(1, vec![])),
        Err(MirrorError::Uninitialised)
    );
}

#[test]
fn snapshot_then_delta_applies_rows_and_cursor() {
    let mut mirror = Mirror::new();
    mirror.apply_snapshot(snapshot(3, 2));
    assert!(mirror.is_valid());
    assert_eq!(mirror.rev(), None, "a snapshot carries no rev (SD-15)");
    let mut d = delta(
        7,
        vec![RowPayload {
            row: 1,
            cells: row_cells("abc", 3),
            flags: 0,
        }],
    );
    d.cursor = CursorState {
        pos: CellPos { row: 1, col: 2 },
        visible: true,
        style: 1,
    };
    mirror.apply_delta(&d).unwrap();
    assert_eq!(mirror.rev(), Some(7));
    assert_eq!(text(&mirror, 1), "abc");
    assert_eq!(mirror.grid().unwrap().cursor.pos.col, 2);
}

#[test]
fn a_rev_gap_invalidates_the_mirror_until_a_snapshot() {
    let mut mirror = Mirror::new();
    mirror.apply_snapshot(snapshot(3, 2));
    mirror.apply_delta(&delta(1, vec![])).unwrap();
    assert_eq!(
        mirror.apply_delta(&delta(3, vec![])),
        Err(MirrorError::NeedsSnapshot {
            expected: 2,
            got: 3
        })
    );
    assert!(!mirror.is_valid(), "a gap drops the mirror");
    assert_eq!(
        mirror.apply_delta(&delta(4, vec![])),
        Err(MirrorError::NeedsSnapshot {
            expected: 2,
            got: 4
        })
    );
    mirror.apply_snapshot(snapshot(3, 2));
    assert!(mirror.is_valid());
    mirror.apply_delta(&delta(99, vec![])).unwrap();
    assert_eq!(mirror.rev(), Some(99));
}

#[test]
fn a_stale_delta_is_ignored_without_invalidating_the_mirror() {
    let mut mirror = Mirror::new();
    mirror.apply_snapshot(snapshot(3, 2));
    mirror.apply_delta(&delta(2, vec![])).unwrap();
    assert_eq!(
        mirror.apply_delta(&delta(2, vec![])),
        Err(MirrorError::Stale { rev: 2 })
    );
    assert_eq!(
        mirror.apply_delta(&delta(1, vec![])),
        Err(MirrorError::Stale { rev: 1 })
    );
    assert!(mirror.is_valid());
    assert_eq!(mirror.rev(), Some(2));
}

#[test]
fn scroll_moves_rows_up_inside_the_region() {
    let mut mirror = Mirror::new();
    let mut snap = snapshot(3, 4);
    for (i, line) in ["aaa", "bbb", "ccc", "ddd"].iter().enumerate() {
        set_row(&mut snap, i as u16, line);
    }
    mirror.apply_snapshot(snap);
    let mut d = delta(1, vec![]);
    d.scroll = Some(ScrollOp {
        top: 0,
        bottom: 3,
        delta: -1,
    });
    mirror.apply_delta(&d).unwrap();
    assert_eq!(text(&mirror, 0), "bbb");
    assert_eq!(text(&mirror, 1), "ccc");
    assert_eq!(text(&mirror, 2), "ddd");
    assert_eq!(text(&mirror, 3), "", "the exposed row starts blank");
}

#[test]
fn row_payloads_fill_the_rows_the_scroll_exposed() {
    let mut mirror = Mirror::new();
    let mut snap = snapshot(3, 4);
    for (i, line) in ["aaa", "bbb", "ccc", "ddd"].iter().enumerate() {
        set_row(&mut snap, i as u16, line);
    }
    mirror.apply_snapshot(snap);
    let mut d = delta(
        1,
        vec![RowPayload {
            row: 3,
            cells: row_cells("zzz", 3),
            flags: 0,
        }],
    );
    d.scroll = Some(ScrollOp {
        top: 0,
        bottom: 3,
        delta: -1,
    });
    mirror.apply_delta(&d).unwrap();
    assert_eq!(text(&mirror, 2), "ddd");
    assert_eq!(text(&mirror, 3), "zzz", "the payload wins over the blank");
}

#[test]
fn a_row_payload_carries_its_line_flags_into_the_mirror() {
    let mut mirror = Mirror::new();
    mirror.apply_snapshot(snapshot(3, 2));
    let d = delta(
        1,
        vec![RowPayload {
            row: 0,
            cells: row_cells("abc", 3),
            flags: LINE_WRAPPED,
        }],
    );
    mirror.apply_delta(&d).unwrap();
    let g = mirror.grid().unwrap();
    assert_ne!(g.row_flags[0] & LINE_WRAPPED, 0);
    assert_eq!(g.row_flags[1], 0);
}

#[test]
fn scroll_rotates_line_flags_with_the_rows() {
    let mut mirror = Mirror::new();
    let mut snap = snapshot(3, 4);
    for (i, line) in ["aaa", "bbb", "ccc", "ddd"].iter().enumerate() {
        set_row(&mut snap, i as u16, line);
    }
    snap.row_flags[0] = LINE_WRAPPED;
    snap.row_flags[2] = LINE_WRAPPED;
    mirror.apply_snapshot(snap);
    let mut d = delta(1, vec![]);
    d.scroll = Some(ScrollOp {
        top: 0,
        bottom: 3,
        delta: -1,
    });
    mirror.apply_delta(&d).unwrap();
    let g = mirror.grid().unwrap();
    assert_eq!(
        g.row_flags[0] & LINE_WRAPPED,
        0,
        "the moved-off row is gone"
    );
    assert_ne!(g.row_flags[1] & LINE_WRAPPED, 0, "old row 2 moved to row 1");
    assert_eq!(g.row_flags[2] & LINE_WRAPPED, 0);
    assert_eq!(g.row_flags[3] & LINE_WRAPPED, 0);
}

#[test]
fn damage_is_accumulated_then_taken_once() {
    let mut mirror = Mirror::new();
    mirror.apply_snapshot(snapshot(3, 2));
    let mut d = delta(1, vec![]);
    d.damage = Damage {
        rows: vec![0, 1],
        full: true,
        ..Damage::default()
    };
    mirror.apply_delta(&d).unwrap();
    let taken = mirror.take_damage();
    assert_eq!(taken.rows, vec![0, 1]);
    assert!(taken.full);
    assert!(mirror.take_damage().rows.is_empty());
}

#[test]
fn a_malformed_delta_is_rejected_without_partial_application() {
    let mut mirror = Mirror::new();
    let mut snap = snapshot(3, 2);
    set_row(&mut snap, 0, "aaa");
    mirror.apply_snapshot(snap);
    let bad = delta(
        1,
        vec![RowPayload {
            row: 0,
            cells: vec![Cell::BLANK; 2],
            flags: 0,
        }],
    );
    assert_eq!(
        mirror.apply_delta(&bad),
        Err(MirrorError::RowShape {
            row: 0,
            expected: 3,
            got: 2
        })
    );
    assert_eq!(mirror.rev(), None, "the revision must not advance");
    assert_eq!(text(&mirror, 0), "aaa", "row 0 must be untouched");
    let out_of_range = delta(
        2,
        vec![RowPayload {
            row: 9,
            cells: row_cells("abc", 3),
            flags: 0,
        }],
    );
    assert_eq!(
        mirror.apply_delta(&out_of_range),
        Err(MirrorError::RowOutOfRange { row: 9 })
    );
}
