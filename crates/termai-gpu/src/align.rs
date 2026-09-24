//! Grid-alignment measurement: the mechanism behind HARNESS section 5 "网格对齐误差 ≤0.5px".
//!
//! Authority and the non-gating boundary
//! -------------------------------------
//! The measurement *contract* is `docs/spec/kernel/03-rendering-pipeline.md` section 3.5.3 and
//! its Open Question **OQ-RND-04** (decided by **AR-24** clause 3), carried as **RP-05**:
//!
//! > max over drawn glyphs of `abs(glyph_bitmap_origin - cell_box_origin) <= 0.5px`, judged
//! > separately at 100 / 125 / 150 / 200% DPI (RM-C, golden pixel measurement).
//!
//! **kernel/03 RP-05 is the authority**; if this module and that document disagree, the document
//! wins, and kernel/06 owns the sampling methodology (AR-24 clause 3). This module implements the
//! mechanism, so the numbers it produces are only as good as the contract they are read against.
//!
//! **ADR-0014 iron law**: on a non-T0 backend every number produced here is **NON-GATING**. The
//! section 5 gates may be judged only on an RM-A / RM-C T0 backend (DX12 on Windows, Metal on
//! macOS, Vulkan 1.3 on Linux) with the pinned reference driver; a T1 secondary API, a T2 software
//! rasteriser (WARP, lavapipe) or T3 safe mode may run this measurement for information only, and
//! ADR-0014 rule 4 forbids recording a failure there as a pass. The tests in this crate therefore
//! print the probed tier and assert the value on every tier instead of claiming a gate result.
//!
//! What is measured, and what is not
//! ---------------------------------
//! ADR-0027 D2 scopes this slice to the GPU side of kernel/03 stage **S8 绘制/提交**. Glyphs,
//! shaping (S6) and the atlas (S7) belong to later slices, so the alignment pattern used here is a
//! grid of plain cell-aligned quads, not glyph bitmaps. What this module measures is therefore the
//! *edge placement of a drawn cell quad relative to its ideal grid line*, which is the geometric
//! half of RP-05: the pipeline, the target, the readback and the pixel-space reconstruction. It
//! does **not** measure a glyph bitmap origin inside a cell box; that needs S6/S7 and is the next
//! slice's job (see the module docs of the crate root for the honest boundary).
//!
//! Method
//! ------
//! The pattern is a checkerboard of cell-sized quads so every drawn quad owns isolated edges. The
//! renderer draws each quad with exact grayscale coverage (AR-14: grayscale AA, hinting off), and
//! [`measure_alignment`] reconstructs each edge position **from the pixels only**:
//!
//! 1. take the middle half of the cell (pixel rows whose centres lie in the middle half of the
//!    ideal cell height) so every sampled row is entirely inside the drawn quad;
//! 2. sum the coverage of a horizontal window that reaches `WINDOW_PAD_PX` beyond the ideal cell;
//!    the row sums give `mass`, and each fully covered row contributes exactly the quad width, so
//!    `width = mass / rows`;
//! 3. the coverage-weighted centroid of that window is the quad's centre, hence
//!    `left = centroid - width / 2` and `right = centroid + width / 2`;
//! 4. the vertical edges come from the transposed pass;
//! 5. each measured edge is compared with its **ideal grid line** (`ideal.x`, `ideal.right()`,
//!    `ideal.y`, `ideal.bottom()`), and the worst absolute deviation over all four edges of all
//!    drawn cells is the reported value.
//!
//! The window is checked on both borders before it is used: a non-zero border pixel means the
//! drawn quad reaches outside the window, and the sample is refused with
//! [`MeasureError::WindowClipped`] instead of reporting a biased number. The pattern shift that a
//! caller may inject for the self-check must stay below `WINDOW_PAD_PX` and below a quarter of the
//! cell size; the tests use 0.75px on cells of at least 7.8px.

use core::fmt;
use std::error::Error;

/// AR-22 section 2: the grid row height is a fixed 1.25 x font size.
pub const LINE_HEIGHT_RATIO: f32 = 1.25;

/// HARNESS section 5, "网格对齐误差": the contract value this mechanism measures against.
///
/// RP-05 in kernel/03 section 5 restates it as `网格对齐误差 <= 0.5px` at 100/125/150/200% DPI.
pub const HARNESS_ALIGNMENT_CONTRACT_PX: f32 = 0.5;

/// The four DPI scales RP-05 judges separately (kernel/03 section 5).
pub const RP05_DPI_SCALES: [f32; 4] = [1.0, 1.25, 1.5, 2.0];

