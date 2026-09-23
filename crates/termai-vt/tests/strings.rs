//! Test 2: string states always terminate, and following ground bytes reach print().

use termai_vt::{Terminal, VteBackend};

fn rows_joined(term: &Terminal) -> String {
    let mut out = String::new();
    for row in 0..term.grid().rows() {
        out.push_str(&term.grid().row_text(row));
    }
    out
}

#[test]
fn osc_terminates_by_every_mechanism() {
    let terminators: [&[u8]; 5] = [b"\x07", b"\x1b\\", b"\x18", b"\x1a", b"\x1bA"];
    for term in terminators {
        let mut t = Terminal::new(40, 4);
        t.feed(b"\x1b]0;payload");
        t.feed(term);
        t.feed(b"Z");
        assert!(
            rows_joined(&t).contains('Z'),
            "OSC did not return to ground for terminator {term:?}"
        );
    }
}

#[test]
fn dcs_sos_pm_apc_terminate_and_resume_printing() {
    let introducers: [&[u8]; 4] = [b"\x1bP", b"\x1bX", b"\x1b^", b"\x1b_"];
    let terminators: [&[u8]; 4] = [b"\x1b\\", b"\x18", b"\x1a", b"\x1bA"];
    for intro in introducers {
        for term in terminators {
            let mut t = Terminal::new(40, 4);
            t.feed(intro);
            t.feed(b"payload");
            t.feed(term);
            t.feed(b"Z");
            assert!(
                rows_joined(&t).contains('Z'),
                "string {intro:?} did not resume printing for terminator {term:?}"
            );
        }
    }
}

#[test]
fn length_limit_returns_to_ground() {
    let mut t = Terminal::with_backend(40, 4, Box::new(VteBackend::new().with_limits(16, 16)));
    t.feed(b"\x1b]0;");
    t.feed(&[b'x'; 40]);
    assert_eq!(t.counters().get(termai_vt::ParseErrorKind::OscOverflow), 1);
    t.feed(b"Z");
    assert!(rows_joined(&t).contains('Z'));

    let mut t = Terminal::with_backend(40, 4, Box::new(VteBackend::new().with_limits(16, 16)));
    t.feed(b"\x1bPq");
    t.feed(&[b'x'; 40]);
    assert_eq!(t.counters().get(termai_vt::ParseErrorKind::DcsOverflow), 1);
    t.feed(b"Z");
    assert!(rows_joined(&t).contains('Z'));
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }
}

#[test]
fn property_fuzz_like_inputs_always_resume() {
    const ALPHABET: [u8; 12] = [
        b'a', 0x1b, 0x18, 0x1a, 0x07, b']', b'[', b'\\', b'P', b'^', b'_', 0x9b,
    ];
    let prefixes: [&[u8]; 5] = [b"\x1b]", b"\x1bP", b"\x1b^", b"\x1b_", b"\x1b["];
    let mut rng = Lcg(0x1234_5678_9abc_def0);
    for (case, prefix) in prefixes.iter().enumerate() {
        for round in 0..50 {
            let mut t = Terminal::new(20, 4);
            t.feed(prefix);
            let mut noise = Vec::new();
            for _ in 0..12 {
                let pick = (rng.next() as usize + case + round) % ALPHABET.len();
                noise.push(ALPHABET[pick]);
            }
            t.feed(&noise);
            t.feed(b"\x1b\\");
            t.feed(b"Z");
            assert!(
                rows_joined(&t).contains('Z'),
                "case {case} round {round}: noise {noise:02x?} swallowed the trailing Z"
            );
        }
    }
}

#[test]
fn aborted_osc_processes_the_escape() {
    let mut t = Terminal::new(40, 4);
    t.feed(b"\x1b]0;title\x1b[31mred");
    assert_eq!(t.counters().get(termai_vt::ParseErrorKind::OscAborted), 1);
    assert_eq!(t.grid().row_text(0), "red");
}
