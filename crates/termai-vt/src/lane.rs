//! Byte-fidelity lanes and their verdicts (AR-25 / kernel/01 K-02).
//!
//! The 100% / >=99% compatibility gate may ONLY be produced on the L0 parser lane.
//! L1 (transport) and L2 (end-to-end) never emit Pass/Fail for that gate; they
//! register differences or report NotApplicable.

/// The three consistency lanes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lane {
    /// Pure parser: bytes fed straight to the VtBackend (G1 is judged here only).
    L0Parser,
    /// Transport: PTY / ConPTY / SSH / container / WSL.
    L1Transport,
    /// End-to-end: headless session + Local API + replay.
    L2EndToEnd,
}

/// Outcome of a lane evaluation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// Gate passed.
    Pass,
    /// Gate failed, with the first failing offset / case named.
    Fail {
        /// Human readable reason, including the first failing offset.
        reason: String,
    },
    /// Differences were registered; not a compatibility gate verdict.
    Registered {
        /// Number of registered differences.
        differences: usize,
    },
    /// The lane does not produce a gate verdict.
    NotApplicable {
        /// Why the lane is not applicable.
        why: &'static str,
    },
}

/// Evaluate a lane. L0 requires 100%; a single miss is a Fail naming the first
/// failing offset. L1/L2 always produce Registered or NotApplicable.
#[must_use]
pub fn lane_verdict(lane: Lane, passed: usize, total: usize) -> Verdict {
    if total == 0 {
        return Verdict::NotApplicable {
            why: "no cases executed",
        };
    }
    let passed = passed.min(total);
    match lane {
        Lane::L0Parser => {
            if passed == total {
                Verdict::Pass
            } else {
                Verdict::Fail {
                    reason: format!(
                        "L0 parser lane requires 100%: {passed}/{total} passed; first failing offset is case {}",
                        passed + 1
                    ),
                }
            }
        }
        Lane::L1Transport => {
            if passed == total {
                Verdict::NotApplicable {
                    why: "L1 transport does not gate VT compatibility; bytes were faithful",
                }
            } else {
                Verdict::Registered {
                    differences: total - passed,
                }
            }
        }
        Lane::L2EndToEnd => {
            if passed == total {
                Verdict::NotApplicable {
                    why: "L2 e2e does not gate VT compatibility; replay passed",
                }
            } else {
                Verdict::Registered {
                    differences: total - passed,
                }
            }
        }
    }
}
