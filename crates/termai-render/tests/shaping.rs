//! S6 shaping + S7 atlas against a **real** system font resolved through `fontdb`.
//!
//! These tests are the evidence for the two slices: they shape actual glyph ids out of an
//! actual font file and place them in actual atlas slots. There is no stub font and no silent
//! skip: if `fontdb` resolves no parseable system face at all, every test in this file fails
//! with an explicit message, because a green run that never loaded a font would prove nothing
//! about S6 or S7.
//!
//! **Font portability.** This file is a merge-blocking gate that also runs on `ubuntu-latest`
//! and `macos-latest`, where a monospaced face covering CJK (this host: BIZ UDGothic) commonly
//! does not exist. The evidence face is therefore resolved in three **printed** degradation
//! steps (see [`FontBranch`]), and every assertion that genuinely depends on the host font says
//! in its own message which step won and which face it used. Run with `-- --nocapture` to see
//! the `S6/S7 font resolution:` line and the per-branch reasons.
//!
//! Three families of assertion depend on the resolved face, and none is ever silently dropped:
//!
//! * *"this face really has a glyph for U+4E2D"* - on a face with coverage the shaper must
//!   report glyph id `!= 0` and `missing_glyphs == 0`; on a face without it the honest,
//!   documented outcome is glyph id `0` (`.notdef`) and `missing_glyphs == 1`. Both directions
//!   are asserted explicitly, so neither branch is skipped.
//! * *"a monospaced face's ASCII advance is the cell advance"* ([`an_ascii_row_needs_no_fit`]) -
//!   kept behind the monospaced branch with a printed reason, because deriving the expected
//!   advance from the same measurement the shaper used would make the assertion a tautology
//!   rather than a check that the face is a fixed-pitch face.
//! * *"a two-column CJK cluster needs no fit"* (the `fit_squeezed == 0` control in
//!   [`a_two_cell_ideograph_occupies_exactly_two_cells`] and in
//!   [`a_two_cell_glyph_in_a_one_cell_span_is_squeezed_and_the_control_is_not`]) - the same
//!   fixed-pitch property, kept behind the same monospaced branch and printed with its own
//!   reason, because a proportional face legitimately writes a CJK glyph wider than two of its
//!   narrow `"0"` advances. The assertions those two tests are *for* (the span never overflows
//!   its cells, the under-anchored row is refused) stay unconditional.
//!
//! The `fit_squeezed` **positive** case is unconditional: [`SQUEEZE_PROBE`]'s columns are
//! under-reported relative to an advance the face itself defines (the cell advance *is* this
//! face's `"0"`), so the counter is reachable on any face and no step needs the host to own a
//! wide glyph. `ubuntu-latest`, `macos-latest` and any normal desktop reach step 1 or step 2,
//! where every assertion in this file runs at full strength; the step-3-only host class (fonts
//! present, not one of them monospaced) prints what it cannot exercise and still asserts the
//! boundaries above.
//!
//! Column widths never come from the font: the K-04 spans are decided by the injected
//! [`CellWidthSource`] alone, so the two-cell cluster assertions hold in every step.
//!
//! The K-04 width port is bound here to its **production** implementation, `VtWidthSource`,
//! i.e. `termai_vt::width::measure` - the same rule `termai_vt::Grid::print` applies when it
//! places the cells. This file and the crate under test therefore carry no width table at all:
//! `unicode-width` left termai-render's manifest with the ADR-0027 D2 crate edge, so the
//! agreement checked below (`the_render_width_port_and_the_vt_grid_agree_on_every_cluster`)
//! cannot be an accident of two copies of the same lookup table.
//!
//! **S7 rasterisation (this slice).** The last block of tests drives the atlas's `swash` path:
//! an `AtlasKey` becomes an 8-bit alpha coverage bitmap, grayscale AA with hinting off (AR-14),
//! stored in the page at the slot the allocator handed out. The bitmap assertions are:
//! byte-identical output for the same key (and a different `px_size` producing a different
//! bitmap), ink for an ASCII glyph with dimensions matching the reported slot, a storage shape
//! of exactly one `u8` per pixel, a non-empty `.notdef` for an uncovered scalar, page byte
//! accounting equal to the sum of the stored bitmaps, and one configuration-pinning test
//! (`the_atlas_rasterises_with_the_ar14_settings`) that re-drives `swash` independently with
//! `.hint(false)` + `Format::Alpha` and compares bytes, so flipping either setting in `atlas.rs`
//! fails a test instead of silently changing every glyph on screen. Colour glyphs are refused
//! with a structured error ([`AtlasError::ColorGlyph`]); the test takes that branch, or prints
//! why this host cannot exercise it.
//!
//! **S6/S7 seam (this slice).** The block after that closes the loop RP-05 / OQ-RND-04 actually
//! name: a shaped row plus real atlas bitmaps become an explicit `glyph_bitmap_origin` and
//! `cell_box_origin` per drawn glyph, and
//! `max abs(glyph_bitmap_origin - cell_box_origin)` is measured at all four device scales the
//! contract judges (100/125/150/200%).
//!
//! That block is the one that also runs on `ubuntu-latest` / `macos-latest`, where a *different*
//! face is resolved (step 2 or step 3 above), so its assertions are split by what they are
//! actually a property of:
//!
//! * **Unconditional** (true of the placement on any face): every drawn glyph's cell-box
//!   deviation is `<= 0.5px` at all four scales; the sample count is the number of drawn glyphs
//!   and drawn + refused is the row's glyph count; a measurement over zero drawn glyphs is
//!   *refused* (`MeasureError::NoDrawnGlyphs`) rather than reported as a clean zero; the pen is
//!   centered in the cell box and sits on the font's baseline; the bitmap frame is the snapped
//!   cell origin and the ink box starts at the snapped pen plus the atlas's own bearing; the
//!   atlas key carries S6's `fit_scale`; and - plan section 6.3 rule 10 - a deliberate +1px
//!   misplacement exceeds the contract while the untouched placement does not.
//! * **Conditional, with a printed reason** (the claim holds only for particular metrics, so it is
//!   asserted wherever the resolved face exercises it and its absence is always printed): that the
//!   cell metrics are genuinely fractional; that the probe pattern can tell a rounding placement
//!   from a truncating one at that scale; and the strict "the drawn ink box is inside its cell box
//!   within one device pixel" reading of V-10, which the atlas's integer ink box can falsify on a
//!   face whose glyph ink is wider than, or offset inside, its cell (that excursion is the face's,
//!   not the placement's; see [`FRAME_SNAP_PX`] and the per-glyph printing in the test).
//!
//! The first two come from a **candidate search** over [`PLACEMENT_LOGICAL_SIZES`]
//! ([`probe_shape`]), which reports what it found per claim and per scale instead of assuming it:
//! the size it chooses is the best evidence configuration the face admits, and when no candidate
//! can exercise a claim the test prints that and still asserts the contract.

use std::path::PathBuf;
use std::sync::OnceLock;

use termai_render::atlas::{AtlasConfig, AtlasError, GlyphAtlas, GlyphBitmap, GlyphSize};
use termai_render::place::{
    measure_placement, place_row, place_row_with_vt_widths, placement_keys, CellMetrics,
    GlyphRefusal, MeasureError, PlacedRow, Point, HARNESS_ALIGNMENT_CONTRACT_PX, LINE_HEIGHT_RATIO,
    RP05_DPI_SCALES,
};
use termai_render::shape::{
    glyph_flag, shape_row, shape_row_with_vt_widths, AaMode, CellWidthSource, ClusterInput,
    FontFace, RowClusters, ShapeContext, ShapeError, VtWidthSource, VT_WIDTH,
};

/// Every test in this file shapes this transcript, so degradation step 1 asks for one face that
/// covers all of it.
const REQUIRED: &[char] = &[
    'T', 'e', 'r', 'm', 'A', 'I', ' ', '0', '1', '2', '3', '\u{4E2D}',
];

/// The ASCII control row: no cluster may need a fit.
const ASCII_ROW: &str = "TermAI 0123";

/// The CJK probe scalar whose glyph coverage picks the branch of the ideograph assertions.
/// It is `REQUIRED`'s only non-ASCII scalar: the one a minimal CI host lacks.
const IDEOGRAPH: char = '\u{4E2D}';

/// A cluster that provably overflows a **one**-cell span on **any** face.
///
/// Every context in this file measures its cell advance from this face's own `"0"` (see
/// [`context`], kernel/03 section 3.5.3), so this eleven-scalar ASCII run advances many cells at
/// once. No OpenType substitution collapses a run that long into a single cell, which is what
/// makes the `fit_squeezed` positive case reachable whether or not the host owns a wide glyph -
/// the thing the ideograph alone cannot do on a host without CJK coverage, where the shaper
/// honestly sees a narrow `.notdef` and does not squeeze it.
const SQUEEZE_PROBE: &str = ASCII_ROW;

/// The production K-04 port binding (see the module doc): termai-vt's width API.
const HONEST: VtWidthSource = VT_WIDTH;

/// A deliberately **wrong** width table, used only to prove the `fit_squeezed` counter can
/// move: it reports its one target cluster as a single column whatever it really measures, which
/// is exactly the width-table drift K-04 exists to catch. A counter that no input can move is
/// not evidence.
struct UnderReports(&'static str);

impl CellWidthSource for UnderReports {
    fn cluster_columns(&self, cluster: &str) -> u8 {
        if cluster == self.0 {
            1
        } else {
            HONEST.cluster_columns(cluster)
        }
    }
}

/// Under-reports the two-column ideograph as one column: the K-04 drift the port exists to catch.
const UNDER: UnderReports = UnderReports("\u{4E2D}");

/// Under-reports [`SQUEEZE_PROBE`] as one column: the font-independent way to move the counter.
const UNDER_PROBE: UnderReports = UnderReports(SQUEEZE_PROBE);

/// Which step of the printed degradation chain produced the evidence face.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FontBranch {
    /// Step 1: a monospaced face covering every scalar of [`REQUIRED`].
    MonospacedCovering,
    /// Step 2: a monospaced face that does **not** cover [`REQUIRED`] - the usual outcome on
    /// `ubuntu-latest` / `macos-latest`, where no monospaced face has CJK coverage.
    MonospacedOnly,
    /// Step 3: any parseable face - the host has no face `fontdb` calls monospaced at all.
    AnyFace,
}

impl FontBranch {
    /// The step, printed in the resolution line and in every font-dependent assertion message.
    fn label(self) -> &'static str {
        match self {
            Self::MonospacedCovering => "step 1/3: monospaced face covering the probe string",
            Self::MonospacedOnly => "step 2/3: monospaced face without full probe coverage",
            Self::AnyFace => "step 3/3: any parseable face (no monospaced face on this host)",
        }
    }
}

/// What the chosen face actually covers. Assertions are gated on these facts and never on the
/// branch that produced them: step 3 can still hand out a face that covers the ideograph.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Coverage {
    /// Every scalar of [`REQUIRED`], [`IDEOGRAPH`] included: the step-1 preference.
    probe: bool,
    /// [`IDEOGRAPH`] alone: selects the glyph-id / `missing_glyphs` branch of the S6 test.
    ideograph: bool,
    /// Every scalar of [`ASCII_ROW`]: selects the ASCII coverage assertions.
    ascii_row: bool,
}

struct ResolvedFont {
    id: fontdb::ID,
    index: u32,
    data: Vec<u8>,
    family: String,
    post_script_name: String,
    path: Option<PathBuf>,
    branch: FontBranch,
    monospaced: bool,
    coverage: Coverage,
}

impl ResolvedFont {
    fn describe(&self) -> String {
        format!(
            "branch={} family={:?} post_script_name={:?} path={:?} face_index={} \
             monospaced={} covers_probe={} covers_ideograph={} covers_ascii_row={}",
            self.branch.label(),
            self.family,
            self.post_script_name,
            self.path,
            self.index,
            self.monospaced,
            self.coverage.probe,
            self.coverage.ideograph,
            self.coverage.ascii_row
        )
    }
}

fn source_path(source: &fontdb::Source) -> Option<PathBuf> {
    match source {
        fontdb::Source::File(path) => Some(path.clone()),
        fontdb::Source::SharedFile(path, _) => Some(path.clone()),
        fontdb::Source::Binary(_) => None,
    }
}

/// Resolve one real system face in the three documented degradation steps, or fail loudly when
/// the host has no parseable font at all.
///
/// A panic here means "this host cannot run the S6/S7 evidence at all", which is honest: a green
/// run that never loaded a font would prove nothing. A host that merely lacks a monospaced CJK
/// face takes step 2 and keeps every assertion that does not need CJK coverage.
fn font() -> &'static ResolvedFont {
    static FONT: OnceLock<ResolvedFont> = OnceLock::new();
    FONT.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        assert!(
            !db.is_empty(),
            "fontdb resolved no system font at all; the S6/S7 tests need a real face and must \
             never silently skip"
        );
        // The chain, in preference order: (step, must be monospaced, must cover REQUIRED).
        for (branch, monospaced, covering) in [
            (FontBranch::MonospacedCovering, true, true),
            (FontBranch::MonospacedOnly, true, false),
            (FontBranch::AnyFace, false, false),
        ] {
            if let Some(resolved) = pick_face(&db, branch, monospaced, covering) {
                eprintln!("S6/S7 font resolution: {}", resolved.describe());
                if !resolved.monospaced {
                    // Not a skip: this is the documented statement of what a proportional face
                    // can and cannot demonstrate, printed before any assertion that depends on
                    // it runs. From here on every claim that only a fixed-pitch face can carry
                    // (an ASCII advance equal to the cell advance; a two-column CJK cluster
                    // fitting its two columns, whether the counter stays at zero or not) is
                    // printed with this branch and its own numbers instead of being asserted,
                    // while every boundary those tests exist for - the span never overflowing
                    // its cells, the under-anchored row being refused - still runs.
                    eprintln!(
                        "S6/S7 font resolution: WARNING {} resolved a proportional face: the \
                         fixed-pitch claims of this file (a face's ASCII advance is its cell \
                         advance, and a two-column CJK cluster fits exactly its two columns) are \
                         not exercisable here and each one is printed at its own assertion with \
                         the values this face produced; the hard boundaries still run",
                        resolved.branch.label()
                    );
                }
                return resolved;
            }
        }
        panic!(
            "fontdb found {} system faces but none could be parsed as an OpenType face; the \
             S6/S7 evidence requires a real font and must never silently skip",
            db.len()
        );
    })
}

