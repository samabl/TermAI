//! S6/S7 seam: placing a rasterised glyph bitmap into its cell, and measuring RP-05's
//! `abs(glyph_bitmap_origin - cell_box_origin)`.
//!
//! Where this sits, and why (kernel/03 section 3.1)
//! ------------------------------------------------
//! kernel/03 splits the pipeline into **S6 shaping** (`crate::shape` -> [`RowGlyphs`]), **S7
//! atlas** (`crate::atlas` -> a coverage bitmap plus its ink box and pen bearing) and **S8
//! draw/submit** (`termai-gpu`, wgpu). The spec's per-cluster placement rule is section 3.5.1
//! step 6 - `逐 cluster 落位：glyph 原点 = cell_box 内居中`, *glyph origin = centered in the
//! cell_box* - and that rule cannot run inside S6: centering a glyph in its cell box needs the
//! **rasterised** ink box and the pen bearing, which only exist after S7. It cannot run inside
//! S8 either: that slice owns the GPU, while this arithmetic is pure CPU geometry that must be
//! testable with no adapter, and ADR-0027 D2 keeps the `termai-render -> termai-gpu` edge
//! unadmitted (`tools/kernel-gates/check.mjs` K4). The placement is therefore the **S6/S7 -> S8
//! handoff**, and it lives here next to the two stages whose outputs it joins. It consumes both
//! and introduces no shaping, no rasterisation and no width table of its own.
//!
//! Contract, authority, NON-GATING boundary
//! ----------------------------------------
//! The measurement contract is kernel/03 section 5 **RP-05** and its Open Question
//! **OQ-RND-04** (decided by **AR-24** clause 3):
//!
//! > `max over drawn glyphs of abs(glyph_bitmap_origin - cell_box_origin) <= 0.5px`, judged
//! > separately at 100 / 125 / 150 / 200% DPI (RM-C, golden pixel measurement).
//!
//! **kernel/03 RP-05 is the authority**: if this module and that document disagree, the document
//! wins, and kernel/06 section 3.2 owns the sampling methodology (AR-24 clause 3). Following
//! **ADR-0014** (and the same rule `termai-gpu::align` states for its half), every number this
//! module produces is **NON-GATING** unless it was produced on an **RM-C T0** backend with the
//! pinned reference driver. This mechanism has no GPU in its path at all: what it can prove is a
//! property of this arithmetic at four device scales, never a frame-level gate result.
//!
//! The two origins
//! ---------------
//! For one drawn glyph of one shaped row:
//!
//! * [`PlacedGlyph::cell_box_origin`] - the **ideal, fractional** device-pixel origin of the
//!   cell box the glyph draws into: `origin + (col, row) * (cell_w, cell_h)`, where the cell
//!   metrics are the font backend's `"0"` advance and AR-22 section 2's `1.25 x font size`,
//!   both multiplied by the device scale (kernel/03 section 3.5.3). At 125/150% these are
//!   genuinely fractional, which is the case AR-14's `<=0.5px` rule is about.
//! * [`PlacedGlyph::glyph_bitmap_origin`] - the **whole-pixel** device-pixel origin the
//!   placement anchors that cell's bitmap frame at: the nearest device pixel to the cell box
//!   origin. A device pixel grid is integral; a cell grid is not, so a placement that rounds to
//!   nearest is within half a pixel of the grid line and one that truncates is not. RP-05's
//!   `<=0.5px` is exactly that rule, made measurable.
//!
//! The glyph's own shape is applied **inside** that frame and reported separately, never mixed
//! into the deviation: [`PlacedGlyph::glyph_origin`] is the spec's glyph origin (the pen, on the
//! font's baseline - kernel/03 section 3.5.2 supplies ascent/descent - with the intra-cluster
//! advance of a ligature's second glyph included) and [`PlacedGlyph::bitmap_quad_origin`] is
//! where the coverage bytes actually start (`glyph_origin + (bearing.left, -bearing.top)`, S7's
//! placement, positive-up `top`). A bearing is a property of the glyph, not of the grid: folding
//! it into the measured pair would turn RP-05 into "how far does this glyph's ink sit from its
//! pen", which is not what section 3.5.3 or OQ-RND-04 name.
//!
//! What the number covers, and what it does not
//! -------------------------------------------
//! Covered: the sub-pixel error of the device-pixel snapping of **every drawn glyph's cell box**
//! at 100/125/150/200%, i.e. a wrong device scale, a wrong or drifted cell advance, a wrong cell
//! index, an off-by-one span and a truncating (`floor`/`as u16`) placement all push it past
//! 0.5px. Not covered: hinting, the driver's own pixel snapping, present-time/frame behaviour
//! (the rest of RP-05's judgement), the "Soft" AA preset's glyph-edge-centroid criterion
//! (OQ-RND-04's second half), and glyph shape/centering. The
//! `an_injected_misplacement_exceeds_the_contract_and_the_untouched_placement_does_not` test is
//! the control (plan section 6.3 rule 10) that keeps this from being an always-green number.
//!
//! Device scale and the atlas key (K-12)
//! -------------------------------------
//! [`AtlasKey`] is **not** extended by this slice. Its `px_size` is already the *device* pixel
//! size the rasteriser is driven with, so a DPI change is a different key by construction
//! (kernel/03 section 3.4 / K-12: `DPI 变化不做位图重采样，而是按新 scale 重建`), and two scales
//! that round to the same device size would key the identical bitmap the rasteriser would
//! produce anyway. K-12's `scale_q8` / `size_q6` stay the eventual home of the two-hot-scale
//! bookkeeping (RP-11) and of a *fractional* rasterisation size; nothing here needs them, and an
//! additive field with no consumer would be a second, silently divergent source of truth for
//! the same quantity.
//!
//! K-04: `termai-render` owns no column-width table, and neither does this module. The cell box
//! width is `cells * cell_w_px` where `cells` is the S6 span width, itself produced from
//! `termai_vt::width::measure`. [`place_row_with_vt_widths`] additionally re-reads every span's
//! columns from that same authority and refuses the row ([`PlaceError::ColumnAuthority`]) when
//! the span and the authority disagree - the cluster/column drift K-04 and RP-07 exist to catch.

