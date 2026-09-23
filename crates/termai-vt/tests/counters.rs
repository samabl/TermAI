//! Test 1: every one of the 14 counter keys has at least one triggering input.

use termai_vt::{ParseErrorKind, Terminal, VteBackend};

fn record(term: &Terminal, kind: ParseErrorKind, seen: &mut Vec<ParseErrorKind>) {
    if term.counters().get(kind) >= 1 {
        seen.push(kind);
    }
}

#[test]
fn every_counter_key_has_a_trigger() {
    let mut seen: Vec<ParseErrorKind> = Vec::new();

    let mut t = Terminal::new(40, 4);
    t.feed(b"\x1b[99z");
    record(&t, ParseErrorKind::CsiUnknown, &mut seen);

    let mut t = Terminal::new(40, 4);
    t.feed(b"\x1b[1<2m");
    record(&t, ParseErrorKind::CsiMalformed, &mut seen);

    let mut t = Terminal::new(40, 4);
    t.feed(b"\x1bZ");
    record(&t, ParseErrorKind::EscUnknown, &mut seen);

    let mut t = Terminal::new(40, 4);
    t.feed(b"\x1b]999;payload\x07");
    record(&t, ParseErrorKind::OscUnknown, &mut seen);

    let mut t = Terminal::new(40, 4);
    t.feed(b"\x1b]0;x\x1bA");
    record(&t, ParseErrorKind::OscAborted, &mut seen);

    let mut t = Terminal::new(40, 4);
    let mut osc = b"\x1b]0;".to_vec();
    osc.extend(std::iter::repeat(b'x').take(70_000));
    osc.push(0x07);
    t.feed(&osc);
    record(&t, ParseErrorKind::OscOverflow, &mut seen);

    let mut t = Terminal::new(40, 4);
    t.feed(b"\x1bPzpayload\x1b\\");
    record(&t, ParseErrorKind::DcsUnknown, &mut seen);

    let mut t = Terminal::with_backend(40, 4, Box::new(VteBackend::new().with_limits(64, 64)));
    let mut dcs = b"\x1bPq".to_vec();
    dcs.extend(std::iter::repeat(b'x').take(200));
    dcs.extend_from_slice(b"\x1b\\");
    t.feed(&dcs);
    record(&t, ParseErrorKind::DcsOverflow, &mut seen);

    let mut t = Terminal::new(40, 4);
    t.feed(b"\x1bPq\x07x\x1b\\");
    record(&t, ParseErrorKind::DcsBelInData, &mut seen);

    let mut t = Terminal::new(40, 4);
    t.feed(b"\x1b]0;x\x18");
    record(&t, ParseErrorKind::StringCancelled, &mut seen);

    let mut t = Terminal::new(40, 4);
    t.feed(b"\x1b\\");
    record(&t, ParseErrorKind::StStray, &mut seen);

    let mut t = Terminal::new(40, 4);
    t.feed(&[0x9b]);
    record(&t, ParseErrorKind::C1EightBitInUtf8, &mut seen);

    let mut t = Terminal::with_backend(40, 4, Box::new(VteBackend::new().with_eight_bit_c1(true)));
    t.feed(&[0x9b]);
    record(&t, ParseErrorKind::C1EightBitUsed, &mut seen);

    let mut t = Terminal::new(40, 4);
    t.feed(b"\xff");
    record(&t, ParseErrorKind::InvalidUtf8, &mut seen);

    for kind in ParseErrorKind::ALL {
        assert!(
            seen.contains(&kind),
            "no triggering input wired for counter key {}",
            kind.key()
        );
    }
    assert_eq!(seen.len(), 14);
}

#[test]
fn counter_pairs_are_sorted_and_osc_unknown_breakdown_works() {
    let mut t = Terminal::new(40, 4);
    t.feed(b"\x1b]999;a\x07\x1b]999;b\x07\x1b]1000;c\x07");
    assert_eq!(t.counters().get(ParseErrorKind::OscUnknown), 3);
    assert_eq!(t.counters().get_key("osc_unknown[999]"), Some(2));
    assert_eq!(t.counters().get_key("osc_unknown[1000]"), Some(1));

    let pairs = t.counters().as_pairs();
    assert_eq!(pairs.len(), 14);
    let mut sorted = pairs.clone();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    assert_eq!(pairs, sorted);
}

#[test]
fn counters_merge_sums() {
    let mut a = termai_vt::VtCounters::new();
    a.bump(ParseErrorKind::CsiUnknown);
    a.bump_osc_unknown(7);
    let mut b = termai_vt::VtCounters::new();
    b.bump(ParseErrorKind::CsiUnknown);
    b.bump_osc_unknown(7);
    a.merge(&b);
    assert_eq!(a.get(ParseErrorKind::CsiUnknown), 2);
    assert_eq!(a.get(ParseErrorKind::OscUnknown), 2);
    assert_eq!(a.osc_unknown_count(7), 2);
    assert_eq!(a.total(), 4);
}