/// Quiet border drawn around the grid inside the render target, in physical pixels.
pub const DEFAULT_MARGIN_PX: f32 = 4.0;

/// How far the measurement window reaches beyond the ideal cell rect, in physical pixels.
///
/// It must exceed the largest injected pattern shift (the tests use 0.75px) so that the drawn quad
/// stays inside the window, and it must stay well below the cell size so that the window does not
/// reach a neighbouring drawn quad.
pub const WINDOW_PAD_PX: f32 = 2.0;

/// Colour the alignment pattern paints: white, whose red channel equals the analytic coverage.
pub const ALIGNMENT_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// The axis an edge position is measured along.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    /// Left / right edges, reconstructed from a horizontal coverage profile.
    Horizontal,
    /// Top / bottom edges, reconstructed from a vertical coverage profile.
    Vertical,
}

impl Axis {
    /// Stable lowercase label for diagnostics.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

/// Which edge of a cell box a deviation belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edge {
    /// The ideal `x` of the cell box.
    Left,
    /// The ideal `x + width` of the cell box.
    Right,
    /// The ideal `y` of the cell box.
    Top,
    /// The ideal `y + height` of the cell box.
    Bottom,
}

impl Edge {
    /// Stable lowercase label for diagnostics.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

/// A rectangle in physical (device) pixels, with its origin at the top-left of the target.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width in pixels (may be fractional).
    pub w: f32,
    /// Height in pixels (may be fractional).
    pub h: f32,
}

impl Rect {
    /// Build a rectangle from its top-left corner and its size.
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// The right edge, `x + w`.
    #[must_use]
    pub fn right(self) -> f32 {
        self.x + self.w
    }

    /// The bottom edge, `y + h`.
    #[must_use]
    pub fn bottom(self) -> f32 {
        self.y + self.h
    }

    /// The same rectangle moved by `(dx, dy)` pixels.
    #[must_use]
    pub fn shifted(self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    /// The same rectangle grown by `by` pixels on every side.
    #[must_use]
    pub fn expanded(self, by: f32) -> Self {
        Self::new(
            self.x - by,
            self.y - by,
            self.w + 2.0 * by,
            self.h + 2.0 * by,
        )
    }
}

/// The cell grid in physical pixels: `cols x rows` cells of a possibly fractional size.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GridGeometry {
    /// Columns in the grid.
    pub cols: u16,
    /// Rows in the grid.
    pub rows: u16,
    /// Physical pixels per column (the "0" advance x DPI scale; kernel/03 section 3.5.3).
    pub cell_w: f32,
    /// Physical pixels per row (`1.25 x font size x DPI scale`; AR-22 section 2).
    pub cell_h: f32,
    /// Quiet border around the grid inside the target, in physical pixels.
    pub margin: f32,
    /// Metadata only: the DPI scale the physical size was derived from (RP-05 reports per scale).
    ///
    /// It does not enter any geometry: the multiplication has already been folded into
    /// [`GridGeometry::cell_w`] and [`GridGeometry::cell_h`].
    pub dpi_scale: f32,
}

impl GridGeometry {
    /// A grid whose physical cell size is given directly, with the default margin and scale 1.0.
    #[must_use]
    pub const fn new(cols: u16, rows: u16, cell_w: f32, cell_h: f32) -> Self {
        Self {
            cols,
            rows,
            cell_w,
            cell_h,
            margin: DEFAULT_MARGIN_PX,
            dpi_scale: 1.0,
        }
    }

    /// The same geometry with a different quiet border.
    #[must_use]
    pub const fn with_margin(mut self, margin: f32) -> Self {
        self.margin = margin;
        self
    }

    /// The physical cell size from font metrics and a DPI scale (kernel/03 section 3.5.3).
    ///
    /// `advance_px` is the logical advance of "0" from the font backend and `font_size_px` the
    /// logical font size; `cell_h = font_size_px * 1.25 * scale` per AR-22 section 2. A scale such
    /// as 1.25 leaves a fractional physical cell, which is exactly the case RP-05 must judge.
    #[must_use]
    pub fn from_metrics(
        cols: u16,
        rows: u16,
        advance_px: f32,
        font_size_px: f32,
        scale: f32,
    ) -> Self {
        Self {
            cols,
            rows,
            cell_w: advance_px * scale,
            cell_h: font_size_px * LINE_HEIGHT_RATIO * scale,
            margin: DEFAULT_MARGIN_PX,
            dpi_scale: scale,
        }
    }

