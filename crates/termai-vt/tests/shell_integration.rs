//! Test 8: OSC 133 / 633 command blocks and cwd handling.

use termai_vt::{ClipboardDecision, ClipboardDirection, Terminal};

#[test]
fn osc133_produces_exactly_one_block_with_exit_code() {
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b]133;A\x07prompt\x1b]133;B\x07echo hi\x1b]133;C\x07\r\nhi\r\n\x1b]133;D;0\x07");
    let blocks = t.shell().blocks();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].exit_code, Some(0));
    assert_eq!(blocks[0].cmd_id, 1);
    assert!(blocks[0].ended_ns.is_some());
    assert!(blocks[0].ended_ns >= Some(blocks[0].started_ns));
}

#[test]
fn missing_exit_code_stays_none() {
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b]133;A\x07\x1b]133;B\x07\x1b]133;C\x07x\r\n\x1b]133;D\x07");
    let blocks = t.shell().blocks();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].exit_code, None);
}

#[test]
fn nonzero_exit_code_is_preserved() {
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b]133;A\x07\x1b]133;B\x07\x1b]133;C\x07\r\nout\r\n\x1b]133;D;130\x07");
    assert_eq!(t.shell().blocks()[0].exit_code, Some(130));
}

#[test]
fn unpaired_d_is_dropped() {
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b]133;D;0\x07");
    assert!(t.shell().blocks().is_empty());
    assert_eq!(t.shell().unpaired_count(), 1);
}

#[test]
fn osc633_captures_command_locally_and_cwd() {
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b]633;A\x07\x1b]633;B\x07\x1b]633;E;rm -rf /tmp\x07\x1b]633;C\x07\r\nout\r\n\x1b]633;D;0\x07");
    t.feed(b"\x1b]633;P;Cwd=/home/me\x07");
    assert_eq!(t.shell().last_command(), Some("rm -rf /tmp"));
    assert_eq!(t.shell().cwd(), Some("/home/me"));
    assert_eq!(t.grid().cwd(), Some("/home/me"));
    // The command text must never leak into the grid.
    let mut screen = String::new();
    for row in 0..t.grid().rows() {
        screen.push_str(&t.grid().row_text(row));
    }
    assert!(!screen.contains("rm -rf"));
    assert_eq!(t.shell().blocks().len(), 1);
    assert_eq!(t.shell().blocks()[0].confidence, 100);
}

#[test]
fn osc7_parses_file_uri_and_marks_remote() {
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b]7;file://remotehost/home/me\x07");
    assert_eq!(t.grid().cwd(), Some("/home/me"));
    assert!(t.grid().cwd_remote());
    assert_eq!(t.shell().cwd(), Some("/home/me"));

    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b]7;file://localhost/a%20b\x07");
    assert_eq!(t.grid().cwd(), Some("/a b"));
    assert!(!t.grid().cwd_remote());
}

#[test]
fn osc52_read_is_denied_and_write_is_guarded() {
    // Read (query) is denied by default.
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b]52;c;?\x07");
    assert_eq!(t.grid().clipboard_read_requests(), 1);
    assert_eq!(t.grid().clipboard_write_requests(), 0);
    assert_eq!(t.grid().clipboard_denied_reads(), 1);
    let verdict = t.grid().last_clipboard().expect("read verdict");
    assert_eq!(verdict.direction, ClipboardDirection::Read);
    assert_eq!(verdict.decision, ClipboardDecision::DeniedRead);
    assert_eq!(t.grid().row_text(0), "");

    // Single-line, control-free write: allowed and NOT counted as denied.
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b]52;c;aGVsbG8=\x07");
    assert_eq!(t.grid().clipboard_write_requests(), 1);
    assert_eq!(t.grid().clipboard_allowed_single_line(), 1);
    assert_eq!(t.grid().clipboard_denied(), 0);
    let verdict = t.grid().last_clipboard().expect("write verdict");
    assert_eq!(verdict.direction, ClipboardDirection::Write);
    assert_eq!(verdict.decision, ClipboardDecision::GuardedAllowSingleLine);
    assert_eq!(verdict.decoded_len, 5);
    assert_eq!(verdict.line_count, 1);
    assert_eq!(t.grid().row_text(0), "");

    // Multi-line write: NeedsConfirm, never silently allowed.
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b]52;c;bGluZTEKbGluZTI=\x07");
    assert_eq!(t.grid().clipboard_needs_confirm_multi_line(), 1);
    assert_eq!(t.grid().clipboard_allowed_single_line(), 0);
    let verdict = t.grid().last_clipboard().expect("multiline verdict");
    assert_eq!(verdict.decision, ClipboardDecision::NeedsConfirmMultiLine);
    assert_eq!(verdict.line_count, 2);

    // Control characters (ESC) in a single line: NeedsConfirm.
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b]52;c;YRti\x07");
    assert_eq!(t.grid().clipboard_needs_confirm_control_chars(), 1);
    let verdict = t.grid().last_clipboard().expect("control verdict");
    assert_eq!(
        verdict.decision,
        ClipboardDecision::NeedsConfirmControlChars
    );
    assert_eq!(verdict.line_count, 1);

    // A malformed base64 payload falls back to raw-byte classification.
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b]52;c;not-base64!!\x07");
    assert_eq!(t.grid().clipboard_allowed_single_line(), 1);
    assert_eq!(t.grid().clipboard_write_requests(), 1);
}

