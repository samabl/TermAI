//! `termai-render` - the UI side of the render pipeline (kernel/03).
//!
//! AR-01 / AR-03: this crate sits on the UI side of the damage -> shaping split
//! (kernel/03 K-02). It **never** parses VT bytes and **never** computes a column width
//! (kernel/03 K-04: the only width table lives in termai-vt).
//!
//! This first slice is deliberately pure logic and depends only on `termai-core`: the
//! render mirror and its revision self-healing. Shaping, atlas and GPU presentation arrive
//! with later ADRs, together with the dependency admission ADR-0015 requires (ADR-0024 D3
//! admits no third-party crate, so wgpu / rustybuzz / swash / winit are not in this build).
#![forbid(unsafe_code)]

pub mod mirror;
pub mod vrm;
