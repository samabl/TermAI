//! S6 shaping: a row of clusters becomes per-cell glyph spans (kernel/03 section 3.1 S6).
//!
//! Input = a row of clusters (the S5 mirror's materialised row) plus a font/DPI/anti-aliasing
//! context; output = [`RowGlyphs`], where every cluster owns the half-open cell span
//! `[start_cell, end_cell)` it draws into, together with what to draw there.
//!
//! Engines (kernel/03 K-05, admitted by ADR-0027 D1): `rustybuzz` is the only OpenType
//! shaping engine and `swash` the only outline/colour-glyph engine. This module extracts
//! outlines and metrics only - it rasterises nothing, allocates no atlas page and produces no
//! pixels. That is S7/S8 and belongs to the GPU slice.
//!
//! K-04: `termai-render` owns **no** column-width table. A cluster's cell width comes from
//! the injected [`CellWidthSource`] port, whose production binding is [`VtWidthSource`] -
//! `termai_vt::width::measure`, i.e. the very rule `termai_vt::Grid::print` applies. The crate
//! edge `termai-render -> termai-vt` is registered by ADR-0027 D2, so nothing here may classify
//! a scalar itself. The shaper also refuses to run when the injected widths disagree
//! with the row's own grid anchors ([`ShapeError::ColumnMismatch`]): that mismatch is the
//! cluster/column drift K-04 exists to catch (kernel/03 RP-07).
//!
//! Determinism: shaping is a pure function of (cluster text, font bytes, px size, AA mode,
//! ligature flag, width source). Same input plus the same font yields byte-identical output -
//! see [`RowGlyphs::canonical_bytes`], which is what the RP-08/RP-10 style golden checks need.

use std::fmt;
use std::ops::Range;

use rustybuzz::ttf_parser::Tag;
use rustybuzz::{Direction, Face, Feature, UnicodeBuffer};

pub use fontdb::ID as FontId;

use crate::atlas::AtlasKey;

/// Fit tolerance in device pixels: a rounding-level overhang is not a squeeze.
const FIT_EPSILON_PX: f32 = 1.0e-3;

/// Kernel/03 section 3.4 anti-aliasing preset. AR-14 admits exactly two, both grayscale.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum AaMode {
    /// "Sharp" preset (the default).
    #[default]
    Sharp,
    /// "Soft" preset. Still grayscale AA - AR-14 does not promise subpixel AA.
    Soft,
}

/// The only source of a cluster's cell columns (kernel/03 K-04).
///
/// `termai-render` carries no wcwidth table, so this port is the single width authority for
/// the whole shaping path. The production implementor is [`VtWidthSource`], which wraps
/// termai-vt's width API (UAX #11 per scalar, kernel/03 section 3.5.1 step 1); a cluster that
/// measures 0 columns is a combining mark or a variation selector and joins the preceding
/// cluster (kernel/03 section 3.5.1 step 6 / 3.5.2).
pub trait CellWidthSource {
    /// Columns occupied by one grapheme cluster's original scalar sequence.
    fn cluster_columns(&self, cluster: &str) -> u8;
}

/// The production [`CellWidthSource`] (kernel/03 K-04): `termai_vt::width::measure`.
///
/// This is the whole point of the K-04 crate edge: the shaper asks the same function the VT
/// grid used to place the cells, so a cluster's span and the grid's columns are decided by one
/// implementation instead of two tables that can drift. The port stays injectable (the tests
/// drive [`CellWidthSource`] with a deliberately wrong source to prove the fit counter moves),
/// but this is the binding that ships.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct VtWidthSource;

impl CellWidthSource for VtWidthSource {
    fn cluster_columns(&self, cluster: &str) -> u8 {
        termai_vt::width::measure(cluster)
    }
}

/// The production width source, for callers that want a value rather than a type.
pub const VT_WIDTH: VtWidthSource = VtWidthSource;

