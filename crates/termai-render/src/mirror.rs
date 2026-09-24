//! Render mirror: the UI-side projection of the sessiond grid (kernel/03 K-01).
//!
//! The grid's single source of truth is sessiond; this mirror is disposable. It must
//! converge by a full snapshot and never by trusting a gap in the delta stream
//! (kernel/03 section 3.3 item 6: a rev gap drops the mirror and requests a snapshot).
//!
//! Application order is scroll -> row payloads -> cursor. A ScrollOp carries no row
//! content, it rotates row slots; the payloads then fill the rows the scroll exposed
//! (kernel/03 section 3.3 items 1-3).

use termai_core::grid::{Cell, GridDelta, GridSnapshot, ScrollOp};

/// Why a delta could not be applied.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MirrorError {
    /// No snapshot has been applied yet, so there is nothing to patch.
    Uninitialised,
    /// `delta.rev` skipped at least one revision. The mirror is now invalid: the caller
    /// must obtain a full GridSnapshot before applying anything else.
    NeedsSnapshot {
        /// Revision the mirror expected next.
        expected: u32,
        /// Revision the delta carried.
        got: u32,
    },
    /// `delta.rev` is not newer than the mirror, so the delta is a duplicate and is ignored.
    Stale {
        /// The duplicate revision.
        rev: u32,
    },
    /// A row payload is not exactly one grid row wide.
    RowShape {
        /// Row the payload targeted.
        row: u16,
        /// Width the grid requires.
        expected: u16,
        /// Width the payload carried.
        got: usize,
    },
    /// A row payload targeted a row outside the grid.
    RowOutOfRange {
        /// The offending row.
        row: u16,
    },
}

/// Damage accumulated since the last take_damage call.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct DamageSet {
    /// Dirty viewport rows, as reported by the producer.
    pub rows: Vec<u16>,
    /// The producer promoted this update to a full repaint.
    pub full: bool,
}

/// UI-side disposable grid projection.
#[derive(Clone, Debug, Default)]
pub struct Mirror {
    grid: Option<GridSnapshot>,
    rev: Option<u32>,
    valid: bool,
    damage: DamageSet,
}

impl Mirror {
    /// An empty mirror: not valid until a snapshot lands.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// True while the mirror can accept incremental deltas.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// The current grid, if a snapshot has been applied.
    #[must_use]
    pub fn grid(&self) -> Option<&GridSnapshot> {
        self.grid.as_ref()
    }

    /// The last applied revision, or None right after a snapshot (see SD-15).
    #[must_use]
    pub fn rev(&self) -> Option<u32> {
        self.rev
    }

    /// Replace the mirror with a full snapshot. This is the only way out of an invalid
    /// mirror (kernel/03 section 3.3 item 6).
    ///
    /// A GridSnapshot carries no rev, so the mirror accepts the next delta regardless of
    /// its revision and adopts that revision as the new baseline. That rule is a reading
    /// of the frozen field set, not a spec sentence: it is registered as SD-15.
    pub fn apply_snapshot(&mut self, mut snapshot: GridSnapshot) {
        // ADR-0025 D1: row_flags length must equal rows. Normalise here so a snapshot
        // from an older producer (missing rows) cannot desynchronise the mirror.
        let rows = usize::from(snapshot.rows);
        if snapshot.row_flags.len() != rows {
            snapshot.row_flags.resize(rows, 0);
        }
        self.grid = Some(snapshot);
        self.rev = None;
        self.valid = true;
        self.damage = DamageSet::default();
    }

