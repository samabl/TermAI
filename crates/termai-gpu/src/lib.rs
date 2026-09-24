//! `termai-gpu` - the GPU backend decision point for the render pipeline (kernel/03 K-14).
//!
//! ADR-0027 D2 creates this crate as the T1 owner of "GPU backend probe, T0-T3 degradation,
//! draw/present". ADR-0027 D1 admits wgpu only (30.0.1, MIT OR Apache-2.0); winit, rustybuzz,
//! swash and fontdb belong to later slices, three of them under the ADR-0031 W-01 time-boxed
//! exception.
//!
//! What this crate does today:
//!
//! - owns the ADR-0014 decision 4 ladder (T0 -> T1 -> T2 -> T3) as a public grading enum
//!   ([`backend`]);
//! - owns the pure classification rule from a capability record to a level, with the ADR-0014
//!   mapping embedded next to it so it can be unit-tested without a GPU;
//! - probes wgpu headlessly, without a window and without panicking ([`probe`]): no adapter (the CI
//!   case) degrades to T3 with a structured reason, and a wgpu that cannot start is reported as
//!   "probe unavailable" rather than unwinding;
//! - owns the first real pixel path, **offscreen only** ([`offscreen`]): a texture + view, a
//!   minimal pipeline that draws cell-aligned quads from a plain rect-and-colour descriptor, and a
//!   deterministic readback. No window, no surface, no swapchain, no present (kernel/03 stage S9 is
//!   a later slice);
//! - owns the **grid-alignment measurement mechanism** ([`align`]) behind HARNESS section 5
//!   "网格对齐误差 ≤0.5px" / kernel/03 RP-05: it renders a cell-grid pattern offscreen, reads it
//!   back, and reports the worst deviation between a drawn quad edge and its ideal grid line in
//!   pixels, together with the number of edge samples behind that number.
//!
//! Authority and non-gating boundary (read this before quoting a number)
//! --------------------------------------------------------------------
//! The measurement **contract** belongs to `docs/spec/kernel/03-rendering-pipeline.md`: section
//! 3.5.3, RP-05, and OQ-RND-04 as decided by AR-24 clause 3 -
//! `max over drawn glyphs of abs(glyph_bitmap_origin - cell_box_origin) <= 0.5px`, judged separately
//! at 100/125/150/200% DPI. **kernel/03 RP-05 is the authority; kernel/06 owns the sampling
//! methodology.** This crate implements the mechanism, not the contract.
//!
//! Per the ADR-0014 iron law, on a non-T0 backend (or on a T0 backend that is not an RM-A / RM-C
//! with the pinned reference driver) **every number produced here is NON-GATING**: it may inform,
//! it may not judge a HARNESS section 5 gate, and ADR-0014 rule 4 forbids recording a failure on
//! T1/T2/T3 as a pass. The GPU tests therefore print the probed tier and the gate-eligibility flag
//! and assert the measured value on whatever tier the host offers, and when no adapter exists at
//! all they report an explicit "UNAVAILABLE" reason instead of a silent pass.
//!
//! What this crate deliberately does not do yet: glyphs (kernel/03 S6 shaping / S7 atlas), present
//! to a display (S9), a window or an IME (E-P0-2's remaining half), damage-driven redraw, and any
//! claim about a section 5 gate. Per ADR-0014 rule 1 those are judged on an RM-A / RM-C T0 backend
//! only, and per the crate's dependency position this crate is a leaf consumer of termai-core,
//! never of an app.
#![forbid(unsafe_code)]

pub mod align;
pub mod backend;
pub mod offscreen;
pub mod probe;

pub use align::{
    measure_alignment, rasterize_cpu, rect_coverage, AlignmentPattern, Axis, Edge, GridAlignment,
    GridGeometry, MeasureError, PatternCell, PixelBuffer, PixelBufferError, Rect,
    HARNESS_ALIGNMENT_CONTRACT_PX, LINE_HEIGHT_RATIO, RP05_DPI_SCALES,
};
pub use backend::{
    classify, classify_detailed, AdapterApi, AdapterCapability, AdapterClass, Backend, Capability,
    Classification, DegradationEvent, Platform, Reason,
};
pub use offscreen::{cell_quads, CellQuad, GpuError, OffscreenRenderer, TARGET_FORMAT};
pub use probe::{
    is_reference_driver, probe, AdapterReport, ProbeReport, ProbeUnavailable,
    PINNED_REFERENCE_DRIVERS,
};