use core::fmt;
use std::error::Error;

use crate::atlas::{AtlasKey, GlyphAtlas};
use crate::shape::{
    glyph_flag, CellWidthSource as _, ClusterSpan, RowClusters, RowGlyphs, ShapedGlyph, VT_WIDTH,
};

/// HARNESS section 5 "网格对齐误差": the contract this mechanism measures against, carried as
/// kernel/03 section 5 **RP-05** and defined by OQ-RND-04 / AR-24 clause 3.
pub const HARNESS_ALIGNMENT_CONTRACT_PX: f32 = 0.5;

/// The four device scales RP-05 judges separately (kernel/03 section 5). 1.0 = 100%.
pub const RP05_DPI_SCALES: [f32; 4] = [1.0, 1.25, 1.5, 2.0];

/// AR-22 section 2: the grid row height is a fixed `1.25 x font size`.
pub const LINE_HEIGHT_RATIO: f32 = 1.25;

/// A point in device pixels.
///
/// The cell box origin of a cell is fractional at 125/150%, the origin a bitmap frame is
/// anchored at is a whole pixel: both are this type, and the difference between them is what
/// RP-05 measures.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Point {
    /// Horizontal device pixel coordinate.
    pub x: f32,
    /// Vertical device pixel coordinate (y grows downwards, as everywhere in this crate).
    pub y: f32,
}

impl Point {
    /// A point at `(x, y)` device pixels.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// The same point moved by `(dx, dy)` device pixels.
    #[must_use]
    pub const fn shifted(self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }

    /// The worst-axis absolute deviation to `other`, in device pixels.
    ///
    /// RP-05 compares a point with a point and a device pixel is a square, so the deviation of a
    /// sample is the larger of its two axis errors (the Chebyshev distance), not their Euclidean
    /// norm: a 0.4px error on both axes is a 0.4px grid-alignment error, not 0.57px.
    #[must_use]
    pub fn deviation_px(self, other: Self) -> f32 {
        (self.x - other.x).abs().max((self.y - other.y).abs())
    }
}

/// The device-space cell grid a shaped row is placed against (kernel/03 section 3.5.3,
/// AR-22 section 2).
///
/// Held in **device** pixels as `f32`, never as integers: at 125/150% the cell width and height
/// are genuinely fractional, and an integer metric would silently hide every rounding defect
/// this module exists to measure. `scale_q8` is kernel/03 section 3.4 / K-12's device scale
/// (device pixels per logical pixel, times 256) and is metadata for the report - the
/// multiplication is already folded into the pixel fields, exactly as
/// `termai-gpu::align::GridGeometry` folds it into `cell_w`/`cell_h`.
///
/// `ascent_px` / `descent_px` are the row's **baseline**, which kernel/03 section 3.5.2 lists as
/// a font-backend metric (度量: advance / 行高 / 基线) rather than something this crate may
/// invent: the glyph origin of kernel/03 section 3.5.1 step 6 is a *pen* origin, so a placement
/// that centered each glyph's ink box vertically instead would give every glyph its own wobbling
/// baseline. The line box (`ascent + descent`) is centered in the row box and the baseline sits
/// `ascent` below its top, which is [AR-22 section 2](LINE_HEIGHT_RATIO)'s `1.25 x font size` row
/// with the glyphs centered in it (kernel/03 section 3.5.3).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CellMetrics {
    /// Device pixels per logical pixel, times 256 (kernel/03 section 3.4 `scale_q8`).
    pub scale_q8: u16,
    /// Device-pixel x of the grid's origin inside the frame (padding x scale).
    pub origin_x_px: f32,
    /// Device-pixel y of the grid's origin inside the frame (padding x scale).
    pub origin_y_px: f32,
    /// Cell width in device pixels: the font backend's `"0"` advance x scale (may be fractional).
    pub cell_w_px: f32,
    /// Cell height in device pixels: `1.25 x font size x scale` (may be fractional, AR-22 §2).
    pub cell_h_px: f32,
    /// Line-box top to baseline, device pixels (the font backend's ascent x scale).
    pub ascent_px: f32,
    /// Baseline to line-box bottom, device pixels (the font backend's descent x scale).
    pub descent_px: f32,
}

impl CellMetrics {
    /// Device metrics from the font backend's **logical** measurements and a device scale
    /// (kernel/03 sections 3.5.2 and 3.5.3): `cell_w = advance x scale`, `cell_h = 1.25 x font
    /// size x scale`, the baseline from the face's ascent/descent, grid origin at the frame
    /// origin.
    ///
    /// The scale is snapped into [`CellMetrics::scale_q8`] (kernel/03 section 3.4 / K-12's
    /// `scale_q8`), so a scale the fixed-point field cannot carry is refused rather than
    /// silently truncated.
    ///
    /// # Errors
    /// [`PlaceError::BadMetrics`] when a metric is non-finite or non-positive, when the line box
    /// is empty, or when `scale` does not fit `scale_q8`.
    pub fn from_logical(
        logical_advance_px: f32,
        logical_font_px: f32,
        logical_ascent_px: f32,
        logical_descent_px: f32,
        scale: f32,
    ) -> Result<Self, PlaceError> {
        let bad = || PlaceError::BadMetrics {
            cell_w_px: logical_advance_px * scale,
            cell_h_px: logical_font_px * LINE_HEIGHT_RATIO * scale,
            scale_q8: 0,
        };
        if !scale.is_finite() || scale <= 0.0 {
            return Err(bad());
        }
        let scale_q8 = (scale * 256.0).round();
        if !scale_q8.is_finite() || scale_q8 < 1.0 || scale_q8 > f32::from(u16::MAX) {
            return Err(bad());
        }
        let metrics = Self {
            scale_q8: scale_q8 as u16,
            origin_x_px: 0.0,
            origin_y_px: 0.0,
            cell_w_px: logical_advance_px * scale,
            cell_h_px: logical_font_px * LINE_HEIGHT_RATIO * scale,
            ascent_px: logical_ascent_px * scale,
            descent_px: logical_descent_px * scale,
        };
        metrics.validate()?;
        Ok(metrics)
    }

