//! S6 shaping + S7 atlas against a **real** system font resolved through `fontdb`.
//!
//! These tests are the evidence for the two slices: they shape actual glyph ids out of an
//! actual font file and place them in actual atlas slots. There is no stub font and no silent
//! skip: if `fontdb` cannot produce a face that covers the transcripts below, every test in
//! this file fails with an explicit message, because a green run that never loaded a font
//! would prove nothing about S6 or S7.
//!
//! The K-04 width port is bound here to `unicode-width`, which is the same upstream table
//! `termai-vt` itself uses inside `crates/termai-vt/src/grid.rs`. The production binding is
//! termai-vt's width API; that crate edge is registered by its own admission step (ADR-0027
//! D2), so this file stands in for it without introducing a second width table.

use std::path::PathBuf;
use std::sync::OnceLock;

use termai_render::atlas::{AtlasConfig, GlyphAtlas, GlyphSize};
use termai_render::shape::{
    shape_row, AaMode, CellWidthSource, ClusterInput, FontFace, RowClusters, ShapeContext,
    ShapeError,
};
use unicode_width::UnicodeWidthStr;

/// Every test in this file shapes this transcript, so one face has to cover all of it.
const REQUIRED: &[char] = &[
    'T', 'e', 'r', 'm', 'A', 'I', ' ', '0', '1', '2', '3', '\u{4E2D}',
];

/// The ASCII control row: no cluster may need a fit.
const ASCII_ROW: &str = "TermAI 0123";

/// The honest K-04 port binding (see the module doc).
struct WidthTable;

impl CellWidthSource for WidthTable {
    fn cluster_columns(&self, cluster: &str) -> u8 {
        u8::try_from(UnicodeWidthStr::width(cluster)).unwrap_or(u8::MAX)
    }
}

/// A deliberately **wrong** width table, used only to prove the `fit_squeezed` counter can
/// move: it reports the two-column ideograph as one column, which is exactly the width-table
/// drift K-04 exists to catch. A counter that no input can move is not evidence.
struct UnderReporting;

impl CellWidthSource for UnderReporting {
    fn cluster_columns(&self, cluster: &str) -> u8 {
        if cluster.contains('\u{4E2D}') {
            1
        } else {
            WidthTable.cluster_columns(cluster)
        }
    }
}

const HONEST: WidthTable = WidthTable;
const UNDER: UnderReporting = UnderReporting;

struct ResolvedFont {
    id: fontdb::ID,
    index: u32,
    data: Vec<u8>,
    family: String,
    post_script_name: String,
    path: Option<PathBuf>,
}

impl ResolvedFont {
    fn describe(&self) -> String {
        format!(
            "family={:?} post_script_name={:?} path={:?} face_index={}",
            self.family, self.post_script_name, self.path, self.index
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

/// Resolve one monospaced system face that covers [`REQUIRED`], or fail loudly.
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
        for face in db.faces() {
            if !face.monospaced {
                continue;
            }
            let found = db.with_face_data(face.id, |data, index| {
                let parsed = rustybuzz::ttf_parser::Face::parse(data, index).ok()?;
                let covers = REQUIRED.iter().all(|ch| parsed.glyph_index(*ch).is_some());
                Some((data.to_vec(), covers))
            });
            let Some(Some((data, true))) = found else {
                continue;
            };
            let resolved = ResolvedFont {
                id: face.id,
                index: face.index,
                data,
                family: face
                    .families
                    .first()
                    .map_or_else(String::new, |(name, _)| name.clone()),
                post_script_name: face.post_script_name.clone(),
                path: source_path(&face.source),
            };
            eprintln!("S6/S7 evidence font: {}", resolved.describe());
            return resolved;
        }
        panic!(
            "fontdb found {} system faces but none was monospaced and covered all of {REQUIRED:?}; \
             the S6/S7 evidence requires a real font and must never silently skip",
            db.len()
        );
    })
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
    assert!(
        glyphs.spans[0].advance_px > ctx.cell_advance_px + 1.0e-3,
        "a two-column glyph must advance more than one cell, got {} vs cell {}",
        glyphs.spans[0].advance_px,
        ctx.cell_advance_px
    );
    assert_ne!(
        glyphs.spans[0].glyphs[0].glyph_id,
        0,
        "the resolved face really has a glyph for U+4E2D (font: {})",
        font.describe()
    );
    assert_eq!(glyphs.fit_squeezed, 0, "a two-column glyph fits two cells");
    assert_eq!(glyphs.missing_glyphs, 0);
    assert_eq!(glyphs.covered_cells, 3);
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
    assert_eq!(
        glyphs.fit_squeezed,
        0,
        "an ASCII row in a monospaced face must not move the fit counter (font: {})",
        font.describe()
    );
    assert_eq!(glyphs.missing_glyphs, 0);
    assert_eq!(glyphs.spans.len(), ASCII_ROW.chars().count());
    for span in &glyphs.spans {
        assert_eq!(span.cells, 1);
        assert_eq!(span.end_cell - span.start_cell, 1);
        assert!(!span.is_squeezed());
    }
}

#[test]
fn a_two_cell_glyph_in_a_one_cell_span_is_squeezed_and_the_control_is_not() {
    let font = font();
    let ctx = context(font, 16);
    // The width table under-reports U+4E2D as one column, so the row's own anchors follow it.
    let narrow = RowClusters::new(0, 0, vec![cluster(0, "\u{4E2D}"), cluster(1, "A")]);
    let squeezed = shape_row(&narrow, &ctx, &UNDER).expect("the row must shape");
    assert_eq!(
        squeezed.fit_squeezed, 1,
        "the fit counter must be reachable: a two-cell glyph in a one-cell span has to fit"
    );
    assert!(squeezed.spans[0].is_squeezed());
    assert!(
        squeezed.spans[0].fit_scale < 1.0,
        "the fit scale must be recorded for S7, got {}",
        squeezed.spans[0].fit_scale
    );
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

    // Control: the same text with the honest table stays at zero.
    let wide = RowClusters::new(0, 0, vec![cluster(0, "\u{4E2D}"), cluster(2, "A")]);
    let control = shape_row(&wide, &ctx, &HONEST).expect("the row must shape");
    assert_eq!(
        control.fit_squeezed, 0,
        "the control row must not move the counter"
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
    assert_ne!(glyph_id, 0);

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
