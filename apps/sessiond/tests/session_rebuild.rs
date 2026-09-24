//! AR-26 item 4, mechanism half: a sessiond rebuild / session restore reproduces the screen.
//!
//! HARNESS section 8.2's reliability row is a *timing* claim ("sessiond 重建 P95 <=2s /
//! P99 <=5s"), but a timing claim about a rebuild is worthless unless the rebuild reproduces
//! the screen, so this test asserts the half that must never be flaky: **state equality, not
//! a threshold**. The timing half lives in `tools/bench/sessiond-rebuild.mjs` (measurement
//! definition: kernel/06 section 3.10, registered in `tools/bench/reliability.mjs`).
//!
//! The fixture writes its Session Log through the real daemon path
//! (`sessiond::restore::fixture` -> `Registry::feed_pty_out`, and `broker::on_resize`'s
//! resize-then-append order for `Resize`), so the "before" screen is the live session's own
//! snapshot, and the rebuild is the real recovery entry point (`recover_session` replayed into
//! a fresh `VtEngine` through the rebuild's replay sink).
//!
//! What is compared, and why each one is here:
//!
//! * `GridSnapshot` equality - cells, attributes, colours, links, cursor, modes, alt screen,
//!   wrap/origin flags, title, `scrollback_len`, scroll region, the grid size and the per-row
//!   `LINE_WRAPPED` flags ADR-0025 added. This is the strongest available statement of "same
//!   screen";
//! * `canonical_bytes()` equality - the digest API's input, byte for byte (ADR-0025 D2);
//! * digest equality - what `ATTACH_ACK.snapshot_ref.grid_digest` carries, i.e. what the
//!   client compares against;
//! * per-row `row_text` equality - the visible rows, so a failure says *which* row drifted.
//!
//! Coverage of the Log's screen-affecting record kinds (debt-p0 A24): `PtyOut` (the no-resize
//! control below), `Resize` in the middle of the Log and `Resize` at the tail of it, plus a
//! fixture that proves the resize is applied **where it sits** rather than at the final size
//! or not at all. `CheckpointRef` is *not* covered: CAS restore is unimplemented, so the
//! fixture still asserts it writes no checkpoint and that stays an open gap (A24 item 1).

use std::path::PathBuf;

use sessiond::registry::TerminalEngine;
use sessiond::restore::{fixture, rebuild_session, Rebuilt};
use termai_session::log::LogError;
use termai_session::state::SessionState;

const COLS: u16 = 120;
const ROWS: u16 = 40;
/// Enough lines to overfill the screen several times, so scrollback, wrap flags and the
/// cursor position all participate in the comparison.
const LINES: usize = 2_000;
/// Geometry the resized fixtures end at. Deliberately unrelated to `COLS`/`ROWS` by any simple
/// ratio, so "it happened to look the same" cannot be mistaken for "it replayed".
const MID_COLS: u16 = 61;
const MID_ROWS: u16 = 17;
const END_COLS: u16 = 80;
const END_ROWS: u16 = 24;

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

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// The four equality axes, so every test compares the same thing and a drift names itself.
/// The snapshot and canonical-byte axes are spelled as `assert!` on the comparison with the
/// digest in the message: a mismatch then reports 64 hex characters instead of dumping two
/// whole screens, which is what keeps the failure legible.
fn assert_rebuild_equals_live(rebuilt: &Rebuilt, fx: &fixture::LogFixture, tag: &str) {
    let after = rebuilt.engine.snapshot();
    let after_digest = rebuilt.engine.digest();
    // (1) the whole screen: cells + attributes + links + cursor + modes + title + scrollback
    //     + scroll region + grid size + per-row flags.
    assert!(
        after == fx.state,
        "{tag}: the rebuilt GridSnapshot differs from the screen the live session held \
         (rebuilt {} vs live {}; rebuilt {}x{} vs live {}x{}, scrollback {} vs {})",
        hex(&after_digest),
        hex(&fx.digest),
        after.cols,
        after.rows,
        fx.state.cols,
        fx.state.rows,
        after.scrollback_len,
        fx.state.scrollback_len
    );
    // (2) the digest API's input, byte for byte.
    assert!(
        after.canonical_bytes() == fx.state.canonical_bytes(),
        "{tag}: canonical bytes differ after the rebuild (rebuilt {} vs live {})",
        hex(&after_digest),
        hex(&fx.digest)
    );
    // (3) the digest the ATTACH_ACK references.
    assert_eq!(
        after_digest, fx.digest,
        "{tag}: the rebuilt grid digest must equal the pre-rebuild digest"
    );
    // (4) the visible rows, individually, so a drift names its row.
    for row in 0..fx.rows {
        assert_eq!(
            after.row_text(row),
            fx.state.row_text(row),
            "{tag}: row {row} drifted across the rebuild"
        );
    }
}