    /// The same metrics with the grid placed at `(x, y)` device pixels inside the frame.
    #[must_use]
    pub const fn with_origin(mut self, x_px: f32, y_px: f32) -> Self {
        self.origin_x_px = x_px;
        self.origin_y_px = y_px;
        self
    }

    /// The device scale as a factor (100% = 1.0).
    #[must_use]
    pub fn scale(self) -> f32 {
        f32::from(self.scale_q8) / 256.0
    }

    /// The ideal, fractional device-pixel origin of the cell at `(col, row)`.
    #[must_use]
    pub fn cell_box_origin(self, col: u16, row: u16) -> Point {
        Point::new(
            self.origin_x_px + f32::from(col) * self.cell_w_px,
            self.origin_y_px + f32::from(row) * self.cell_h_px,
        )
    }

    /// Width in device pixels of a cell box `cells` columns wide.
    #[must_use]
    pub fn cell_box_width(self, cells: u8) -> f32 {
        f32::from(cells) * self.cell_w_px
    }

    /// Height of the face's line box in device pixels (`ascent + descent`).
    #[must_use]
    pub fn line_box_height_px(self) -> f32 {
        self.ascent_px + self.descent_px
    }

    /// Cell box top to baseline, in device pixels: the line box centered in the row box with the
    /// pen `ascent` below the line box top (kernel/03 section 3.5.3: the row is `1.25 x font size`
    /// and the glyph is centered in it).
    #[must_use]
    pub fn baseline_offset_px(self) -> f32 {
        (self.cell_h_px - self.line_box_height_px()) * 0.5 + self.ascent_px
    }

    /// Whether these metrics can produce a cell box and a baseline at all.
    ///
    /// # Errors
    /// [`PlaceError::BadMetrics`] for a non-finite or non-positive cell size, a negative or
    /// non-finite ascent/descent, or an empty line box.
    pub fn validate(self) -> Result<(), PlaceError> {
        let finite = self.cell_w_px.is_finite()
            && self.cell_h_px.is_finite()
            && self.origin_x_px.is_finite()
            && self.origin_y_px.is_finite()
            && self.ascent_px.is_finite()
            && self.descent_px.is_finite();
        if !finite
            || self.cell_w_px <= 0.0
            || self.cell_h_px <= 0.0
            || self.ascent_px < 0.0
            || self.descent_px < 0.0
            || self.line_box_height_px() <= 0.0
        {
            return Err(PlaceError::BadMetrics {
                cell_w_px: self.cell_w_px,
                cell_h_px: self.cell_h_px,
                scale_q8: self.scale_q8,
            });
        }
        Ok(())
    }
}

/// Why a row could not be placed.
#[derive(Clone, PartialEq, Debug)]
pub enum PlaceError {
    /// The cell metrics cannot produce a cell box (non-finite or non-positive size).
    ///
    /// A row placed against such metrics would produce plausible-looking origins out of
    /// nonsense, so it is refused instead.
    BadMetrics {
        /// The offending cell width.
        cell_w_px: f32,
        /// The offending cell height.
        cell_h_px: f32,
        /// The device scale the metrics carried.
        scale_q8: u16,
    },
    /// A span's cell columns disagree with the K-04 width authority
    /// (`termai_vt::width::measure`, kernel/03 section 3.5.1 step 1 / RP-07).
    ///
    /// This is the cluster/column drift K-04 exists to catch: placing a glyph into a cell box
    /// the grid did not give it is a wrong picture behind a right-looking origin, so the row is
    /// refused rather than measured.
    ColumnAuthority {
        /// Cluster span index inside the row.
        index: usize,
        /// Where the S6 span said the cluster starts.
        span_start: u16,
        /// How many columns the S6 span claimed.
        span_cells: u8,
        /// Where the K-04 authority places the cluster's first column.
        measured_start: u16,
        /// How many columns the K-04 authority measures for the cluster.
        measured_cells: u16,
    },
}

impl fmt::Display for PlaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMetrics {
                cell_w_px,
                cell_h_px,
                scale_q8,
            } => write!(
                f,
                "cell metrics {cell_w_px}x{cell_h_px}px at scale_q8 {scale_q8} cannot produce a \
                 cell box"
            ),
            Self::ColumnAuthority {
                index,
                span_start,
                span_cells,
                measured_start,
                measured_cells,
            } => write!(
                f,
                "span {index} claims cells {span_start}..{} but the termai-vt width authority \
                 measures {measured_start}..{} (K-04)",
                u16::from(*span_cells) + *span_start,
                *measured_start + *measured_cells
            ),
        }
    }
}

impl Error for PlaceError {}