/// Shape one row with the production width source (kernel/03 section 3.5.1 step 1:
/// `列宽真源 = termai-vt::width::measure(scalars)`).
///
/// # Errors
/// As [`shape_row`].
pub fn shape_row_with_vt_widths(
    row: &RowClusters,
    ctx: &ShapeContext<'_>,
) -> Result<RowGlyphs, ShapeError> {
    shape_row(row, ctx, &VT_WIDTH)
}

/// One cluster of a row as it arrives from the mirror: the grid column it starts at and its
/// original scalars, byte-faithful (kernel/03 section 3.2.1 item 2 - clusters, not cells, are
/// what copy and the AI context read).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ClusterInput {
    /// Grid column this cluster starts at (0-based).
    pub col: u16,
    /// The cluster's original scalar sequence.
    pub text: String,
}

/// A whole row of clusters: the S6 input (kernel/03 section 3.1 S6 "HotRow").
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct RowClusters {
    /// Viewport row this belongs to.
    pub row: u16,
    /// LineFlags of the row (`termai_core::grid::LINE_WRAPPED` is bit 0).
    pub flags: u16,
    /// Clusters in logical (column) order.
    pub clusters: Vec<ClusterInput>,
}

impl RowClusters {
    /// Build a row from explicit clusters.
    #[must_use]
    pub fn new(row: u16, flags: u16, clusters: Vec<ClusterInput>) -> Self {
        Self {
            row,
            flags,
            clusters,
        }
    }

    /// Materialise one row of a mirror snapshot into clusters.
    ///
    /// A wide character is expressed in the S5 DTO as a lead cell plus `WIDE_CONTINUATION`
    /// cells (kernel/03 section 3.2.1 item 1), so the continuation cells extend the cluster
    /// that precedes them instead of opening a new one. Trailing blank cells are dropped -
    /// the same "used" trim the grid applies (kernel/03 section 3.2.1 item 4).
    #[must_use]
    pub fn from_snapshot(grid: &termai_core::grid::GridSnapshot, row: u16) -> Option<Self> {
        if row >= grid.rows {
            return None;
        }
        let blank = termai_core::grid::Cell::BLANK.ch;
        let mut clusters: Vec<ClusterInput> = Vec::with_capacity(usize::from(grid.cols));
        for col in 0..grid.cols {
            let cell = grid.cell(row, col)?;
            if cell.is_wide_continuation() {
                continue;
            }
            clusters.push(ClusterInput {
                col,
                text: cell.ch.to_string(),
            });
        }
        while let Some(last) = clusters.last() {
            if last.text.chars().eq(std::iter::once(blank)) {
                clusters.pop();
            } else {
                break;
            }
        }
        Some(Self {
            row,
            flags: grid.row_flags.get(usize::from(row)).copied().unwrap_or(0),
            clusters,
        })
    }
}

/// One resolved font face: the discovery/fallback result (kernel/03 section 3.5.2) plus the
/// bytes it borrows. Parsed once here, so shaping never re-parses a font per row.
#[derive(Clone)]
pub struct FontFace<'a> {
    /// Logical font id - the `fontdb` face id, i.e. the fallback chain's hit result
    /// (kernel/03 section 3.4: `AtlasKey.font`).
    pub id: FontId,
    /// Face index inside a collection (`fontdb::FaceInfo::index`).
    pub index: u32,
    /// The font bytes the face borrows.
    pub data: &'a [u8],
    face: Face<'a>,
}

impl fmt::Debug for FontFace<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FontFace")
            .field("id", &self.id.to_string())
            .field("index", &self.index)
            .field("bytes", &self.data.len())
            .field("units_per_em", &self.face.units_per_em())
            .finish()
    }
}

impl<'a> FontFace<'a> {
    /// Parse one face out of the font bytes. `None` when the bytes are not a font or the face
    /// index is out of range: a broken face is reported, never shaped into wrong glyph ids.
    #[must_use]
    pub fn from_slice(id: FontId, data: &'a [u8], index: u32) -> Option<Self> {
        let face = Face::from_slice(data, index)?;
        Some(Self {
            id,
            index,
            data,
            face,
        })
    }

    /// Font design units per em.
    #[must_use]
    pub fn units_per_em(&self) -> i32 {
        self.face.units_per_em()
    }
}

