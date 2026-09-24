//! VRM slice tests (kernel/03 section 3.8 / RP-08 half).

use termai_core::grid::{GridSnapshot, LINE_WRAPPED};
use termai_render::vrm::{
    clip_visible_offset, display_row_count, logical_lines, LogicalLine, WrapMode,
};

/// A `rows`-tall snapshot whose listed rows continue into the next one.
fn snapshot_with_links(rows: u16, linked: &[u16]) -> GridSnapshot {
    let mut snapshot = GridSnapshot::new(4, rows);
    for &row in linked {
        snapshot.row_flags[usize::from(row)] = LINE_WRAPPED;
    }
    snapshot
}

#[test]
fn independent_rows_yield_one_logical_line_each() {
    let snapshot = snapshot_with_links(3, &[]);
    let lines = logical_lines(&snapshot);
    assert_eq!(lines.len(), 3);
    assert_eq!(
        lines.iter().map(|l| l.start_row).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert!(lines.iter().all(|l| l.row_count == 1));
}

#[test]
fn wrapped_chain_forms_a_single_logical_line() {
    let snapshot = snapshot_with_links(3, &[0, 1]);
    let lines = logical_lines(&snapshot);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0],
        LogicalLine {
            start_row: 0,
            row_count: 3,
        }
    );
}

#[test]
fn display_row_count_follows_the_mode() {
    let line = LogicalLine {
        start_row: 0,
        row_count: 3,
    };
    assert_eq!(display_row_count(&line, WrapMode::Fold), 3);
    assert_eq!(display_row_count(&line, WrapMode::Clip), 1);
}

#[test]
fn vrm_is_display_only_and_clip_follows_the_cursor() {
    let snapshot = snapshot_with_links(4, &[0, 1]);
    let before = snapshot.canonical_bytes();
    let lines = logical_lines(&snapshot);
    let after = snapshot.canonical_bytes();
    assert_eq!(
        before, after,
        "VRM must not change the canonical grid bytes"
    );

    let chain = lines[0];
    assert_eq!(chain.row_count, 3);
    assert_eq!(clip_visible_offset(&chain, 1, WrapMode::Clip), 1);
    assert_eq!(clip_visible_offset(&chain, 3, WrapMode::Clip), 0);
    assert_eq!(clip_visible_offset(&chain, 1, WrapMode::Fold), 0);
}