/// One step of the chain: the first face that is monospaced (when required) and covers
/// [`REQUIRED`] (when required) wins, together with the coverage facts its assertions gate on.
fn pick_face(
    db: &fontdb::Database,
    branch: FontBranch,
    monospaced: bool,
    covering: bool,
) -> Option<ResolvedFont> {
    for face in db.faces() {
        if monospaced && !face.monospaced {
            continue;
        }
        let found = db.with_face_data(face.id, |data, index| {
            let parsed = rustybuzz::ttf_parser::Face::parse(data, index).ok()?;
            let coverage = Coverage {
                probe: REQUIRED.iter().all(|ch| parsed.glyph_index(*ch).is_some()),
                ideograph: parsed.glyph_index(IDEOGRAPH).is_some(),
                ascii_row: ASCII_ROW.chars().all(|ch| parsed.glyph_index(ch).is_some()),
            };
            Some((data.to_vec(), coverage))
        });
        let Some(Some((data, coverage))) = found else {
            continue;
        };
        if covering && !coverage.probe {
            continue;
        }
        return Some(ResolvedFont {
            id: face.id,
            index: face.index,
            data,
            family: face
                .families
                .first()
                .map_or_else(String::new, |(name, _)| name.clone()),
            post_script_name: face.post_script_name.clone(),
            path: source_path(&face.source),
            branch,
            monospaced: face.monospaced,
            coverage,
        });
    }
    None
}

fn cluster(col: u16, text: &str) -> ClusterInput {
    ClusterInput {
        col,
        text: text.to_owned(),
    }
}

/// A row of single-column ASCII clusters, with anchors computed from the honest table.
fn ascii_row(text: &str) -> RowClusters {
    let mut col = 0_u16;
    let mut clusters = Vec::new();
    for ch in text.chars() {
        let one = ch.to_string();
        clusters.push(cluster(col, &one));
        col += u16::from(HONEST.cluster_columns(&one));
    }
    RowClusters::new(0, 0, clusters)
}

/// The face's own cell advance at `px_size`, in device pixels, measured from its `"0"`
/// (kernel/03 section 3.5.3: `cell_w = "0" advance`).
///
/// [`context`] measures the same quantity with a 1 em provisional cell box and asserts that the
/// probe was not squeezed. That box is wide enough for every text face, but an *icon* face whose
/// `"0"` advances more than one em would be squeezed by it, and a squeezed probe reports a scaled
/// advance - i.e. the measurement would silently be of the wrong number. This probe widens the
/// provisional box (1, 2, 4, 8 em) until the shaped `"0"` is untouched, so the number it returns
/// is always the face's own advance, and refuses loudly only when no box up to 8 em can measure
/// it at all.
fn measure_advance_ratio(font: &ResolvedFont, px_size: u16) -> f32 {
    let face = FontFace::from_slice(font.id, &font.data, font.index)
        .expect("fontdb handed out a face that rustybuzz cannot parse");
    for ems in [1.0_f32, 2.0, 4.0, 8.0] {
        let ctx = ShapeContext {
            font: face.clone(),
            px_size,
            scale_q8: 256,
            aa: AaMode::Sharp,
            ligatures: true,
            cell_advance_px: ems * f32::from(px_size),
            row_height_px: LINE_HEIGHT_RATIO * ems * f32::from(px_size),
        };
        let probe = shape_row(
            &RowClusters::new(0, 0, vec![cluster(0, "0")]),
            &ctx,
            &HONEST,
        )
        .expect("the cell-advance probe must shape");
        if probe.fit_squeezed != 0 {
            continue;
        }
        assert!(
            probe.spans[0].advance_px > 0.0,
            "the font's '0' has no advance; the cell box cannot be derived (font: {})",
            font.describe()
        );
        return probe.spans[0].advance_px / f32::from(px_size);
    }
    panic!(
        "the font's '0' is still squeezed by an 8em cell box, so its cell advance cannot be \
         measured unscaled (font: {})",
        font.describe()
    );
}

/// The S6 context for one device pixel size. The cell advance is measured from the font's own
/// `"0"` (kernel/03 section 3.5.3), never assumed.
fn context(font: &ResolvedFont, px_size: u16) -> ShapeContext<'_> {
    let face = FontFace::from_slice(font.id, &font.data, font.index)
        .expect("fontdb handed out a face that rustybuzz cannot parse");
    let mut ctx = ShapeContext {
        font: face,
        px_size,
        scale_q8: 256,
        aa: AaMode::Sharp,
        ligatures: true,
        // 1 em provisional: wide enough that the probe below cannot be squeezed.
        cell_advance_px: f32::from(px_size),
        row_height_px: 1.25 * f32::from(px_size),
    };
    let probe = shape_row(
        &RowClusters::new(0, 0, vec![cluster(0, "0")]),
        &ctx,
        &HONEST,
    )
    .expect("the cell-advance probe must shape");
    assert_eq!(
        probe.fit_squeezed, 0,
        "the '0' probe must not be squeezed, otherwise the measured cell advance is scaled"
    );
    assert!(
        probe.spans[0].advance_px > 0.0,
        "the font's '0' has no advance; the cell box cannot be derived"
    );
    ctx.cell_advance_px = probe.spans[0].advance_px;
    ctx
}

#[test]
fn a_two_cell_ideograph_occupies_exactly_two_cells() {
    let font = font();
    let ctx = context(font, 16);
    let row = RowClusters::new(0, 0, vec![cluster(0, "\u{4E2D}"), cluster(2, "A")]);
    let glyphs = shape_row(&row, &ctx, &HONEST).expect("the row must shape");

    assert_eq!(glyphs.validate(), Ok(()));
    assert_eq!(
        glyphs.spans[0].cells, 2,
        "U+4E2D measures two columns in the width table (K-04)"
    );
    assert_eq!(glyphs.spans[0].start_cell, 0);
    assert_eq!(glyphs.spans[0].end_cell, 2);
    assert_eq!(
        glyphs.cluster_span(0),
        Some(0..2),
        "the ideograph occupies exactly cells 0..2"
    );
    assert_eq!(
        glyphs.spans[0].end_cell, glyphs.spans[1].start_cell,
        "a cluster span may never overlap or skip its successor"
    );
    let span_px = 2.0 * ctx.cell_advance_px;
    assert!(
        glyphs.spans[0].advance_px <= span_px + 1.0e-3,
        "the cluster must never overflow its own cell span: advance {} > span {}",
        glyphs.spans[0].advance_px,
        span_px
    );
    // Face-dependent, and the reason is printed rather than the claim being dropped: "a two-column
    // glyph fits two cells" says the face's CJK glyph is no wider than two of its *own* `"0"`
    // advances, which is the definition of a fixed-pitch face. A proportional face (step 3, a host
    // with no monospaced face at all) legitimately writes a CJK glyph wider than its narrow `"0"`,
    // and the fit counter then moves for the face's reason rather than because of a width drift.
    // Asserted on every fixed-pitch face; the span-overflow assertion above is unconditional.
    if font.monospaced {
        assert_eq!(
            glyphs.fit_squeezed,
            0,
            "a two-column glyph fits two cells in a fixed-pitch face ({}; font: {})",
            font.branch.label(),
            font.describe()
        );
    } else {
        eprintln!(
            "a_two_cell_ideograph_occupies_exactly_two_cells: {} - the resolved face is not \
             fixed-pitch, so 'a two-column glyph fits two cells' cannot hold as a face property \
             (its CJK glyph is wider than two of its own `0` advances); the fit counter reports {} \
             here and the span still never overflows its two cells, which is asserted above (font: \
             {})",
            font.branch.label(),
            glyphs.fit_squeezed,
            font.describe()
        );
    }
    assert_eq!(glyphs.covered_cells, 3);

    // The one pair of facts in this test that depends on the resolved face rather than on the
    // injected width table. Which direction runs is a printed property of that face, never a
    // silent skip: `-- --nocapture` shows the `S6/S7 font resolution:` line and the reason below.
    let glyph_id = glyphs.spans[0].glyphs[0].glyph_id;
    if font.coverage.ideograph {
        assert!(
            glyphs.spans[0].advance_px > ctx.cell_advance_px + 1.0e-3,
            "a real two-column glyph must advance more than one cell, got {} vs cell {} \
             (font: {})",
            glyphs.spans[0].advance_px,
            ctx.cell_advance_px,
            font.describe()
        );
        assert_ne!(
            glyph_id,
            0,
            "the resolved face really has a glyph for {IDEOGRAPH:?} (font: {})",
            font.describe()
        );
        assert_eq!(
            glyphs.missing_glyphs,
            0,
            "a face that covers {IDEOGRAPH:?} must report no .notdef (font: {})",
            font.describe()
        );
    } else {
        eprintln!(
            "a_two_cell_ideograph_occupies_exactly_two_cells: {} - no glyph for U+4E2D, so the \
             shaper must draw .notdef: taking the documented missing-glyph branch instead of the \
             coverage branch",
            font.branch.label()
        );
        assert_eq!(
            glyph_id,
            0,
            "the shaper must report .notdef for {IDEOGRAPH:?} when the face has no glyph for it \
             (font: {})",
            font.describe()
        );
        assert_eq!(
            glyphs.missing_glyphs,
            1,
            "exactly one .notdef for the one uncovered scalar (font: {})",
            font.describe()
        );
    }
}

/// K-04 consolidation evidence (kernel/03 section 3.5.1 step 1: the column-width truth is
/// `termai-vt::width::measure(scalars)`). For a corpus of clusters covering ASCII, CJK wide,
/// combining marks, an emoji ZWJ sequence, a variation selector and C0 controls, the
/// render-side cluster -> cell result is the number the termai-vt grid itself decided when it
/// placed the same scalars, and the number `termai_vt::width` reports. One authority, read
/// three ways.
#[test]
fn the_render_width_port_and_the_vt_grid_agree_on_every_cluster() {
    // ASCII narrow, CJK wide, base + combining mark, a lone combining mark, an emoji ZWJ
    // sequence, an emoji, a variation selector and two C0 controls.
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

    for text in CORPUS {
        // termai-vt through its own print path: how many columns did the grid itself spend?
        let mut grid = termai_vt::Grid::new(200, 1);
        grid.print('x'); // a base cell, so a zero-width cluster has something to join
        for scalar in text.chars() {
            grid.print(scalar);
        }
        let grid_cells = grid.snapshot().cursor.pos.col - 1;
        // termai-vt through the public width API (kernel/03 section 3.5.1 step 1).
        let vt_cells = termai_vt::width::measure(text);
        // termai-render through the production CellWidthSource port (K-04).
        let render_cells = VT_WIDTH.cluster_columns(text);
        assert_eq!(
            u16::from(render_cells),
            grid_cells,
            "the render width port must report what the termai-vt grid spent on {text:?}"
        );
        assert_eq!(
            render_cells, vt_cells,
            "termai-render and termai_vt::width must be one authority for {text:?}"
        );
    }

    // The shaper turns those columns into spans. The anchors below come from the grid's own
    // cursor, so a width drift between the two authorities shows up as a ColumnMismatch rather
    // than as a green test.
    let font = font();
    let ctx = context(font, 16);
    let mut grid = termai_vt::Grid::new(200, 1);
    let mut inputs = Vec::new();
    for text in CORPUS {
        let anchor = grid.snapshot().cursor.pos.col;
        for scalar in text.chars() {
            grid.print(scalar);
        }
        if grid.snapshot().cursor.pos.col == anchor {
            continue; // zero columns: the cluster joins its predecessor, nothing to shape alone
        }
        inputs.push(cluster(anchor, text));
    }
    let row = RowClusters::new(0, 0, inputs);
    assert_eq!(
        row.clusters.len(),
        7,
        "seven corpus clusters own at least one column"
    );
    let glyphs = shape_row_with_vt_widths(&row, &ctx).expect("the corpus row must shape");
    assert_eq!(glyphs.validate(), Ok(()));
    assert_eq!(glyphs.spans.len(), row.clusters.len());
    for (span, input) in glyphs.spans.iter().zip(&row.clusters) {
        assert_eq!(
            span.start_cell, input.col,
            "the span must start at the grid's own column for {:?}",
            input.text
        );
        assert_eq!(
            u16::from(span.cells),
            u16::from(termai_vt::width::measure(&input.text)),
            "the span width must be the termai-vt width of {:?}",
            input.text
        );
    }
}

#[test]
fn shaping_the_same_row_twice_is_byte_identical() {
    let font = font();
    let row = RowClusters::new(0, 0, vec![cluster(0, "\u{4E2D}"), cluster(2, "A")]);
    let first = shape_row(&row, &context(font, 16), &HONEST).expect("the row must shape");
    let second = shape_row(&row, &context(font, 16), &HONEST).expect("the row must shape");
    assert_eq!(first, second, "shaping is a pure function of its inputs");
    assert_eq!(
        first.canonical_bytes(),
        second.canonical_bytes(),
        "same input + same font must give byte-identical output"
    );
    // A second, independently parsed face instance must not change a byte either.
    let third = shape_row(&row, &context(font, 16), &HONEST).expect("the row must shape");
    assert_eq!(first.canonical_bytes(), third.canonical_bytes());
}

#[test]
fn an_ascii_row_needs_no_fit() {
    let font = font();
    let ctx = context(font, 16);
    let glyphs = shape_row(&ascii_row(ASCII_ROW), &ctx, &HONEST).expect("the row must shape");
    assert_eq!(glyphs.validate(), Ok(()));
    // Font-independent: these cell spans come from the injected width table, never from the face.
    assert_eq!(glyphs.spans.len(), ASCII_ROW.chars().count());
    for span in &glyphs.spans {
        assert_eq!(span.cells, 1);
        assert_eq!(span.end_cell - span.start_cell, 1);
    }
    // Face-dependent: "one ASCII scalar advances exactly one cell, so nothing has to fit" is the
    // definition of a fixed-pitch face, and only degradation steps 1-2 can demonstrate it.
    // Deriving the expected advance from the shaper's own measurement would turn this into a
    // tautology, so the honest option is to print why it is not asserted when the host has no
    // monospaced face at all - and to keep asserting it whenever it does.
    if font.monospaced {
        assert_eq!(
            glyphs.fit_squeezed,
            0,
            "an ASCII row in a monospaced face must not move the fit counter ({}; font: {})",
            font.branch.label(),
            font.describe()
        );
        for span in &glyphs.spans {
            assert!(!span.is_squeezed());
        }
    } else {
        eprintln!(
            "an_ascii_row_needs_no_fit: {} - a non-monospaced face has no fixed ASCII advance \
             equal to the cell advance, so the fit-counter and per-span squeeze assertions are \
             not asserted on this host; the span/cell assertions above still are",
            font.branch.label()
        );
        for span in &glyphs.spans {
            assert!(
                span.advance_px <= ctx.cell_advance_px + 1.0e-3,
                "even a proportional face must not overflow its one-cell span: {} > {}",
                span.advance_px,
                ctx.cell_advance_px
            );
        }
    }
    if font.coverage.ascii_row {
        assert_eq!(
            glyphs.missing_glyphs,
            0,
            "a face covering the ASCII row must report no .notdef (font: {})",
            font.describe()
        );
    } else {
        eprintln!(
            "an_ascii_row_needs_no_fit: {} - the resolved face does not cover every scalar of \
             {ASCII_ROW:?}, so the ASCII coverage assertion is not asserted on this host",
            font.branch.label()
        );
    }
}

