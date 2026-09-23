//! Required-API smoke coverage: digest, take_delta, reset, caps, Params.

use termai_vt::{
    BackendId, EscapeSink, Params, StringKind, StringTerm, Terminal, VtBackend, VteBackend,
    DCS_LEN_LIMIT_DEFAULT, MAX_PARAMS, OSC_LEN_LIMIT_DEFAULT,
};

#[test]
fn digest_is_stable_and_content_sensitive() {
    let mut t = Terminal::new(10, 3);
    t.feed(b"abc");
    let first = t.grid().digest();
    let second = t.grid().digest();
    assert_eq!(first, second);

    let mut u = Terminal::new(10, 3);
    u.feed(b"abd");
    assert_ne!(first, u.grid().digest());
}

#[test]
fn take_delta_reports_damage_and_full_resync() {
    let mut t = Terminal::new(10, 3);
    let initial = t.take_delta(0).expect("initial damage");
    assert!(initial.damage.full);
    assert_eq!(initial.rows.len(), 3);
    assert!(t.take_delta(initial.rev).is_none());

    t.feed(b"x");
    let delta = t.take_delta(initial.rev).expect("damage after print");
    assert!(delta.damage.rows.contains(&0));
    assert_eq!(delta.rows.len(), 1);
    assert_eq!(delta.rows[0].cells[0].ch, 'x');

    t.feed(b"y");
    let stale = t.take_delta(0).expect("stale rev forces a resync");
    assert!(stale.damage.full);
}

#[test]
fn reset_clears_grid_and_counters() {
    let mut t = Terminal::new(10, 3);
    t.feed(b"\x1b[31mabc\x1b[99z");
    assert!(t.counters().total() > 0);
    t.reset();
    assert_eq!(t.grid().row_text(0), "");
    assert_eq!(t.counters().total(), 0);
    assert_eq!(t.grid().cursor(), (0, 0));
}

#[test]
fn default_caps_match_the_contract_constants() {
    assert_eq!(OSC_LEN_LIMIT_DEFAULT, 64 * 1024);
    assert_eq!(DCS_LEN_LIMIT_DEFAULT, 1024 * 1024);
    assert_eq!(MAX_PARAMS, 16);
    let backend = VteBackend::new();
    let caps = backend.caps();
    assert_eq!(caps.osc_len_limit, OSC_LEN_LIMIT_DEFAULT);
    assert_eq!(caps.dcs_len_limit, DCS_LEN_LIMIT_DEFAULT);
    assert!(!caps.eight_bit_c1);
    assert!(!caps.byte_offsets);
    assert_eq!(backend.id(), BackendId::Vte { version: "0.15" });
}

#[derive(Default)]
struct CaptureSink {
    csi: Vec<u16>,
    osc_params: Vec<Vec<u8>>,
    osc_term: Option<StringTerm>,
    sos_kind: Option<StringKind>,
}

impl EscapeSink for CaptureSink {
    fn print(&mut self, _ch: char) {}
    fn execute(&mut self, _byte: u8) {}
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, _action: u8) {
        self.csi = params.iter().collect();
        assert_eq!(params.len(), self.csi.len());
        assert_eq!(params.as_slice(), self.csi.as_slice());
    }
    fn osc_dispatch(&mut self, params: &[&[u8]], term: StringTerm) {
        self.osc_params = params.iter().map(|p| p.to_vec()).collect();
        self.osc_term = Some(term);
    }
    fn dcs_hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: u8) {}
    fn dcs_put(&mut self, _byte: u8) {}
    fn dcs_unhook(&mut self, _term: StringTerm) {}
    fn sos_pm_apc(&mut self, kind: StringKind, _params: &[&[u8]], _term: StringTerm) {
        self.sos_kind = Some(kind);
    }
}

#[test]
fn params_flatten_and_iterate_in_order() {
    let mut backend = VteBackend::new();
    let mut sink = CaptureSink::default();
    backend.advance(b"\x1b[38;5;200m", &mut sink);
    assert_eq!(sink.csi, vec![38, 5, 200]);
    assert_eq!(Params::new().get(3), 0);
    assert!(Params::new().is_empty());

    let mut sink = CaptureSink::default();
    backend.advance(b"\x1b]0;hello\x07", &mut sink);
    assert_eq!(sink.osc_params, vec![b"0".to_vec(), b"hello".to_vec()]);
    assert_eq!(sink.osc_term, Some(StringTerm::Bel));

    let mut sink = CaptureSink::default();
    backend.advance(b"\x1b^payload\x1b\\", &mut sink);
    assert_eq!(sink.sos_kind, Some(StringKind::Pm));
}