/// Everything S6 needs besides the row itself: the resolved font, the device pixel size, the
/// DPI scale, the AA preset and the cell metrics the font backend measured.
#[derive(Clone, Debug)]
pub struct ShapeContext<'a> {
    /// The resolved font face.
    pub font: FontFace<'a>,
    /// Device pixels per em. kernel/03 section 3.4 stores this as `size_q6`; the fixed-point
    /// form arrives with the DPI slice, this slice rounds to whole device pixels.
    pub px_size: u16,
    /// Device pixels per logical pixel, times 256 (kernel/03 section 3.4 `scale_q8`). It is
    /// part of the atlas key's eventual field set (K-12) but not yet part of [`AtlasKey`].
    pub scale_q8: u16,
    /// Anti-aliasing preset (AR-14).
    pub aa: AaMode,
    /// Whether `liga`/`calt` are on (AR-14 default is on; kernel/03 K-06 keeps ligatures from
    /// ever changing the column count).
    pub ligatures: bool,
    /// Cell width in device pixels: the advance of `"0"` in this font (kernel/03 section
    /// 3.5.3). Measured by the font backend and passed in, so the shaper owns no metrics table.
    pub cell_advance_px: f32,
    /// Row height in device pixels: fixed at 1.25 x font size (AR-22 section 2).
    pub row_height_px: f32,
}

impl ShapeContext<'_> {
    /// The S7 atlas key for one shaped glyph, so a glyph and its slot can never disagree about
    /// which font size and AA preset produced them.
    #[must_use]
    pub fn atlas_key(&self, glyph_id: u32) -> AtlasKey {
        AtlasKey {
            font_id: self.font.id,
            glyph_id,
            px_size: self.px_size,
            aa_mode: self.aa,
        }
    }
}

/// Per-glyph flags.
pub mod glyph_flag {
    /// Glyph id 0: the font has no glyph for this cluster (`rustybuzz` reports `.notdef`).
    pub const MISSING: u16 = 1 << 0;
    /// Colour glyph (COLR/CBDT): S7 must place it on the RGBA page (kernel/03 section 3.4).
    pub const COLOR: u16 = 1 << 1;
}

/// Per-span flags.
pub mod span_flag {
    /// The cluster was scaled down to fit its cell span (`fit_squeezed`, kernel/03 section
    /// 3.5.1 step 6 / RV-01 / V-10).
    pub const SQUEEZED: u16 = 1 << 0;
}

/// One glyph of a cluster, in the cluster's own logical order.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ShapedGlyph {
    /// Font glyph id (not a codepoint: a ligature or a variant gets its own id, K-12).
    pub glyph_id: u32,
    /// Advance in device pixels, already scaled by `fit_scale` when the span was squeezed.
    pub advance_px: f32,
    /// Byte offset of this glyph's cluster inside the cluster text.
    pub cluster_offset: u16,
    /// `glyph_flag` bits.
    pub flags: u16,
}

/// One cluster's placement: which cells it covers and what is drawn there.
#[derive(Clone, PartialEq, Debug)]
pub struct ClusterSpan {
    /// First cell column the cluster covers (inclusive).
    pub start_cell: u16,
    /// One past the last cell column the cluster covers.
    pub end_cell: u16,
    /// The cluster's measured cell width from the [`CellWidthSource`] (K-04). `end_cell -
    /// start_cell` must equal this exactly: a span wider than the measured width is the V-10
    /// overflow bug, and a narrower one would silently drop columns.
    pub cells: u8,
    /// Total advance in device pixels after the fit scale was applied.
    pub advance_px: f32,
    /// Uniform fit scale (1.0 = untouched). S7 rasterises the outline at this scale.
    pub fit_scale: f32,
    /// `span_flag` bits.
    pub flags: u16,
    /// The cluster's glyph run.
    pub glyphs: Vec<ShapedGlyph>,
}

impl ClusterSpan {
    /// The half-open cell span this cluster covers.
    #[must_use]
    pub fn cell_span(&self) -> Range<u16> {
        self.start_cell..self.end_cell
    }