#[test]
fn a_rebuild_reproduces_the_screen_the_live_session_held() {
    let dir = tmp_dir("equality");
    let fx = fixture::write_session_log(&dir, COLS, ROWS, LINES).expect("write the Log fixture");

    // The fixture has to be a rebuild worth measuring, and measurably faithful: a
    // checkpointed Log would take the CAS branch, which is unimplemented (see restore.rs).
    assert_eq!(fx.checkpoints, 0, "the fixture must not carry a checkpoint");
    assert_eq!(
        fx.resize_records, 0,
        "this is the no-resize control: the change must not make the rebuild \
         resize-only-correct, so a Log with no Resize record still has to rebuild equal"
    );
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
    assert_rebuild_equals_live(&rebuilt, &fx, "control (no Resize)");
    assert_eq!(
        (
            rebuilt.engine.snapshot().cols,
            rebuilt.engine.snapshot().rows
        ),
        (COLS, ROWS),
        "with no Resize in the Log the rebuild must stay at the geometry it was given"
    );

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
    assert_rebuild_equals_live(&again, &fx, "control (no Resize), second rebuild");
    assert_eq!(again.engine.digest(), fx.digest);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_rebuild_replays_a_mid_log_resize_and_a_trailing_one() {
    let dir = tmp_dir("resize");
    // Two geometry changes: one a third of the way into the Log, one after the last PtyOut.
    // The live engine is resized at exactly those positions, so `fx.state` is the screen of a
    // session that really ran at 120x40, was resized to 61x17 mid-stream and then to 80x24 at
    // the end - and the rebuild only reproduces it if both records replay in Log order.
    let mid = fixture::script_len(COLS, ROWS, LINES) / 3;
    let resizes = [
        fixture::ResizeAt::after_bytes(mid, MID_COLS, MID_ROWS),
        fixture::ResizeAt::at_end(END_COLS, END_ROWS),
    ];
    let fx = fixture::write_session_log_with_resizes(&dir, COLS, ROWS, LINES, &resizes)
        .expect("write the resized Log fixture");

    assert_eq!(fx.checkpoints, 0, "the fixture must not carry a checkpoint");
    assert_eq!(
        fx.resize_records, 2,
        "the fixture must carry the mid-Log resize and the trailing one"
    );
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
    assert_eq!(
        (fx.state.cols, fx.state.rows),
        (END_COLS, END_ROWS),
        "the live session ended at the trailing resize's geometry"
    );

    // The rebuild is handed the spawn geometry, exactly as the daemon would on a reconnect;
    // the Log has to carry it from there to 80x24.
    let rebuilt = rebuild_session(&dir, COLS, ROWS).expect("rebuild the resized session");
    let after = rebuilt.engine.snapshot();
    assert_eq!(
        (after.cols, after.rows),
        (END_COLS, END_ROWS),
        "the rebuild must end at the geometry the live session ended with, not the spawn one"
    );
    assert_rebuild_equals_live(&rebuilt, &fx, "mid-Log + trailing Resize");

    // Recovery really replayed the whole tail, and every PtyOut is accounted for: a resume
    // from a checkpoint would show up here as a smaller count (and is unimplemented anyway).
    assert_eq!(
        rebuilt.outcome.resumed_from, None,
        "no checkpoint in the Log"
    );
    assert_eq!(rebuilt.outcome.raw_replayed, fx.pty_out_records);
    assert_eq!(rebuilt.outcome.meta.raw_bytes, fx.pty_out_bytes);
    assert!(!rebuilt.outcome.tail_truncated);
    assert_eq!(rebuilt.outcome.state, SessionState::Detached);

    let again = rebuild_session(&dir, COLS, ROWS).expect("second rebuild of the resized session");
    assert_rebuild_equals_live(&again, &fx, "mid-Log + trailing Resize, second rebuild");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_resize_is_applied_where_it_sits_not_at_the_end_or_from_the_start() {
    let dir_mid = tmp_dir("resize-position-mid");
    let dir_never = tmp_dir("resize-position-never");
    let dir_start = tmp_dir("resize-position-start");
    let mid = fixture::script_len(COLS, ROWS, LINES) / 2;

    // (a) the same byte script, resized mid-Log: everything after the resize is written at
    //     61x17, everything before it at 120x40;
    let fx_mid = fixture::write_session_log_with_resizes(
        &dir_mid,
        COLS,
        ROWS,
        LINES,
        &[fixture::ResizeAt::after_bytes(mid, MID_COLS, MID_ROWS)],
    )
    .expect("write the mid-resize fixture");
    // (b) the same byte script, never resized: this is what a rebuild that ignored the
    //     `Resize` record would produce;
    let fx_never = fixture::write_session_log(&dir_never, COLS, ROWS, LINES)
        .expect("write the control fixture");
    // (c) the same number of lines, created at 61x17 from the start: this is what a rebuild
    //     that replayed every byte at the final size would produce.
    let fx_start = fixture::write_session_log(&dir_start, MID_COLS, MID_ROWS, LINES)
        .expect("write the created-at-final-size fixture");

    // The discriminator has to be real, or the assertions below are vacuous: the resize
    // position changes the screen a live session ends with.
    assert_eq!(
        (fx_mid.state.cols, fx_mid.state.rows),
        (MID_COLS, MID_ROWS),
        "the resized session ended at the resize's geometry"
    );
    assert!(
        fx_mid.state != fx_never.state,
        "the same script at 120x40 throughout must not end on the same screen as a script \
         resized to 61x17 halfway through; otherwise this test proves nothing"
    );
    assert!(
        fx_mid.state != fx_start.state,
        "a session resized halfway must not end on the same screen as one created at the final \
         size; otherwise this test proves nothing"
    );

    let rebuilt = rebuild_session(&dir_mid, COLS, ROWS).expect("rebuild the mid-resize session");
    let after = rebuilt.engine.snapshot();
    assert_eq!(
        (after.cols, after.rows),
        (MID_COLS, MID_ROWS),
        "the rebuild must adopt the resize's geometry"
    );
    // (i) faithful: equal to the screen the resized live session held ...
    assert_rebuild_equals_live(&rebuilt, &fx_mid, "resize applied at its position");
    // (ii) ... which is *not* what ignoring the Resize would give (everything at 120x40) ...
    assert!(
        after != fx_never.state,
        "the rebuild landed on the pre-resize screen ({} vs {}): the Resize record was ignored",
        hex(&rebuilt.engine.digest()),
        hex(&fx_never.digest)
    );
    // (iii) ... and not what replaying the whole Log at the final size would give either.
    assert!(
        after != fx_start.state,
        "the rebuild landed on a session created at the final size ({} vs {}): the resize was \
         applied before (or instead of at) its position in the Log",
        hex(&rebuilt.engine.digest()),
        hex(&fx_start.digest)
    );

    // Both controls rebuild equal on their own terms, so the change is not resize-only-correct.
    let never = rebuild_session(&dir_never, COLS, ROWS).expect("rebuild the control session");
    assert_rebuild_equals_live(&never, &fx_never, "control (no Resize)");
    let start = rebuild_session(&dir_start, MID_COLS, MID_ROWS).expect("rebuild 61x17 session");
    assert_rebuild_equals_live(
        &start,
        &fx_start,
        "control (created at the resize's geometry)",
    );

    std::fs::remove_dir_all(&dir_mid).ok();
    std::fs::remove_dir_all(&dir_never).ok();
    std::fs::remove_dir_all(&dir_start).ok();
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