#[test]
fn a_two_cell_glyph_in_a_one_cell_span_is_squeezed_and_the_control_is_not() {
    let font = font();
    let ctx = context(font, 16);

    // Positive case 1 of 2, unconditional on every host: the SQUEEZE_PROBE run under-reports to
    // one column, but its shaped advance is many cells because `context` defines the cell
    // advance as this same face's `"0"` advance. That makes the counter's reachability a
    // property of the port and the face metrics rather than of the host owning a wide glyph.
    let probe_row = RowClusters::new(0, 0, vec![cluster(0, SQUEEZE_PROBE), cluster(1, "A")]);
    assert_eq!(
        shape_row(&probe_row, &ctx, &HONEST),
        Err(ShapeError::ColumnMismatch {
            index: 1,
            anchor: 1,
            expected: u16::from(HONEST.cluster_columns(SQUEEZE_PROBE)),
        }),
        "the probe row must really be under-anchored: the honest table measures {} columns",
        HONEST.cluster_columns(SQUEEZE_PROBE)
    );
    let probe = shape_row(&probe_row, &ctx, &UNDER_PROBE).expect("the probe row must shape");
    // The counter's reachability is "at least one cluster had to be scaled down", asserted here
    // for the probe cluster itself rather than as an exact row total: how many of the row's
    // clusters a proportional face also squeezes is a face property, not port evidence.
    assert!(
        probe.fit_squeezed >= 1,
        "the fit counter must be reachable on any resolved face ({}; font: {})",
        font.branch.label(),
        font.describe()
    );
    assert!(probe.spans[0].is_squeezed());
    assert!(
        probe.spans[0].fit_scale < 1.0,
        "the fit scale must be recorded for S7, got {}",
        probe.spans[0].fit_scale
    );
    assert_eq!(probe.spans[0].cells, 1);
    assert_eq!(
        probe.spans[0].end_cell - probe.spans[0].start_cell,
        1,
        "the span still never exceeds the measured width"
    );
    assert!(
        probe.spans[0].advance_px <= ctx.cell_advance_px + 1.0e-3,
        "a squeezed cluster is scaled back into its cell: {} > {}",
        probe.spans[0].advance_px,
        ctx.cell_advance_px
    );
    assert_eq!(probe.validate(), Ok(()));

    // Positive case 2 of 2, the original K-04 drift: the width table under-reports U+4E2D as one
    // column, so the row's own anchors follow it.
    let narrow = RowClusters::new(0, 0, vec![cluster(0, "\u{4E2D}"), cluster(1, "A")]);
    let squeezed = shape_row(&narrow, &ctx, &UNDER).expect("the row must shape");
    assert_eq!(
        squeezed.spans[0].end_cell - squeezed.spans[0].start_cell,
        1,
        "the span still never exceeds the measured width"
    );
    assert!(
        squeezed.spans[0].advance_px <= ctx.cell_advance_px + 1.0e-3,
        "a squeezed glyph is scaled back into its cell: {} > {}",
        squeezed.spans[0].advance_px,
        ctx.cell_advance_px
    );
    assert_eq!(squeezed.validate(), Ok(()));
    if font.coverage.ideograph {
        // The counter's reachability is asserted on the cluster itself: the row *total* also
        // counts whatever else a proportional face had to squeeze, which is a face property.
        assert!(
            squeezed.fit_squeezed >= 1,
            "the fit counter must be reachable: a two-cell glyph in a one-cell span has to fit \
             (font: {})",
            font.describe()
        );
        assert!(squeezed.spans[0].is_squeezed());
        assert!(
            squeezed.spans[0].fit_scale < 1.0,
            "the fit scale must be recorded for S7, got {}",
            squeezed.spans[0].fit_scale
        );
    } else {
        eprintln!(
            "a_two_cell_glyph_in_a_one_cell_span_is_squeezed_and_the_control_is_not: {} - no \
             glyph for U+4E2D, so the shaper draws a narrow .notdef that a one-cell span does not \
             have to squeeze; the counter's reachability is asserted unconditionally by the \
             SQUEEZE_PROBE case above, and the documented missing-glyph facts are asserted here",
            font.branch.label()
        );
        assert_eq!(
            squeezed.spans[0].glyphs[0].glyph_id,
            0,
            "an uncovered scalar must shape to .notdef (font: {})",
            font.describe()
        );
        assert_eq!(
            squeezed.missing_glyphs,
            1,
            "exactly one .notdef for the one uncovered scalar (font: {})",
            font.describe()
        );
    }

    // Control: the same text with the honest table stays at zero - on a fixed-pitch face. This is
    // the same face property [`a_two_cell_ideograph_occupies_exactly_two_cells`] prints: a
    // proportional face writes the CJK glyph wider than its two narrow `"0"` cells, so the counter
    // moves for the face's reason and not because the width table drifted. Asserted whenever the
    // resolved face is fixed-pitch, printed with its numbers otherwise.
    let wide = RowClusters::new(0, 0, vec![cluster(0, "\u{4E2D}"), cluster(2, "A")]);
    let control = shape_row(&wide, &ctx, &HONEST).expect("the row must shape");
    if font.monospaced {
        assert_eq!(
            control.fit_squeezed,
            0,
            "the control row must not move the counter ({}; font: {})",
            font.branch.label(),
            font.describe()
        );
    } else {
        eprintln!(
            "a_two_cell_glyph_in_a_one_cell_span_is_squeezed_and_the_control_is_not: {} - the \
             resolved face is not fixed-pitch, so the control row's honest-table fit counter is a \
             face property rather than a width-table one; it reports {} here and the \
             under-anchored row is still refused by the honest table below, which is the claim \
             this control guards (font: {})",
            font.branch.label(),
            control.fit_squeezed,
            font.describe()
        );
    }

    // And the shaper refuses the under-anchored row when the honest table is used: this is the
    // cluster/column assertion at the S6 entry point (kernel/03 RP-07, K-04).
    assert_eq!(
        shape_row(&narrow, &ctx, &HONEST),
        Err(ShapeError::ColumnMismatch {
            index: 1,
            anchor: 1,
            expected: 2,
        })
    );
}

#[test]
fn the_atlas_returns_the_same_slot_for_the_same_key_and_a_new_slot_for_a_new_px_size() {
    let font = font();
    let ctx = context(font, 16);
    let glyphs = shape_row(&ascii_row(ASCII_ROW), &ctx, &HONEST).expect("the row must shape");
    let glyph_id = glyphs.spans[0].glyphs[0].glyph_id;
    // The slot assertions below do not depend on which glyph id this is, but keying a real
    // outline is the point of the probe, so the coverage direction is asserted explicitly.
    if font.coverage.ascii_row {
        assert_ne!(
            glyph_id,
            0,
            "the probe must be a real glyph of a face covering {ASCII_ROW:?} (font: {})",
            font.describe()
        );
    } else {
        eprintln!(
            "the_atlas_returns_the_same_slot_for_the_same_key_and_a_new_slot_for_a_new_px_size: \
             {} - the resolved face does not cover every scalar of {ASCII_ROW:?}, so the probe \
             keys .notdef (glyph id 0); the same-key / different-key assertions below hold for \
             either id",
            font.branch.label()
        );
    }

    let mut atlas = GlyphAtlas::new(AtlasConfig::default());
    let size = GlyphSize::new(12, 20);
    let first = atlas
        .get_or_insert(ctx.atlas_key(glyph_id), size)
        .expect("the slot must fit a 1024x1024 page");
    let same = atlas
        .get_or_insert(ctx.atlas_key(glyph_id), size)
        .expect("the slot must fit a 1024x1024 page");
    assert_eq!(
        first,
        same,
        "the same AtlasKey must return the same slot (font: {})",
        font.describe()
    );

    let other = context(font, 18);
    assert_ne!(other.atlas_key(glyph_id), ctx.atlas_key(glyph_id));
    let different = atlas
        .get_or_insert(other.atlas_key(glyph_id), size)
        .expect("the slot must fit a 1024x1024 page");
    assert_ne!(
        first, different,
        "a different px_size is a different bitmap and must not reuse the slot"
    );
    assert_eq!(first.gen, atlas.generation());
    assert_eq!(atlas.misses(), 2);
    assert_eq!(atlas.hits(), 1);
    assert_eq!(atlas.page_count(), 1);
    assert_eq!(atlas.used_bytes(), 1024 * 1024);
}

#[test]
fn the_mirror_row_materialises_into_clusters_the_shaper_accepts() {
    use termai_core::grid::{Cell, GridSnapshot};

    let font = font();
    let ctx = context(font, 16);
    let mut grid = GridSnapshot::new(6, 1);
    for (col, ch) in "ab\u{4E2D}".chars().enumerate() {
        grid.cells[col] = Cell { ch, ..Cell::BLANK };
    }
    // The wide character occupies its lead cell plus the continuation cell, as the S5 DTO
    // expresses it (kernel/03 section 3.2.1 item 1).
    grid.cells[3] = Cell::WIDE_CONTINUATION;
    let row = RowClusters::from_snapshot(&grid, 0).expect("row 0 exists");
    assert_eq!(row.clusters.len(), 3, "trailing blanks are trimmed (used)");
    assert_eq!(row.clusters[2].col, 2);
    assert_eq!(row.clusters[2].text, "\u{4E2D}");
    let glyphs = shape_row(&row, &ctx, &HONEST).expect("the row must shape");
    assert_eq!(glyphs.validate(), Ok(()));
    assert_eq!(glyphs.cluster_span(2), Some(2..4));
    assert_eq!(glyphs.spans.len(), 3);
}

// ---------------------------------------------------------------------------------------------
// S7 rasterisation (kernel/03 section 3.4 `Rasterizer::rasterize`, K-05, AR-14).
//
// Everything below runs on the CPU with no GPU, no window and no swapchain: an atlas page is a
// plain `Vec<u8>` in the `R8Unorm` layout the S8 upload will read. The font is still the real
// face resolved by the printed degradation chain at the top of this file.
// ---------------------------------------------------------------------------------------------

/// The S6 -> S7 probe glyph: shape `"0"` with the production width port and take the glyph id S6
/// itself decided. S7 must key that id - a second, independent cmap lookup here could disagree
/// with what the shaper draws, which is exactly the drift the atlas key exists to prevent.
fn probe_glyph_id(ctx: &ShapeContext<'_>) -> u32 {
    let glyphs = shape_row(&ascii_row("0"), ctx, &HONEST).expect("the '0' probe must shape");
    glyphs.spans[0].glyphs[0].glyph_id
}

/// A bitmap digest line for `-- --nocapture` evidence: geometry, placement and the byte digest.
fn describe_bitmap(bitmap: &GlyphBitmap) -> String {
    format!(
        "key(glyph={} px={} aa={:?}) slot(page={} x={} y={} w={} h={} gen={}) \
         bearing=({},{}) bytes={} opaque={} digest={:#018x}",
        bitmap.key.glyph_id,
        bitmap.key.px_size,
        bitmap.key.aa_mode,
        bitmap.slot.page,
        bitmap.slot.x,
        bitmap.slot.y,
        bitmap.width,
        bitmap.height,
        bitmap.slot.gen,
        bitmap.left,
        bitmap.top,
        bitmap.data.len(),
        bitmap.opaque_pixels(),
        bitmap.digest()
    )
}

#[test]
fn an_atlas_bitmap_is_byte_identical_for_the_same_key() {
    let font = font();
    let ctx = context(font, 16);
    let glyph_id = probe_glyph_id(&ctx);
    let key = ctx.atlas_key(glyph_id);

    let mut atlas = GlyphAtlas::new(AtlasConfig::default());
    let first = atlas
        .rasterize(key, &ctx.font)
        .expect("the probe glyph must rasterise");
    let again = atlas
        .rasterize(key, &ctx.font)
        .expect("the probe glyph must rasterise again");
    assert_eq!(
        first.data,
        again.data,
        "the same AtlasKey must yield byte-identical coverage (font: {})",
        font.describe()
    );
    assert_eq!(first.slot, again.slot, "and the identical slot");
    assert_eq!(first.digest(), again.digest());
    assert_eq!(atlas.hits(), 1, "the second call is a bitmap hit");
    assert_eq!(atlas.misses(), 1);

    // A second atlas and a second, independently parsed face must not change a pixel either.
    // This is the determinism the RP-08/RP-10 style golden checks need.
    let other_ctx = context(font, 16);
    let mut second_atlas = GlyphAtlas::new(AtlasConfig::default());
    let second = second_atlas
        .rasterize(
            other_ctx.atlas_key(probe_glyph_id(&other_ctx)),
            &other_ctx.font,
        )
        .expect("the probe glyph must rasterise in a fresh atlas");
    assert_eq!(
        first.data,
        second.data,
        "an independent face parse and atlas must give the same pixels (font: {})",
        font.describe()
    );
    assert_eq!(first.digest(), second.digest());
    assert_eq!(first.left, second.left);
    assert_eq!(first.top, second.top);
    eprintln!("S7 atlas bitmap A: {}", describe_bitmap(&first));
    eprintln!("S7 atlas bitmap B: {}", describe_bitmap(&second));
}

#[test]
fn a_different_px_size_rasterises_a_different_bitmap() {
    let font = font();
    let small_ctx = context(font, 16);
    let large_ctx = context(font, 32);
    let small_glyph = probe_glyph_id(&small_ctx);
    let large_glyph = probe_glyph_id(&large_ctx);
    assert_eq!(
        small_glyph, large_glyph,
        "the probe glyph id must not depend on the device pixel size"
    );

    let mut atlas = GlyphAtlas::new(AtlasConfig::default());
    let small = atlas
        .rasterize(small_ctx.atlas_key(small_glyph), &small_ctx.font)
        .expect("the 16px probe must rasterise");
    let large = atlas
        .rasterize(large_ctx.atlas_key(large_glyph), &large_ctx.font)
        .expect("the 32px probe must rasterise");

    assert_ne!(small.key, large.key, "px_size is part of the key");
    assert_ne!(
        small.data,
        large.data,
        "a different px_size must yield a different bitmap, not a reused one (font: {})",
        font.describe()
    );
    assert_ne!(small.digest(), large.digest());
    assert_ne!(small.slot, large.slot, "and its own slot");
    assert!(
        large.width > small.width || large.height > small.height,
        "a 32px glyph must not rasterise to the 16px ink box: small={:?} large={:?}",
        (small.width, small.height),
        (large.width, large.height)
    );
    assert_eq!(atlas.misses(), 2);
    assert_eq!(atlas.hits(), 0);
    eprintln!("S7 atlas bitmap 16px: {}", describe_bitmap(&small));
    eprintln!("S7 atlas bitmap 32px: {}", describe_bitmap(&large));
}

