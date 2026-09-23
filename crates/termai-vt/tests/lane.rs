//! Test 9: lane verdicts (AR-25.1). Only L0 may produce Pass/Fail for the gate.

use termai_vt::{lane_verdict, Lane, Verdict};

#[test]
fn l0_requires_one_hundred_percent() {
    assert_eq!(lane_verdict(Lane::L0Parser, 2000, 2000), Verdict::Pass);
    let verdict = lane_verdict(Lane::L0Parser, 1999, 2000);
    match verdict {
        Verdict::Fail { reason } => assert!(reason.contains("1999/2000")),
        other => panic!("expected Fail, got {other:?}"),
    }
    assert_eq!(
        lane_verdict(Lane::L0Parser, 0, 0),
        Verdict::NotApplicable {
            why: "no cases executed"
        }
    );
}

#[test]
fn l1_and_l2_never_pass_or_fail() {
    for lane in [Lane::L1Transport, Lane::L2EndToEnd] {
        for (passed, total) in [(0usize, 10usize), (5, 10), (10, 10)] {
            let verdict = lane_verdict(lane, passed, total);
            assert!(
                matches!(
                    verdict,
                    Verdict::Registered { .. } | Verdict::NotApplicable { .. }
                ),
                "lane {lane:?} returned {verdict:?}"
            );
        }
    }
}

#[test]
fn l2_with_failures_registers_difference_count() {
    assert_eq!(
        lane_verdict(Lane::L2EndToEnd, 8, 10),
        Verdict::Registered { differences: 2 }
    );
    assert_eq!(
        lane_verdict(Lane::L1Transport, 9, 10),
        Verdict::Registered { differences: 1 }
    );
}
