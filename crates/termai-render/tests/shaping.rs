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
//! Only two families of assertion depend on the resolved face, and neither is ever silently
//! dropped:
//!
//! * *"this face really has a glyph for U+4E2D"* - on a face with coverage the shaper must
//!   report glyph id `!= 0` and `missing_glyphs == 0`; on a face without it the honest,
//!   documented outcome is glyph id `0` (`.notdef`) and `missing_glyphs == 1`. Both directions
//!   are asserted explicitly, so neither branch is skipped.
//! * *"a monospaced face's ASCII advance is the cell advance"* ([`an_ascii_row_needs_no_fit`]) -
//!   kept behind the monospaced branch with a printed reason, because deriving the expected
//!   advance from the same measurement the shaper used would make the assertion a tautology
//!   rather than a check that the face is a fixed-pitch face.
//!
//! The `fit_squeezed` **positive** case is unconditional: [`SQUEEZE_PROBE`]'s columns are
//! under-reported relative to an advance the face itself defines (the cell advance *is* this
//! face's `"0"`), so the counter is reachable on any face and no step needs the host to own a
//! wide glyph. The two **control** claims that a two-column CJK cluster needs no fit stay
//! unconditional too; they assert a fixed-pitch property, so on the step-3-only host class
//! (fonts present, not one of them monospaced) a proportional face may legitimately falsify
//! them - the resolution line prints that warning before those tests run, rather than hiding it.
//! `ubuntu-latest`, `macos-latest` and any normal desktop reach step 1 or step 2, where every
//! assertion in this file runs at full strength.
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

use std::path::PathBuf;
use std::sync::OnceLock;

use termai_render::atlas::{AtlasConfig, AtlasError, GlyphAtlas, GlyphBitmap, GlyphSize};
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
                    // it runs. The two `fit_squeezed == 0` control claims below assert that a
                    // 2-cell CJK cluster fits exactly its 2 columns, which a proportional face
                    // is free to falsify (it is written with a narrower "0"); every other
                    // assertion in this file is independent of that and still runs.
                    eprintln!(
                        "S6/S7 font resolution: WARNING {} resolved a proportional face, so the \
                         two `fit_squeezed == 0` control claims (a two-column CJK cluster fits \
                         its two columns) are a fixed-pitch property and may fail honestly here",
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
    assert_eq!(glyphs.fit_squeezed, 0, "a two-column glyph fits two cells");
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

    // Control: the same text with the honest table stays at zero.
    let wide = RowClusters::new(0, 0, vec![cluster(0, "\u{4E2D}"), cluster(2, "A")]);
    let control = shape_row(&wide, &ctx, &HONEST).expect("the row must shape");
    assert_eq!(
        control.fit_squeezed,
        0,
        "the control row must not move the counter ({}; font: {})",
        font.branch.label(),
        font.describe()
    );

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