#[test]
fn an_ascii_glyph_has_ink_and_its_dimensions_match_its_slot() {
    let font = font();
    let ctx = context(font, 16);
    let glyph_id = probe_glyph_id(&ctx);
    // The one face-dependent fact in this test. Which direction runs is printed, never skipped:
    // `-- --nocapture` shows the `S6/S7 font resolution:` line above and the reason below.
    if font.coverage.ascii_row {
        assert_ne!(
            glyph_id,
            0,
            "the probe must be a real ASCII glyph of a face covering {ASCII_ROW:?} (font: {})",
            font.describe()
        );
    } else {
        eprintln!(
            "an_ascii_glyph_has_ink_and_its_dimensions_match_its_slot: {} - the resolved face \
             does not cover every scalar of {ASCII_ROW:?}, so the probe keys .notdef (glyph id 0); \
             the ink and dimension assertions below hold for either id, and .notdef is exactly \
             what S6 hands S7 in this case",
            font.branch.label()
        );
    }

    let mut atlas = GlyphAtlas::new(AtlasConfig::default());
    let bitmap = atlas
        .rasterize(ctx.atlas_key(glyph_id), &ctx.font)
        .expect("the probe glyph must rasterise");
    assert!(
        !bitmap.is_empty(),
        "an ASCII glyph must produce a bitmap with an ink box (font: {})",
        font.describe()
    );
    assert!(
        bitmap.has_ink(),
        "an ASCII glyph must produce at least one nonzero coverage byte (font: {})",
        font.describe()
    );
    // Anti-aliased coverage with body, not a stray byte. Note what is deliberately NOT asserted:
    // that some pixel reaches full coverage. Measured on this host with the degradation chain
    // forced to step 3, a light proportional face (Alibaba PuHuiTi 3.0 Light) rasterises '0' at
    // 16px with peak coverage 253 and **zero** fully covered pixels, so "opaque_pixels() > 0" is
    // a font property rather than an S7 property. Peak and ink mass are.
    let mass: u64 = bitmap.data.iter().map(|byte| u64::from(*byte)).sum();
    assert!(
        bitmap.data.iter().any(|byte| *byte >= 128),
        "the rasterised glyph must have substantially covered pixels, peak coverage was {} \
         (font: {})",
        bitmap.data.iter().copied().max().unwrap_or(0),
        font.describe()
    );
    assert!(
        mass >= u64::from(bitmap.width) * u64::from(bitmap.height) * 16,
        "the ink must have body: coverage mass {mass} over {} pixels is {:.1} per pixel \
         (font: {})",
        bitmap.data.len(),
        mass as f64 / bitmap.data.len().max(1) as f64,
        font.describe()
    );
    // The dimensions the atlas reports *are* the bitmap's dimensions: the slot is the ink box,
    // so a caller can never read a rect that does not cover its pixels.
    assert_eq!(bitmap.width, bitmap.slot.width);
    assert_eq!(bitmap.height, bitmap.slot.height);
    assert_eq!(
        bitmap.data.len(),
        usize::from(bitmap.width) * usize::from(bitmap.height),
        "one coverage byte per pixel of the reported ink box"
    );
    // ...and they are consistent with the metric the key carries: device pixels per em. A
    // rasterisation at the wrong unit (unscaled font units, a 64x scale_q6 mix-up) would come
    // back hundreds of pixels wide here.
    assert!(
        bitmap.width > 0 && u32::from(bitmap.width) <= 2 * u32::from(ctx.px_size),
        "a {}px glyph must rasterise to at most {}px wide, got {} (font: {})",
        ctx.px_size,
        2 * u32::from(ctx.px_size),
        bitmap.width,
        font.describe()
    );
    assert!(
        bitmap.height > 0 && u32::from(bitmap.height) <= 2 * u32::from(ctx.px_size),
        "a {}px glyph must rasterise to at most {}px tall, got {} (font: {})",
        ctx.px_size,
        2 * u32::from(ctx.px_size),
        bitmap.height,
        font.describe()
    );
    assert!(
        bitmap.top > 0,
        "the ink box must sit above the baseline (swash placement top = {}), got {}",
        bitmap.height,
        bitmap.top
    );
    // The bitmap really is on the page at the slot the atlas reported: read the same rows out of
    // the raw page buffer and compare.
    let stride = usize::from(atlas.config().page_size);
    let page = atlas.page_pixels(bitmap.slot.page);
    assert_eq!(page.len(), usize::from(atlas.config().page_size).pow(2));
    for row in 0..usize::from(bitmap.height) {
        let start = (usize::from(bitmap.slot.y) + row) * stride + usize::from(bitmap.slot.x);
        let width = usize::from(bitmap.width);
        assert_eq!(
            &page[start..start + width],
            &bitmap.data[row * width..(row + 1) * width],
            "row {row} of the slot must hold the rasterised row on the page"
        );
    }
    eprintln!("S7 atlas ink: {}", describe_bitmap(&bitmap));
}

#[test]
fn atlas_coverage_is_eight_bit_single_channel() {
    let font = font();
    let ctx = context(font, 16);
    let glyph_id = probe_glyph_id(&ctx);
    let mut atlas = GlyphAtlas::new(AtlasConfig::default());
    let bitmap = atlas
        .rasterize(ctx.atlas_key(glyph_id), &ctx.font)
        .expect("the probe glyph must rasterise");

    // The storage shape, not the values: `data` is one `u8` per pixel, so a three-sample subpixel
    // mask or a four-byte RGBA bitmap cannot be represented by this API at all.
    let bytes: &[u8] = bitmap.data.as_slice();
    assert_eq!(
        bytes.len(),
        usize::from(bitmap.width) * usize::from(bitmap.height),
        "an 8-bit mask is exactly one byte per pixel, never 3 or 4 (font: {})",
        font.describe()
    );
    assert_eq!(
        u32::try_from(bytes.len()).unwrap() % u32::from(bitmap.width).max(1),
        0,
        "the coverage buffer must divide evenly into rows"
    );
    // The page is `R8Unorm` 1024x1024 = 1 MiB (kernel/03 section 3.4). An RGBA or subpixel page
    // would be 4 MiB / 3 MiB: the page size is itself a single-channel assertion.
    let page = atlas.page_pixels(bitmap.slot.page);
    assert_eq!(
        page.len(),
        1024 * 1024,
        "a text page is one byte per pixel: 1024x1024 R8Unorm (font: {})",
        font.describe()
    );
    assert_eq!(atlas.used_bytes(), 1024 * 1024);
    // The bytes read back out of the page are the bytes that were rasterised.
    let read_back = atlas
        .bitmap(&bitmap.key)
        .expect("a stored bitmap must be readable");
    assert_eq!(read_back, bitmap, "the page round-trips the bitmap");
    // Grayscale AA, not a 1-bit mask: an anti-aliased unhinted outline leaves partial coverage.
    assert!(
        bitmap.data.iter().any(|byte| *byte != 0 && *byte != 255),
        "every coverage byte is 0 or 255, which is a 1-bit mask rather than AR-14's grayscale AA \
         (font: {})",
        font.describe()
    );
    eprintln!(
        "S7 atlas channel shape: {} bytes = {}x{} of u8; page {} bytes",
        bitmap.data.len(),
        bitmap.width,
        bitmap.height,
        page.len()
    );
}

#[test]
fn an_uncovered_scalar_rasterises_a_notdef_with_ink() {
    let font = font();
    let ctx = context(font, 16);

    // A noncharacter: no text face maps it, so S6 must report `.notdef` for it. The candidate
    // list exists only so the test cannot fail because some exotic face happens to map one of
    // them; whichever scalar is picked, the assertion is that S6 said glyph id 0.
    const NONCHARACTERS: &[char] = &['\u{10FFFD}', '\u{FDD0}', '\u{0FFFF}', '\u{E0001}'];
    let parses = rustybuzz::ttf_parser::Face::parse(&font.data, font.index).is_ok();
    assert!(
        parses,
        "the resolved face must be parseable for the coverage probe (font: {})",
        font.describe()
    );
    let uncovered = NONCHARACTERS.iter().copied().find(|scalar| {
        rustybuzz::ttf_parser::Face::parse(&font.data, font.index)
            .ok()
            .and_then(|face| face.glyph_index(*scalar))
            .is_none()
    });
    eprintln!(
        "an_uncovered_scalar_rasterises_a_notdef_with_ink: uncovered probe scalar = {uncovered:?} \
         (branch={})",
        font.branch.label()
    );

    let scalar = uncovered.unwrap_or('\u{10FFFD}');
    let row = RowClusters::new(0, 0, vec![cluster(0, &scalar.to_string())]);
    let glyphs = shape_row(&row, &ctx, &HONEST).expect("the uncovered row must shape");
    let glyph = glyphs.spans[0].glyphs[0];
    assert_eq!(
        glyph.glyph_id,
        0,
        "S6 must report .notdef (glyph id 0) for the uncovered scalar {scalar:?} (font: {})",
        font.describe()
    );
    assert!(
        glyph.flags & glyph_flag::MISSING != 0,
        "and must mark it MISSING so S7 is never asked to look up a real glyph"
    );

    let mut atlas = GlyphAtlas::new(AtlasConfig::default());
    match atlas.rasterize(ctx.atlas_key(0), &ctx.font) {
        Ok(bitmap) => {
            // The documented non-empty branch: a `.notdef` box, not a silent empty slot.
            assert!(
                !bitmap.is_empty(),
                "the .notdef bitmap must have an ink box (font: {})",
                font.describe()
            );
            assert!(
                bitmap.has_ink(),
                "the .notdef bitmap must have ink; a stored all-zero slot would be a silent hole \
                 (font: {})",
                font.describe()
            );
            assert!(atlas.get(&ctx.atlas_key(0)).is_some());
            assert_eq!(
                atlas.bitmap_bytes(),
                bitmap.data.len(),
                "the accounting counts the .notdef bitmap too"
            );
            eprintln!("S7 atlas .notdef: {}", describe_bitmap(&bitmap));
        }
        Err(error) => {
            // The documented refusal branch: this face cannot produce a .notdef image at all.
            // What must NOT happen is an empty slot stored anyway.
            eprintln!(
                "an_uncovered_scalar_rasterises_a_notdef_with_ink: {} - the rasteriser produced \
                 no image for .notdef, so the documented structured refusal is asserted instead \
                 of an empty slot (font: {})",
                font.branch.label(),
                font.describe()
            );
            assert_eq!(
                error,
                AtlasError::NoOutline {
                    font_id: font.id,
                    glyph_id: 0,
                }
            );
            assert!(
                atlas.get(&ctx.atlas_key(0)).is_none(),
                "a refused glyph must leave no slot behind"
            );
            assert_eq!(atlas.bitmap_bytes(), 0);
        }
    }
}

#[test]
fn the_page_bitmap_accounting_equals_the_sum_of_the_stored_bitmaps() {
    let font = font();
    let mut atlas = GlyphAtlas::new(AtlasConfig::default());
    let mut stored: Vec<GlyphBitmap> = Vec::new();
    // Three sizes of the '0' probe plus one other ASCII glyph, so a page holds more than one
    // bitmap and the accounting is a sum rather than a single value.
    for px_size in [16_u16, 20, 24] {
        let ctx = context(font, px_size);
        let bitmap = atlas
            .rasterize(ctx.atlas_key(probe_glyph_id(&ctx)), &ctx.font)
            .expect("the probe glyph must rasterise");
        stored.push(bitmap);
        let other = shape_row(&ascii_row("1"), &ctx, &HONEST).expect("the '1' probe must shape");
        let other_id = other.spans[0].glyphs[0].glyph_id;
        stored.push(
            atlas
                .rasterize(ctx.atlas_key(other_id), &ctx.font)
                .expect("the '1' glyph must rasterise"),
        );
    }

    let expected: usize = stored.iter().map(|bitmap| bitmap.data.len()).sum();
    assert!(
        expected > 0,
        "the stored bitmaps must not all be empty (font: {})",
        font.describe()
    );
    assert_eq!(
        atlas.bitmap_bytes(),
        expected,
        "the atlas's bitmap accounting must be the sum of the stored bitmaps (font: {})",
        font.describe()
    );
    let per_page: usize = (0..u8::try_from(atlas.page_count()).unwrap())
        .map(|page| atlas.page_bitmap_bytes(page))
        .sum();
    assert_eq!(
        per_page, expected,
        "and the per-page accounting must sum to the same number"
    );
    for bitmap in &stored {
        assert_eq!(
            atlas.page_bitmap_bytes(bitmap.slot.page),
            stored
                .iter()
                .filter(|other| other.slot.page == bitmap.slot.page)
                .map(|other| other.data.len())
                .sum::<usize>(),
            "page {} must account for exactly the bitmaps it holds",
            bitmap.slot.page
        );
    }
    // The page cost is the page's, not the bitmaps': `used_bytes` stays the 1 MiB R8Unorm page
    // the S8 upload allocates (kernel/03 section 3.4), and the bitmaps fit inside it.
    assert_eq!(atlas.page_count(), 1);
    assert_eq!(atlas.used_bytes(), 1024 * 1024);
    assert!(
        atlas.bitmap_bytes() <= atlas.used_bytes(),
        "stored bitmap bytes {} cannot exceed the page bytes {}",
        atlas.bitmap_bytes(),
        atlas.used_bytes()
    );
    eprintln!(
        "S7 atlas accounting: {} bitmaps, bitmap_bytes={} of used_bytes={} on {} page(s)",
        stored.len(),
        atlas.bitmap_bytes(),
        atlas.used_bytes(),
        atlas.page_count()
    );
}

#[test]
fn the_atlas_rasterises_with_the_ar14_settings() {
    // The configuration pin for AR-14. The test re-drives swash itself - an independent call
    // sequence with hinting off and a grayscale Alpha format - and compares the bytes against
    // what the atlas produced. Flipping `hint(false)` to `hint(true)` or `Format::Alpha` to
    // `Format::Subpixel` in `atlas.rs` changes the atlas's pixels and this test fails, which is
    // what makes "hinting is off" an assertion rather than a comment.
    let font = font();
    let ctx = context(font, 16);
    let glyph_id = probe_glyph_id(&ctx);
    let mut atlas = GlyphAtlas::new(AtlasConfig::default());
    let bitmap = atlas
        .rasterize(ctx.atlas_key(glyph_id), &ctx.font)
        .expect("the probe glyph must rasterise");

    let font_ref = swash::FontRef::from_index(&font.data, font.index as usize)
        .expect("the resolved face must parse in swash");
    let mut scale_context = swash::scale::ScaleContext::new();
    let mut scaler = scale_context
        .builder(font_ref)
        .size(f32::from(ctx.px_size))
        // AR-14: hinting OFF.
        .hint(false)
        .build();
    let mut render = swash::scale::Render::new(&[swash::scale::Source::Outline]);
    render
        // AR-14: grayscale AA = one coverage byte per pixel.
        .format(swash::zeno::Format::Alpha)
        .style(swash::zeno::Style::default())
        .offset(swash::zeno::Vector::new(0.0, 0.0));
    let reference = render
        .render(
            &mut scaler,
            u16::try_from(glyph_id).expect("the probe glyph id fits u16"),
        )
        .expect("the reference rasterisation must produce an image");

    assert_eq!(
        reference.placement.left,
        i32::from(bitmap.left),
        "the atlas must report swash's own bearing (font: {})",
        font.describe()
    );
    assert_eq!(
        reference.placement.top,
        i32::from(bitmap.top),
        "the atlas must report swash's own placement top (font: {})",
        font.describe()
    );
    assert_eq!(
        reference.placement.width,
        u32::from(bitmap.width),
        "the atlas slot must be the rasterised ink box (font: {})",
        font.describe()
    );
    assert_eq!(
        reference.placement.height,
        u32::from(bitmap.height),
        "the atlas slot must be the rasterised ink box (font: {})",
        font.describe()
    );
    assert_eq!(
        reference.data,
        bitmap.data,
        "the atlas must rasterise with AR-14's settings (hinting off, Format::Alpha, zero \
         offset); a different configuration changes every coverage byte (font: {})",
        font.describe()
    );
    eprintln!(
        "S7 atlas AR-14 pin: {} reference bytes == {} atlas bytes",
        reference.data.len(),
        bitmap.data.len()
    );
}