    /// Whether the cluster was scaled down to fit (kernel/03 S6's `fit_squeezed`).
    #[must_use]
    pub fn is_squeezed(&self) -> bool {
        self.flags & span_flag::SQUEEZED != 0
    }
}

/// One row's shaped glyphs: the S6 output (kernel/03 section 3.1 S6 "RowGlyphs").
#[derive(Clone, PartialEq, Debug)]
pub struct RowGlyphs {
    /// Viewport row this belongs to.
    pub row: u16,
    /// The font the whole row was shaped with.
    pub font_id: FontId,
    /// Device pixel size the row was shaped at.
    pub px_size: u16,
    /// Anti-aliasing preset the row was shaped with (AR-14).
    pub aa: AaMode,
    /// Columns covered by the last span.
    pub covered_cells: u16,
    /// One span per cluster, in column order.
    pub spans: Vec<ClusterSpan>,
    /// S6 failure-localisation counter: clusters scaled down to fit their cells (kernel/03
    /// section 3.1 S6 failure column and section 3.5.1). A counter that can never move is not
    /// evidence, so the tests drive it with a deliberately under-reporting width source.
    pub fit_squeezed: u32,
    /// Number of glyphs reported as `.notdef` (gid 0).
    pub missing_glyphs: u32,
}

impl RowGlyphs {
    /// The cell span of one cluster, or `None` when the row has no such cluster.
    #[must_use]
    pub fn cluster_span(&self, index: usize) -> Option<Range<u16>> {
        self.spans.get(index).map(ClusterSpan::cell_span)
    }

    /// Total advance of the row in device pixels.
    #[must_use]
    pub fn advance_px(&self) -> f32 {
        self.spans.iter().map(|s| s.advance_px).sum()
    }

    /// The cluster/column assertion (kernel/03 section 3.1 S6 failure column, RP-07).
    ///
    /// Every span must be non-empty, exactly its measured width, contiguous with its
    /// predecessor and carry at least one glyph. Returns the first violation instead of
    /// panicking: library code does not unwind across a boundary (AGENTS section 6).
    ///
    /// # Errors
    /// [`SpanError`] describing the first inconsistent span.
    pub fn validate(&self) -> Result<(), SpanError> {
        let mut cursor: Option<u16> = None;
        for (index, span) in self.spans.iter().enumerate() {
            if span.end_cell <= span.start_cell {
                return Err(SpanError::EmptySpan {
                    index,
                    start_cell: span.start_cell,
                    end_cell: span.end_cell,
                });
            }
            if span.end_cell - span.start_cell != u16::from(span.cells) {
                return Err(SpanError::SpanWidthMismatch {
                    index,
                    start_cell: span.start_cell,
                    end_cell: span.end_cell,
                    cells: span.cells,
                });
            }
            if let Some(expected) = cursor {
                if span.start_cell != expected {
                    return Err(SpanError::NotContiguous {
                        index,
                        expected,
                        got: span.start_cell,
                    });
                }
            }
            if span.glyphs.is_empty() {
                return Err(SpanError::NoGlyphs { index });
            }
            cursor = Some(span.end_cell);
        }
        Ok(())
    }

