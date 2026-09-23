//! Test 7: golden write -> parse -> write is a fixed point and the hash is stable.

use termai_vt::{golden_hash, parse_golden, write_golden, Terminal};

#[test]
fn golden_write_parse_write_is_fixed_point() {
    let mut t = Terminal::new(10, 3);
    t.feed(b"hi\x1b[31m!\x1b[0m");
    t.feed(b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07");
    t.feed("\u{6c49}".as_bytes());
    let snap = t.snapshot();
    let text1 = write_golden(&snap);
    let doc = parse_golden(&text1).expect("golden should parse");
    assert_eq!(doc.snapshot, snap, "parsed snapshot differs:\n{text1}");
    let text2 = write_golden(&doc.snapshot);
    assert_eq!(text1, text2, "write/parse/write is not a fixed point");
}

#[test]
fn golden_hash_is_stable_across_runs() {
    let mut t = Terminal::new(4, 2);
    t.feed(b"abc");
    let snap = t.snapshot();
    let first = write_golden(&snap);
    let second = write_golden(&snap);
    assert_eq!(first, second);
    let doc = parse_golden(&first).expect("parse");
    assert_eq!(
        doc.hash,
        golden_hash(&first[..first.find("hash ").expect("hash line")])
    );
}

#[test]
fn golden_records_wide_right_half() {
    let mut t = Terminal::new(4, 2);
    t.feed("\u{6c49}".as_bytes());
    let text = write_golden(&t.snapshot());
    assert!(
        text.contains("\\0"),
        "wide continuation should be written as \\0: {text}"
    );
    let doc = parse_golden(&text).expect("parse");
    assert_eq!(doc.snapshot, t.snapshot());
}

#[test]
fn golden_round_trips_attributes() {
    let mut t = Terminal::new(6, 2);
    t.feed(b"\x1b[1;4;38;2;1;2;3;48;5;7mZ");
    let snap = t.snapshot();
    let text = write_golden(&snap);
    assert!(
        text.contains("attr "),
        "attributes should produce an attr line: {text}"
    );
    let doc = parse_golden(&text).expect("parse");
    assert_eq!(doc.snapshot, snap);
}

#[test]
fn golden_rejects_tampered_body() {
    let mut t = Terminal::new(4, 2);
    t.feed(b"x");
    let text = write_golden(&t.snapshot());
    let tampered = text.replace("row 0000 x", "row 0000 y");
    assert!(parse_golden(&tampered).is_err());
}

#[test]
fn golden_has_no_nondeterministic_tokens() {
    let mut t = Terminal::new(8, 2);
    t.feed(b"hello");
    let text = write_golden(&t.snapshot());
    assert!(!text.contains("0x7ff"));
    assert!(!text.contains("timestamp"));
    assert!(!text.contains("commit"));
}