#[test]
fn a_colour_glyph_is_refused_with_a_reason() {
    // DC-17 / kernel/03 section 3.4 put colour glyphs on the RGBA8 colour page this slice does
    // not own, and AR-14 promises no subpixel AA: the atlas refuses rather than approximating.
    // This test finds a colour glyph on the host if one exists. If the host has no colour font,
    // that is printed - the rest of this file still runs at full strength.
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let mut found: Option<(fontdb::ID, u32, Vec<u8>, u32, String)> = None;
    'faces: for face in db.faces() {
        let data = db.with_face_data(face.id, |data, _| data.to_vec());
        let Some(data) = data else { continue };
        let Some(font_ref) = swash::FontRef::from_index(&data, face.index as usize) else {
            continue;
        };
        let mut scale_context = swash::scale::ScaleContext::new();
        let mut scaler = scale_context
            .builder(font_ref)
            .size(16.0)
            .hint(false)
            .build();
        let has_colour = scaler.has_color_outlines() || scaler.has_color_bitmaps();
        if !has_colour {
            continue;
        }
        let glyph_count = u32::from(font_ref.metrics(&[]).glyph_count);
        for glyph_id in 0..glyph_count.min(4096) {
            let Ok(probe) = u16::try_from(glyph_id) else {
                continue;
            };
            let colour = (scaler.has_color_outlines()
                && scaler.scale_color_outline(probe).is_some())
                || (scaler.has_color_bitmaps()
                    && scaler
                        .scale_color_bitmap(probe, swash::scale::StrikeWith::BestFit)
                        .is_some());
            if colour {
                found = Some((
                    face.id,
                    face.index,
                    data.clone(),
                    glyph_id,
                    face.post_script_name.clone(),
                ));
                break 'faces;
            }
        }
    }

    let Some((font_id, face_index, data, glyph_id, name)) = found else {
        eprintln!(
            "a_colour_glyph_is_refused_with_a_reason: no colour-capable face on this host, so the \
             refusal branch cannot be exercised here; the grayscale refusals asserted in \
             `the_atlas_refuses_a_key_the_rasteriser_cannot_honour` still run (host faces: {})",
            db.len()
        );
        return;
    };

    let face = FontFace::from_slice(font_id, &data, face_index)
        .expect("fontdb handed out a colour face that rustybuzz cannot parse");
    let mut atlas = GlyphAtlas::new(AtlasConfig::default());
    let key = termai_render::atlas::AtlasKey {
        font_id,
        glyph_id,
        px_size: 16,
        aa_mode: AaMode::Sharp,
    };
    assert_eq!(
        atlas.rasterize(key, &face),
        Err(AtlasError::ColorGlyph { font_id, glyph_id }),
        "a COLR/CBDT glyph must be refused with a reason, not squeezed into the R8 page \
         (face: {name:?})"
    );
    assert!(
        atlas.get(&key).is_none(),
        "a refused colour glyph must leave no slot behind"
    );
    assert_eq!(atlas.bitmap_bytes(), 0);
    eprintln!(
        "S7 atlas colour refusal: face={name:?} glyph={glyph_id} -> ColorGlyph (RGBA8 colour page \
         is the emoji slice's, kernel/03 section 3.4)"
    );
}

#[test]
fn the_atlas_refuses_a_key_the_rasteriser_cannot_honour() {
    let font = font();
    let ctx = context(font, 16);
    let glyph_id = probe_glyph_id(&ctx);
    let mut atlas = GlyphAtlas::new(AtlasConfig::default());

    // A zero device pixel size: swash reads `size(0)` as "unscaled font units" and would hand
    // back a glyph hundreds of pixels wide instead of erroring.
    let mut zero = ctx.atlas_key(glyph_id);
    zero.px_size = 0;
    assert_eq!(
        atlas.rasterize(zero, &ctx.font),
        Err(AtlasError::ZeroPixelSize),
        "a zero px_size must be refused before anything is allocated (font: {})",
        font.describe()
    );
    // A glyph id outside swash's 16-bit glyph id space.
    let mut huge = ctx.atlas_key(glyph_id);
    huge.glyph_id = u32::from(u16::MAX) + 1;
    assert_eq!(
        atlas.rasterize(huge, &ctx.font),
        Err(AtlasError::GlyphOutOfRange {
            glyph_id: u32::from(u16::MAX) + 1,
        })
    );
    // The key names the resolved face; a key from another font may never be drawn with these
    // outlines. A second real face off the host is the honest way to build that mismatch.
    let other_font = {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let other = db
            .faces()
            .find(|face| face.id != font.id)
            .map(|face| face.id);
        other
    };
    if let Some(other_id) = other_font {
        let mut foreign = ctx.atlas_key(glyph_id);
        foreign.font_id = other_id;
        assert_eq!(
            atlas.rasterize(foreign, &ctx.font),
            Err(AtlasError::FontMismatch {
                key_font: other_id,
                face: font.id,
            }),
            "a key from another face must be refused (font: {})",
            font.describe()
        );
    } else {
        eprintln!(
            "the_atlas_refuses_a_key_the_rasteriser_cannot_honour: the host has exactly one face, \
             so the font-mismatch refusal is not exercised here; the zero-px-size and glyph-range \
             refusals above still are"
        );
    }
    assert_eq!(
        atlas.page_count(),
        0,
        "no refusal may have allocated a page (font: {})",
        font.describe()
    );
    assert_eq!(atlas.bitmap_bytes(), 0);
    // And the control: the very same key, unchanged, does rasterise.
    assert!(atlas.rasterize(ctx.atlas_key(glyph_id), &ctx.font).is_ok());
    assert_eq!(atlas.page_count(), 1);
}

// ---------------------------------------------------------------------------------------------
// S6/S7 seam: the RP-05 placement (kernel/03 section 3.5.1 step 6 / section 3.5.3, contract
// RP-05, judgement defined by OQ-RND-04 / AR-24 clause 3).
//
// CPU geometry only: no GPU, no window, no frame, no damage loop. A shaped row plus real
// rasterised bitmaps out of the atlas become an explicit `glyph_bitmap_origin` and
// `cell_box_origin` per drawn glyph, and the worst `abs(difference)` is measured at all four
// device scales RP-05 judges. The face is still the real one resolved by the printed chain at
// the top of this file, and everything that depends on it is printed rather than skipped.
// ---------------------------------------------------------------------------------------------

/// The RP-05 probe row: ten ASCII scalars, every one of them a member of [`REQUIRED`], so the
/// coverage branch this block takes is the same printed one the S6/S7 block above takes. No
/// blank, so on a covering face every glyph is a drawn glyph with an ink box.
const PLACEMENT_ROW: &str = "TermAI0123";

/// The grid rows the probe places. Row 0 would make the vertical cell origin exactly 0 on every
/// host, which would leave half of the measurement untested.
const PLACEMENT_ROWS: [u16; 4] = [1, 2, 3, 4];

/// Logical font sizes the RP-05 probe may be built from, in preference order. The chosen one is
/// the first whose product with this host face's advance ratio is fractional at **all four**
/// device scales *and* whose device cells put the probe's columns/rows at least once more than
/// half a pixel away from a whole pixel: a whole-pixel cell, or a pattern a truncating placement
/// would pass, is exactly the case AR-14's `<=0.5px` rule says nothing about. When a face admits
/// no such candidate, [`probe_shape`] falls back to the best one it does admit and reports which
/// claim the face cannot exercise; it never silently drops the claim or the contract.
const PLACEMENT_LOGICAL_SIZES: [f32; 6] = [12.25, 12.75, 12.5, 13.25, 11.75, 12.1];

/// How far the device-pixel grid can move a frame origin: `round` to the nearest device pixel is
/// never worse than half a pixel, which is the very same half pixel RP-05's contract budgets for.
/// A face whose glyph ink already reaches further than this past its cell box before any snapping
/// cannot be contained by *any* placement, so the ink-containment assert in the RP-05 block is
/// bounded by it instead of pretending the face's own ink is the placement's error.
const FRAME_SNAP_PX: f32 = HARNESS_ALIGNMENT_CONTRACT_PX;

/// What the printed probe selection proved about this host's face.
///
/// The three facts are reported **separately** and are never inferred from each other: a
/// candidate that is fractional but cannot expose truncation is still the size the test runs at,
/// with the truncation claim printed as vacuous, rather than being reported as "no candidate".
#[derive(Clone, Copy, PartialEq, Debug)]
struct ProbeShape {
    /// The chosen logical font size in logical pixels.
    logical_px: f32,
    /// Every device cell metric is fractional at all four RP-05 scales.
    fractional: bool,
    /// Per RP-05 scale (index into [`RP05_DPI_SCALES`]): a truncating (`floor`/`as u16`)
    /// placement would be exposed by this probe's columns/rows at that scale.
    discriminating: [bool; RP05_DPI_SCALES.len()],
    /// How many candidate logical sizes the search tried and rejected before the chosen one.
    rejected: usize,
}

impl ProbeShape {
    /// Whether the pattern can expose a truncating placement at `scale_index` (0 when the index
    /// is out of range, which no caller can reach: the loop enumerates the same array).
    fn exposes_truncation_at(self, scale_index: usize) -> bool {
        self.discriminating
            .get(scale_index)
            .copied()
            .unwrap_or(false)
    }

    /// How many of the four scales exercise the truncation claim.
    fn discriminating_scales(self) -> usize {
        self.discriminating.iter().filter(|flag| **flag).count()
    }
}

/// Whether a whole-pixel placement of a `cell`-sized step from the grid origin would ever be more
/// than the contract away from the grid line under **truncation**: `max` over the probe's
/// columns/rows of the fractional part of `k * cell`. A `floor`/`as u16` placement has to be
/// exposed by this pattern, or a green result would only prove the pattern is whole-pixel.
fn placement_discriminates(cell_px: f32, steps: (u16, u16)) -> bool {
    (steps.0..=steps.1)
        .any(|step| (cell_px * f32::from(step)).fract() > HARNESS_ALIGNMENT_CONTRACT_PX)
}

/// Search [`PLACEMENT_LOGICAL_SIZES`] for the best evidence configuration this face admits, and
/// report exactly which claims it exercises.
///
/// Preference order: the first candidate whose device cell metrics are fractional at **all four**
/// RP-05 scales *and* whose columns and rows can expose a truncating placement at **all four**;
/// failing that, the first candidate that is at least fractional (so the fractional-metrics claim
/// is still asserted); failing that, the first candidate, reported as exercising neither claim.
/// The search is exhaustive over its candidate list, so "nothing found" is a fact about the face's
/// advance ratio, not an early exit - and the caller prints it with the numbers.
fn probe_shape(advance_ratio: f32) -> ProbeShape {
    let columns = (
        0,
        u16::try_from(PLACEMENT_ROW.chars().count() - 1).unwrap_or(1),
    );
    let rows = (
        PLACEMENT_ROWS[0],
        u16::try_from(PLACEMENT_ROWS.len()).unwrap_or(1),
    );
    let mut fallback: Option<ProbeShape> = None;
    let mut rejected = 0;
    for size in PLACEMENT_LOGICAL_SIZES {
        let logical_advance = advance_ratio * size;
        let mut fractional = true;
        let mut discriminating = [true; RP05_DPI_SCALES.len()];
        for (index, scale) in RP05_DPI_SCALES.into_iter().enumerate() {
            let cell_w = logical_advance * scale;
            let cell_h = size * LINE_HEIGHT_RATIO * scale;
            fractional &= cell_w.fract() != 0.0 && cell_h.fract() != 0.0;
            discriminating[index] =
                placement_discriminates(cell_w, columns) && placement_discriminates(cell_h, rows);
        }
        let candidate = ProbeShape {
            logical_px: size,
            fractional,
            discriminating,
            rejected,
        };
        if fractional && discriminating.iter().all(|flag| *flag) {
            return candidate;
        }
        if fallback.is_none() && fractional {
            fallback = Some(candidate);
        }
        rejected += 1;
    }
    fallback.unwrap_or(ProbeShape {
        logical_px: PLACEMENT_LOGICAL_SIZES[0],
        fractional: false,
        discriminating: [false; RP05_DPI_SCALES.len()],
        rejected,
    })
}

/// The face's ascent and descent in logical pixels at `logical_px`, from the font's own metrics:
/// kernel/03 section 3.5.2 makes the font backend - not this crate, and not a heuristic - the
/// source of the baseline the glyph origin (section 3.5.1 step 6) sits on.
fn logical_baseline(font: &ResolvedFont, logical_px: f32) -> (f32, f32) {
    let face = rustybuzz::ttf_parser::Face::parse(&font.data, font.index)
        .expect("the resolved face must parse for its metrics");
    let upem = f32::from(face.units_per_em());
    (
        f32::from(face.ascender()) * logical_px / upem,
        -f32::from(face.descender()) * logical_px / upem,
    )
}

/// The device metrics and the device pixel size the probe shapes and rasterises at, for one
/// RP-05 device scale.
///
/// The metrics are the font backend's **logical** measurements times the device scale (kernel/03
/// section 3.5.3), so they are fractional at 125/150%; the device pixel size S6 shapes at and
/// [`termai_render::place::placement_key`] keys the atlas with is that same product rounded,
/// which is the only whole-pixel truncation in the path and the reason a real renderer wants
/// K-12's `size_q6`.
fn probe_scale(
    font: &ResolvedFont,
    advance_ratio: f32,
    shape: ProbeShape,
    scale: f32,
) -> (CellMetrics, u16) {
    let (ascent, descent) = logical_baseline(font, shape.logical_px);
    let metrics = CellMetrics::from_logical(
        advance_ratio * shape.logical_px,
        shape.logical_px,
        ascent,
        descent,
        scale,
    )
    .expect("the RP-05 probe metrics are finite and positive");
    let device_px = (shape.logical_px * scale)
        .round()
        .clamp(1.0, f32::from(u16::MAX)) as u16;
    (metrics, device_px)
}

/// Whether a cell origin sits exactly on a half pixel, where rounding to the nearest device pixel
/// is at its 0.5px worst case (i.e. exactly on RP-05's contract rather than inside it).
fn is_half_pixel(origin: f32) -> bool {
    (origin.fract() - 0.5).abs() < 1.0e-4
}