#[test]
fn osc52_payload_never_leaks_into_metadata() {
    // AR-12 / AR-29 item 6: metadata only -- no content, summary or hash.
    let token = "UNIQUESECRETTOKEN";
    let encoded = "VU5JUVVFU0VDUkVUVE9LRU4=";
    let mut t = Terminal::new(80, 24);
    let report = t.feed(format!("\x1b]52;c;{encoded}\x07").as_bytes());

    let mut screen = String::new();
    for row in 0..t.grid().rows() {
        screen.push_str(&t.grid().row_text(row));
    }
    assert!(!screen.contains(token), "payload leaked to the grid");
    assert!(
        !screen.contains(encoded),
        "encoded payload leaked to the grid"
    );

    for (key, _) in t.counters().as_pairs() {
        assert!(!key.contains(token), "payload leaked into a counter name");
    }
    let verdict = t.grid().last_clipboard().expect("verdict");
    assert!(
        !format!("{verdict:?}").contains(token),
        "payload leaked into Debug"
    );
    assert!(!format!("{:?}", report.first_error).contains(token));
    assert!(!t.grid().title().contains(token));
    assert_eq!(t.shell().last_command(), None);

    // Only metadata is carried: length and line count.
    assert_eq!(verdict.decoded_len, token.len());
    assert_eq!(verdict.line_count, 1);
}

#[test]
fn osc9_progress_is_not_a_notification() {
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b]9;4;1;50\x07");
    assert_eq!(t.grid().progress_count(), 1);
    assert_eq!(t.grid().notification_count(), 0);
    t.feed(b"\x1b]9;build finished\x07");
    assert_eq!(t.grid().notification_count(), 1);
    t.feed(b"\x1b]777;notify;title;body\x07");
    assert_eq!(t.grid().notification_count(), 2);
}

#[test]
fn osc_title_strips_bidi_and_control() {
    let mut t = Terminal::new(80, 24);
    t.feed("\x1b]2;safe\u{202e}evil\u{2066}x\x07".as_bytes());
    assert_eq!(t.grid().title(), "safeevilx");
}

#[test]
fn osc8_scheme_whitelist() {
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b]8;;https://example.com\x07ok\x1b]8;;\x07");
    assert_eq!(t.grid().links().len(), 1);
    assert_eq!(t.grid().links()[0].target, "https://example.com");
    t.feed(b"\x1b]8;;javascript:alert(1)\x07bad\x1b]8;;\x07");
    assert_eq!(t.grid().links().len(), 1);
    assert_eq!(t.grid().rejected_links(), 1);
    t.feed(b"\x1b]8;;data:text/html,x\x07bad\x1b]8;;\x07");
    assert_eq!(t.grid().rejected_links(), 2);
}

#[test]
fn alternating_a_overwrites_pending_block() {
    let mut t = Terminal::new(80, 24);
    t.feed(b"\x1b]133;A\x07\x1b]133;A\x07\x1b]133;B\x07\x1b]133;C\x07\r\nx\r\n\x1b]133;D;0\x07");
    assert_eq!(t.shell().overlap_count(), 1);
    assert_eq!(t.shell().blocks().len(), 1);
}