    /// The ideal (grid-authoritative) rect of one cell, in physical pixels.
    #[must_use]
    pub fn ideal_cell(&self, col: u16, row: u16) -> Rect {
        Rect::new(
            self.margin + f32::from(col) * self.cell_w,
            self.margin + f32::from(row) * self.cell_h,
            self.cell_w,
            self.cell_h,
        )
    }

    /// The size of the grid itself, without the margin.
    #[must_use]
    pub fn grid_size(&self) -> (f32, f32) {
        (
            f32::from(self.cols) * self.cell_w,
            f32::from(self.rows) * self.cell_h,
        )
    }

    /// The render target size in whole pixels: the grid plus both margins, rounded up.
    #[must_use]
    pub fn target_size(&self) -> (u32, u32) {
        let (width, height) = self.grid_size();
        let w = (width + 2.0 * self.margin).ceil();
        let h = (height + 2.0 * self.margin).ceil();
        (w.max(1.0) as u32, h.max(1.0) as u32)
    }
}

/// One drawn quad of the alignment pattern: the ideal grid rect and the rect actually drawn.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PatternCell {
    /// Grid column.
    pub col: u16,
    /// Grid row.
    pub row: u16,
    /// The grid line positions this cell is judged against.
    pub ideal: Rect,
    /// The rect handed to the renderer (equal to `ideal` unless a shift was injected).
    pub drawn: Rect,
    /// RGBA colour; the measurement reads the red channel, so red must be 1.0.
    pub color: [f32; 4],
}

/// The alignment pattern: a checkerboard of cell-sized quads plus the target size it needs.
#[derive(Clone, PartialEq, Debug)]
pub struct AlignmentPattern {
    /// Cell geometry the pattern was built from.
    pub geometry: GridGeometry,
    /// Render target width in pixels.
    pub width: u32,
    /// Render target height in pixels.
    pub height: u32,
    /// Horizontal shift injected into every drawn rect, in pixels (0.0 for the real measurement).
    pub shift_x: f32,
    /// Vertical shift injected into every drawn rect, in pixels (0.0 for the real measurement).
    pub shift_y: f32,
    /// The drawn quads.
    pub cells: Vec<PatternCell>,
}

impl AlignmentPattern {
    /// The unmodified pattern: every drawn quad sits exactly on its ideal grid rect.
    #[must_use]
    pub fn checkerboard(geometry: GridGeometry) -> Self {
        Self::shifted(geometry, 0.0, 0.0)
    }

    /// The same pattern with every drawn quad moved by `(dx, dy)` pixels.
    ///
    /// The ideal rects stay on the grid, so the measurement must report approximately
    /// `max(abs(dx), abs(dy))`. This is the self-check that keeps the mechanism from being
    /// always-green; keep `abs(dx)` and `abs(dy)` below [`WINDOW_PAD_PX`] and below a quarter of
    /// the cell size, or the measurement refuses the sample instead of reporting a number.
    #[must_use]
    pub fn shifted(geometry: GridGeometry, dx: f32, dy: f32) -> Self {
        let (width, height) = geometry.target_size();
        let mut cells = Vec::new();
        for row in 0..geometry.rows {
            for col in 0..geometry.cols {
                if (col + row) % 2 != 0 {
                    continue;
                }
                let ideal = geometry.ideal_cell(col, row);
                cells.push(PatternCell {
                    col,
                    row,
                    ideal,
                    drawn: ideal.shifted(dx, dy),
                    color: ALIGNMENT_COLOR,
                });
            }
        }
        Self {
            geometry,
            width,
            height,
            shift_x: dx,
            shift_y: dy,
            cells,
        }
    }

    /// The largest shift that was injected into the drawn rects, in pixels.
    #[must_use]
    pub fn injected_shift_px(&self) -> f32 {
        self.shift_x.abs().max(self.shift_y.abs())
    }
}

/// Why a pixel buffer could not be built.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PixelBufferError {
    /// A zero-sized buffer is never a render target.
    Empty,
    /// The byte count does not match `width * height * 4`.
    LengthMismatch {
        /// Bytes required for the declared size.
        expected: usize,
        /// Bytes supplied.
        actual: usize,
    },
}

impl fmt::Display for PixelBufferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("the pixel buffer has a zero dimension"),
            Self::LengthMismatch { expected, actual } => write!(
                f,
                "the pixel buffer needs {expected} bytes for its size but got {actual}"
            ),
        }
    }
}

impl Error for PixelBufferError {}