/// The S6 context for one probe row, bound to the placement's own device grid: the cell advance
/// the shaper fits against *is* the grid's device cell width, so a cluster that fills its cell
/// exactly records `fit_scale` for it instead of being fitted against a second, provisional box.
///
/// Built directly rather than through [`context`]: every metric it needs is overridden here
/// anyway, and `context`'s provisional 1 em probe can be squeezed by an icon face whose `"0"` is
/// wider than one em, which would fail the probe before the placement under test is ever reached.
fn probe_context(font: &ResolvedFont, metrics: CellMetrics, device_px: u16) -> ShapeContext<'_> {
    let face = FontFace::from_slice(font.id, &font.data, font.index)
        .expect("fontdb handed out a face that rustybuzz cannot parse");
    ShapeContext {
        font: face,
        px_size: device_px,
        scale_q8: metrics.scale_q8,
        aa: AaMode::Sharp,
        ligatures: true,
        cell_advance_px: metrics.cell_w_px,
        row_height_px: metrics.cell_h_px,
    }
}

/// One placed probe row, together with the S6 numbers the placement's own arithmetic is checked
/// against: the pen is centred on the span's *fitted advance*, which only S6 knows.
struct PlacedProbeRow {
    placed: PlacedRow,
    /// `advance_px` of each S6 span, indexed by the `cluster_index` the placed glyphs carry.
    span_advance: Vec<f32>,
}

/// Shape one probe row, rasterise exactly the bitmaps the placement will look up (a blank glyph
/// legitimately has none: S7 refuses it with `AtlasError::NoOutline` and stores no slot), and
/// place the row through the production K-04 entry point.
fn place_probe_row(
    font: &ResolvedFont,
    metrics: &CellMetrics,
    device_px: u16,
    atlas: &mut GlyphAtlas,
    row_index: u16,
) -> PlacedProbeRow {
    let ctx = probe_context(font, *metrics, device_px);
    let mut input = ascii_row(PLACEMENT_ROW);
    input.row = row_index;
    let shaped = shape_row(&input, &ctx, &HONEST).expect("the RP-05 probe row must shape");
    for key in placement_keys(&shaped) {
        match atlas.rasterize(key, &ctx.font) {
            Ok(_) => {}
            Err(AtlasError::NoOutline { .. }) => {}
            Err(error) => panic!("every non-blank probe glyph must rasterise: {error:?}"),
        }
    }
    let placed = place_row_with_vt_widths(&shaped, &input, metrics, atlas)
        .expect("the probe row's spans agree with the K-04 width authority");
    // The K-04 cross-check is additive: the span-only entry point produces the identical row.
    assert_eq!(
        place_row(&shaped, metrics, atlas).expect("the probe row must place"),
        placed,
        "place_row and place_row_with_vt_widths must agree when the authority agrees"
    );
    PlacedProbeRow {
        placed,
        span_advance: shaped.spans.iter().map(|span| span.advance_px).collect(),
    }
}

#[test]
fn the_placement_stays_within_half_a_pixel_at_every_rp05_device_scale() {
    let font = font();
    let advance_ratio = measure_advance_ratio(font, 16);
    let shape = probe_shape(advance_ratio);
    eprintln!(
        "RP-05 placement probe: logical font size {}px, advance ratio {:.6}, device cell sizes \
         fractional at all four scales: {}, truncation exposed by the probe pattern at {}/{} \
         scale(s), candidate logical sizes rejected before it: {} of {PLACEMENT_LOGICAL_SIZES:?} \
         (font: {})",
        shape.logical_px,
        advance_ratio,
        shape.fractional,
        shape.discriminating_scales(),
        RP05_DPI_SCALES.len(),
        shape.rejected,
        font.describe()
    );
    // Claim 1 of the probe's candidate search: genuinely fractional device cell metrics
    // (kernel/03 section 3.5.3, the case AR-14's <=0.5px rule is about). Asserted per scale
    // below whenever the search found it, printed here when it could not - never dropped.
    if !shape.fractional {
        eprintln!(
            "the_placement_stays_within_half_a_pixel_at_every_rp05_device_scale: {} - none of the \
             candidate logical sizes {PLACEMENT_LOGICAL_SIZES:?} gives fractional cell metrics at \
             all four RP-05 scales for this face's advance ratio {:.6}: the probe places into \
             whole-pixel cell metrics, where a snapped placement and a perfect one are the same \
             picture, so the 'cell metrics are fractional' claim is vacuous on this face. The \
             <=0.5px contract assertion, the sample/refusal accounting and the injection control \
             are unaffected and still run (font: {})",
            font.branch.label(),
            advance_ratio,
            font.describe()
        );
    }

    let glyphs_per_row = PLACEMENT_ROW.chars().count();
    let glyphs_per_scale = PLACEMENT_ROWS.len() * glyphs_per_row;
    for (scale_index, scale) in RP05_DPI_SCALES.into_iter().enumerate() {
        let (metrics, device_px) = probe_scale(font, advance_ratio, shape, scale);
        let mut atlas = GlyphAtlas::new(AtlasConfig::default());
        let mut worst_px = 0.0_f32;
        let mut truncated_worst_px = 0.0_f32;
        let mut boundary_samples = 0_usize;
        let mut samples = 0_usize;
        // How many drawn glyphs exercised the strict one-device-pixel ink-box containment, and
        // the printed reasons of the ones whose own ink box made it unreachable.
        let mut strict_contained = 0_usize;
        let mut face_owned: Vec<String> = Vec::new();
        let mut refusals: Vec<GlyphRefusal> = Vec::new();
        for row_index in PLACEMENT_ROWS {
            let probe = place_probe_row(font, &metrics, device_px, &mut atlas, row_index);
            let placed = &probe.placed;
            assert_eq!(
                placed.glyphs.len() + placed.refusals.len(),
                glyphs_per_row,
                "every glyph of row {row_index} is either drawn or refused with a reason"
            );
            if placed.glyphs.is_empty() {
                // A face that draws none of the probe row at all (an icon face, or ASCII mapped to
                // colour/empty glyphs) has no RP-05 sample to judge. That is printed with the
                // refusals, and the documented refusal is asserted in place of a clean zero: the
                // measurement must never come back as 0.0px over nothing.
                assert_eq!(
                    measure_placement(placed),
                    Err(MeasureError::NoDrawnGlyphs {
                        refusals: glyphs_per_row
                    }),
                    "a row with no drawn glyph must be refused, never reported as 0.0px (font: {})",
                    font.describe()
                );
                eprintln!(
                    "the_placement_stays_within_half_a_pixel_at_every_rp05_device_scale: {} - at \
                     {:.0}% DPI the resolved face draws none of the {} glyphs of probe row \
                     {row_index}; all {} are refused with a reason ({:?}), so RP-05 has no sample \
                     to judge here and the refusal is asserted instead of a clean zero (font: {})",
                    font.branch.label(),
                    scale * 100.0,
                    glyphs_per_row,
                    placed.refusals.len(),
                    placed.refusals,
                    font.describe()
                );
                refusals.extend(placed.refusals.iter().copied());
                continue;
            }
            let alignment = measure_placement(placed)
                .unwrap_or_else(|error| panic!("row {row_index}: {error}"));
            if row_index == PLACEMENT_ROWS[0] {
                if let Some(glyph) = placed.glyphs.first() {
                    eprintln!(
                        "RP-05 placement sample: scale={:.0}% glyph={} cell_box={:?} \
                         bitmap_frame={:?} pen={:?} quad={:?} ink={}x{} bearing=({},{}) \
                         baseline={:.4}px below the cell top (deviation {:.4}px)",
                        scale * 100.0,
                        glyph.glyph_id,
                        glyph.cell_box_origin,
                        glyph.glyph_bitmap_origin,
                        glyph.glyph_origin,
                        glyph.bitmap_quad_origin,
                        glyph.ink_width,
                        glyph.ink_height,
                        glyph.bearing_left,
                        glyph.bearing_top,
                        metrics.baseline_offset_px(),
                        glyph.deviation_px()
                    );
                }
            }
            assert_eq!(
                alignment.samples,
                placed.glyphs.len(),
                "the sample count is the number of drawn glyphs, never more and never fewer \
                 (row {row_index})"
            );
            for glyph in &placed.glyphs {
                // What a truncating (`floor`/`as u16`) placement would have reported for this very
                // sample: the pattern has to be able to tell the two apart, or a green run would
                // only prove the pattern is whole-pixel.
                let truncated = Point::new(
                    glyph.cell_box_origin.x.floor(),
                    glyph.cell_box_origin.y.floor(),
                );
                truncated_worst_px =
                    truncated_worst_px.max(glyph.cell_box_origin.deviation_px(truncated));
                // A cell origin that lands exactly on a half pixel is round-to-nearest's worst
                // case, i.e. exactly the contract value. It is counted and printed rather than
                // hidden: it is a property of the grid, not a defect in the placement.
                if is_half_pixel(glyph.cell_box_origin.x) || is_half_pixel(glyph.cell_box_origin.y)
                {
                    boundary_samples += 1;
                }
                // The placement must key the atlas with S6's `fit_scale` applied. `max(1)` is
                // `place::placement_key`'s documented floor: a `u16` pixel size cannot be zero,
                // and `AtlasKey::px_size == 0` is refused by the atlas (S7).
                assert_eq!(
                    glyph.key.px_size,
                    ((f32::from(device_px) * glyph.fit_scale).round() as u16).max(1),
                    "the placement must key the atlas with S6's fit_scale applied (glyph {})",
                    glyph.glyph_id
                );
                // The ink box the atlas stored must be consistent with the device pixel size the
                // key names: a rasterisation at the wrong unit (font units, a 64x scale_q6
                // mix-up) comes back hundreds of pixels wide, far past the two em an ASCII probe
                // glyph can fill. Face-independent, and S7's own probe test asserts the same bound.
                assert!(
                    u32::from(glyph.ink_width) <= 2 * u32::from(glyph.key.px_size)
                        && u32::from(glyph.ink_height) <= 2 * u32::from(glyph.key.px_size),
                    "glyph {} rasterised to an ink box {}x{} at {}px, which is more than two em \
                     (font: {})",
                    glyph.glyph_id,
                    glyph.ink_width,
                    glyph.ink_height,
                    glyph.key.px_size,
                    font.describe()
                );
                // The pen origin is on the *font's baseline* (kernel/03 section 3.5.2), so every
                // glyph of the row shares it - a placement that centered each ink box instead
                // would give every glyph its own wobbling baseline.
                assert_eq!(
                    glyph.glyph_origin.y,
                    glyph.cell_box_origin.y + metrics.baseline_offset_px(),
                    "every glyph of the row must sit on the same baseline (glyph {})",
                    glyph.glyph_id
                );
                // ...and horizontally the *fitted advance* S6 measured is centered in the cell box
                // the grid gave the cluster (kernel/03 section 3.5.1 step 6: the pen is centered,
                // not the ink box). Face-independent: both numbers are the placement's own inputs.
                let span_advance = probe.span_advance[glyph.cluster_index];
                assert_eq!(
                    glyph.glyph_origin.x,
                    glyph.cell_box_origin.x
                        + (metrics.cell_box_width(glyph.cells) - span_advance) * 0.5,
                    "the pen must be centered in the cell box (glyph {}, cluster {}, span \
                     {span_advance:.5}px in {:.5}px)",
                    glyph.glyph_id,
                    glyph.cluster_index,
                    metrics.cell_box_width(glyph.cells)
                );
                // RP-05's numerator is the nearest device pixel to the grid line, and S7's bitmap
                // frame starts at the snapped pen plus the glyph's own bearing (kernel/03 section
                // 3.4). Both are identities the contract and the ink box below rest on, so both
                // are pinned rather than assumed.
                assert_eq!(
                    glyph.glyph_bitmap_origin,
                    Point::new(
                        glyph.cell_box_origin.x.round(),
                        glyph.cell_box_origin.y.round()
                    ),
                    "the bitmap frame must be the snapped cell box origin (glyph {})",
                    glyph.glyph_id
                );
                assert_eq!(
                    glyph.bitmap_quad_origin,
                    Point::new(
                        (glyph.glyph_origin.x + f32::from(glyph.bearing_left)).round(),
                        (glyph.glyph_origin.y - f32::from(glyph.bearing_top)).round()
                    ),
                    "the drawn ink box must start at the snapped pen + bearing (glyph {})",
                    glyph.glyph_id
                );
                // kernel/03 section 3.5.1 step 6 / V-10: the drawn ink box stays inside its cell
                // box. The box is the atlas's - the glyph's outline rounded OUT to whole device
                // pixels, so its size and its bearings are the face's - while the pen and the snap
                // are the placement's. The strict reading ("inside the cell box within one device
                // pixel") is therefore exercisable exactly by the glyphs whose *un-snapped* ink box
                // is already inside the cell box up to that same half pixel; for the others the
                // box's own excursion past the cell edge is the face's, is printed with its
                // numbers, and is added to the budget instead of being asserted away.
                let cell_w = metrics.cell_box_width(glyph.cells);
                let cell_right = glyph.cell_box_origin.x + cell_w;
                let cell_bottom = glyph.cell_box_origin.y + metrics.cell_h_px;
                let ink_left = glyph.glyph_origin.x + f32::from(glyph.bearing_left);
                let ink_top = glyph.glyph_origin.y - f32::from(glyph.bearing_top);
                // How far the ideal (un-snapped) and the drawn (snapped) ink box reach past the
                // cell box, per side; positive means "outside it".
                let ideal = [
                    glyph.cell_box_origin.x - ink_left,
                    ink_left + f32::from(glyph.ink_width) - cell_right,
                    glyph.cell_box_origin.y - ink_top,
                    ink_top + f32::from(glyph.ink_height) - cell_bottom,
                ];
                let drawn = [
                    glyph.cell_box_origin.x - glyph.bitmap_quad_origin.x,
                    glyph.bitmap_quad_origin.x + f32::from(glyph.ink_width) - cell_right,
                    glyph.cell_box_origin.y - glyph.bitmap_quad_origin.y,
                    glyph.bitmap_quad_origin.y + f32::from(glyph.ink_height) - cell_bottom,
                ];
                for (side, drawn_reach, ideal_reach) in [
                    ("left", drawn[0], ideal[0]),
                    ("right", drawn[1], ideal[1]),
                    ("top", drawn[2], ideal[2]),
                    ("bottom", drawn[3], ideal[3]),
                ] {
                    assert!(
                        drawn_reach <= ideal_reach + FRAME_SNAP_PX + 1.0e-3,
                        "the drawn ink box may only be the ideal ink box snapped by at most \
                         {FRAME_SNAP_PX}px: its {side} side reaches {drawn_reach:.4}px past the \
                         cell box where this glyph's own ink reaches {ideal_reach:.4}px (glyph {}, \
                         ink {}x{} at bearing ({},{}), cell {:.5}x{:.5}px, font: {})",
                        glyph.glyph_id,
                        glyph.ink_width,
                        glyph.ink_height,
                        glyph.bearing_left,
                        glyph.bearing_top,
                        cell_w,
                        metrics.cell_h_px,
                        font.describe()
                    );
                }
                if ideal.iter().all(|reach| *reach <= FRAME_SNAP_PX) {
                    // This face's ink box is inside the cell box before any snapping, so the only
                    // thing that can push it out is the placement: the strict one-device-pixel
                    // containment is exercisable here and is asserted, never skipped.
                    assert!(
                        drawn.iter().all(|reach| *reach <= 1.0 + 1.0e-3),
                        "the drawn ink box of glyph {} ({:?} + {}x{}) must stay inside its cell box \
                         ({:?} {:.5}x{:.5}) within one device pixel (font: {})",
                        glyph.glyph_id,
                        glyph.bitmap_quad_origin,
                        glyph.ink_width,
                        glyph.ink_height,
                        glyph.cell_box_origin,
                        cell_w,
                        metrics.cell_h_px,
                        font.describe()
                    );
                    strict_contained += 1;
                } else {
                    face_owned.push(format!(
                        "glyph {} ({}x{} ink box at bearing ({},{})) already reaches {:.4}px past \
                         its {:.5}x{:.5}px cell box before any snapping, so one-device-pixel \
                         containment is a property of this face's ink rather than of the placement; \
                         the drawn box is asserted against that excursion",
                        glyph.glyph_id,
                        glyph.ink_width,
                        glyph.ink_height,
                        glyph.bearing_left,
                        glyph.bearing_top,
                        ideal.iter().copied().fold(0.0_f32, f32::max),
                        cell_w,
                        metrics.cell_h_px
                    ));
                }
            }
            worst_px = worst_px.max(alignment.worst_px);
            samples += alignment.samples;
            refusals.extend(placed.refusals.iter().copied());
        }
        if samples == 0 {
            // Never a clean zero: there is no sample to report a worst deviation for, and the
            // refusal that took its place is asserted in the row loop above.
            eprintln!(
                "RP-05 placement: scale={:.0}% device_px={} cell={:.5}x{:.5}px NOT MEASURED - 0 \
                 drawn glyph(s) of {glyphs_per_scale}, {} refusal(s): RP-05 is never judged on zero \
                 samples (font: {})",
                scale * 100.0,
                device_px,
                metrics.cell_w_px,
                metrics.cell_h_px,
                refusals.len(),
                font.describe()
            );
        } else {
            eprintln!(
                "RP-05 placement: scale={:.0}% device_px={} cell={:.5}x{:.5}px worst={:.4}px \
                 samples={} refusals={} half-pixel grid lines among the samples={} strict ink-box \
                 containment exercised by {strict_contained}/{samples} drawn glyph(s), {} glyph(s) \
                 whose own ink box leaves its cell box before any snapping (floor() would report \
                 {:.4}px; contract {:.1}px)",
                scale * 100.0,
                device_px,
                metrics.cell_w_px,
                metrics.cell_h_px,
                worst_px,
                samples,
                refusals.len(),
                boundary_samples,
                face_owned.len(),
                truncated_worst_px,
                HARNESS_ALIGNMENT_CONTRACT_PX
            );
        }
        for reason in face_owned.iter().take(4) {
            eprintln!(
                "the_placement_stays_within_half_a_pixel_at_every_rp05_device_scale: {reason} \
                 (font: {})",
                font.describe()
            );
        }
        if face_owned.len() > 4 {
            eprintln!(
                "the_placement_stays_within_half_a_pixel_at_every_rp05_device_scale: ... and {} \
                 more glyph(s) of the same kind at {:.0}% DPI",
                face_owned.len() - 4,
                scale * 100.0
            );
        }
        if samples > 0 && strict_contained == 0 {
            eprintln!(
                "the_placement_stays_within_half_a_pixel_at_every_rp05_device_scale: {} - no drawn \
                 glyph of the probe row exercises the strict one-device-pixel ink-box containment \
                 at {:.0}% DPI on this face: every glyph's rasterised ink box leaves its cell box \
                 before any snapping (the per-glyph numbers are printed above), so that claim is \
                 vacuous here and is not asserted. The face-independent claims - the pen centered \
                 in the cell box, the frame at the snapped cell origin, the ink box at the snapped \
                 pen, the sample/refusal accounting and the {:.1}px contract over every drawn glyph \
                 - all still run (font: {})",
                font.branch.label(),
                scale * 100.0,
                HARNESS_ALIGNMENT_CONTRACT_PX,
                font.describe()
            );
        }
        // Claim 1 of the candidate search, per scale: asserted wherever the search reported it,
        // and printed with its reason at the top of the test when it could not.
        if shape.fractional {
            assert!(
                metrics.cell_w_px.fract() != 0.0 && metrics.cell_h_px.fract() != 0.0,
                "the candidate search reported fractional cell metrics at {:.0}% DPI but {}x{}px \
                 are whole-pixel (font: {})",
                scale * 100.0,
                metrics.cell_w_px,
                metrics.cell_h_px,
                font.describe()
            );
        }
        assert_eq!(
            samples + refusals.len(),
            glyphs_per_scale,
            "drawn glyphs plus refusals must be every glyph of the probe at {:.0}% DPI (font: {})",
            scale * 100.0,
            font.describe()
        );
        if samples == 0 {
            assert_eq!(
                refusals.len(),
                glyphs_per_scale,
                "a scale that measured nothing must account for every probe glyph as a refusal \
                 (font: {})",
                font.describe()
            );
            continue;
        }
        if font.coverage.probe {
            assert_eq!(
                refusals.len(),
                0,
                "a face covering every probe scalar must refuse no glyph at {:.0}% DPI (font: {})",
                scale * 100.0,
                font.describe()
            );
            assert_eq!(
                samples,
                glyphs_per_scale,
                "the sample count must be the number of drawn glyphs at {:.0}% DPI (font: {})",
                scale * 100.0,
                font.describe()
            );
        } else {
            eprintln!(
                "the_placement_stays_within_half_a_pixel_at_every_rp05_device_scale: {} - the \
                 resolved face does not cover every scalar of {PLACEMENT_ROW:?}, so {} glyph(s) \
                 are refused with a reason instead of being measured: {:?}",
                font.branch.label(),
                refusals.len(),
                refusals
            );
        }
        assert!(
            worst_px <= HARNESS_ALIGNMENT_CONTRACT_PX,
            "RP-05 at {:.0}% DPI: worst {:.4}px over {} glyph sample(s) exceeds the {:.1}px \
             contract (cell {:.5}x{:.5}px, scale_q8 {}, font: {})",
            scale * 100.0,
            worst_px,
            samples,
            HARNESS_ALIGNMENT_CONTRACT_PX,
            metrics.cell_w_px,
            metrics.cell_h_px,
            metrics.scale_q8,
            font.describe()
        );
        // A grid line that lands exactly on a half pixel is round-to-nearest's worst case: its
        // deviation is exactly the contract, so it is counted and printed rather than folded
        // silently into the number above.
        if boundary_samples > 0 {
            eprintln!(
                "RP-05 placement at {:.0}% DPI: {boundary_samples} sample(s) sit exactly on a \
                 half-pixel grid line, where round-to-nearest is at its 0.5px worst case - the \
                 contract itself, not a defect (cell {:.5}x{:.5}px)",
                scale * 100.0,
                metrics.cell_w_px,
                metrics.cell_h_px
            );
        }
        assert!(
            boundary_samples > 0 || worst_px < HARNESS_ALIGNMENT_CONTRACT_PX,
            "with no sample on a half-pixel grid line the worst deviation must be strictly inside \
             the {:.1}px contract, got {:.4}px over {} sample(s) (font: {})",
            HARNESS_ALIGNMENT_CONTRACT_PX,
            worst_px,
            samples,
            font.describe()
        );
        // Claim 2 of the candidate search, per scale: a truncating (`floor`/`as u16`) placement
        // would be exposed by this probe's columns/rows. Asserted wherever the search reported it
        // for this scale, and printed with its reason where it could not.
        if shape.exposes_truncation_at(scale_index) {
            assert!(
                truncated_worst_px > HARNESS_ALIGNMENT_CONTRACT_PX,
                "the probe pattern must expose a truncating placement at {:.0}% DPI: floor() would \
                 have reported {:.4}px, which is inside the {:.1}px contract, so a green run would \
                 say nothing (cell {:.5}x{:.5}px, font: {})",
                scale * 100.0,
                truncated_worst_px,
                HARNESS_ALIGNMENT_CONTRACT_PX,
                metrics.cell_w_px,
                metrics.cell_h_px,
                font.describe()
            );
        } else {
            eprintln!(
                "the_placement_stays_within_half_a_pixel_at_every_rp05_device_scale: {} - at {:.0}% \
                 DPI the probe pattern cannot expose a truncating placement on this face: no column \
                 or row of {PLACEMENT_ROW:?} x rows {PLACEMENT_ROWS:?} puts a floor() error past \
                 {:.1}px (the worst floor() error the drawn samples admit is {:.4}px), so the \
                 'a truncating placement would exceed the contract' claim is vacuous at this scale \
                 and is not asserted; the <=0.5px contract assertion above still runs (cell \
                 {:.5}x{:.5}px, font: {})",
                font.branch.label(),
                scale * 100.0,
                HARNESS_ALIGNMENT_CONTRACT_PX,
                truncated_worst_px,
                metrics.cell_w_px,
                metrics.cell_h_px,
                font.describe()
            );
        }
    }
}

