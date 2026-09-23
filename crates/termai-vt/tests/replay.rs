//! Replay format parsing and running.

use termai_vt::{parse_trec, run, Step, Terminal};

#[test]
fn parse_and_run_script() {
    let text = "TERMAI-REPLAY 1\nmeta {\"cols\":80,\"rows\":24}\n+0.000000 KEY \"\\x1b[31mred\\x1b[0m\"\n+0.000100 ASSERT ROW 0 = red\n+0.000200 ASSERT COUNTER csi_unknown = 0\n+0.000300 ASSERT CURSOR 0 3\n+0.000400 ASSERT TITLE \n";
    let script = parse_trec(text).expect("parse");
    assert_eq!(script.steps.len(), 5);
    let mut terminal = Terminal::new(80, 24);
    let report = run(&script, &mut terminal);
    assert!(
        report.failed.is_empty(),
        "unexpected failures: {:?}",
        report.failed
    );
    assert_eq!(report.passed, 5);
}

#[test]
fn reconnect_style_hex_feed_and_resize() {
    let text = "FEED 1b5b33316d\nRESIZE 40x10\nASSERT ROW 0 = \nASSERT CURSOR 0 0\n";
    let script = parse_trec(text).expect("parse");
    assert!(matches!(script.steps[0], Step::Feed(_)));
    assert_eq!(script.steps[1], Step::Resize { cols: 40, rows: 10 });
    let mut terminal = Terminal::new(80, 24);
    let report = run(&script, &mut terminal);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(terminal.grid().cols(), 40);
}

#[test]
fn replay_reports_failures() {
    let script = parse_trec("ASSERT ROW 0 = nope\n").expect("parse");
    let mut terminal = Terminal::new(80, 24);
    terminal.feed(b"yes");
    let report = run(&script, &mut terminal);
    assert_eq!(report.passed, 0);
    assert_eq!(report.failed.len(), 1);
    assert!(report.failed[0].contains("nope"));
}

#[test]
fn replay_rejects_unknown_directive() {
    assert!(parse_trec("EXPLODE now\n").is_err());
}

#[test]
fn replay_accepts_pty_out_and_sub_terminated_osc() {
    let text = "PTY_OUT 1b5d303b6869 07\nASSERT TITLE hi\n";
    let script = parse_trec(text).expect("parse");
    let mut terminal = Terminal::new(80, 24);
    let report = run(&script, &mut terminal);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
}