/// An 8-bit RGBA pixel buffer, row-major, `width * 4` bytes per row, no padding.
///
/// This is what a readback produces and what the measurement consumes, so the measurement can be
/// exercised without a GPU (the CPU test rasterises the same coverage function).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PixelBuffer {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl PixelBuffer {
    /// Take ownership of a compact RGBA8 buffer, checking its length.
    pub fn from_rgba8(width: u32, height: u32, data: Vec<u8>) -> Result<Self, PixelBufferError> {
        if width == 0 || height == 0 {
            return Err(PixelBufferError::Empty);
        }
        let expected = width as usize * height as usize * 4;
        if data.len() != expected {
            return Err(PixelBufferError::LengthMismatch {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The raw RGBA8 bytes, for hashing or byte-for-byte determinism checks.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The red channel of one pixel as coverage in `0.0..=1.0`, or None when out of bounds.
    ///
    /// The alignment pattern is painted white, so red equals the analytic coverage the rasteriser
    /// computed for that pixel.
    #[must_use]
    pub fn coverage(&self, x: u32, y: u32) -> Option<f32> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = (y as usize * self.width as usize + x as usize) * 4;
        self.data.get(index).map(|red| f32::from(*red) / 255.0)
    }

    /// Coverage of one pixel, treating an out-of-bounds read as zero.
    fn coverage_or_zero(&self, x: u32, y: u32) -> f32 {
        self.coverage(x, y).unwrap_or(0.0)
    }
}

/// Why a pattern could not be measured.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MeasureError {
    /// The pixel buffer is not the pattern's target size.
    SizeMismatch {
        /// Buffer size as `(width, height)`.
        buffer: (u32, u32),
        /// Pattern size as `(width, height)`.
        pattern: (u32, u32),
    },
    /// The pattern contains no drawn quad.
    EmptyPattern,
    /// The cell is too small for the middle-half band, so the mechanism cannot sample it.
    BandTooSmall {
        /// Grid column of the offending cell.
        col: u16,
        /// Grid row of the offending cell.
        row: u16,
        /// Axis whose band failed.
        axis: Axis,
    },
    /// The measured window is entirely empty: nothing was drawn where the grid says a quad is.
    NoCoverage {
        /// Grid column of the offending cell.
        col: u16,
        /// Grid row of the offending cell.
        row: u16,
        /// Axis whose window was empty.
        axis: Axis,
    },
    /// The window border holds coverage, so the drawn quad reaches outside the measured window.
    ///
    /// This is the guard that keeps a too-large injected shift from being reported as a number:
    /// the sample is refused instead.
    WindowClipped {
        /// Grid column of the offending cell.
        col: u16,
        /// Grid row of the offending cell.
        row: u16,
        /// Axis whose window was clipped.
        axis: Axis,
    },
}

impl fmt::Display for MeasureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SizeMismatch { buffer, pattern } => write!(
                f,
                "pixel buffer is {}x{} but the pattern needs {}x{}",
                buffer.0, buffer.1, pattern.0, pattern.1
            ),
            Self::EmptyPattern => f.write_str("the alignment pattern has no drawn cell"),
            Self::BandTooSmall { col, row, axis } => write!(
                f,
                "cell ({col},{row}) is too small to sample its {} band",
                axis.label()
            ),
            Self::NoCoverage { col, row, axis } => write!(
                f,
                "cell ({col},{row}) has no coverage inside its {} window",
                axis.label()
            ),
            Self::WindowClipped { col, row, axis } => write!(
                f,
                "cell ({col},{row}) reaches outside its {} window",
                axis.label()
            ),
        }
    }
}

impl Error for MeasureError {}

/// The measurement result: the worst deviation and how many edge samples produced it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GridAlignment {
    /// Worst absolute deviation between a measured edge and its ideal grid line, in pixels.
    pub worst_px: f32,
    /// Number of edge samples that went into `worst_px` (four per drawn cell).
    pub samples: usize,
    /// Axis of the worst sample.
    pub worst_axis: Axis,
    /// Edge of the worst sample.
    pub worst_edge: Edge,
    /// Grid column of the worst sample.
    pub worst_col: u16,
    /// Grid row of the worst sample.
    pub worst_row: u16,
    /// Grid columns.
    pub cols: u16,
    /// Grid rows.
    pub rows: u16,
    /// Physical cell width the pattern was built with.
    pub cell_w: f32,
    /// Physical cell height the pattern was built with.
    pub cell_h: f32,
    /// DPI scale metadata of the pattern.
    pub dpi_scale: f32,
}