/// A drawn glyph the placement cannot judge. Never a silent zero: every glyph of every span ends
/// up either in [`PlacedRow::glyphs`] or in [`PlacedRow::refusals`], and the two counts always
/// add up to the row's glyph count.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GlyphRefusal {
    /// The atlas holds no bitmap for this glyph's key: the caller never rasterised it, its page
    /// was evicted, or the rasteriser found no outline for it (S7's [`crate::atlas::AtlasError`]
    /// refusals). There is no ink box and no bearing, so there is no origin to judge.
    MissingBitmap {
        /// The glyph the row asked for.
        glyph_id: u32,
        /// The key that had no bitmap.
        key: AtlasKey,
    },
    /// The glyph is a COLR/CBDT colour glyph (kernel/03 section 3.4, DC-17): it belongs on the
    /// RGBA8 colour page this pipeline does not own, so its origin is not this mechanism's to
    /// measure (AR-14 promises no subpixel AA for it either).
    ColorGlyph {
        /// The colour glyph id.
        glyph_id: u32,
    },
    /// The resolved face does not cover the cluster: S6 reported `.notdef` (glyph id 0,
    /// `glyph_flag::MISSING`). The `.notdef` box is the font's statement that it has no glyph,
    /// not the cluster's glyph, so measuring its origin as if it were the cluster's would be a
    /// right-looking number about the wrong thing.
    UncoveredCluster {
        /// The `.notdef` glyph id (always 0).
        glyph_id: u32,
    },
    /// The atlas bitmap exists but has no ink box at all (a blank glyph such as U+0020, whose
    /// rasterisation the atlas refuses with `AtlasError::NoOutline` in the first place). Nothing
    /// is drawn, so nothing can be misaligned - but it is reported rather than dropped, so a
    /// sample count can never quietly shrink.
    NoInk {
        /// The blank glyph.
        glyph_id: u32,
        /// The key that had no ink.
        key: AtlasKey,
    },
}

impl fmt::Display for GlyphRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBitmap { glyph_id, key } => write!(
                f,
                "glyph {glyph_id} has no bitmap at (font {}, {}px, {:?})",
                key.font_id, key.px_size, key.aa_mode
            ),
            Self::ColorGlyph { glyph_id } => {
                write!(f, "glyph {glyph_id} is a colour glyph (RGBA8 page)")
            }
            Self::UncoveredCluster { glyph_id } => {
                write!(
                    f,
                    "the face has no glyph for this cluster (.notdef {glyph_id})"
                )
            }
            Self::NoInk { glyph_id, key } => write!(
                f,
                "glyph {glyph_id} rasterised to an empty ink box at {}px",
                key.px_size
            ),
        }
    }
}

/// One drawn glyph, placed: the two origins RP-05 names plus the arithmetic that produced them.
#[derive(Clone, PartialEq, Debug)]
pub struct PlacedGlyph {
    /// Cluster span index inside the row.
    pub cluster_index: usize,
    /// Glyph index inside that cluster's run.
    pub glyph_index: usize,
    /// Font glyph id (kernel/03 section 3.4 / K-12: never a codepoint).
    pub glyph_id: u32,
    /// Byte offset of the glyph's cluster inside the cluster text (S6).
    pub cluster_offset: u16,
    /// First cell the cluster covers (inclusive).
    pub start_cell: u16,
    /// One past the last cell the cluster covers.
    pub end_cell: u16,
    /// Columns the cluster covers, from the K-04 authority by way of S6.
    pub cells: u8,
    /// S6 `glyph_flag` bits, as shaped.
    pub flags: u16,
    /// The atlas key this glyph was looked up under (the fitted device pixel size, see
    /// [`placement_key`]).
    pub key: AtlasKey,
    /// The S6 `fit_scale` recorded for this glyph's cluster (1.0 = untouched).
    pub fit_scale: f32,
    /// Ink box width in device pixels (S7's rasterised placement).
    pub ink_width: u16,
    /// Ink box height in device pixels.
    pub ink_height: u16,
    /// Pen origin to ink-box left edge, device pixels (S7 bearing `left`).
    pub bearing_left: i16,
    /// Pen origin to ink-box top edge, device pixels, positive up (S7 bearing `top`).
    pub bearing_top: i16,
    /// The ideal, fractional device-pixel origin of the cell box (RP-05's denominator).
    pub cell_box_origin: Point,
    /// The spec's glyph origin: the whole-pixel pen origin, centered in the cell box
    /// (kernel/03 section 3.5.1 step 6) and advanced by the preceding glyphs of the cluster.
    pub glyph_origin: Point,
    /// The whole-pixel device-pixel origin the cell's bitmap frame is anchored at: the nearest
    /// device pixel to [`PlacedGlyph::cell_box_origin`] (RP-05's numerator).
    pub glyph_bitmap_origin: Point,
    /// Where the coverage bytes actually start: `glyph_origin + (left, -top)`.
    ///
    /// Deliberately **not** RP-05's numerator: it carries the glyph's own bearing, i.e. glyph
    /// shape, which section 3.5.3's alignment rule does not judge.
    pub bitmap_quad_origin: Point,
}

impl PlacedGlyph {
    /// `abs(glyph_bitmap_origin - cell_box_origin)` in device pixels (RP-05's sample value).
    #[must_use]
    pub fn deviation_px(&self) -> f32 {
        self.glyph_bitmap_origin.deviation_px(self.cell_box_origin)
    }
}

/// One shaped row, placed: every drawn glyph with its two origins, every refused glyph with a
/// reason.
#[derive(Clone, PartialEq, Debug)]
pub struct PlacedRow {
    /// Viewport row this belongs to.
    pub row: u16,
    /// Columns covered by the row's last span (S6).
    pub covered_cells: u16,
    /// The device grid the row was placed against.
    pub metrics: CellMetrics,
    /// Drawn glyphs, in span order then glyph order - the RP-05 sample set.
    pub glyphs: Vec<PlacedGlyph>,
    /// Glyphs the placement refused to judge, with the reason.
    pub refusals: Vec<GlyphRefusal>,
}

/// The atlas key the placement looks a glyph up under.
///
/// This is S6's device pixel size with S6's `fit_scale` applied and rounded to the whole device
/// pixel [`AtlasKey::px_size`] can carry (kernel/03 section 3.5.1 step 6: an over-wide glyph is
/// scaled down to its cell box, and `ClusterSpan` records "S7 rasterises the outline at this
/// scale"). A caller rasterises exactly the keys [`placement_keys`] returns, then places.
#[must_use]
pub fn placement_key(row: &RowGlyphs, span: &ClusterSpan, glyph: &ShapedGlyph) -> AtlasKey {
    AtlasKey {
        font_id: row.font_id,
        glyph_id: glyph.glyph_id,
        px_size: fitted_px_size(row, span),
        aa_mode: row.aa,
    }
}

