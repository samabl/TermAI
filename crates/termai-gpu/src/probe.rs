//! Headless wgpu capability probe: the only module in this crate that touches wgpu.
//!
//! It never creates a window or a surface, never unwraps and never panics across the
//! boundary (AGENTS section 6). Three outcomes are distinguished:
//!
//! 1. an adapter was enumerated -> the classification is derived from it;
//! 2. wgpu ran but found nothing (CI has no GPU) -> T3 with Reason::NoAdapter;
//! 3. wgpu could not be started -> T3 with an explicit ProbeUnavailable.
//!
//! There is no async runtime dependency: wgpu's adapter-enumeration future resolves on its
//! first poll on native targets, and the bounded poll loop below turns a future that never
//! becomes ready into outcome 3 instead of spinning forever.

use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use std::sync::Arc;
use std::task::Wake;

use crate::backend::{
    classify_detailed, AdapterApi, AdapterCapability, AdapterClass, Capability, Classification,
    Platform,
};

/// Driver-class substrings registered as the RM-A / RM-C pinned classes (ADR-0014 decision 1).
///
/// The pinning itself lives in the release-engineering machine fingerprint
/// (machine-fingerprint.json, ADR-0014 decision 1 rule 2), which this repository does not
/// carry yet. Nothing is registered here, so the probe stays conservative and reports
/// gate_eligible = false until a fingerprint lands. Registering a class is a deliberate
/// change to this list together with the ADR-0014 baseline reset.
pub const PINNED_REFERENCE_DRIVERS: &[&str] = &[];

/// Bounded poll budget for the adapter-enumeration future. The native wgpu future resolves on
/// its first poll; the bound only exists so that a future which never becomes ready cannot
/// turn this crate into a spinning loop. No runtime is added for it.
const MAX_PROBE_POLLS: usize = 1024;

/// Why the probe could not run at all. Both variants degrade to T3.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProbeUnavailable {
    /// No wgpu backend feature is compiled in for this target. wgpu::Instance::new has a
    /// documented panic for this case, so the probe checks first and returns instead.
    NoBackendFeature,
    /// The adapter-enumeration future never became ready within the bounded poll budget.
    EnumerationNotReady,
}

/// One enumerated adapter, in the vocabulary ADR-0014 classifies on.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AdapterReport {
    /// Adapter name as reported by the driver.
    pub name: String,
    /// Graphics API it was enumerated from.
    pub api: AdapterApi,
    /// Hardware, software or unclassified.
    pub class: AdapterClass,
    /// Backend-specific vendor id.
    pub vendor: u32,
    /// Backend-specific device id.
    pub device: u32,
    /// Driver name.
    pub driver: String,
    /// Driver detail string (version / build).
    pub driver_info: String,
}

impl AdapterReport {
    fn from_info(info: &wgpu::AdapterInfo) -> Self {
        Self {
            name: info.name.clone(),
            api: AdapterApi::from_wgpu(info.backend),
            class: AdapterClass::from_wgpu(info.device_type),
            vendor: info.vendor,
            device: info.device,
            driver: info.driver.clone(),
            driver_info: info.driver_info.clone(),
        }
    }
}

/// The probe outcome. classification is always safe to act on: it is T3 whenever anything
/// was missing, degraded or unavailable.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ProbeReport {
    /// The selected classification.
    pub classification: Classification,
    /// The adapter the classification was derived from, if any.
    pub adapter: Option<AdapterReport>,
    /// How many adapters wgpu enumerated before the best one was selected.
    pub adapter_count: usize,
    /// Set when the probe itself could not run, as opposed to running and finding nothing.
    pub unavailable: Option<ProbeUnavailable>,
}

impl ProbeReport {
    fn unavailable(reason: ProbeUnavailable, platform: Platform) -> Self {
        Self {
            classification: classify_detailed(&Capability {
                platform,
                adapter: None,
            }),
            adapter: None,
            adapter_count: 0,
            unavailable: Some(reason),
        }
    }
}