impl fmt::Display for GridAlignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "grid alignment: worst {:.4}px over {} edge sample(s) \
             ({}x{} cells, cell {:.4}x{:.4}px, {:.0}% DPI; worst at cell ({},{}) {} {}; contract {:.1}px{})",
            self.worst_px,
            self.samples,
            self.cols,
            self.rows,
            self.cell_w,
            self.cell_h,
            self.dpi_scale * 100.0,
            self.worst_col,
            self.worst_row,
            self.worst_axis.label(),
            self.worst_edge.label(),
            HARNESS_ALIGNMENT_CONTRACT_PX,
            if self.worst_px <= HARNESS_ALIGNMENT_CONTRACT_PX {
                ""
            } else {
                " EXCEEDED"
            }
        )
    }
}

/// Exact coverage of the pixel square centred on `(px, py)` by `rect`.
///
/// This mirrors the WGSL fragment stage in `offscreen.rs` byte for byte in intent: the same
/// one-dimensional overlap is computed per axis and multiplied. The two must stay in sync; the GPU
/// test `offscreen_readback_matches_the_cpu_coverage_mirror` fails if they drift apart.
#[must_use]
pub fn rect_coverage(px: f32, py: f32, rect: &Rect) -> f32 {
    axis_coverage(px, rect.x, rect.right()) * axis_coverage(py, rect.y, rect.bottom())
}

/// Overlap of `[centre - 0.5, centre + 0.5]` with `[lo, hi]`, in pixels.
#[must_use]
pub fn axis_coverage(centre: f32, lo: f32, hi: f32) -> f32 {
    let upper = (centre + 0.5).clamp(lo, hi);
    let lower = (centre - 0.5).clamp(lo, hi);
    (upper - lower).max(0.0)
}