    /// Canonical bytes for equality and golden comparison. Must never contain timestamps,
    /// addresses or allocation-order dependent values: shaping the same row twice with the
    /// same font must produce the identical byte string.
    ///
    /// The font id participates, so two different faces that happen to produce the same glyph
    /// ids do not hash alike. `fontdb::ID` is only stable inside one database, so these bytes
    /// are determinism evidence, not a cross-machine golden - a cross-machine golden needs the
    /// font file hash, which is the atlas slice's business.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(48 + self.spans.len() * 32);
        out.extend_from_slice(b"rowglyphs1\n");
        out.extend_from_slice(&self.row.to_le_bytes());
        out.extend_from_slice(self.font_id.to_string().as_bytes());
        out.push(0);
        out.extend_from_slice(&self.px_size.to_le_bytes());
        out.push(match self.aa {
            AaMode::Sharp => 0,
            AaMode::Soft => 1,
        });
        out.extend_from_slice(&self.covered_cells.to_le_bytes());
        out.extend_from_slice(&self.fit_squeezed.to_le_bytes());
        out.extend_from_slice(&self.missing_glyphs.to_le_bytes());
        out.extend_from_slice(
            &u32::try_from(self.spans.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        for span in &self.spans {
            out.extend_from_slice(&span.start_cell.to_le_bytes());
            out.extend_from_slice(&span.end_cell.to_le_bytes());
            out.push(span.cells);
            out.extend_from_slice(&span.advance_px.to_le_bytes());
            out.extend_from_slice(&span.fit_scale.to_le_bytes());
            out.extend_from_slice(&span.flags.to_le_bytes());
            out.extend_from_slice(
                &u32::try_from(span.glyphs.len())
                    .unwrap_or(u32::MAX)
                    .to_le_bytes(),
            );
            for glyph in &span.glyphs {
                out.extend_from_slice(&glyph.glyph_id.to_le_bytes());
                out.extend_from_slice(&glyph.advance_px.to_le_bytes());
                out.extend_from_slice(&glyph.cluster_offset.to_le_bytes());
                out.extend_from_slice(&glyph.flags.to_le_bytes());
            }
        }
        out
    }
}

/// A violated cluster/column invariant (kernel/03 RP-07).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpanError {
    /// A span covers no cell at all.
    EmptySpan {
        /// Cluster index.
        index: usize,
        /// Span start.
        start_cell: u16,
        /// Span end.
        end_cell: u16,
    },
    /// A span's width is not the cluster's measured width (K-04): it may never exceed it, and
    /// must not fall short of it either.
    SpanWidthMismatch {
        /// Cluster index.
        index: usize,
        /// Span start.
        start_cell: u16,
        /// Span end.
        end_cell: u16,
        /// The measured cell width the span had to match.
        cells: u8,
    },
    /// The span does not start where its predecessor ended.
    NotContiguous {
        /// Cluster index.
        index: usize,
        /// Where the span had to start.
        expected: u16,
        /// Where it started.
        got: u16,
    },
    /// The span carries no glyph.
    NoGlyphs {
        /// Cluster index.
        index: usize,
    },
}

/// Why a row could not be shaped. The shaper never guesses: an inconsistent input row is
/// reported, so cluster/column drift is localised at S6 instead of reaching the screen.
#[derive(Clone, PartialEq, Debug)]
pub enum ShapeError {
    /// The row's own grid anchor for a cluster disagrees with the column cursor built from the
    /// injected width table. Either the grid recorded a different width or the width table
    /// drifted - both are exactly what K-04/RP-07 must catch.
    ColumnMismatch {
        /// Cluster index.
        index: usize,
        /// The anchor the row carried.
        anchor: u16,
        /// The column the width cursor expected.
        expected: u16,
    },
    /// A cluster measured zero columns but no cluster preceded it, so there is nothing to join.
    LeadingZeroWidth {
        /// Cluster index.
        index: usize,
        /// Its grid column.
        col: u16,
    },
    /// The row's columns do not fit in the u16 column space.
    RowTooWide {
        /// Cluster index.
        index: usize,
        /// Its grid column.
        col: u16,
        /// Its measured width.
        cells: u8,
    },
    /// The font has no usable em size, so advances cannot be converted to device pixels.
    BadFont {
        /// The unusable face.
        font_id: FontId,
    },
    /// The cell advance is missing or non-positive: the cell box cannot be derived.
    BadCellMetrics {
        /// The advance that was passed in.
        cell_advance_px: f32,
    },
    /// The output failed its own cluster/column validation. Reaching this is a bug in this
    /// module, not in the caller - which is why it is reported rather than unwrapped.
    Span(SpanError),
}

