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
//! `wgpu` / `winit` / `termai-gpu` / `termai-vt` dependency (ADR-0024 D1, ADR-0027 D2: the K4
//! edge table stays `termai-render -> [termai-core]`, and no fake dependency is declared).
#![forbid(unsafe_code)]

pub mod atlas;
pub mod mirror;
pub mod shape;
pub mod vrm;

pub use atlas::{AtlasConfig, AtlasError, AtlasKey, GlyphAtlas, GlyphSize, GlyphSlot};
pub use shape::{
    glyph_flag, shape_row, span_flag, AaMode, CellWidthSource, ClusterInput, ClusterSpan, FontFace,
    FontId, RowClusters, RowGlyphs, ShapeContext, ShapeError, ShapedGlyph, SpanError,
};
