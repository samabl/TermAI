//! Test 10: the public API is usable without naming any vte type.
//!
//! This crate has no vte dependency of its own, so if a vte type leaked into a public
//! signature this file would not compile.

use termai_vt::{
    AdvanceReport, BackendCaps, BackendId, EscapeSink, Params, StringKind, StringTerm, Terminal,
    VtBackend, VtCounters,
};

#[derive(Default)]
struct RecordingSink {
    printed: String,
    osc_count: usize,
}

impl EscapeSink for RecordingSink {
    fn print(&mut self, ch: char) {
        self.printed.push(ch);
    }
    fn execute(&mut self, _byte: u8) {}
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
    fn csi_dispatch(
        &mut self,
        _params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: u8,
    ) {
    }
    fn osc_dispatch(&mut self, _params: &[&[u8]], _term: StringTerm) {
        self.osc_count += 1;
    }
    fn dcs_hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: u8) {}
    fn dcs_put(&mut self, _byte: u8) {}
    fn dcs_unhook(&mut self, _term: StringTerm) {}
    fn sos_pm_apc(&mut self, _kind: StringKind, _params: &[&[u8]], _term: StringTerm) {}
}

struct PassthroughBackend;

impl VtBackend for PassthroughBackend {
    fn advance(&mut self, bytes: &[u8], sink: &mut dyn EscapeSink) -> AdvanceReport {
        for &byte in bytes {
            sink.print(char::from(byte));
        }
        AdvanceReport {
            consumed: bytes.len(),
            first_error: None,
            counters: VtCounters::new(),
        }
    }
    fn reset(&mut self) {}
    fn id(&self) -> BackendId {
        BackendId::CleanRoom { rev: 1 }
    }
    fn caps(&self) -> BackendCaps {
        BackendCaps::default()
    }
}

#[test]
fn custom_backend_and_sink_compile_without_vte() {
    let mut terminal = Terminal::with_backend(10, 2, Box::new(PassthroughBackend));
    terminal.feed(b"hi");
    assert_eq!(terminal.grid().row_text(0), "hi");
    assert_eq!(terminal.backend_label(), "cleanroom-r1");

    let mut sink = RecordingSink::default();
    let mut backend = PassthroughBackend;
    let report = backend.advance(b"ab", &mut sink);
    assert_eq!(report.consumed, 2);
    assert_eq!(sink.printed, "ab");
    assert_eq!(backend.id(), BackendId::CleanRoom { rev: 1 });
    assert!(!backend.caps().byte_offsets);
}