/// Shape one row into per-cell glyph spans (kernel/03 section 3.5.1).
///
/// # Errors
/// [`ShapeError`] when the row's anchors disagree with the injected width table, when a
/// zero-width cluster has no base, or when the font/cell metrics cannot support the conversion.
pub fn shape_row(
    row: &RowClusters,
    ctx: &ShapeContext<'_>,
    width: &dyn CellWidthSource,
) -> Result<RowGlyphs, ShapeError> {
    if ctx.font.units_per_em() <= 0 {
        return Err(ShapeError::BadFont {
            font_id: ctx.font.id,
        });
    }
    let cell_advance_px = ctx.cell_advance_px;
    if cell_advance_px <= 0.0 || !cell_advance_px.is_finite() {
        return Err(ShapeError::BadCellMetrics { cell_advance_px });
    }
    let groups = group_clusters(row, width)?;
    let upem = ctx.font.units_per_em() as f32;
    let features = ligature_features(ctx.ligatures);
    let mut spans = Vec::with_capacity(groups.len());
    let mut fit_squeezed = 0_u32;
    let mut missing_glyphs = 0_u32;
    for group in &groups {
        let (span, squeezed, missing) = shape_group(group, ctx, upem, &features);
        if squeezed {
            fit_squeezed += 1;
        }
        missing_glyphs += missing;
        spans.push(span);
    }
    let covered_cells = spans.last().map_or(0, |s| s.end_cell);
    let row_glyphs = RowGlyphs {
        row: row.row,
        font_id: ctx.font.id,
        px_size: ctx.px_size,
        aa: ctx.aa,
        covered_cells,
        spans,
        fit_squeezed,
        missing_glyphs,
    };
    // S6 is the last place that can localise a cluster/column bug (kernel/03 section 3.1).
    row_glyphs.validate().map_err(ShapeError::Span)?;
    Ok(row_glyphs)
}

/// One cluster after the zero-width fold: the columns it owns and the text to shape.
#[derive(Clone, PartialEq, Eq, Debug)]
struct ClusterGroup {
    start_cell: u16,
    cells: u8,
    text: String,
}

/// Column anchoring (kernel/03 section 3.5.1 steps 2-3): walk the row in column order, check
/// every cluster's grid anchor against the width cursor, and fold zero-width clusters (combining
/// marks, variation selectors) into the cluster they belong to.
fn group_clusters(
    row: &RowClusters,
    width: &dyn CellWidthSource,
) -> Result<Vec<ClusterGroup>, ShapeError> {
    let mut groups: Vec<ClusterGroup> = Vec::with_capacity(row.clusters.len());
    let mut cursor: u16 = 0;
    for (index, cluster) in row.clusters.iter().enumerate() {
        if index == 0 {
            // The first cluster anchors the row; there is no predecessor to check against.
            cursor = cluster.col;
        }
        if cluster.col != cursor {
            return Err(ShapeError::ColumnMismatch {
                index,
                anchor: cluster.col,
                expected: cursor,
            });
        }
        let cells = width.cluster_columns(&cluster.text);
        if cells == 0 {
            let Some(last) = groups.last_mut() else {
                return Err(ShapeError::LeadingZeroWidth {
                    index,
                    col: cluster.col,
                });
            };
            last.text.push_str(&cluster.text);
            continue;
        }
        if u32::from(cluster.col) + u32::from(cells) > u32::from(u16::MAX) {
            return Err(ShapeError::RowTooWide {
                index,
                col: cluster.col,
                cells,
            });
        }
        groups.push(ClusterGroup {
            start_cell: cluster.col,
            cells,
            text: cluster.text.clone(),
        });
        cursor += u16::from(cells);
    }
    Ok(groups)
}

/// `liga` is on by default (AR-14); turning it off is an explicit feature from the caller.
fn ligature_features(ligatures: bool) -> Vec<Feature> {
    if ligatures {
        Vec::new()
    } else {
        vec![Feature::new(Tag::from_bytes(b"liga"), 0, ..)]
    }
}

