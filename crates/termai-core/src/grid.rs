//! Grid DTO field set (kernel/03 section 3.3 recommended field set).
//!
//! ADR-0025 D1/D2 added per-row LineFlags: minor 1 -> 2. Minor only adds fields
//! (DC-40), so a minor-1 reader sees a superset it can ignore and a minor-2 reader
//! of a minor-1 document treats the missing `row_flags` as all zero.
#![allow(clippy::module_name_repetitions)]

/// Grid DTO version (major.minor). minor only adds fields.
pub const GRID_DTO_MAJOR: u16 = 0;
pub const GRID_DTO_MINOR: u16 = 2;

/// Per-row `LineFlags` bit 0: this row was hard-wrapped by DECAWM, so it belongs to
/// the same logical line as the row below it (kernel/03 section 3.3 / 3.8).
///
/// ADR-0025 D1: the remaining bits are reserved. Writers MUST write 0 there; readers
/// must tolerate unknown bits (DC-40 N-2 forward compatibility) and never fail on them.
pub const LINE_WRAPPED: u16 = 1 << 0;

/// Color: default / 256-indexed / truecolor.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Color {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

pub const ATTR_BOLD: u16 = 1 << 0;
pub const ATTR_DIM: u16 = 1 << 1;
pub const ATTR_ITALIC: u16 = 1 << 2;
pub const ATTR_UNDERLINE: u16 = 1 << 3;
pub const ATTR_BLINK: u16 = 1 << 4;
pub const ATTR_REVERSE: u16 = 1 << 5;
pub const ATTR_HIDDEN: u16 = 1 << 6;
pub const ATTR_STRIKETHROUGH: u16 = 1 << 7;
pub const ATTR_DOUBLE_UNDERLINE: u16 = 1 << 8;
pub const ATTR_OVERLINE: u16 = 1 << 9;

/// A cell. The right half of a wide char is always WIDE_CONTINUATION (U+0000).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: u16,
    /// Index into GridSnapshot::links, plus 1. 0 = no link (OSC 8).
    pub link: u32,
}

impl Cell {
    pub const WIDE_CONTINUATION: Cell = Cell {
        ch: '\0',
        fg: Color::Default,
        bg: Color::Default,
        attrs: 0,
        link: 0,
    };
    pub const BLANK: Cell = Cell {
        ch: ' ',
        fg: Color::Default,
        bg: Color::Default,
        attrs: 0,
        link: 0,
    };

    #[must_use]
    pub const fn is_wide_continuation(self) -> bool {
        self.ch == '\0'
    }
}

impl Default for Cell {
    fn default() -> Self {
        Cell::BLANK
    }
}

/// Cell position (viewport coordinates, 0-based).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct CellPos {
    pub row: u16,
    pub col: u16,
}

/// Full-screen scroll fast path. delta < 0 = content moves up.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct ScrollOp {
    pub top: u16,
    pub bottom: u16,
    pub delta: i16,
}

/// Damage set: row numbers plus flags only, never row content.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Damage {
    pub rows: Vec<u16>,
    pub scroll: Option<ScrollOp>,
    pub cursor: bool,
    pub selection: bool,
    pub overlay: bool,
    pub full: bool,
}

impl Damage {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
            && self.scroll.is_none()
            && !self.cursor
            && !self.selection
            && !self.overlay
            && !self.full
    }
}

/// Cursor state.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct CursorState {
    pub pos: CellPos,
    pub visible: bool,
    pub style: u8,
}

/// OSC 8 link span (horizontal coords are logical-row based).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LinkSpan {
    pub row: u16,
    pub start_col: u16,
    pub end_col: u16,
    pub id: String,
    pub target: String,
}

/// Full grid snapshot (attach / recovery / headless / plugin read-only mirror).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct GridSnapshot {
    pub cols: u16,
    pub rows: u16,
    /// Row-major; length must equal cols * rows.
    pub cells: Vec<Cell>,
    /// Per-row `LineFlags` (ADR-0025 D1). Row-major; length must equal rows.
    /// A document written before minor 2 has no such field and decodes to all zero.
    pub row_flags: Vec<u16>,
    pub cursor: CursorState,
    pub alt: bool,
    pub wrap_pending: bool,
    pub origin_mode: bool,
    pub modes: u64,
    pub title: String,
    pub links: Vec<LinkSpan>,
    pub scrollback_len: u32,
    pub scroll: (u32, u32),
    pub backend: String,
}