#[test]
fn a_measurement_over_zero_drawn_glyphs_is_refused_not_a_clean_zero() {
    let font = font();
    let advance_ratio = measure_advance_ratio(font, 16);
    let shape = probe_shape(advance_ratio);
    let (metrics, device_px) = probe_scale(font, advance_ratio, shape, 1.25);
    let ctx = probe_context(font, metrics, device_px);
    let mut input = ascii_row(PLACEMENT_ROW);
    input.row = 1;
    let shaped = shape_row(&input, &ctx, &HONEST).expect("the RP-05 probe row must shape");

    // An empty atlas: no key has a bitmap, so every glyph is a structured refusal and there is
    // nothing to measure. The measurement must refuse - never report 0.0px over zero samples.
    let empty = GlyphAtlas::new(AtlasConfig::default());
    let placed = place_row_with_vt_widths(&shaped, &input, &metrics, &empty)
        .expect("an un-rasterised row is a placement, not an error");
    assert_eq!(placed.glyphs.len(), 0);
    assert_eq!(placed.refusals.len(), PLACEMENT_ROW.chars().count());
    // Which refusal an empty atlas produces for a glyph is itself a coverage fact: a scalar the
    // resolved face does not cover is refused without ever consulting the atlas
    // (`UncoveredCluster`), and only the covered ones become `MissingBitmap`. Both directions are
    // asserted, and the resolved face's coverage is printed with the reason - never skipped.
    if font.coverage.probe {
        assert!(
            placed
                .refusals
                .iter()
                .all(|refusal| matches!(refusal, GlyphRefusal::MissingBitmap { .. })),
            "an empty atlas must refuse every glyph as MissingBitmap, got {:?}",
            placed.refusals
        );
    } else {
        eprintln!(
            "a_measurement_over_zero_drawn_glyphs_is_refused_not_a_clean_zero: {} - the resolved \
             face does not cover every scalar of {PLACEMENT_ROW:?}, so an empty atlas refuses the \
             uncovered scalars without consulting it (UncoveredCluster) instead of as \
             MissingBitmap; both are documented refusals and every glyph is still refused exactly \
             once: {:?}",
            font.branch.label(),
            placed.refusals
        );
        assert!(
            placed.refusals.iter().all(|refusal| matches!(
                refusal,
                GlyphRefusal::MissingBitmap { .. } | GlyphRefusal::UncoveredCluster { .. }
            )),
            "an empty atlas must refuse every glyph with a documented reason, got {:?}",
            placed.refusals
        );
    }
    assert_eq!(
        measure_placement(&placed),
        Err(MeasureError::NoDrawnGlyphs {
            refusals: PLACEMENT_ROW.chars().count()
        }),
        "a measurement over zero drawn glyphs must be refused, never a silent zero"
    );
    assert!(measure_placement(&placed)
        .unwrap_err()
        .to_string()
        .contains("zero samples"));
    eprintln!(
        "RP-05 placement: an empty atlas refuses all {} glyphs as MissingBitmap and \
         measure_placement reports NoDrawnGlyphs instead of 0.0px",
        placed.refusals.len()
    );

    // The control: the very same row with its bitmaps rasterised measures every glyph it covers.
    let mut atlas = GlyphAtlas::new(AtlasConfig::default());
    let control = place_probe_row(font, &metrics, device_px, &mut atlas, 1);
    if control.placed.glyphs.is_empty() {
        // An icon face draws none of the probe row, so this control cannot be built on it: the
        // refusal is asserted instead of a measurement, and the reason is printed.
        eprintln!(
            "a_measurement_over_zero_drawn_glyphs_is_refused_not_a_clean_zero: {} - the resolved \
             face draws none of the {} probe glyphs (all refused: {:?}), so the rasterised control \
             cannot measure anything here; the empty-atlas refusal asserted above still ran, and \
             this row's own refusal is asserted here (font: {})",
            font.branch.label(),
            PLACEMENT_ROW.chars().count(),
            control.placed.refusals,
            font.describe()
        );
        assert_eq!(
            measure_placement(&control.placed),
            Err(MeasureError::NoDrawnGlyphs {
                refusals: PLACEMENT_ROW.chars().count()
            }),
            "a rasterised row with no drawn glyph must still be refused, never a silent zero"
        );
        return;
    }
    let alignment =
        measure_placement(&control.placed).expect("the rasterised control row must measure");
    // The control's sample set is exactly the glyphs the face draws: a face that covers fewer
    // scalars of the probe row refuses the rest with a reason, which is a coverage fact of the
    // resolved face rather than a property of the measurement.
    assert_eq!(
        alignment.samples,
        control.placed.glyphs.len(),
        "the control's sample count is the number of drawn glyphs, never more and never fewer \
         (font: {})",
        font.describe()
    );
    assert_eq!(
        control.placed.refusals.len(),
        PLACEMENT_ROW.chars().count() - control.placed.glyphs.len(),
        "every glyph of the control row is either drawn or refused"
    );
    if control.placed.refusals.is_empty() {
        assert_eq!(alignment.samples, PLACEMENT_ROW.chars().count());
    } else {
        eprintln!(
            "a_measurement_over_zero_drawn_glyphs_is_refused_not_a_clean_zero: {} - the resolved \
             face draws {} of the {} probe glyphs and refuses {} with a reason ({:?}), so the \
             'every glyph measures' control is asserted over the drawn sample set instead (font: \
             {})",
            font.branch.label(),
            control.placed.glyphs.len(),
            PLACEMENT_ROW.chars().count(),
            control.placed.refusals.len(),
            control.placed.refusals,
            font.describe()
        );
    }
    assert!(alignment.within_contract());
    eprintln!("RP-05 placement control: {alignment}");
}