/// Shape one group with rustybuzz, then fit it to its cell box using swash metrics.
///
/// Returns the span, whether it was squeezed, and how many `.notdef` glyphs it contained.
fn shape_group(
    group: &ClusterGroup,
    ctx: &ShapeContext<'_>,
    upem: f32,
    features: &[Feature],
) -> (ClusterSpan, bool, u32) {
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(&group.text);
    // kernel/03 K-13 keeps bidi reordering out of this slice: clusters are shaped in logical
    // left-to-right order, and the UBA pass is a separate stage with its own counters.
    buffer.set_direction(Direction::LeftToRight);
    let shaped = rustybuzz::shape(&ctx.font.face, features, buffer);

    let infos = shaped.glyph_infos();
    let positions = shaped.glyph_positions();
    let mut glyph_ids = Vec::with_capacity(infos.len());
    let mut advances = Vec::with_capacity(infos.len());
    let mut missing = 0_u32;
    for (info, position) in infos.iter().zip(positions) {
        if info.glyph_id == 0 {
            missing += 1;
        }
        glyph_ids.push(info.glyph_id);
        advances.push(position.x_advance as f32 * f32::from(ctx.px_size) / upem);
    }

    let span_px = f32::from(group.cells) * ctx.cell_advance_px;
    let shaped_advance: f32 = advances.iter().sum();
    let ink = run_ink(ctx, &glyph_ids, &advances);
    // The cell box is what the glyph may occupy: kernel/03 section 3.5.1 step 6 scales an
    // over-wide/over-tall glyph down to it instead of letting it cover a neighbour (V-10).
    let extent_w = shaped_advance.max(ink.as_ref().map_or(0.0, |i| i.width));
    let extent_h = ink.as_ref().map_or(0.0, |i| i.height);
    let mut fit_scale = 1.0_f32;
    if extent_w > span_px + FIT_EPSILON_PX {
        fit_scale = fit_scale.min(span_px / extent_w);
    }
    if extent_h > ctx.row_height_px + FIT_EPSILON_PX {
        fit_scale = fit_scale.min(ctx.row_height_px / extent_h);
    }
    let squeezed = fit_scale < 1.0;
    let color = ink.as_ref().is_some_and(|i| i.color);

    let mut flags = 0_u16;
    if squeezed {
        flags |= span_flag::SQUEEZED;
    }
    let mut glyphs = Vec::with_capacity(infos.len());
    for (info, advance) in infos.iter().zip(&advances) {
        let mut per_glyph = 0_u16;
        if info.glyph_id == 0 {
            per_glyph |= glyph_flag::MISSING;
        }
        if color {
            per_glyph |= glyph_flag::COLOR;
        }
        glyphs.push(ShapedGlyph {
            glyph_id: info.glyph_id,
            advance_px: advance * fit_scale,
            cluster_offset: u16::try_from(info.cluster).unwrap_or(u16::MAX),
            flags: per_glyph,
        });
    }
    let span = ClusterSpan {
        start_cell: group.start_cell,
        end_cell: group.start_cell + u16::from(group.cells),
        cells: group.cells,
        advance_px: shaped_advance * fit_scale,
        fit_scale,
        flags,
        glyphs,
    };
    (span, squeezed, missing)
}

/// Ink extent of one cluster's glyph run in device pixels.
struct RunInk {
    width: f32,
    height: f32,
    color: bool,
}