/// The keys the placement will look up for a row, in deterministic span/glyph order, deduplicated.
///
/// Glyphs the placement refuses without the atlas (colour glyphs, `.notdef`) are excluded, so
/// this is exactly the set a caller must rasterise before calling [`place_row`].
#[must_use]
pub fn placement_keys(row: &RowGlyphs) -> Vec<AtlasKey> {
    let mut keys: Vec<AtlasKey> = Vec::new();
    for span in &row.spans {
        for glyph in &span.glyphs {
            if refused_without_atlas(glyph) {
                continue;
            }
            let key = placement_key(row, span, glyph);
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    keys
}

/// The device pixel size S6's `fit_scale` asks the rasteriser for, as a whole device pixel.
///
/// `fit_scale` is a ratio; a `u16` pixel size can only carry its rounded product. A glyph whose
/// fitted size rounds back to the row's size therefore reuses the row's bitmap - which is
/// correct, because the rasteriser would produce that very bitmap.
fn fitted_px_size(row: &RowGlyphs, span: &ClusterSpan) -> u16 {
    let scaled = (f32::from(row.px_size) * span.fit_scale).round();
    if !scaled.is_finite() || scaled < 1.0 {
        return 1;
    }
    scaled.min(f32::from(u16::MAX)) as u16
}

/// Whether the placement refuses this glyph without consulting the atlas at all.
fn refused_without_atlas(glyph: &ShapedGlyph) -> bool {
    glyph.glyph_id == 0
        || glyph.flags & glyph_flag::MISSING != 0
        || glyph.flags & glyph_flag::COLOR != 0
}

/// Place a shaped row into its cells against the device grid.
///
/// Every drawn glyph gets its [`PlacedGlyph::cell_box_origin`] and
/// [`PlacedGlyph::glyph_bitmap_origin`]; every glyph that cannot be judged is reported in
/// [`PlacedRow::refusals`] instead of silently vanishing from the sample set. The row's cell
/// columns are taken from the S6 spans, i.e. from the K-04 authority that produced them.
///
/// # Errors
/// [`PlaceError::BadMetrics`] when the grid cannot produce a cell box. (The K-04 cross-check of
/// [`place_row_with_vt_widths`] is not part of this entry point because it needs the row's
/// clusters; a caller that has them should use that function.)
pub fn place_row(
    row: &RowGlyphs,
    metrics: &CellMetrics,
    atlas: &GlyphAtlas,
) -> Result<PlacedRow, PlaceError> {
    place_inner(row, None, metrics, atlas)
}

/// [`place_row`] with the **production** K-04 binding: every span's columns are re-read from
/// `termai_vt::width::measure` (kernel/03 K-04 / section 3.5.1 step 1) and the row is refused if
/// the span and the authority disagree ([`PlaceError::ColumnAuthority`]).
///
/// `clusters` is the same S6 input the row was shaped from. The zero-width fold of kernel/03
/// section 3.5.1 step 6 is honoured: a combining mark or a variation selector measures 0 columns
/// and belongs to the cluster before it, exactly as S6 grouped it.
///
/// # Errors
/// As [`place_row`], plus [`PlaceError::ColumnAuthority`].
pub fn place_row_with_vt_widths(
    row: &RowGlyphs,
    clusters: &RowClusters,
    metrics: &CellMetrics,
    atlas: &GlyphAtlas,
) -> Result<PlacedRow, PlaceError> {
    place_inner(row, Some(clusters), metrics, atlas)
}

/// The placement itself, with the optional K-04 cross-check.
fn place_inner(
    row: &RowGlyphs,
    clusters: Option<&RowClusters>,
    metrics: &CellMetrics,
    atlas: &GlyphAtlas,
) -> Result<PlacedRow, PlaceError> {
    metrics.validate()?;
    if let Some(clusters) = clusters {
        verify_columns(row, clusters)?;
    }
    let mut glyphs: Vec<PlacedGlyph> = Vec::new();
    let mut refusals: Vec<GlyphRefusal> = Vec::new();
    for (cluster_index, span) in row.spans.iter().enumerate() {
        let cell_box_origin = metrics.cell_box_origin(span.start_cell, row.row);
        // RP-05's numerator: the nearest device pixel to the grid line. `f32::round` is
        // round-half-away-from-zero, i.e. never worse than 0.5px from the ideal cell origin.
        let glyph_bitmap_origin = Point::new(cell_box_origin.x.round(), cell_box_origin.y.round());
        // The spec's glyph origin (kernel/03 section 3.5.1 step 6): the pen origin, centered in
        // the cell box. Horizontally the cluster's fitted advance is centered in the columns it
        // owns; vertically every glyph of the row shares the font's baseline (kernel/03 section
        // 3.5.2), so the line box is centered in the row box and the pen sits on it.
        let baseline_y = cell_box_origin.y + metrics.baseline_offset_px();
        let mut pen_x =
            cell_box_origin.x + (metrics.cell_box_width(span.cells) - span.advance_px) * 0.5;
        for (glyph_index, glyph) in span.glyphs.iter().enumerate() {
            let glyph_origin = Point::new(pen_x, baseline_y);
            pen_x += glyph.advance_px;
            if glyph.glyph_id == 0 || glyph.flags & glyph_flag::MISSING != 0 {
                refusals.push(GlyphRefusal::UncoveredCluster {
                    glyph_id: glyph.glyph_id,
                });
                continue;
            }
            if glyph.flags & glyph_flag::COLOR != 0 {
                refusals.push(GlyphRefusal::ColorGlyph {
                    glyph_id: glyph.glyph_id,
                });
                continue;
            }
            let key = placement_key(row, span, glyph);
            let Some(bitmap) = atlas.bitmap(&key) else {
                refusals.push(GlyphRefusal::MissingBitmap {
                    glyph_id: glyph.glyph_id,
                    key,
                });
                continue;
            };
            if bitmap.is_empty() {
                refusals.push(GlyphRefusal::NoInk {
                    glyph_id: glyph.glyph_id,
                    key,
                });
                continue;
            }
            let bitmap_quad_origin = Point::new(
                (glyph_origin.x + f32::from(bitmap.left)).round(),
                (glyph_origin.y - f32::from(bitmap.top)).round(),
            );
            glyphs.push(PlacedGlyph {
                cluster_index,
                glyph_index,
                glyph_id: glyph.glyph_id,
                cluster_offset: glyph.cluster_offset,
                start_cell: span.start_cell,
                end_cell: span.end_cell,
                cells: span.cells,
                flags: glyph.flags,
                key,
                fit_scale: span.fit_scale,
                ink_width: bitmap.width,
                ink_height: bitmap.height,
                bearing_left: bitmap.left,
                bearing_top: bitmap.top,
                cell_box_origin,
                glyph_origin,
                glyph_bitmap_origin,
                bitmap_quad_origin,
            });
        }
    }
    Ok(PlacedRow {
        row: row.row,
        covered_cells: row.covered_cells,
        metrics: *metrics,
        glyphs,
        refusals,
    })
}

/// Re-read every span's columns from the K-04 width authority and refuse the first disagreement
/// (kernel/03 section 3.5.1 step 1, RP-07).
fn verify_columns(row: &RowGlyphs, clusters: &RowClusters) -> Result<(), PlaceError> {
    let mut cursor = 0usize;
    for (index, span) in row.spans.iter().enumerate() {
        let mut measured_cells = 0u16;
        let mut measured_start: Option<u16> = None;
        while let Some(cluster) = clusters.clusters.get(cursor) {
            let cells = u16::from(VT_WIDTH.cluster_columns(&cluster.text));
            // A cluster belongs to this span when its anchor is inside the span, or when it is a
            // zero-width cluster sitting exactly on the span's end - the combining mark S6 folds
            // into the cluster before it (kernel/03 section 3.5.1 steps 2-3 and 6).
            let belongs =
                cluster.col < span.end_cell || (cluster.col == span.end_cell && cells == 0);
            if !belongs {
                break;
            }
            if cells > 0 {
                if measured_start.is_none() {
                    measured_start = Some(cluster.col);
                }
                measured_cells = measured_cells.saturating_add(cells);
            }
            cursor += 1;
        }
        if measured_start != Some(span.start_cell) || measured_cells != u16::from(span.cells) {
            return Err(PlaceError::ColumnAuthority {
                index,
                span_start: span.start_cell,
                span_cells: span.cells,
                measured_start: measured_start.unwrap_or(span.start_cell),
                measured_cells,
            });
        }
    }
    Ok(())
}

/// Why a placement could not be measured.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MeasureError {
    /// The row has no drawn glyph at all, so a "worst deviation" would be a silent zero over an
    /// empty sample set. RP-05 is never judged on zero samples: the caller must place glyphs
    /// (and rasterise their bitmaps) or report the refusals instead.
    NoDrawnGlyphs {
        /// How many glyphs were refused (the reason is in [`PlacedRow::refusals`]).
        refusals: usize,
    },
}

impl fmt::Display for MeasureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDrawnGlyphs { refusals } => write!(
                f,
                "the placement has no drawn glyph to measure ({refusals} refused): RP-05 must \
                 never be judged on zero samples"
            ),
        }
    }
}

