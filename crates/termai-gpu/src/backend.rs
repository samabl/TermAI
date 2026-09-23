//! The ADR-0014 GPU backend ladder and its pure classification rule.
//!
//! ADR-0014 decision 4 fixes one contract: four levels T0 -> T1 -> T2 -> T3, selected
//! automatically on capability, each degradation structured and visible, and performance /
//! compatibility gates judged on T0 only. This module owns the ladder and the pure function
//! that maps a capability record onto it. It holds no wgpu state, so the whole table is
//! testable on a machine without a GPU (ADR-0014 reproducibility rule).

use termai_core::time::MonoTime;

/// Host platform family for the ADR-0014 decision 4 backend mapping.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Platform {
    /// Windows: T0 is DX12, T1 is Vulkan.
    Windows,
    /// macOS: T0 is Metal, T1 is OpenGL 4.1 (compatibility only).
    MacOs,
    /// Linux: T0 is Vulkan 1.3, T1 is OpenGL 4.5 / EGL.
    Linux,
    /// Any other target. No primary backend is defined, so nothing can claim T0.
    Other,
}

impl Platform {
    /// The platform this build runs on.
    #[must_use]
    pub const fn host() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Other
        }
    }

    /// ADR-0014 decision 4, T0 column: the gate backend for this platform.
    #[must_use]
    pub const fn primary_api(self) -> Option<AdapterApi> {
        match self {
            Self::Windows => Some(AdapterApi::Dx12),
            Self::MacOs => Some(AdapterApi::Metal),
            Self::Linux => Some(AdapterApi::Vulkan),
            Self::Other => None,
        }
    }

    /// ADR-0014 decision 4, T1 column: the degraded-but-usable backend for this platform.
    #[must_use]
    pub const fn secondary_api(self) -> Option<AdapterApi> {
        match self {
            Self::Windows => Some(AdapterApi::Vulkan),
            Self::MacOs | Self::Linux => Some(AdapterApi::Gl),
            Self::Other => None,
        }
    }
}

/// The graphics API wgpu enumerated the adapter from (ADR-0014 decision 4 backend column).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AdapterApi {
    /// Direct3D 12 (Windows T0).
    Dx12,
    /// Vulkan (Linux T0, Windows T1).
    Vulkan,
    /// Metal (macOS T0).
    Metal,
    /// OpenGL / OpenGL ES / EGL (the T1 compatibility path).
    Gl,
    /// Any other wgpu backend (the noop test backend, browser WebGPU, a future API). It is
    /// never a gate backend: ADR-0014 rule 4 forbids calling anything T0 by accident.
    Other,
}

impl AdapterApi {
    /// Map a wgpu backend onto the ADR-0014 vocabulary.
    #[must_use]
    pub fn from_wgpu(backend: wgpu::Backend) -> Self {
        match backend {
            wgpu::Backend::Dx12 => Self::Dx12,
            wgpu::Backend::Vulkan => Self::Vulkan,
            wgpu::Backend::Metal => Self::Metal,
            wgpu::Backend::Gl => Self::Gl,
            _ => Self::Other,
        }
    }

    /// Stable lowercase label for diagnostics and the capability report.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dx12 => "dx12",
            Self::Vulkan => "vulkan",
            Self::Metal => "metal",
            Self::Gl => "gl",
            Self::Other => "other",
        }
    }
}

/// Whether the adapter is real GPU hardware or a CPU rasteriser (ADR-0014 decision 4 T2).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AdapterClass {
    /// Discrete, integrated or virtualised GPU.
    Hardware,
    /// CPU rasteriser: WARP (D3D12 software adapter), lavapipe, or a vendor software fallback.
    Software,
    /// wgpu reported Other, so the adapter cannot honestly be placed in either class. It is
    /// admitted at the degraded level rather than claimed as gate hardware (AR-20).
    Unclassified,
}

impl AdapterClass {
    /// Map wgpu's DeviceType onto the two classes ADR-0014 names.
    #[must_use]
    pub fn from_wgpu(device_type: wgpu::DeviceType) -> Self {
        match device_type {
            wgpu::DeviceType::Cpu => Self::Software,
            wgpu::DeviceType::IntegratedGpu
            | wgpu::DeviceType::DiscreteGpu
            | wgpu::DeviceType::VirtualGpu => Self::Hardware,
            wgpu::DeviceType::Other => Self::Unclassified,
        }
    }
}

