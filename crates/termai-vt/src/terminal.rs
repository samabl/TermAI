//! Terminal: ties a live Grid to a VtBackend and implements EscapeSink.

use termai_core::grid::GridDelta;
use termai_core::grid::GridSnapshot;

use crate::backend::{AdvanceReport, EscapeSink, Params, StringKind, StringTerm, VtBackend};
use crate::counters::VtCounters;
use crate::grid::{parse_file_uri, sanitize_title, Grid};
use crate::shell::ShellIntegration;
use crate::vte_adapter::VteBackend;

/// A live terminal: grid + backend + shell integration + counters.
pub struct Terminal {
    grid: Grid,
    backend: Box<dyn VtBackend>,
    shell: ShellIntegration,
    counters: VtCounters,
}

impl Terminal {
    /// Create a terminal backed by the default vte backend.
    #[must_use]
    pub fn new(cols: u16, rows: u16) -> Self {
        Self::with_backend(cols, rows, Box::new(VteBackend::new()))
    }

    /// Create a terminal with a caller-supplied backend.
    #[must_use]
    pub fn with_backend(cols: u16, rows: u16, backend: Box<dyn VtBackend>) -> Self {
        let mut grid = Grid::new(cols, rows);
        grid.set_backend_label(&backend.id().label());
        Self {
            grid,
            backend,
            shell: ShellIntegration::new(),
            counters: VtCounters::new(),
        }
    }

    /// Feed bytes and accumulate counters.
    pub fn feed(&mut self, bytes: &[u8]) -> AdvanceReport {
        let Terminal {
            grid,
            backend,
            shell,
            counters,
        } = self;
        let mut sink = TerminalSink { grid, shell };
        let report = backend.advance(bytes, &mut sink);
        counters.merge(&report.counters);
        report
    }

    /// Full reset.
    pub fn reset(&mut self) {
        let Terminal {
            grid,
            backend,
            shell,
            counters,
        } = self;
        grid.reset();
        backend.reset();
        *shell = ShellIntegration::new();
        *counters = VtCounters::new();
    }

    /// Resize the grid.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.grid.resize(cols, rows);
    }

    /// Read-only grid access.
    #[must_use]
    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    /// Mutable grid access.
    pub fn grid_mut(&mut self) -> &mut Grid {
        &mut self.grid
    }

    /// Shell integration state.
    #[must_use]
    pub fn shell(&self) -> &ShellIntegration {
        &self.shell
    }

    /// Cumulative counters.
    #[must_use]
    pub fn counters(&self) -> &VtCounters {
        &self.counters
    }

    /// Full snapshot DTO.
    #[must_use]
    pub fn snapshot(&self) -> GridSnapshot {
        self.grid.snapshot()
    }

    /// Damage accumulated since the caller last received rev.
    pub fn take_delta(&mut self, rev: u32) -> Option<GridDelta> {
        self.grid.take_delta(rev)
    }

    /// Drain the terminal responses the application is waiting for (DSR/CPR). A caller
    /// driving a real pty MUST write these back, or a pseudoconsole client blocks forever
    /// on its first cursor position request.
    pub fn take_responses(&mut self) -> Vec<Vec<u8>> {
        self.grid.take_responses()
    }

    /// Backend identity label (for example vte-0.15).
    #[must_use]
    pub fn backend_label(&self) -> String {
        self.backend.id().label()
    }
}

impl EscapeSink for Terminal {
    fn print(&mut self, ch: char) {
        self.grid.print(ch);
    }

    fn execute(&mut self, byte: u8) {
        self.grid.execute(byte);
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        self.grid.esc_dispatch(intermediates, ignore, byte);
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8) {
        self.grid
            .csi_dispatch(params, intermediates, ignore, action);
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], term: StringTerm) {
        route_osc(&mut self.grid, &mut self.shell, params, term);
    }

    fn dcs_hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: u8) {}

    fn dcs_put(&mut self, _byte: u8) {}

    fn dcs_unhook(&mut self, _term: StringTerm) {}

    fn sos_pm_apc(&mut self, _kind: StringKind, _params: &[&[u8]], _term: StringTerm) {}
}

struct TerminalSink<'a> {
    grid: &'a mut Grid,
    shell: &'a mut ShellIntegration,
}

impl EscapeSink for TerminalSink<'_> {
    fn print(&mut self, ch: char) {
        self.grid.print(ch);
    }

    fn execute(&mut self, byte: u8) {
        self.grid.execute(byte);
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        self.grid.esc_dispatch(intermediates, ignore, byte);
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8) {
        self.grid
            .csi_dispatch(params, intermediates, ignore, action);
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], term: StringTerm) {
        route_osc(self.grid, self.shell, params, term);
    }

    fn dcs_hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: u8) {}

    fn dcs_put(&mut self, _byte: u8) {}

    fn dcs_unhook(&mut self, _term: StringTerm) {}

    fn sos_pm_apc(&mut self, _kind: StringKind, _params: &[&[u8]], _term: StringTerm) {}
}

fn route_osc(grid: &mut Grid, shell: &mut ShellIntegration, params: &[&[u8]], term: StringTerm) {
    if !matches!(term, StringTerm::Bel | StringTerm::St) {
        return;
    }
    let num = match params.first().and_then(|raw| parse_u32(raw)) {
        Some(num) => num,
        None => return,
    };
    match num {
        0 | 2 => {
            let title = join_params(params, 1);
            grid.set_title(&sanitize_title(&title));
        }
        7 => {
            let payload = join_params(params, 1);
            if let Some((path, remote)) = parse_file_uri(&payload) {
                grid.set_cwd(&path);
                grid.set_cwd_remote(remote);
            }
            shell.on_osc(7, params);
        }
        8 => grid.osc8(params),
        9 => {
            if first_byte(params, 1) == Some(b'4') {
                grid.record_progress();
            } else {
                grid.record_notification();
            }
        }
        52 => {
            // AR-29 item 5: read = deny, write = guarded. Only metadata is kept; the
            // verdict is routed to the clipboard broker (never executed here).
            let _decision = grid.osc52(params);
        }
        133 | 633 => {
            shell.on_osc(num, params);
            if num == 633 {
                if let Some(cwd) = cwd_from_633(params) {
                    grid.set_cwd(&cwd);
                }
            }
        }
        777 => grid.record_notification(),
        _ => {}
    }
}

fn cwd_from_633(params: &[&[u8]]) -> Option<String> {
    if first_byte(params, 1) != Some(b'P') {
        return None;
    }
    let text = join_params(params, 2);
    text.strip_prefix("Cwd=").map(str::to_string)
}

fn first_byte(params: &[&[u8]], index: usize) -> Option<u8> {
    params.get(index).and_then(|raw| raw.first()).copied()
}

fn join_params(params: &[&[u8]], start: usize) -> String {
    if start >= params.len() {
        return String::new();
    }
    let mut out = String::new();
    for (i, raw) in params[start..].iter().enumerate() {
        if i > 0 {
            out.push(';');
        }
        out.push_str(&String::from_utf8_lossy(raw));
    }
    out
}

fn parse_u32(raw: &[u8]) -> Option<u32> {
    std::str::from_utf8(raw).ok()?.parse::<u32>().ok()
}