/// Round a coverage value the way an 8-bit unorm render target does.
fn quantize(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Rasterise the pattern on the CPU with the same coverage function and the same compositing the
/// shader uses.
///
/// This is the GPU-free path: it renders the pattern identically to the offscreen pipeline (the GPU
/// test compares the two pixel by pixel), so the measurement mechanism can always be exercised, on
/// any host, with no adapter at all.
///
/// The compositing is a sequence of premultiplied source-over blends in draw order, quantised to
/// 8 bits after every fragment exactly as a `Rgba8Unorm` colour target does. That matters: a quad
/// is rasterised one pixel larger than its true rect, so two neighbouring quads can both touch a
/// partial edge pixel, and the pixel value is their composite, not the last writer's value.
pub fn rasterize_cpu(pattern: &AlignmentPattern) -> Result<PixelBuffer, PixelBufferError> {
    let width = pattern.width;
    let height = pattern.height;
    let mut data = vec![0u8; width as usize * height as usize * 4];
    for cell in &pattern.cells {
        let area = cell.drawn.expanded(1.0);
        let first_x = area.x.max(0.0) as u32;
        let first_y = area.y.max(0.0) as u32;
        let last_x = (area.right().max(0.0) as u32).min(width);
        let last_y = (area.bottom().max(0.0) as u32).min(height);
        for y in first_y..last_y {
            for x in first_x..last_x {
                let coverage = rect_coverage(x as f32 + 0.5, y as f32 + 0.5, &cell.drawn);
                if coverage <= 0.0 {
                    continue;
                }
                let index = (y as usize * width as usize + x as usize) * 4;
                let Some(pixel) = data.get_mut(index..index + 4) else {
                    continue;
                };
                for (channel, stored) in pixel.iter_mut().enumerate() {
                    let source = coverage * cell.color[channel];
                    let destination = f32::from(*stored) / 255.0;
                    *stored = quantize(source + destination * (1.0 - coverage));
                }
            }
        }
    }
    PixelBuffer::from_rgba8(width, height, data)
}

/// Measure the worst deviation between a drawn quad edge and its ideal grid line, in pixels.
///
/// `pattern.width`/`pattern.height` must match the buffer; the returned
/// [`GridAlignment::samples`] is the number of edge samples behind [`GridAlignment::worst_px`].
///
/// # Errors
///
/// Returns a [`MeasureError`] when the buffer does not match the pattern, when a cell is too small
/// for the sampling band, when a window holds no coverage, or when a drawn quad reaches outside its
/// window (see [`MeasureError::WindowClipped`]).
pub fn measure_alignment(
    buffer: &PixelBuffer,
    pattern: &AlignmentPattern,
) -> Result<GridAlignment, MeasureError> {
    if buffer.width() != pattern.width || buffer.height() != pattern.height {
        return Err(MeasureError::SizeMismatch {
            buffer: (buffer.width(), buffer.height()),
            pattern: (pattern.width, pattern.height),
        });
    }
    if pattern.cells.is_empty() {
        return Err(MeasureError::EmptyPattern);
    }

    let mut result = GridAlignment {
        worst_px: 0.0,
        samples: 0,
        worst_axis: Axis::Horizontal,
        worst_edge: Edge::Left,
        worst_col: 0,
        worst_row: 0,
        cols: pattern.geometry.cols,
        rows: pattern.geometry.rows,
        cell_w: pattern.geometry.cell_w,
        cell_h: pattern.geometry.cell_h,
        dpi_scale: pattern.geometry.dpi_scale,
    };

    for cell in &pattern.cells {
        let (left, right) = horizontal_edges(buffer, cell)?;
        let (top, bottom) = vertical_edges(buffer, cell)?;
        let samples = [
            (left, cell.ideal.x, Axis::Horizontal, Edge::Left),
            (right, cell.ideal.right(), Axis::Horizontal, Edge::Right),
            (top, cell.ideal.y, Axis::Vertical, Edge::Top),
            (bottom, cell.ideal.bottom(), Axis::Vertical, Edge::Bottom),
        ];
        for (measured, ideal, axis, edge) in samples {
            let deviation = (measured - ideal).abs();
            result.samples += 1;
            if result.samples == 1 || deviation > result.worst_px {
                result.worst_px = deviation;
                result.worst_axis = axis;
                result.worst_edge = edge;
                result.worst_col = cell.col;
                result.worst_row = cell.row;
            }
        }
    }

    Ok(result)
}

/// The refusal a sampling pass raises for one cell and axis, built the same way in both passes.
#[derive(Clone, Copy)]
struct Refusal {
    cell: PatternCell,
    axis: Axis,
}

impl Refusal {
    /// The band or window this cell and axis needs does not exist.
    const fn band(self) -> MeasureError {
        MeasureError::BandTooSmall {
            col: self.cell.col,
            row: self.cell.row,
            axis: self.axis,
        }
    }

    /// The measured window holds no coverage at all.
    const fn empty(self) -> MeasureError {
        MeasureError::NoCoverage {
            col: self.cell.col,
            row: self.cell.row,
            axis: self.axis,
        }
    }

    /// The drawn quad reaches outside the measured window.
    const fn clipped(self) -> MeasureError {
        MeasureError::WindowClipped {
            col: self.cell.col,
            row: self.cell.row,
            axis: self.axis,
        }
    }
}

/// Left and right edge positions of one drawn quad, reconstructed from its pixels.
fn horizontal_edges(buffer: &PixelBuffer, cell: &PatternCell) -> Result<(f32, f32), MeasureError> {
    let refusal = Refusal {
        cell: *cell,
        axis: Axis::Horizontal,
    };

    let (y0, y1) =
        middle_band(cell.ideal.y, cell.ideal.h, buffer.height()).ok_or(refusal.band())?;
    let (x0, x1) = window(cell.ideal.x, cell.ideal.w, buffer.width()).ok_or(refusal.band())?;

    let mut mass = 0.0f32;
    let mut moment = 0.0f32;
    for y in y0..y1 {
        if buffer.coverage_or_zero(x0, y) > 0.0 || buffer.coverage_or_zero(x1 - 1, y) > 0.0 {
            return Err(refusal.clipped());
        }
        for x in x0..x1 {
            let coverage = buffer.coverage_or_zero(x, y);
            mass += coverage;
            moment += coverage * (x as f32 + 0.5);
        }
    }
    if mass <= 0.0 {
        return Err(refusal.empty());
    }

    // Every sampled row is entirely inside the drawn quad, so it contributes the quad's full
    // width; the row count is therefore the scale between the summed area and the width.
    let width = mass / (y1 - y0) as f32;
    let centre = moment / mass;
    Ok((centre - width * 0.5, centre + width * 0.5))
}

/// Top and bottom edge positions of one drawn quad, reconstructed from its pixels.
fn vertical_edges(buffer: &PixelBuffer, cell: &PatternCell) -> Result<(f32, f32), MeasureError> {
    let refusal = Refusal {
        cell: *cell,
        axis: Axis::Vertical,
    };

    let (y0, y1) = window(cell.ideal.y, cell.ideal.h, buffer.height()).ok_or(refusal.band())?;
    let (x0, x1) = middle_band(cell.ideal.x, cell.ideal.w, buffer.width()).ok_or(refusal.band())?;

    let mut mass = 0.0f32;
    let mut moment = 0.0f32;
    for x in x0..x1 {
        if buffer.coverage_or_zero(x, y0) > 0.0 || buffer.coverage_or_zero(x, y1 - 1) > 0.0 {
            return Err(refusal.clipped());
        }
        for y in y0..y1 {
            let coverage = buffer.coverage_or_zero(x, y);
            mass += coverage;
            moment += coverage * (y as f32 + 0.5);
        }
    }
    if mass <= 0.0 {
        return Err(refusal.empty());
    }

    let height = mass / (x1 - x0) as f32;
    let centre = moment / mass;
    Ok((centre - height * 0.5, centre + height * 0.5))
}

/// Pixel indices strictly inside the middle half of `[lo, lo + extent]`, clipped to `limit`.
///
/// Bounds are `[start, end)`. Every pixel in this range has its centre inside the middle half, so
/// a quad drawn within a quarter cell of its ideal rect covers all of them completely.
fn middle_band(lo: f32, extent: f32, limit: u32) -> Option<(u32, u32)> {
    if extent <= 0.0 || limit == 0 {
        return None;
    }
    let first = (lo + extent * 0.25).ceil().max(0.0) as u32;
    let last = (lo + extent * 0.75).floor().max(0.0) as u32;
    let start = first.min(limit);
    let end = last.min(limit);
    if end > start {
        Some((start, end))
    } else {
        None
    }
}

/// Pixel indices covering `[lo - WINDOW_PAD_PX, lo + extent + WINDOW_PAD_PX]`, clipped to `limit`.
///
/// Bounds are `[start, end)`.
fn window(lo: f32, extent: f32, limit: u32) -> Option<(u32, u32)> {
    if extent <= 0.0 || limit == 0 {
        return None;
    }
    let first = (lo - WINDOW_PAD_PX).max(0.0) as u32;
    let last = (lo + extent + WINDOW_PAD_PX).ceil().max(0.0) as u32;
    let start = first.min(limit);
    let end = last.min(limit);
    if end > start {
        Some((start, end))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JetBrains Mono class metrics: 13px font, 0.6 advance ratio, AR-22's 1.25 line height.
    const ADVANCE_PX: f32 = 7.8;
    const FONT_SIZE_PX: f32 = 13.0;
    const COLS: u16 = 6;
    const ROWS: u16 = 4;

    fn geometry(scale: f32) -> GridGeometry {
        GridGeometry::from_metrics(COLS, ROWS, ADVANCE_PX, FONT_SIZE_PX, scale)
    }

    #[test]
    fn cpu_layout_is_fractional_at_every_rp05_dpi_scale() {
        for scale in RP05_DPI_SCALES {
            let grid = geometry(scale);
            assert_eq!(grid.cell_w, ADVANCE_PX * scale);
            assert_eq!(grid.cell_h, FONT_SIZE_PX * LINE_HEIGHT_RATIO * scale);
            assert_ne!(grid.cell_w.fract(), 0.0);
            assert_ne!(grid.cell_h.fract(), 0.0);

            let cell = grid.ideal_cell(2, 3);
            assert_eq!(cell.x, grid.margin + 2.0 * grid.cell_w);
            assert_eq!(cell.y, grid.margin + 3.0 * grid.cell_h);
            assert_eq!(cell.w, grid.cell_w);
            assert_eq!(cell.h, grid.cell_h);
            assert_eq!(cell.right(), grid.margin + 3.0 * grid.cell_w);
            assert_eq!(cell.bottom(), grid.margin + 4.0 * grid.cell_h);

            let (width, height) = grid.target_size();
            let (grid_w, grid_h) = grid.grid_size();
            assert_eq!(width, (grid_w + 2.0 * grid.margin).ceil() as u32);
            assert_eq!(height, (grid_h + 2.0 * grid.margin).ceil() as u32);
            assert_eq!(grid.dpi_scale, scale);
        }
    }

    #[test]
    fn cpu_raster_paints_the_drawn_cells_and_nothing_else() {
        let pattern = AlignmentPattern::checkerboard(geometry(1.25));
        let pixels = rasterize_cpu(&pattern).expect("the CPU rasteriser sizes its own buffer");
        assert_eq!(
            (pixels.width(), pixels.height()),
            (pattern.width, pattern.height)
        );

        for cell in &pattern.cells {
            let x = (cell.ideal.x + cell.ideal.w * 0.5) as u32;
            let y = (cell.ideal.y + cell.ideal.h * 0.5) as u32;
            assert_eq!(
                pixels.coverage(x, y),
                Some(1.0),
                "drawn cell ({},{})",
                cell.col,
                cell.row
            );
            assert_eq!(cell.color[0], 1.0);
        }

        let skipped = geometry(1.25).ideal_cell(1, 0);
        let x = (skipped.x + skipped.w * 0.5) as u32;
        let y = (skipped.y + skipped.h * 0.5) as u32;
        assert_eq!(pixels.coverage(x, y), Some(0.0));
    }

    #[test]
    fn cpu_measurement_is_exact_and_far_below_the_contract() {
        for scale in RP05_DPI_SCALES {
            let pattern = AlignmentPattern::checkerboard(geometry(scale));
            let pixels = rasterize_cpu(&pattern).expect("the CPU rasteriser sizes its own buffer");
            let measured =
                measure_alignment(&pixels, &pattern).expect("an ideal CPU pattern must measure");
            println!("{measured}");

            assert_eq!(measured.samples, pattern.cells.len() * 4);
            assert!(measured.samples >= 48);
            assert!(measured.worst_px <= HARNESS_ALIGNMENT_CONTRACT_PX);
            assert!(
                measured.worst_px <= 0.05,
                "8-bit coverage quantisation alone must stay far below the 0.5px contract: {measured}"
            );
        }
    }

    #[test]
    fn cpu_measurement_detects_an_injected_sub_pixel_shift() {
        for scale in RP05_DPI_SCALES {
            let pattern = AlignmentPattern::shifted(geometry(scale), 0.75, -0.6);
            assert_eq!(pattern.injected_shift_px(), 0.75);
            let pixels = rasterize_cpu(&pattern).expect("the CPU rasteriser sizes its own buffer");
            let measured =
                measure_alignment(&pixels, &pattern).expect("a shifted pattern must measure");
            println!("injected 0.75px/-0.60px -> {measured}");

            assert!(
                measured.worst_px > HARNESS_ALIGNMENT_CONTRACT_PX,
                "the mechanism must catch an injected misalignment, not always report green: {measured}"
            );
            assert!(measured.worst_px < 1.0, "{measured}");
        }
    }

    #[test]
    fn every_drawn_quad_stays_inside_the_target() {
        for scale in RP05_DPI_SCALES {
            let pattern = AlignmentPattern::shifted(geometry(scale), 0.75, -0.6);
            let width = pattern.width as f32;
            let height = pattern.height as f32;
            for cell in &pattern.cells {
                assert!(cell.drawn.x >= 0.0 && cell.drawn.y >= 0.0);
                assert!(cell.drawn.right() <= width && cell.drawn.bottom() <= height);
                assert_ne!(cell.ideal, cell.drawn);
            }
        }
    }

    #[test]
    fn measurement_refuses_inputs_it_cannot_vouch_for() {
        let pattern = AlignmentPattern::checkerboard(geometry(1.0));

        let tiny = PixelBuffer::from_rgba8(2, 2, vec![0; 16]).expect("2x2 RGBA8 is 16 bytes");
        assert!(matches!(
            measure_alignment(&tiny, &pattern),
            Err(MeasureError::SizeMismatch { .. })
        ));

        assert_eq!(
            PixelBuffer::from_rgba8(2, 2, vec![0; 15]),
            Err(PixelBufferError::LengthMismatch {
                expected: 16,
                actual: 15
            })
        );
        assert_eq!(
            PixelBuffer::from_rgba8(0, 2, Vec::new()),
            Err(PixelBufferError::Empty)
        );

        let blank = vec![0u8; pattern.width as usize * pattern.height as usize * 4];
        let blank = PixelBuffer::from_rgba8(pattern.width, pattern.height, blank).expect("sized");
        assert!(matches!(
            measure_alignment(&blank, &pattern),
            Err(MeasureError::NoCoverage { .. })
        ));

        let empty = AlignmentPattern {
            cells: Vec::new(),
            ..AlignmentPattern::checkerboard(geometry(1.0))
        };
        assert_eq!(
            measure_alignment(&blank, &empty),
            Err(MeasureError::EmptyPattern)
        );
    }

    #[test]
    fn a_shift_beyond_the_window_is_refused_instead_of_measured() {
        // 1.5px is below the window pad but above a quarter cell only for tiny cells; the point
        // here is the opposite case: pushing the quad past the window must be an error, never a
        // number that could be mistaken for a pass.
        let pattern = AlignmentPattern::shifted(geometry(1.0), 3.0, 0.0);
        let pixels = rasterize_cpu(&pattern).expect("the CPU rasteriser sizes its own buffer");
        assert!(matches!(
            measure_alignment(&pixels, &pattern),
            Err(MeasureError::WindowClipped { .. })
        ));
    }
}