impl Error for MeasureError {}

/// The RP-05 result: the worst deviation over the drawn glyphs and how many glyphs produced it.
///
/// The vocabulary mirrors `termai-gpu::align::GridAlignment` (worst value, sample count, the
/// worst sample's coordinates, the cell geometry, the contract and an `EXCEEDED` marker) on
/// purpose: the two halves of RP-05 - that module's geometry and this module's glyph placement -
/// are reported in one shape so a reader can compare them. It is a separate type rather than a
/// shared one because the `termai-render -> termai-gpu` edge is unadmitted (ADR-0027 D2, gate
/// K4): the *shape* is copied, no dependency is created.
///
/// **NON-GATING** off an RM-C T0 backend with the pinned reference driver (ADR-0014). kernel/03
/// RP-05 is the contract authority.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GridAlignment {
    /// Worst `abs(glyph_bitmap_origin - cell_box_origin)` over the drawn glyphs, in device px.
    pub worst_px: f32,
    /// Number of glyph samples behind `worst_px` (one per drawn glyph, never zero).
    pub samples: usize,
    /// Glyph id of the worst sample.
    pub worst_glyph_id: u32,
    /// Cluster span index of the worst sample.
    pub worst_cluster: usize,
    /// First cell of the worst sample's cluster.
    pub worst_cell: u16,
    /// Viewport row the samples came from.
    pub row: u16,
    /// Cell width the samples were placed against.
    pub cell_w_px: f32,
    /// Cell height the samples were placed against.
    pub cell_h_px: f32,
    /// Device scale the samples were placed at, times 256 (kernel/03 section 3.4 `scale_q8`).
    pub scale_q8: u16,
    /// Glyphs the placement refused to judge (see [`PlacedRow::refusals`]).
    pub refusals: usize,
}

impl GridAlignment {
    /// The device scale as a factor (100% = 1.0).
    #[must_use]
    pub fn dpi_scale(self) -> f32 {
        f32::from(self.scale_q8) / 256.0
    }

    /// Whether this measurement is inside kernel/03 RP-05's contract.
    #[must_use]
    pub fn within_contract(self) -> bool {
        self.worst_px <= HARNESS_ALIGNMENT_CONTRACT_PX
    }
}