/// Enumerate adapters headlessly and classify the best one.
///
/// The best adapter is the one with the lowest Backend value, so a discrete T0 GPU wins over
/// a software fallback that the same driver stack also exposes (ADR-0014 decision 4: walk
/// T0 -> T1 -> T2 -> T3 and take the first available backend).
#[must_use]
pub fn probe() -> ProbeReport {
    let platform = Platform::host();

    if wgpu::Instance::enabled_backend_features().is_empty() {
        return ProbeReport::unavailable(ProbeUnavailable::NoBackendFeature, platform);
    }

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Some(adapters) = block_on(instance.enumerate_adapters(wgpu::Backends::all())) else {
        return ProbeReport::unavailable(ProbeUnavailable::EnumerationNotReady, platform);
    };

    let mut selected: Option<(Classification, AdapterReport)> = None;
    for adapter in &adapters {
        let info = adapter.get_info();
        let report = AdapterReport::from_info(&info);
        let classification = classify_detailed(&Capability {
            platform,
            adapter: Some(AdapterCapability {
                api: report.api,
                class: report.class,
                reference_driver: is_reference_driver(&info),
            }),
        });
        let better = match &selected {
            Some((current, _)) => classification.level < current.level,
            None => true,
        };
        if better {
            selected = Some((classification, report));
        }
    }

    match selected {
        Some((classification, adapter)) => ProbeReport {
            classification,
            adapter: Some(adapter),
            adapter_count: adapters.len(),
            unavailable: None,
        },
        None => ProbeReport {
            classification: classify_detailed(&Capability {
                platform,
                adapter: None,
            }),
            adapter: None,
            adapter_count: 0,
            unavailable: None,
        },
    }
}

/// Whether the driver class matches the pinned RM-A / RM-C class (ADR-0014 decision 1).
///
/// The list is empty in this slice (see PINNED_REFERENCE_DRIVERS), so this is conservatively
/// false on every host until a machine fingerprint is registered.
#[must_use]
pub fn is_reference_driver(info: &wgpu::AdapterInfo) -> bool {
    let driver = info.driver.to_ascii_lowercase();
    let driver_info = info.driver_info.to_ascii_lowercase();
    PINNED_REFERENCE_DRIVERS.iter().any(|pinned| {
        let pinned = pinned.to_ascii_lowercase();
        driver.contains(pinned.as_str()) || driver_info.contains(pinned.as_str())
    })
}

/// Poll a future to completion without an executor. Returns None if it never becomes ready
/// within MAX_PROBE_POLLS.
///
/// A no-op waker is correct here because the wgpu future does not park: it completes
/// synchronously on the first poll on native targets. The bound is the safety net.
///
/// `Waker::noop()` would be shorter but is only stable since Rust 1.85, while this workspace's
/// MSRV is 1.75; `std::task::Wake` + `Waker::from` has been stable since 1.51 and needs no
/// unsafe (this crate forbids it, and clippy's incompatible_msrv lint is denied by K2).
struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
    fn wake_by_ref(self: &Arc<Self>) {}
}

fn block_on<F: Future>(future: F) -> Option<F::Output> {
    let mut future = pin!(future);
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    for _ in 0..MAX_PROBE_POLLS {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return Some(output),
            Poll::Pending => std::thread::yield_now(),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{Backend, Reason};

    #[test]
    fn probe_is_headless_and_never_panics() {
        let report = probe();
        assert!(!report.classification.gate_eligible);
        if report.adapter.is_none() {
            assert_eq!(report.classification.level, Backend::T3);
            assert_eq!(report.classification.reason, Reason::NoAdapter);
            assert_eq!(report.adapter_count, 0);
        } else {
            assert!(report.adapter_count >= 1);
            assert!(report.classification.level <= Backend::T2);
        }
    }

    #[test]
    fn no_adapter_and_unavailable_are_both_safe_mode() {
        let unavailable =
            ProbeReport::unavailable(ProbeUnavailable::NoBackendFeature, Platform::host());
        assert_eq!(unavailable.classification.level, Backend::T3);
        assert_eq!(unavailable.classification.reason, Reason::NoAdapter);
        assert_eq!(
            unavailable.unavailable,
            Some(ProbeUnavailable::NoBackendFeature)
        );
    }
}
