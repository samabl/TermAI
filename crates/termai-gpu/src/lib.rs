//! `termai-gpu` - the GPU backend decision point for the render pipeline (kernel/03 K-14).
//!
//! ADR-0027 D2 creates this crate as the T1 owner of "GPU backend probe, T0-T3 degradation,
//! draw/present". This first slice, per ADR-0027 D1, admits wgpu only (30.0.1,
//! MIT OR Apache-2.0); winit, rustybuzz, swash and fontdb belong to later slices, three of
//! them under the ADR-0031 W-01 time-boxed exception.
//!
//! What this slice does:
//!
//! - owns the ADR-0014 decision 4 ladder (T0 -> T1 -> T2 -> T3) as a public grading enum;
//! - owns the pure classification rule from a capability record to a level, with the
//!   ADR-0014 mapping embedded next to it so it can be unit-tested without a GPU;
//! - probes wgpu headlessly, without a window and without panicking: no adapter (the CI case)
//!   degrades to T3 with a structured reason, and a wgpu that cannot start is reported as
//!   "probe unavailable" rather than unwinding.
//!
//! What this slice deliberately does not do: draw, present, create a device or a surface,
//! own a window, or claim any section 5 / G3 gate. Per ADR-0014 rule 1 those are judged on an
//! RM-A / RM-C T0 backend only, and per the crate's dependency position this crate is a leaf
//! consumer of termai-core, never of an app.
#![forbid(unsafe_code)]

pub mod backend;
pub mod probe;

pub use backend::{
    classify, classify_detailed, AdapterApi, AdapterCapability, AdapterClass, Backend, Capability,
    Classification, DegradationEvent, Platform, Reason,
};
pub use probe::{
    is_reference_driver, probe, AdapterReport, ProbeReport, ProbeUnavailable,
    PINNED_REFERENCE_DRIVERS,
};
