//! Visual row mapping (VRM): the display-layer projection of the wrapped row chain.
//!
//! Kernel/03 section 3.8 (AR-23 item 6) fixes one conclusion: soft wrap / clip is a
//! **display-layer mapping**, never a grid-layer behaviour. The grid keeps every row and
//! every column; switching `WrapMode` produces zero `GridDelta`s and leaves the grid
//! revision untouched (RP-08). A logical line is therefore just a chain of grid rows
//! linked by `LineFlags::WRAPPED` (ADR-0025 D1: the flag sits on the row that continues
//! into the next one).
//!
//! This slice is deliberately only the mapping: no scroll anchor, no hit testing, no
//! a11y projection and no display-row total. Those arrive with later ADRs.

use termai_core::grid::{GridSnapshot, LINE_WRAPPED};

/// How the display layer lays out a logical line (kernel/03 section 3.8).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WrapMode {
    /// Soft wrap on (the default, AR-22 section 4): every grid row of the chain is shown.
    Fold,
    /// Soft wrap off (AR-23 section 6): one display row per logical line.
    Clip,
}

/// A chain of grid rows joined by `LineFlags::WRAPPED`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LogicalLine {
    /// First grid row of the chain.
    pub start_row: u16,
    /// Number of grid rows in the chain; always at least 1.
    pub row_count: u16,
}

/// Group the snapshot's rows into logical lines.
///
/// `row_flags[i] & LINE_WRAPPED != 0` means row `i` continues into row `i + 1`
/// (ADR-0025 D1), so it is a pure display-layer reading of the grid: the snapshot is not
/// touched. A final row that ends a chain is still emitted.
#[must_use]
pub fn logical_lines(snapshot: &GridSnapshot) -> Vec<LogicalLine> {
    let mut lines = Vec::new();
    let mut start_row = 0_u16;
    let mut row_count = 0_u16;
    for row in 0..snapshot.rows {
        if row_count == 0 {
            start_row = row;
        }
        row_count = row_count.saturating_add(1);
        let continues = snapshot
            .row_flags
            .get(usize::from(row))
            .copied()
            .unwrap_or(0)
            & LINE_WRAPPED
            != 0;
        if !continues {
            lines.push(LogicalLine {
                start_row,
                row_count,
            });
            row_count = 0;
        }
    }
    if row_count != 0 {
        lines.push(LogicalLine {
            start_row,
            row_count,
        });
    }
    lines
}

/// Display rows a logical line occupies in `mode`.
///
/// `Fold` shows the whole chain; `Clip` collapses it to exactly one display row
/// (kernel/03 section 3.8, K-11).
#[must_use]
pub fn display_row_count(line: &LogicalLine, mode: WrapMode) -> u16 {
    match mode {
        WrapMode::Fold => line.row_count,
        WrapMode::Clip => 1,
    }
}

/// Which segment of `line` the clipped display row shows.
///
/// `Fold` is never clipped, so the offset is always 0. In `Clip` the visible segment is
/// the one holding `cursor_row` when the cursor is inside this logical line, otherwise the
/// head segment (K-11). This is an offset within the line, never a horizontal pan.
#[must_use]
pub fn clip_visible_offset(line: &LogicalLine, cursor_row: u16, mode: WrapMode) -> u16 {
    match mode {
        WrapMode::Fold => 0,
        WrapMode::Clip => {
            let end = line.start_row.saturating_add(line.row_count);
            if cursor_row >= line.start_row && cursor_row < end {
                cursor_row - line.start_row
            } else {
                0
            }
        }
    }
}
