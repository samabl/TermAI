//! The single column-width authority (kernel/03 K-04; spec line
//! `列宽真源 = termai-vt::width::measure(scalars)`).
//!
//! `termai-vt` owns the only wcwidth table in the workspace. Two callers decide cell columns
//! from it and must never disagree:
//!
//! - `Grid::print` decides what one scalar costs in cells, through [`measure_scalar`];
//! - `termai-render` decides what one grapheme cluster costs in cells, through [`measure`],
//!   reached via its `shape::CellWidthSource` port (`VtWidthSource`).
//!
//! Both go through this module, so they cannot drift. K-04 names that drift the most expensive
//! bug class of the pipeline (cluster/column drift reaching the screen), and two independent
//! width tables are guaranteed to diverge eventually.
//!
//! Semantics: exactly `unicode-width`'s per-scalar `UnicodeWidthChar::width` - UAX #11 East
//! Asian Width plus the tables' own zero-width rules:
//!
//! - `0` for combining marks, variation selectors and other zero-width scalars, including the
//!   C0 controls (`\t`, `ESC`, ...) that `unicode-width` reports as "no width". K-04 reads that
//!   as "joins the preceding cluster";
//! - `2` for East Asian Wide / Fullwidth scalars (WIDE_LEAD + WIDE_CONTINUATION);
//! - `1` otherwise.
//!
//! [`measure`] is deliberately the **sum of the per-scalar widths**, not
//! `UnicodeWidthStr::width`. The string form is context sensitive *inside one string*: it
//! collapses a ZWJ emoji sequence and applies VS15/VS16 presentation to the scalar before it.
//! The grid decides columns one `print(scalar)` call at a time and therefore cannot see that
//! context, so the scalar sum is what the grid actually did - the only definition under which
//! grid and shaper can agree. Changing a sequence's column count (emoji ZWJ collapsing) is a
//! VT-semantics decision, not a width-table decision, and does not belong in this module.

use unicode_width::UnicodeWidthChar;

/// Columns one scalar occupies in the cell grid (kernel/03 K-04).
///
/// This is the rule `Grid::print` applies: `0` means "zero width, join the previous cluster",
/// `2` means a wide cell pair.
#[must_use]
pub fn measure_scalar(scalar: char) -> u8 {
    u8::try_from(UnicodeWidthChar::width(scalar).unwrap_or(0)).unwrap_or(u8::MAX)
}

/// Columns a whole scalar sequence occupies: the sum of [`measure_scalar`] over its scalars,
/// saturated at `u8::MAX`.
///
/// This is the K-04 entry point for a grapheme cluster
/// (`termai_vt::width::measure(cluster)`) and the production binding of
/// `termai_render::shape::CellWidthSource`.
#[must_use]
pub fn measure(scalars: &str) -> u8 {
    let mut columns: u16 = 0;
    for scalar in scalars.chars() {
        columns = columns.saturating_add(u16::from(measure_scalar(scalar)));
    }
    u8::try_from(columns).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Grid;

    /// One cluster per class K-04 has to decide: ASCII, CJK wide, combining mark, emoji / ZWJ
    /// sequence, variation selector and C0 control. The grid prints them one scalar at a time, so
    /// this corpus is also the drift detector for the grid/width coupling.
    const CORPUS: &[&str] = &[
        "A",
        "0",
        " ",
        "\u{4E2D}",
        "e\u{301}",
        "\u{301}",
        "\u{1F469}\u{200D}\u{1F4BB}",
        "\u{1F600}",
        "\u{FE0F}",
        "\t",
        "\u{1B}",
    ];

    #[test]
    fn a_scalar_measures_its_uax11_width() {
        assert_eq!(measure_scalar('A'), 1);
        assert_eq!(measure_scalar('0'), 1);
        assert_eq!(measure_scalar('\u{4E2D}'), 2);
        assert_eq!(measure_scalar('\u{301}'), 0);
        assert_eq!(measure_scalar('\u{FE0F}'), 0);
        assert_eq!(measure_scalar('\t'), 0);
        assert_eq!(measure_scalar('\u{1B}'), 0);
    }

    #[test]
    fn a_sequence_measures_the_sum_of_its_scalars() {
        assert_eq!(measure(""), 0);
        assert_eq!(measure("A"), 1);
        assert_eq!(measure("TermAI"), 6);
        assert_eq!(measure("\u{4E2D}\u{4E2D}"), 4);
        assert_eq!(measure("e\u{301}"), 1, "a combining mark adds no column");
        assert_eq!(measure("e\u{301}\u{301}"), 1);
        // The grid sees three scalars here, so the ZWJ sequence is 2 + 0 + 2 columns. Collapsing
        // it to one emoji would be a VT decision, not a width-table one (see the module doc).
        assert_eq!(measure("\u{1F469}\u{200D}\u{1F4BB}"), 4);
    }

    #[test]
    fn the_grid_prints_exactly_what_this_module_measures() {
        for cluster in CORPUS {
            // The grid's own decision: print a base scalar, then the cluster, and read how far
            // the cursor moved. A zero-width cluster moves it nowhere (it joins the base).
            let mut grid = Grid::new(200, 1);
            grid.print('x');
            for scalar in cluster.chars() {
                grid.print(scalar);
            }
            let consumed = grid.snapshot().cursor.pos.col - 1;
            assert_eq!(
                u16::from(measure(cluster)),
                consumed,
                "the grid and termai-vt::width::measure must decide the same columns for {cluster:?}"
            );
        }
    }
}
