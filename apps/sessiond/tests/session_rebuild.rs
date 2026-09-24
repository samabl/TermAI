//! AR-26 item 4, mechanism half: a sessiond rebuild / session restore reproduces the screen.
//!
//! HARNESS section 8.2's reliability row is a *timing* claim ("sessiond 重建 P95 <=2s /
//! P99 <=5s"), but a timing claim about a rebuild is worthless unless the rebuild reproduces
//! the screen, so this test asserts the half that must never be flaky: **state equality, not
//! a threshold**. The timing half lives in `tools/bench/sessiond-rebuild.mjs` (measurement
//! definition: kernel/06 section 3.10, registered in `tools/bench/reliability.mjs`).
//!
//! The fixture writes its Session Log through the real daemon path
//! (`sessiond::restore::fixture` -> `Registry::feed_pty_out`), so the "before" screen is the
//! live session's own snapshot, and the rebuild is the real recovery entry point
//! (`recover_session` replayed into a fresh `VtEngine` through `EngineReplay`).
//!
//! What is compared, and why each one is here:
//!
//! * `GridSnapshot` equality - cells, attributes, colours, links, cursor, modes, alt screen,
//!   wrap/origin flags, title, `scrollback_len`, scroll region and the per-row `LINE_WRAPPED`
//!   flags ADR-0025 added. This is the strongest available statement of "same screen";
//! * `canonical_bytes()` equality - the digest API's input, byte for byte (ADR-0025 D2);
//! * digest equality - what `ATTACH_ACK.snapshot_ref.grid_digest` carries, i.e. what the
//!   client compares against;
//! * per-row `row_text` equality - the visible rows, so a failure says *which* row drifted.

use std::path::PathBuf;

use sessiond::registry::TerminalEngine;
use sessiond::restore::{fixture, rebuild_session};
use termai_session::log::LogError;
use termai_session::state::SessionState;

const COLS: u16 = 120;
const ROWS: u16 = 40;
/// Enough lines to overfill the screen several times, so scrollback, wrap flags and the
/// cursor position all participate in the comparison.
const LINES: usize = 2_000;

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "termai-session-rebuild-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&p);
    p
}

#[test]
fn a_rebuild_reproduces_the_screen_the_live_session_held() {
    let dir = tmp_dir("equality");
    let fx = fixture::write_session_log(&dir, COLS, ROWS, LINES).expect("write the Log fixture");

    // The fixture has to be a rebuild worth measuring, and measurably faithful: a
    // checkpointed Log would take the CAS branch, which is unimplemented (see restore.rs).
    assert_eq!(fx.checkpoints, 0, "the fixture must not carry a checkpoint");
    assert!(
        fx.pty_out_records >= 8,
        "the Log must be non-trivial: {} PtyOut record(s)",
        fx.pty_out_records
    );
    assert!(
        fx.bytes > 64 * 1024,
        "the Log must carry a real screen: {} bytes",
        fx.bytes
    );
    assert_eq!(fx.input_events, 1, "one audit-only stdin record");

    let rebuilt = rebuild_session(&dir, COLS, ROWS).expect("rebuild the session from its Log");
    let after = rebuilt.engine.snapshot();

    // (1) the whole screen: cells + attributes + links + cursor + modes + title + scrollback
    //     + scroll region + per-row flags.
    assert_eq!(
        after, fx.state,
        "the rebuilt screen must equal the screen the live session held"
    );
    // (2) the digest API's input, byte for byte.
    assert_eq!(
        after.canonical_bytes(),
        fx.state.canonical_bytes(),
        "canonical bytes must be identical after the rebuild"
    );
    // (3) the digest the ATTACH_ACK references.
    assert_eq!(
        rebuilt.engine.digest(),
        fx.digest,
        "the rebuilt grid digest must equal the pre-rebuild digest"
    );
    // (4) the visible rows, individually, so a drift names its row.
    for row in 0..ROWS {
        assert_eq!(
            after.row_text(row),
            fx.state.row_text(row),
            "row {row} drifted across the rebuild"
        );
    }

    // The rebuild really was a full replay of the Log tail - not an empty directory, not a
    // checkpoint window it silently skipped.
    assert_eq!(
        rebuilt.outcome.resumed_from, None,
        "no checkpoint in the Log"
    );
    assert!(
        !rebuilt.outcome.digest_verified,
        "without a checkpoint there is no digest to verify, and recovery must say so"
    );
    assert_eq!(rebuilt.outcome.raw_replayed, fx.pty_out_records);
    assert_eq!(
        rebuilt.outcome.meta.input_events, 1,
        "PtyIn is indexed, never executed"
    );
    assert_eq!(rebuilt.outcome.meta.raw_bytes, fx.pty_out_bytes);
    assert_eq!(rebuilt.outcome.state, SessionState::Detached);
    assert!(!rebuilt.outcome.tail_truncated);

    // Repeatability on the same directory is what makes the timing producer possible, and it
    // is also the property that keeps the measurement from being a one-shot coincidence.
    let again = rebuild_session(&dir, COLS, ROWS).expect("second rebuild");
    assert_eq!(again.engine.snapshot(), fx.state);
    assert_eq!(again.engine.digest(), fx.digest);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_rebuild_of_an_empty_log_directory_is_an_error_not_an_empty_screen() {
    let dir = tmp_dir("nosession");
    std::fs::create_dir_all(&dir).expect("empty log directory");
    // A rebuild must never fabricate a blank "recovered" screen (AR-20): an empty directory is
    // a structured refusal, which is what makes the timing producer's equality check
    // meaningful rather than vacuous.
    match rebuild_session(&dir, COLS, ROWS) {
        Err(LogError::NoSegments) => {}
        Err(other) => panic!("expected NoSegments, got {other:?}"),
        Ok(_) => panic!("an empty Log directory must not rebuild into a screen"),
    }
    std::fs::remove_dir_all(&dir).ok();
}