impl fmt::Display for GridAlignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "grid alignment (RP-05): worst {:.4}px over {} glyph sample(s) (row {}, cell {}, \
             cluster {}, glyph {}; cell {:.4}x{:.4}px, {:.0}% DPI, {} refusal(s); contract \
             {:.1}px{})",
            self.worst_px,
            self.samples,
            self.row,
            self.worst_cell,
            self.worst_cluster,
            self.worst_glyph_id,
            self.cell_w_px,
            self.cell_h_px,
            self.dpi_scale() * 100.0,
            self.refusals,
            HARNESS_ALIGNMENT_CONTRACT_PX,
            if self.within_contract() {
                ""
            } else {
                " EXCEEDED"
            }
        )
    }
}

/// Measure RP-05's `max over drawn glyphs of abs(glyph_bitmap_origin - cell_box_origin)`.
///
/// `samples` is the number of drawn glyphs that went into `worst_px`, so a measurement can never
/// be mistaken for a clean one over nothing.
///
/// # Errors
/// [`MeasureError::NoDrawnGlyphs`] when the placement holds no drawn glyph at all.
pub fn measure_placement(placed: &PlacedRow) -> Result<GridAlignment, MeasureError> {
    let Some(first) = placed.glyphs.first() else {
        return Err(MeasureError::NoDrawnGlyphs {
            refusals: placed.refusals.len(),
        });
    };
    let mut alignment = GridAlignment {
        worst_px: first.deviation_px(),
        samples: 0,
        worst_glyph_id: first.glyph_id,
        worst_cluster: first.cluster_index,
        worst_cell: first.start_cell,
        row: placed.row,
        cell_w_px: placed.metrics.cell_w_px,
        cell_h_px: placed.metrics.cell_h_px,
        scale_q8: placed.metrics.scale_q8,
        refusals: placed.refusals.len(),
    };
    for glyph in &placed.glyphs {
        let deviation = glyph.deviation_px();
        alignment.samples += 1;
        if deviation > alignment.worst_px {
            alignment.worst_px = deviation;
            alignment.worst_glyph_id = glyph.glyph_id;
            alignment.worst_cluster = glyph.cluster_index;
            alignment.worst_cell = glyph.start_cell;
        }
    }
    Ok(alignment)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(cell_w: f32, cell_h: f32) -> CellMetrics {
        CellMetrics {
            scale_q8: 320,
            origin_x_px: 0.0,
            origin_y_px: 0.0,
            cell_w_px: cell_w,
            cell_h_px: cell_h,
            ascent_px: 13.0,
            descent_px: 3.25,
        }
    }

    fn glyph(glyph_id: u32, cell_box_origin: Point, deviation: f32) -> PlacedGlyph {
        let key = AtlasKey {
            font_id: crate::shape::FontId::dummy(),
            glyph_id,
            px_size: 16,
            aa_mode: crate::shape::AaMode::Sharp,
        };
        PlacedGlyph {
            cluster_index: 0,
            glyph_index: 0,
            glyph_id,
            cluster_offset: 0,
            start_cell: 0,
            end_cell: 1,
            cells: 1,
            flags: 0,
            key,
            fit_scale: 1.0,
            ink_width: 1,
            ink_height: 1,
            bearing_left: 0,
            bearing_top: 1,
            cell_box_origin,
            glyph_origin: cell_box_origin,
            glyph_bitmap_origin: Point::new(cell_box_origin.x + deviation, cell_box_origin.y),
            bitmap_quad_origin: cell_box_origin,
        }
    }

    fn placed(glyphs: Vec<PlacedGlyph>) -> PlacedRow {
        PlacedRow {
            row: 0,
            covered_cells: 1,
            metrics: metrics(7.5, 16.25),
            glyphs,
            refusals: Vec::new(),
        }
    }

    #[test]
    fn a_fractional_cell_origin_rounds_to_the_nearest_device_pixel() {
        let grid = metrics(7.5, 16.25);
        assert_eq!(grid.cell_box_origin(0, 0), Point::new(0.0, 0.0));
        assert_eq!(grid.cell_box_origin(3, 2), Point::new(22.5, 32.5));
        assert_eq!(grid.cell_box_origin(3, 2).x.round(), 23.0);
        assert_eq!(grid.cell_box_origin(3, 2).y.round(), 33.0);
        assert!(
            (grid
                .cell_box_origin(3, 2)
                .deviation_px(Point::new(23.0, 33.0))
                - 0.5)
                .abs()
                < 1.0e-6
        );
        assert_eq!(grid.scale(), 1.25);
        assert_eq!(grid.cell_box_width(2), 15.0);
        // The grid's origin inside the frame shifts every cell box with it, and the deviation of a
        // whole-pixel placement stays bounded by half a pixel there too.
        let shifted = grid.with_origin(1.5, 2.5);
        assert_eq!(shifted.cell_box_origin(0, 0), Point::new(1.5, 2.5));
        assert_eq!(shifted.cell_box_origin(3, 2), Point::new(24.0, 35.0));
        let shifted_cell = shifted.cell_box_origin(1, 1);
        assert_eq!(shifted_cell, Point::new(9.0, 18.75));
        assert!(shifted_cell.deviation_px(Point::new(9.0, 19.0)) <= 0.5);
    }

    #[test]
    fn the_deviation_is_the_worst_axis_and_the_worst_glyph_wins() {
        let row = placed(vec![
            glyph(1, Point::new(10.0, 20.0), 0.1),
            // 0.4px on x and 0.45px on y: the sample value is 0.45, not the Euclidean norm.
            glyph(2, Point::new(10.0, 20.0), 0.4),
            glyph(3, Point::new(0.0, 0.0), 0.25),
        ]);
        let alignment = measure_placement(&row).expect("three drawn glyphs");
        assert_eq!(alignment.samples, 3);
        assert!((alignment.worst_px - 0.4).abs() < 1.0e-6);
        assert_eq!(alignment.worst_glyph_id, 2);
        assert!(alignment.within_contract());
        assert!(measure_placement(&row)
            .unwrap()
            .to_string()
            .contains("0.4000px"));
    }

    #[test]
    fn a_measurement_over_no_drawn_glyph_is_refused_and_the_exceeded_marker_is_reachable() {
        let empty = placed(Vec::new());
        assert_eq!(
            measure_placement(&empty),
            Err(MeasureError::NoDrawnGlyphs { refusals: 0 })
        );
        let mut row = placed(vec![glyph(1, Point::new(0.0, 0.0), 0.75)]);
        row.refusals
            .push(GlyphRefusal::UncoveredCluster { glyph_id: 0 });
        let alignment = measure_placement(&row).expect("one drawn glyph");
        assert_eq!(alignment.refusals, 1);
        assert!(!alignment.within_contract());
        assert!(alignment.to_string().ends_with("EXCEEDED)"));
    }

    fn span(start_cell: u16, end_cell: u16, cells: u8) -> ClusterSpan {
        ClusterSpan {
            start_cell,
            end_cell,
            cells,
            advance_px: 8.0,
            fit_scale: 1.0,
            flags: 0,
            glyphs: vec![ShapedGlyph {
                glyph_id: 36,
                advance_px: 8.0,
                cluster_offset: 0,
                flags: 0,
            }],
        }
    }

    fn row_with_spans(spans: Vec<ClusterSpan>) -> RowGlyphs {
        RowGlyphs {
            row: 0,
            font_id: crate::shape::FontId::dummy(),
            px_size: 16,
            aa: crate::shape::AaMode::Sharp,
            covered_cells: spans.last().map_or(0, |span| span.end_cell),
            spans,
            fit_squeezed: 0,
            missing_glyphs: 0,
        }
    }

    #[test]
    fn a_span_the_width_authority_disagrees_with_is_refused() {
        use crate::shape::ClusterInput;

        // The K-04 authority is `termai_vt::width::measure`, so this check needs no font at all:
        // U+4E2D measures two columns, and a span that claims one must never be placed - that is
        // the cluster/column drift K-04/RP-07 exist to catch.
        let clusters = RowClusters::new(
            0,
            0,
            vec![
                ClusterInput {
                    col: 0,
                    text: "A".to_owned(),
                },
                ClusterInput {
                    col: 1,
                    text: "\u{4E2D}".to_owned(),
                },
            ],
        );
        let control = row_with_spans(vec![span(0, 1, 1), span(1, 3, 2)]);
        assert_eq!(verify_columns(&control, &clusters), Ok(()));
        let drifted = row_with_spans(vec![span(0, 1, 1), span(1, 2, 1)]);
        assert_eq!(
            verify_columns(&drifted, &clusters),
            Err(PlaceError::ColumnAuthority {
                index: 1,
                span_start: 1,
                span_cells: 1,
                measured_start: 1,
                measured_cells: 2,
            })
        );
        assert!(verify_columns(&drifted, &clusters)
            .unwrap_err()
            .to_string()
            .contains("termai-vt width authority"));

        // The zero-width fold of kernel/03 section 3.5.1 step 6: a combining mark measures zero
        // columns, sits on the span's end cell and belongs to the cluster before it.
        let folded = RowClusters::new(
            0,
            0,
            vec![
                ClusterInput {
                    col: 0,
                    text: "e".to_owned(),
                },
                ClusterInput {
                    col: 1,
                    text: "\u{301}".to_owned(),
                },
            ],
        );
        assert_eq!(
            verify_columns(&row_with_spans(vec![span(0, 1, 1)]), &folded),
            Ok(())
        );
    }

    #[test]
    fn metrics_that_cannot_produce_a_cell_box_are_refused() {
        assert_eq!(
            CellMetrics::from_logical(8.0, 16.0, 12.0, 4.0, 0.0),
            Err(PlaceError::BadMetrics {
                cell_w_px: 0.0,
                cell_h_px: 0.0,
                scale_q8: 0,
            })
        );
        assert!(CellMetrics::from_logical(f32::NAN, 16.0, 12.0, 4.0, 1.0).is_err());
        // A face whose ascent and descent are both zero has no baseline to place a pen on.
        assert!(CellMetrics::from_logical(8.0, 16.0, 0.0, 0.0, 1.0).is_err());
        assert!(CellMetrics::from_logical(8.0, 16.0, -1.0, 4.0, 1.0).is_err());
        assert_eq!(
            metrics(0.0, 16.0).validate(),
            Err(PlaceError::BadMetrics {
                cell_w_px: 0.0,
                cell_h_px: 16.0,
                scale_q8: 320,
            })
        );
        assert!(metrics(7.5, 16.25).validate().is_ok());
    }

    #[test]
    fn the_baseline_centers_the_line_box_in_the_row_box() {
        // 16.25px row box with a 13/3.25px line box: the line box exactly fills the row box, so the
        // pen sits 13px below the cell box top on every glyph of the row.
        let grid = metrics(7.5, 16.25);
        assert_eq!(grid.line_box_height_px(), 16.25);
        assert_eq!(grid.baseline_offset_px(), 13.0);
        // The formula is what it is for a line box taller than the row box too; keeping such a face
        // inside its cells is the fit of kernel/03 section 3.5.1 step 6, not this arithmetic.
        let tall = CellMetrics {
            ascent_px: 20.0,
            descent_px: 5.0,
            ..grid
        };
        assert_eq!(tall.baseline_offset_px(), (16.25 - 25.0) * 0.5 + 20.0);
        // The device metrics of the production constructor scale ascent and descent with the grid.
        let scaled = CellMetrics::from_logical(6.25, 12.5, 10.0, 2.5, 1.25)
            .expect("the scaled probe metrics are valid");
        assert_eq!(scaled.cell_w_px, 7.8125);
        assert_eq!(scaled.ascent_px, 12.5);
        assert_eq!(scaled.descent_px, 3.125);
        assert_eq!(
            scaled.baseline_offset_px(),
            (19.53125 - 15.625) * 0.5 + 12.5
        );
    }
}