    /// Apply one incremental delta.
    ///
    /// # Errors
    /// Returns MirrorError when the delta cannot be applied; a revision gap leaves the
    /// mirror invalid until the next snapshot.
    pub fn apply_delta(&mut self, delta: &GridDelta) -> Result<(), MirrorError> {
        let Some(grid) = self.grid.as_mut() else {
            return Err(MirrorError::Uninitialised);
        };
        if !self.valid {
            return Err(MirrorError::NeedsSnapshot {
                expected: self.rev.map_or(0, |r| r + 1),
                got: delta.rev,
            });
        }
        if let Some(current) = self.rev {
            if delta.rev <= current {
                return Err(MirrorError::Stale { rev: delta.rev });
            }
            if delta.rev != current + 1 {
                self.valid = false;
                return Err(MirrorError::NeedsSnapshot {
                    expected: current + 1,
                    got: delta.rev,
                });
            }
        }
        // Validate every payload before touching the grid: a rejected delta must not
        // leave a half-applied mirror behind.
        for payload in &delta.rows {
            if payload.cells.len() != usize::from(grid.cols) {
                return Err(MirrorError::RowShape {
                    row: payload.row,
                    expected: grid.cols,
                    got: payload.cells.len(),
                });
            }
            if grid.idx(payload.row, 0).is_none() {
                return Err(MirrorError::RowOutOfRange { row: payload.row });
            }
        }
        // SD-14: the scroll is carried twice in the frozen DTO. GridDelta.scroll is the
        // authority; damage.scroll is the legacy duplicate.
        if let Some(scroll) = delta.scroll.or(delta.damage.scroll) {
            apply_scroll(grid, scroll);
        }
        for payload in &delta.rows {
            let start = grid.idx(payload.row, 0).expect("validated above");
            grid.cells[start..start + payload.cells.len()].copy_from_slice(&payload.cells);
            // ADR-0025 D3: the mirror carries the producer's LineFlags through to the
            // mirror snapshot so the VRM can rebuild logical lines from it.
            if let Some(flags) = grid.row_flags.get_mut(usize::from(payload.row)) {
                *flags = payload.flags;
            }
        }
        grid.cursor = delta.cursor;
        self.rev = Some(delta.rev);
        self.damage.rows.extend_from_slice(&delta.damage.rows);
        self.damage.full |= delta.damage.full;
        Ok(())
    }

    /// Take the damage accumulated since the previous call.
    pub fn take_damage(&mut self) -> DamageSet {
        std::mem::take(&mut self.damage)
    }
}

/// Rotate row slots inside the inclusive scroll region. Positions the region exposes are
/// blanked; the caller's row payloads then fill them, which is why order matters.
fn apply_scroll(grid: &mut GridSnapshot, scroll: ScrollOp) {
    if scroll.delta == 0 || scroll.top >= scroll.bottom {
        return;
    }
    let top = usize::from(scroll.top);
    let bottom = usize::from(scroll.bottom);
    let cols = usize::from(grid.cols);
    if cols == 0 || bottom >= usize::from(grid.rows) || grid.cells.len() < (bottom + 1) * cols {
        return;
    }
    let height = bottom - top + 1;
    let range_start = top * cols;
    // Snapshot the region first: an overlapping move would otherwise read rows it has
    // already overwritten.
    let range: Vec<Cell> = grid.cells[range_start..range_start + height * cols].to_vec();
    // Flags rotate with their rows (ADR-0025 D1); rows the scroll exposes clear.
    let flags: Vec<u16> = (top..=bottom)
        .map(|row| grid.row_flags.get(row).copied().unwrap_or(0))
        .collect();
    let delta = isize::from(scroll.delta);
    for row in top..=bottom {
        let source = row as isize - delta;
        let dst = row * cols;
        if source >= top as isize && source <= bottom as isize {
            let src_local = (source as usize - top) * cols;
            grid.cells[dst..dst + cols].copy_from_slice(&range[src_local..src_local + cols]);
            let flag = flags[source as usize - top];
            if let Some(slot) = grid.row_flags.get_mut(row) {
                *slot = flag;
            }
        } else {
            grid.cells[dst..dst + cols].fill(Cell::BLANK);
            if let Some(slot) = grid.row_flags.get_mut(row) {
                *slot = 0;
            }
        }
    }
}