/// Extract the glyph outlines at the shaping size and measure them.
///
/// Hinting is off and nothing is rasterised: this is outline extraction for the fit decision
/// and for "is this a colour glyph" (kernel/03 section 3.4 `Rasterizer::is_color`), not S7's
/// bitmap work and not S8's pixels.
fn run_ink(ctx: &ShapeContext<'_>, glyph_ids: &[u32], advances: &[f32]) -> Option<RunInk> {
    let font = swash::FontRef::from_index(ctx.font.data, ctx.font.index as usize)?;
    let mut context = swash::scale::ScaleContext::new();
    let mut scaler = context
        .builder(font)
        .size(f32::from(ctx.px_size))
        .hint(false)
        .build();
    let mut pen = 0.0_f32;
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_height = 0.0_f32;
    let mut color = false;
    for (glyph_id, advance) in glyph_ids.iter().zip(advances) {
        let outline = u16::try_from(*glyph_id)
            .ok()
            .and_then(|id| scaler.scale_outline(id));
        if let Some(outline) = outline {
            color |= outline.is_color();
            let bounds = outline.bounds();
            if !bounds.is_empty() {
                min_x = min_x.min(pen + bounds.min.x);
                max_x = max_x.max(pen + bounds.max.x);
                max_height = max_height.max(bounds.max.y - bounds.min.y);
            }
        }
        pen += advance;
    }
    if !min_x.is_finite() || !max_x.is_finite() {
        return Some(RunInk {
            width: 0.0,
            height: 0.0,
            color,
        });
    }
    Some(RunInk {
        width: max_x - min_x,
        height: max_height,
        color,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(glyph_id: u32, advance_px: f32) -> ShapedGlyph {
        ShapedGlyph {
            glyph_id,
            advance_px,
            cluster_offset: 0,
            flags: 0,
        }
    }

    fn span(start_cell: u16, end_cell: u16, cells: u8) -> ClusterSpan {
        ClusterSpan {
            start_cell,
            end_cell,
            cells,
            advance_px: 8.0,
            fit_scale: 1.0,
            flags: 0,
            glyphs: vec![glyph(36, 8.0)],
        }
    }

    fn row(spans: Vec<ClusterSpan>) -> RowGlyphs {
        RowGlyphs {
            row: 0,
            font_id: FontId::dummy(),
            px_size: 16,
            aa: AaMode::Sharp,
            covered_cells: spans.last().map_or(0, |s| s.end_cell),
            spans,
            fit_squeezed: 0,
            missing_glyphs: 0,
        }
    }

    #[test]
    fn validate_rejects_a_span_wider_than_its_measured_width() {
        // The cluster/column assertion is reachable: a span that grows past the width the
        // K-04 port measured must not pass validation (V-10 is the overflow this blocks).
        let bad = row(vec![span(0, 3, 2)]);
        assert_eq!(
            bad.validate(),
            Err(SpanError::SpanWidthMismatch {
                index: 0,
                start_cell: 0,
                end_cell: 3,
                cells: 2,
            })
        );
    }

    #[test]
    fn validate_rejects_a_gap_and_an_empty_span() {
        let gap = row(vec![span(0, 1, 1), span(2, 3, 1)]);
        assert_eq!(
            gap.validate(),
            Err(SpanError::NotContiguous {
                index: 1,
                expected: 1,
                got: 2,
            })
        );
        let empty = row(vec![span(4, 4, 0)]);
        assert_eq!(
            empty.validate(),
            Err(SpanError::EmptySpan {
                index: 0,
                start_cell: 4,
                end_cell: 4,
            })
        );
    }

    #[test]
    fn validate_rejects_a_span_without_glyphs() {
        let mut bad = row(vec![span(0, 1, 1)]);
        bad.spans[0].glyphs.clear();
        assert_eq!(bad.validate(), Err(SpanError::NoGlyphs { index: 0 }));
    }

    #[test]
    fn validate_accepts_a_contiguous_row() {
        let good = row(vec![span(0, 2, 2), span(2, 3, 1)]);
        assert_eq!(good.validate(), Ok(()));
        assert_eq!(good.cluster_span(0), Some(0..2));
        assert_eq!(good.cluster_span(1), Some(2..3));
        assert_eq!(good.cluster_span(2), None);
        assert!((good.advance_px() - 16.0).abs() < 1.0e-4);
    }

    #[test]
    fn canonical_bytes_are_stable_and_track_glyph_ids() {
        let a = row(vec![span(0, 1, 1), span(1, 3, 2)]);
        let b = row(vec![span(0, 1, 1), span(1, 3, 2)]);
        assert_eq!(a.canonical_bytes(), b.canonical_bytes());
        let mut c = row(vec![span(0, 1, 1), span(1, 3, 2)]);
        c.spans[1].glyphs[0].glyph_id = 37;
        assert_ne!(a.canonical_bytes(), c.canonical_bytes());
    }

    #[test]
    fn an_empty_row_is_valid_and_covers_nothing() {
        let empty = row(vec![]);
        assert_eq!(empty.validate(), Ok(()));
        assert_eq!(empty.covered_cells, 0);
    }
}
