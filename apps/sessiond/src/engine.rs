//! VT-backed terminal engine for sessiond.
//!
//! This is the one place where the daemon couples to the VT implementation; the
//! registry only knows the TerminalEngine trait, which is why the whole protocol is
//! testable with an engine double and no PTY.

use termai_core::grid::GridSnapshot;
use termai_vt::Terminal;

use crate::registry::TerminalEngine;

/// A terminal engine backed by termai-vt.
pub struct VtEngine {
    term: Terminal,
}

impl VtEngine {
    #[must_use]
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            term: Terminal::new(cols, rows),
        }
    }

    #[must_use]
    pub fn terminal(&self) -> &Terminal {
        &self.term
    }

    pub fn terminal_mut(&mut self) -> &mut Terminal {
        &mut self.term
    }
}

impl TerminalEngine for VtEngine {
    fn feed(&mut self, bytes: &[u8]) {
        self.term.feed(bytes);
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        self.term.resize(cols, rows);
    }

    fn snapshot(&self) -> GridSnapshot {
        self.term.snapshot()
    }

    fn digest(&self) -> [u8; 32] {
        *blake3::hash(&self.term.snapshot().canonical_bytes()).as_bytes()
    }
}

impl std::fmt::Debug for VtEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VtEngine")
            .field("backend", &self.term.backend_label())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feeding_vt_bytes_produces_a_snapshot_with_text() {
        let mut e = VtEngine::new(20, 3);
        e.feed(b"hello\r\nworld");
        let s = e.snapshot();
        assert_eq!(s.row_text(0), "hello");
        assert_eq!(s.row_text(1), "world");
        assert_eq!(e.digest().len(), 32);
    }

    #[test]
    fn digest_changes_when_the_grid_changes() {
        let mut e = VtEngine::new(10, 2);
        let before = e.digest();
        e.feed(b"x");
        assert_ne!(before, e.digest());
    }

    #[test]
    fn resize_is_reflected_in_the_snapshot() {
        let mut e = VtEngine::new(10, 2);
        e.resize(40, 5);
        let s = e.snapshot();
        assert_eq!((s.cols, s.rows), (40, 5));
        assert_eq!(s.cells.len(), 200);
    }
}