#[test]
fn a_blank_glyph_is_reported_rather_than_silently_dropped_from_the_sample_set() {
    // U+0020 has an advance but no ink: S7 refuses it with `AtlasError::NoOutline` and stores no
    // slot, so the placement records a structured refusal. The point is that the *other* glyphs
    // still measure and the refusal is listed - a blank must never shrink the sample set quietly.
    let font = font();
    let advance_ratio = measure_advance_ratio(font, 16);
    let shape = probe_shape(advance_ratio);
    let (metrics, device_px) = probe_scale(font, advance_ratio, shape, 1.0);
    let ctx = probe_context(font, metrics, device_px);
    const WITH_BLANK: &str = "TermAI 0123";
    let mut input = ascii_row(WITH_BLANK);
    input.row = 2;
    let shaped = shape_row(&input, &ctx, &HONEST).expect("the blank probe row must shape");

    // What the atlas itself did with the blank's key decides which refusal is the honest one. A
    // face with no glyph for U+0020 (or one that maps the blank to a colour glyph) refuses it
    // *without* the atlas, so that key may legitimately not exist.
    let blank_keys = placement_keys(
        &shape_row(
            &RowClusters::new(0, 0, vec![cluster(0, " ")]),
            &ctx,
            &HONEST,
        )
        .expect("the blank cluster must shape"),
    );
    let blank_key = blank_keys.first().copied();
    let mut atlas = GlyphAtlas::new(AtlasConfig::default());
    for key in placement_keys(&shaped) {
        match atlas.rasterize(key, &ctx.font) {
            Ok(_) => {}
            Err(AtlasError::NoOutline { .. }) => {}
            Err(error) => panic!("every non-blank probe glyph must rasterise: {error:?}"),
        }
    }
    let blank_state = blank_key.and_then(|key| atlas.bitmap(&key));
    let placed = place_row_with_vt_widths(&shaped, &input, &metrics, &atlas)
        .expect("the blank probe row must place");
    if placed.glyphs.is_empty() {
        // A face that draws none of the row (an icon face, or ASCII mapped to blank/colour
        // glyphs) refuses every glyph: the blank's own refusal and the never-a-clean-zero
        // measurement are then what is asserted, and the reason is printed.
        eprintln!(
            "a_blank_glyph_is_reported_rather_than_silently_dropped_from_the_sample_set: {} - the \
             resolved face draws none of the {} glyphs of {WITH_BLANK:?} (refused: {:?}), so the \
             'the other glyphs still measure' half of this test cannot run here; every glyph is \
             accounted for as a refusal and the measurement refuses over zero samples (font: {})",
            font.branch.label(),
            WITH_BLANK.chars().count(),
            placed.refusals,
            font.describe()
        );
        assert_eq!(
            placed.refusals.len(),
            WITH_BLANK.chars().count(),
            "every glyph is either drawn or refused, and here none is drawn"
        );
        assert_eq!(
            measure_placement(&placed),
            Err(MeasureError::NoDrawnGlyphs {
                refusals: WITH_BLANK.chars().count()
            }),
            "a row with no drawn glyph must be refused, never a silent zero"
        );
        return;
    }
    // The unconditional half of this test: the blank never shrinks the sample set quietly. Every
    // glyph of the row is either drawn or refused, and the blank's own refusal is in the list
    // (identified by the key the atlas holds for it) whatever the face does with the other ten.
    assert_eq!(
        placed.glyphs.len() + placed.refusals.len(),
        WITH_BLANK.chars().count(),
        "every glyph of the row is either drawn or refused with a reason"
    );
    let Some(blank_key) = blank_key else {
        eprintln!(
            "a_blank_glyph_is_reported_rather_than_silently_dropped_from_the_sample_set: {} - the \
             resolved face has no drawable glyph for U+0020 (S6 reports .notdef or a colour \
             glyph), so the placement refuses the blank without consulting the atlas and there is \
             no key to compare against; the refusal is still listed and the drawn glyphs still \
             measure. Refusals: {:?} (font: {})",
            font.branch.label(),
            placed.refusals,
            font.describe()
        );
        assert!(
            placed.refusals.iter().any(|refusal| matches!(
                refusal,
                GlyphRefusal::UncoveredCluster { .. } | GlyphRefusal::ColorGlyph { .. }
            )),
            "a blank the face cannot draw must be refused with a reason, got {:?}",
            placed.refusals
        );
        let alignment = measure_placement(&placed)
            .expect("the drawn glyphs of the blank probe row must still measure");
        assert_eq!(alignment.samples, placed.glyphs.len());
        assert_eq!(alignment.refusals, placed.refusals.len());
        assert!(
            alignment.worst_px <= HARNESS_ALIGNMENT_CONTRACT_PX,
            "the blank must not disturb the measurement: {alignment}"
        );
        eprintln!("RP-05 placement blank probe: {alignment}");
        return;
    };
    let blank_refusal = placed.refusals.iter().find(|refusal| match refusal {
        GlyphRefusal::MissingBitmap { key, .. } | GlyphRefusal::NoInk { key, .. } => {
            *key == blank_key
        }
        GlyphRefusal::ColorGlyph { .. } | GlyphRefusal::UncoveredCluster { .. } => false,
    });
    let Some(blank_refusal) = blank_refusal else {
        panic!(
            "the blank glyph must be reported among the row's refusals, not dropped: refusals \
             {:?} for key {blank_key:?}",
            placed.refusals
        );
    };
    match (blank_refusal, blank_state) {
        (GlyphRefusal::MissingBitmap { key, .. }, None) => {
            assert_eq!(*key, blank_key);
            eprintln!(
                "RP-05 placement: the atlas has no bitmap for the blank glyph ({}), so the \
                 placement records MissingBitmap and the measurement still counts {} samples",
                blank_key.glyph_id,
                placed.glyphs.len()
            );
        }
        (GlyphRefusal::NoInk { key, .. }, Some(bitmap)) if bitmap.is_empty() => {
            assert_eq!(*key, blank_key);
            eprintln!(
                "RP-05 placement: the atlas stored an empty ink box for the blank glyph ({}), so \
                 the placement records NoInk and the measurement still counts {} samples",
                blank_key.glyph_id,
                placed.glyphs.len()
            );
        }
        (refusal, state) => panic!(
            "the blank's refusal must match what the atlas holds for its key: refusal \
             {refusal:?}, atlas bitmap {state:?}"
        ),
    }
    // How many of the *other* ten glyphs are drawn is a coverage property of the resolved face
    // (an uncovered scalar is refused with a reason too); the blank itself is refused exactly once
    // either way, which is the claim this test exists for.
    if placed.glyphs.len() == WITH_BLANK.chars().count() - 1 {
        assert_eq!(placed.refusals.len(), 1, "and the blank is refused, once");
    } else {
        eprintln!(
            "a_blank_glyph_is_reported_rather_than_silently_dropped_from_the_sample_set: {} - the \
             resolved face draws {} of the {} glyphs of {WITH_BLANK:?} and refuses {} (the blank \
             plus every scalar it does not cover), so the exact refusal count is a coverage fact \
             of this face; the blank's own refusal is asserted above in every case (font: {})",
            font.branch.label(),
            placed.glyphs.len(),
            WITH_BLANK.chars().count(),
            placed.refusals.len(),
            font.describe()
        );
    }
    let alignment =
        measure_placement(&placed).expect("the inked glyphs of the blank probe row must measure");
    assert_eq!(alignment.samples, placed.glyphs.len());
    assert_eq!(alignment.refusals, placed.refusals.len());
    assert!(
        alignment.worst_px <= HARNESS_ALIGNMENT_CONTRACT_PX,
        "the blank must not disturb the measurement: {alignment}"
    );
    eprintln!("RP-05 placement blank probe: {alignment}");
}

#[test]
fn an_uncovered_cluster_is_refused_with_its_reason_never_measured_as_notdef() {
    let font = font();
    let advance_ratio = measure_advance_ratio(font, 16);
    let shape = probe_shape(advance_ratio);
    let (metrics, device_px) = probe_scale(font, advance_ratio, shape, 1.0);
    let ctx = probe_context(font, metrics, device_px);

    // The same noncharacter candidates the S7 block probes: no text face maps them, so S6 reports
    // `.notdef`. The `.notdef` box is not the cluster's glyph, so RP-05 has nothing to measure.
    const NONCHARACTERS: &[char] = &['\u{10FFFD}', '\u{FDD0}', '\u{0FFFF}', '\u{E0001}'];
    let mut atlas = GlyphAtlas::new(AtlasConfig::default());
    let mut exercised = 0_usize;
    for scalar in NONCHARACTERS {
        let text = scalar.to_string();
        if HONEST.cluster_columns(&text) == 0 {
            // A noncharacter that the K-04 table measures as zero columns is a combining /
            // variation-selector-class scalar: as the *first* cluster of a row S6 refuses it
            // (`ShapeError::LeadingZeroWidth`) because there is no base to join. That is a
            // documented outcome, not a skip: the next candidate carries the branch.
            eprintln!(
                "an_uncovered_cluster_is_refused_with_its_reason_never_measured_as_notdef: \
                 {scalar:?} measures 0 columns in the K-04 table, so it cannot shape as a leading \
                 cluster (S6 LeadingZeroWidth); probing the next candidate"
            );
            continue;
        }
        let input = RowClusters::new(1, 0, vec![cluster(0, &text)]);
        let shaped = shape_row(&input, &ctx, &HONEST).expect("the uncovered row must shape");
        if shaped.missing_glyphs == 0 {
            continue; // this host's face maps this candidate; the next one is the probe
        }
        assert_eq!(shaped.spans[0].glyphs[0].glyph_id, 0);
        // Rasterise the `.notdef` bitmap where the face has one, so the refusal is demonstrably
        // *not* an artefact of a missing bitmap: the box exists and is still refused.
        let notdef_bitmap = atlas.rasterize(ctx.atlas_key(0), &ctx.font).is_ok();
        let placed = place_row_with_vt_widths(&shaped, &input, &metrics, &atlas)
            .expect("an uncovered cluster is a placement, not an error");
        assert_eq!(placed.glyphs.len(), 0);
        assert_eq!(
            placed.refusals,
            vec![GlyphRefusal::UncoveredCluster { glyph_id: 0 }],
            "an uncovered cluster must be refused as UncoveredCluster, not measured as .notdef"
        );
        assert_eq!(
            measure_placement(&placed),
            Err(MeasureError::NoDrawnGlyphs { refusals: 1 })
        );
        exercised += 1;
        eprintln!(
            "RP-05 placement: scalar {scalar:?} is not covered by the face -> S6 .notdef -> \
             structured UncoveredCluster refusal (.notdef bitmap present: {notdef_bitmap}); \
             measure_placement reports NoDrawnGlyphs over 1 refusal"
        );
    }
    if exercised == 0 {
        eprintln!(
            "an_uncovered_cluster_is_refused_with_its_reason_never_measured_as_notdef: {} - this \
             face maps every noncharacter candidate, so the UncoveredCluster branch is not \
             exercised on this host; the MissingBitmap and NoInk refusals are asserted by \
             `a_measurement_over_zero_drawn_glyphs_is_refused_not_a_clean_zero` and \
             `a_blank_glyph_is_reported_rather_than_silently_dropped_from_the_sample_set`",
            font.branch.label()
        );
    }
}

#[test]
fn an_injected_misplacement_exceeds_the_contract_and_the_untouched_placement_does_not() {
    // Plan section 6.3 rule 10: an injection and its control, so the green measurement above
    // cannot be an always-green one. The injection is applied to the *produced* origins, i.e.
    // downstream of everything the placement computed, which is exactly what a wrong cell index,
    // a wrong device scale or a wrong rounding mode would do to them.
    let font = font();
    let advance_ratio = measure_advance_ratio(font, 16);
    let shape = probe_shape(advance_ratio);
    for scale in RP05_DPI_SCALES {
        let (metrics, device_px) = probe_scale(font, advance_ratio, shape, scale);
        let mut atlas = GlyphAtlas::new(AtlasConfig::default());
        let placed = place_probe_row(font, &metrics, device_px, &mut atlas, 3).placed;
        if placed.glyphs.is_empty() {
            // Nothing is drawn at this scale, so there is no produced origin to inject into. The
            // refusal is asserted and printed rather than the injection silently passing.
            eprintln!(
                "an_injected_misplacement_exceeds_the_contract_and_the_untouched_placement_does_\
                 not: {} - at {:.0}% DPI the resolved face draws none of the {} probe glyphs \
                 (refused: {:?}), so the injection control cannot be built at this scale and the \
                 documented refusal is asserted instead (font: {})",
                font.branch.label(),
                scale * 100.0,
                PLACEMENT_ROW.chars().count(),
                placed.refusals,
                font.describe()
            );
            assert_eq!(
                measure_placement(&placed),
                Err(MeasureError::NoDrawnGlyphs {
                    refusals: PLACEMENT_ROW.chars().count()
                }),
                "a row with no drawn glyph must be refused, never measured as 0.0px"
            );
            continue;
        }
        assert!(
            placed.glyphs.len() > 1,
            "the injection needs glyphs to move"
        );

        // Control: the untouched placement.
        let control = measure_placement(&placed).expect("the control row must measure");
        assert!(
            control.worst_px <= HARNESS_ALIGNMENT_CONTRACT_PX,
            "control at {:.0}% DPI must be inside the contract: {control}",
            scale * 100.0
        );
        eprintln!("RP-05 injection control: {control}");

        // Injection 1: every produced origin shifted by one device pixel.
        let mut shifted = placed.clone();
        for glyph in &mut shifted.glyphs {
            glyph.glyph_bitmap_origin = glyph.glyph_bitmap_origin.shifted(1.0, 0.0);
        }
        let injected = measure_placement(&shifted).expect("the shifted row must measure");
        assert!(
            injected.worst_px > HARNESS_ALIGNMENT_CONTRACT_PX,
            "a 1px misplacement must exceed the contract at {:.0}% DPI: {injected}",
            scale * 100.0
        );
        assert!(
            injected.worst_px <= 1.0 + HARNESS_ALIGNMENT_CONTRACT_PX,
            "a 1px misplacement must report about 1px, not more: {injected}"
        );
        eprintln!("RP-05 injection 1px: {injected}");

        // Injection 2: the bitmap placed from the wrong cell - one whole column to the right.
        let mut wrong_cell = placed.clone();
        for glyph in &mut wrong_cell.glyphs {
            glyph.glyph_bitmap_origin = glyph.glyph_bitmap_origin.shifted(metrics.cell_w_px, 0.0);
        }
        let injected = measure_placement(&wrong_cell).expect("the wrong-cell row must measure");
        assert!(
            injected.worst_px > HARNESS_ALIGNMENT_CONTRACT_PX,
            "placing the bitmap from the wrong cell must exceed the contract at {:.0}% DPI: \
             {injected}",
            scale * 100.0
        );
        assert!(
            injected.worst_px >= metrics.cell_w_px - HARNESS_ALIGNMENT_CONTRACT_PX,
            "a one-cell misplacement must report about one cell: {injected}"
        );
        eprintln!(
            "RP-05 injection wrong cell ({:.5}px): {injected}",
            metrics.cell_w_px
        );
    }
}