impl GridSnapshot {
    #[must_use]
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols,
            rows,
            cells: vec![Cell::BLANK; usize::from(cols) * usize::from(rows)],
            row_flags: vec![0; usize::from(rows)],
            ..Default::default()
        }
    }

    #[must_use]
    pub fn idx(&self, row: u16, col: u16) -> Option<usize> {
        if row < self.rows && col < self.cols {
            Some(usize::from(row) * usize::from(self.cols) + usize::from(col))
        } else {
            None
        }
    }

    #[must_use]
    pub fn cell(&self, row: u16, col: u16) -> Option<&Cell> {
        self.idx(row, col).and_then(|i| self.cells.get(i))
    }

    /// Logical text of one row: skips wide-char halves, trims trailing blanks.
    #[must_use]
    pub fn row_text(&self, row: u16) -> String {
        let mut s = String::new();
        for col in 0..self.cols {
            if let Some(c) = self.cell(row, col) {
                if !c.is_wide_continuation() {
                    s.push(c.ch);
                }
            }
        }
        s.truncate(s.trim_end().len());
        s
    }

    /// Canonical bytes for golden hashing / grid digest.
    /// Must never contain timestamps, addresses, build hashes or random ids.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.cells.len() * 8 + 128);
        let head = format!(
            "meta cols={} rows={} cursor={},{} cursor_visible={} alt={} wrap={} origin={} modes={:016x} scroll={},{} title_len={} backend={}\n",
            self.cols,
            self.rows,
            self.cursor.pos.row,
            self.cursor.pos.col,
            self.cursor.visible,
            self.alt,
            self.wrap_pending,
            self.origin_mode,
            self.modes,
            self.scroll.0,
            self.scroll.1,
            self.title.chars().count(),
            self.backend,
        );
        out.extend_from_slice(head.as_bytes());
        for row in 0..self.rows {
            for col in 0..self.cols {
                let c = self.cell(row, col).copied().unwrap_or(Cell::BLANK);
                out.extend_from_slice(&(u32::from(c.ch)).to_le_bytes());
                out.extend_from_slice(&c.attrs.to_le_bytes());
                out.extend_from_slice(&encode_color(c.fg).to_le_bytes());
                out.extend_from_slice(&encode_color(c.bg).to_le_bytes());
                out.push(c.link.min(255) as u8);
            }
        }
        // ADR-0025 D2: per-row LineFlags follow the cell block, row-major, u16 LE.
        // Adding this block is a digest-semantics change, which is why GRID_DTO_MINOR
        // moved 1 -> 2. A missing entry (defensive: a snapshot built by hand) reads 0.
        for row in 0..self.rows {
            let flags = self.row_flags.get(usize::from(row)).copied().unwrap_or(0);
            out.extend_from_slice(&flags.to_le_bytes());
        }
        out
    }
}

/// Deterministic 32-bit encoding of a colour: tag in the high byte.
#[must_use]
pub const fn encode_color(c: Color) -> u32 {
    match c {
        Color::Default => 0,
        Color::Indexed(i) => 0x0100_0000 | (i as u32),
        Color::Rgb(r, g, b) => 0x0200_0000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32),
    }
}

/// One row payload for GridDelta.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RowPayload {
    pub row: u16,
    pub cells: Vec<Cell>,
    /// Per-row `LineFlags` for this row (ADR-0025 D1), e.g. `LINE_WRAPPED`.
    pub flags: u16,
}

/// Incremental grid update. rev is monotonic; a gap forces a full resync.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct GridDelta {
    pub rev: u32,
    pub scroll: Option<ScrollOp>,
    pub damage: Damage,
    pub rows: Vec<RowPayload>,
    pub cursor: CursorState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_grid_is_blank_and_sized() {
        let g = GridSnapshot::new(80, 24);
        assert_eq!(g.cells.len(), 80 * 24);
        assert!(g.cells.iter().all(|c| *c == Cell::BLANK));
        assert_eq!(g.idx(23, 79), Some(80 * 24 - 1));
        assert_eq!(g.idx(24, 0), None);
    }

    #[test]
    fn row_text_skips_wide_continuation_and_trims() {
        let mut g = GridSnapshot::new(8, 1);
        g.cells[0].ch = 'X';
        g.cells[1] = Cell::WIDE_CONTINUATION;
        g.cells[2].ch = 'a';
        assert_eq!(g.row_text(0), "Xa");
    }

    #[test]
    fn canonical_bytes_are_deterministic() {
        let a = GridSnapshot::new(4, 2);
        let b = GridSnapshot::new(4, 2);
        assert_eq!(a.canonical_bytes(), b.canonical_bytes());
        let text = String::from_utf8_lossy(&a.canonical_bytes()).to_string();
        assert!(text.starts_with("meta cols=4 rows=2"));
    }

    #[test]
    fn new_grid_initialises_row_flags_to_the_row_count() {
        let g = GridSnapshot::new(80, 24);
        assert_eq!(g.row_flags.len(), 24);
        assert!(g.row_flags.iter().all(|f| *f == 0));
    }

    #[test]
    fn canonical_bytes_include_nonzero_row_flags() {
        // ADR-0025 D2: the per-row flags are part of the digest input, so a
        // wrapped line chain can no longer hash the same as two independent rows.
        let all_zero = GridSnapshot::new(4, 2);
        let mut wrapped = GridSnapshot::new(4, 2);
        wrapped.row_flags[0] = LINE_WRAPPED;
        assert_eq!(LINE_WRAPPED, 1);
        assert_ne!(
            all_zero.canonical_bytes(),
            wrapped.canonical_bytes(),
            "non-zero row_flags must change canonical bytes"
        );
        let mut same = GridSnapshot::new(4, 2);
        same.row_flags[0] = LINE_WRAPPED;
        assert_eq!(wrapped.canonical_bytes(), same.canonical_bytes());
    }

    #[test]
    fn damage_empty_semantics() {
        let mut d = Damage::default();
        assert!(d.is_empty());
        d.cursor = true;
        assert!(!d.is_empty());
    }
}
