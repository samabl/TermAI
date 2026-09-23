//! Visual row mapping (VRM): the display-layer projection of the wrapped row chain.
//!
//! Kernel/03 section 3.8 (AR-23 item 6) fixes one conclusion: soft wrap / clip is a
//! **display-layer mapping**, never a grid-layer behaviour. The grid keeps every row and
//! every column; switching `WrapMode` produces zero `GridDelta`s and leaves the grid
//! revision untouched (RP-08). A logical line is therefore just a chain of grid rows
//! linked by `LineFlags::WRAPPED` (ADR-0025 D1: the flag sits on the row that continues
//! into the next one).
//!
//! This slice is deliberately only the mapping plus the mode's owner ([`VrmState`]): no
//! scroll anchor, no hit testing, no a11y projection and no display-row total. Those
//! arrive with later ADRs.

use termai_core::grid::{GridSnapshot, LINE_WRAPPED};

/// How the display layer lays out a logical line (kernel/03 section 3.8).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WrapMode {
    /// Soft wrap on (the default, AR-22 section 4): every grid row of the chain is shown.
    #[default]
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

/// One display row produced by the VRM.
///
/// The display layer never edits the grid: this is a projection of the row chain, so a
/// `VisualRow` names the grid rows it stands for and nothing else (kernel/03 section 3.8).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VisualRow {
    /// First grid row of the logical line this display row belongs to.
    pub logical_start_row: u16,
    /// Display segment inside that logical line. `Fold` walks the chain and increments it
    /// per grid row; `Clip` always reports 0 because a logical line has exactly one display
    /// row there.
    pub segment: u16,
    /// True when this display row hides at least one grid row of its logical line.
    ///
    /// `Fold` shows the whole chain, so it is always false. `Clip` shows exactly one
    /// segment, so any multi-row chain hides the others and reports true; a one-row line is
    /// shown in full and reports false.
    pub clipped: bool,
}

/// Owner of the wrap mode: the thing the UI toggles.
///
/// RP-08 has two halves. Computing the mapping from a snapshot is one; the other is that
/// *switching* the mode is an operation, and that operation never reaches the grid. This
/// type holds the mode so the switch exists as an operation at all, and
/// [`VrmState::visual_rows`] is a pure read: it takes `&GridSnapshot` and cannot emit a
/// `GridDelta` (kernel/03 section 3.8: soft wrap is a display-layer mapping).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct VrmState {
    mode: WrapMode,
}

impl VrmState {
    /// A state in the default mode: soft wrap on (`Fold`, AR-22 section 4).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The mode the display layer is currently projecting with.
    #[must_use]
    pub fn mode(&self) -> WrapMode {
        self.mode
    }

    /// Switch the wrap mode. This is a display-layer operation only: it carries no grid
    /// revision, no damage and no row payload, so it produces zero `GridDelta`s (RP-08).
    pub fn set_mode(&mut self, mode: WrapMode) {
        self.mode = mode;
    }

    /// Project `snapshot` into display rows under the current mode.
    ///
    /// `Fold` emits one display row per grid row of each logical line, with `segment`
    /// counting up from 0. `Clip` emits exactly one display row per logical line, showing
    /// the segment that holds `cursor_row` when the cursor is inside that line and the head
    /// segment otherwise (K-11). Nothing here mutates `snapshot`.
    #[must_use]
    pub fn visual_rows(&self, snapshot: &GridSnapshot, cursor_row: u16) -> Vec<VisualRow> {
        let mut visual_rows = Vec::new();
        for line in logical_lines(snapshot) {
            match self.mode {
                WrapMode::Fold => {
                    for segment in 0..line.row_count {
                        visual_rows.push(VisualRow {
                            logical_start_row: line.start_row,
                            segment,
                            clipped: false,
                        });
                    }
                }
                WrapMode::Clip => {
                    let segment = clip_visible_offset(&line, cursor_row, self.mode);
                    // Conservative reading: one display row stands for the whole logical
                    // line, so every row of a multi-row chain hides at least one grid row
                    // (the head segment hides the tail; the cursor segment may hide both
                    // ends).
                    visual_rows.push(VisualRow {
                        logical_start_row: line.start_row,
                        segment,
                        clipped: line.row_count > 1,
                    });
                }
            }
        }
        visual_rows
    }
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
