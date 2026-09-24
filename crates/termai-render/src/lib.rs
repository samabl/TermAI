//! `termai-render` - the UI side of the render pipeline (kernel/03).
//!
//! AR-01 / AR-03: this crate sits on the UI side of the damage -> shaping split (kernel/03
//! K-02). It **never** parses VT bytes and **never** computes a column width (kernel/03 K-04:
//! the only width table lives in termai-vt, reached through the `shape::CellWidthSource` port).
//!
//! Slices landed so far:
//!
//! - S5 `mirror` / `vrm`: the disposable UI-side grid projection and its visual row map
//!   (ADR-0024 D1), pure logic on the core DTO.
//! - S6 `shape`: a row of clusters becomes per-cell glyph spans using the ADR-0027 D1 engines
//!   (`rustybuzz` for OpenType layout, `swash` for outlines and metrics).
//! - S7 `atlas`: the glyph key, page/slot allocation, page-level LRU eviction and the rebuild
//!   generation, with the miss / eviction / generation counters kernel/03 section 3.1 asks for.
//!
//! This crate still draws nothing: no rasterised bitmap, no texture, no swapchain, and no
//! `wgpu` / `winit` / `termai-gpu` dependency. Its one termai edge besides termai-core is the
//! K-04 width edge `termai-render -> termai-vt` (ADR-0027 D2, registered in the K4 edge table):
//! `shape::VtWidthSource` asks `termai_vt::width::measure` for a cluster's columns instead of
//! carrying a second wcwidth table.
#![forbid(unsafe_code)]

pub mod atlas;
pub mod mirror;
pub mod shape;
pub mod vrm;

pub use atlas::{AtlasConfig, AtlasError, AtlasKey, GlyphAtlas, GlyphSize, GlyphSlot};
pub use shape::{
    glyph_flag, shape_row, shape_row_with_vt_widths, span_flag, AaMode, CellWidthSource,
    ClusterInput, ClusterSpan, FontFace, FontId, RowClusters, RowGlyphs, ShapeContext, ShapeError,
    ShapedGlyph, SpanError, VtWidthSource, VT_WIDTH,
};
