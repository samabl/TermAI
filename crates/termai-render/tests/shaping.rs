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

use std::path::PathBuf;
use std::sync::OnceLock;

use termai_render::atlas::{AtlasConfig, GlyphAtlas, GlyphSize};
use termai_render::shape::{
    shape_row, shape_row_with_vt_widths, AaMode, CellWidthSource, ClusterInput, FontFace,
    RowClusters, ShapeContext, ShapeError, VtWidthSource, VT_WIDTH,
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