/// The ADR-0014 decision 4 backend ladder.
///
/// The derived Ord puts T0 first and T3 last, so min selects the best adapter and max walks
/// in the degradation direction T0 -> T1 -> T2 -> T3.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub enum Backend {
    /// T0 - primary gate backend. DX12 on Windows, Metal on macOS, Vulkan 1.3 on Linux.
    /// Full capability (graphics protocols, ligatures, VRR, >=120Hz). Section 5 gates are
    /// judged here only, and only on an RM-A / RM-C whose fingerprint is the pinned one.
    T0,
    /// T1 - degraded but usable. The platform's secondary API: Vulkan on Windows,
    /// OpenGL 4.5 / EGL on Linux, OpenGL 4.1 on macOS. Grid and shell complete; VRR,
    /// high-refresh optimisation and some compositing effects off.
    T1,
    /// T2 - software rasteriser. WARP, lavapipe or a CPU rasteriser. Text grid only:
    /// Sixel / kitty graphics / iTerm2 images, blur and shadow are off, and the frame rate is
    /// capped at 60Hz. Damage-incremental redraw is kept.
    T2,
    /// T3 - safe mode, the last resort. No GPU presentation acceleration: the L2 WebView is
    /// not loaded (AR-02), every graphics protocol and animation is off, and a diagnostics
    /// export is offered at startup. This is also the conservative default when nothing can
    /// be probed.
    #[default]
    T3,
}

/// What the probe found: the host platform plus at most one selected adapter.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Capability {
    /// Platform the probe ran on (injected so the rule stays pure and testable).
    pub platform: Platform,
    /// The selected adapter, or None when wgpu enumerated nothing.
    pub adapter: Option<AdapterCapability>,
}

/// The adapter facts ADR-0014 decision 4 classifies on.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AdapterCapability {
    /// Graphics API the adapter was enumerated from.
    pub api: AdapterApi,
    /// Hardware, software or unclassified.
    pub class: AdapterClass,
    /// Whether the driver matches the pinned RM-A / RM-C class (ADR-0014 decision 1).
    ///
    /// A mismatch does not lower the level: the adapter can still be T0-capable. It removes
    /// gate eligibility, because ADR-0014 rule 1 allows section 5 gates to be judged only on
    /// a reference machine with the pinned driver.
    pub reference_driver: bool,
}

/// Why a level was selected. Structured so a degradation event can be logged or exported
/// without string parsing (ADR-0014 decision 4 rule 2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reason {
    /// wgpu enumerated no adapter at all. The CI case.
    NoAdapter,
    /// The platform's T0 API was found on hardware.
    PrimaryBackend {
        /// The T0 API.
        api: AdapterApi,
        /// Whether the driver class is the pinned reference one.
        reference_driver: bool,
    },
    /// Hardware was found, but only through a non-primary API.
    SecondaryBackend {
        /// The API that was found.
        api: AdapterApi,
    },
    /// An adapter was found whose DeviceType cannot be classified.
    UnclassifiedAdapter {
        /// The API that was found.
        api: AdapterApi,
    },
    /// The adapter is a CPU rasteriser (WARP, lavapipe, CPU fallback).
    SoftwareAdapter {
        /// The API that was found.
        api: AdapterApi,
    },
}

/// A classification result: the level, the structured reason and whether ADR-0014 permits
/// the section 5 gates to be judged on it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Classification {
    /// The selected level.
    pub level: Backend,
    /// Why that level was selected.
    pub reason: Reason,
    /// ADR-0014 decision 1 rule 1: only an RM-A / RM-C at T0 (pinned driver) may judge the
    /// section 5 gates. Everything else is NON-GATING / judged a failure.
    pub gate_eligible: bool,
}

/// A structured degradation event (ADR-0014 decision 4 rule 2): level, reason and the
/// injected monotonic timestamp the audit log records.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DegradationEvent {
    /// When the degradation was observed; injected by the caller, never read from a clock here.
    pub at: MonoTime,
    /// Level that was selected.
    pub level: Backend,
    /// Why it was selected.
    pub reason: Reason,
}

impl Classification {
    /// The degradation event for this result, or None at T0 with a pinned driver.
    ///
    /// ADR-0014 rule 2 only requires an event when the run actually degraded from the gate
    /// backend; a clean T0 on a reference machine is not a degradation.
    #[must_use]
    pub const fn degradation_event(&self, at: MonoTime) -> Option<DegradationEvent> {
        if self.gate_eligible {
            None
        } else {
            Some(DegradationEvent {
                at,
                level: self.level,
                reason: self.reason,
            })
        }
    }
}

/// The pure classification rule: capability record -> ADR-0014 level.
///
/// ADR-0014 decision 4, embedded:
///
/// - no adapter -> T3 (safe mode; no GPU presentation acceleration);
/// - a software adapter (WARP / lavapipe / CPU) -> T2, even on the platform's T0 API;
/// - hardware on the platform's primary API -> T0;
/// - everything else hardware (secondary API, or an unclassifiable adapter) -> T1.
///
/// The pinned reference driver class is deliberately not an input to the level: it only
/// decides gate eligibility, which classify_detailed reports separately.
#[must_use]
pub fn classify(capability: &Capability) -> Backend {
    classify_detailed(capability).level
}

/// The same rule as classify, with the structured reason and the gate-eligibility flag.
#[must_use]
pub fn classify_detailed(capability: &Capability) -> Classification {
    let Some(adapter) = capability.adapter.as_ref() else {
        return Classification {
            level: Backend::T3,
            reason: Reason::NoAdapter,
            gate_eligible: false,
        };
    };

    match adapter.class {
        AdapterClass::Software => Classification {
            level: Backend::T2,
            reason: Reason::SoftwareAdapter { api: adapter.api },
            gate_eligible: false,
        },
        AdapterClass::Unclassified => Classification {
            level: Backend::T1,
            reason: Reason::UnclassifiedAdapter { api: adapter.api },
            gate_eligible: false,
        },
        AdapterClass::Hardware => {
            if capability.platform.primary_api() == Some(adapter.api) {
                Classification {
                    level: Backend::T0,
                    reason: Reason::PrimaryBackend {
                        api: adapter.api,
                        reference_driver: adapter.reference_driver,
                    },
                    gate_eligible: adapter.reference_driver,
                }
            } else {
                Classification {
                    level: Backend::T1,
                    reason: Reason::SecondaryBackend { api: adapter.api },
                    gate_eligible: false,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hardware(platform: Platform, api: AdapterApi, reference_driver: bool) -> Capability {
        Capability {
            platform,
            adapter: Some(AdapterCapability {
                api,
                class: AdapterClass::Hardware,
                reference_driver,
            }),
        }
    }

    fn with_class(platform: Platform, api: AdapterApi, class: AdapterClass) -> Capability {
        Capability {
            platform,
            adapter: Some(AdapterCapability {
                api,
                class,
                reference_driver: false,
            }),
        }
    }

    fn no_adapter(platform: Platform) -> Capability {
        Capability {
            platform,
            adapter: None,
        }
    }

    #[test]
    fn t0_is_hardware_on_the_platform_primary_api() {
        assert_eq!(
            classify(&hardware(Platform::Windows, AdapterApi::Dx12, true)),
            Backend::T0
        );
        assert_eq!(
            classify(&hardware(Platform::MacOs, AdapterApi::Metal, true)),
            Backend::T0
        );
        assert_eq!(
            classify(&hardware(Platform::Linux, AdapterApi::Vulkan, true)),
            Backend::T0
        );
    }

    #[test]
    fn t1_is_hardware_on_the_platform_secondary_api() {
        assert_eq!(
            classify(&hardware(Platform::Windows, AdapterApi::Vulkan, false)),
            Backend::T1
        );
        assert_eq!(
            classify(&hardware(Platform::Linux, AdapterApi::Gl, false)),
            Backend::T1
        );
        assert_eq!(
            classify(&hardware(Platform::MacOs, AdapterApi::Gl, false)),
            Backend::T1
        );
    }

    #[test]
    fn t2_is_a_software_adapter_even_on_the_primary_api() {
        assert_eq!(
            classify(&with_class(
                Platform::Windows,
                AdapterApi::Dx12,
                AdapterClass::Software
            )),
            Backend::T2
        );
        assert_eq!(
            classify(&with_class(
                Platform::Linux,
                AdapterApi::Vulkan,
                AdapterClass::Software
            )),
            Backend::T2
        );
    }

    #[test]
    fn t3_is_the_no_adapter_degradation() {
        let classification = classify_detailed(&no_adapter(Platform::Linux));
        assert_eq!(classification.level, Backend::T3);
        assert_eq!(classification.reason, Reason::NoAdapter);
        assert!(!classification.gate_eligible);
        assert_eq!(classify(&no_adapter(Platform::Linux)), Backend::T3);
    }

    #[test]
    fn ladder_orders_best_to_worst() {
        assert!(Backend::T0 < Backend::T1);
        assert!(Backend::T1 < Backend::T2);
        assert!(Backend::T2 < Backend::T3);
        assert_eq!(Backend::default(), Backend::T3);
    }

    #[test]
    fn reference_driver_governs_gate_eligibility_not_the_level() {
        let pinned = classify_detailed(&hardware(Platform::Linux, AdapterApi::Vulkan, true));
        assert_eq!(pinned.level, Backend::T0);
        assert!(pinned.gate_eligible);

        let unpinned = classify_detailed(&hardware(Platform::Linux, AdapterApi::Vulkan, false));
        assert_eq!(unpinned.level, Backend::T0);
        assert!(!unpinned.gate_eligible);
    }

    #[test]
    fn unclassified_hardware_degrades_to_t1() {
        assert_eq!(
            classify(&with_class(
                Platform::Windows,
                AdapterApi::Dx12,
                AdapterClass::Unclassified
            )),
            Backend::T1
        );
    }

    #[test]
    fn degradation_event_exists_for_every_non_gate_result() {
        let degraded = classify_detailed(&hardware(Platform::Windows, AdapterApi::Vulkan, false));
        let event = degraded.degradation_event(MonoTime::from_millis(7));
        assert!(event.is_some());
        if let Some(event) = event {
            assert_eq!(event.level, Backend::T1);
            assert_eq!(event.at, MonoTime::from_millis(7));
        }

        let gate = classify_detailed(&hardware(Platform::Linux, AdapterApi::Vulkan, true));
        assert!(gate.degradation_event(MonoTime::ZERO).is_none());
    }
}
