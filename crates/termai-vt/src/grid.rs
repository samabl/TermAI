//! Live terminal grid: where VT semantics land (kernel/01, K-08).
//!
//! The grid owns the cell matrix, cursor, SGR state, modes, scroll region, the main
//! and alternate screens, OSC 8 link spans and the damage/rev bookkeeping used for
//! GridDelta production. It deliberately owns no width table: every column decision goes
//! through `crate::width` (kernel/03 K-04), the same public API `termai-render` reads, so no
//! second width authority can drift in.
//!
//! Canonicalization note: Grid::digest hashes termai-core
//! GridSnapshot::canonical_bytes, while golden::golden_hash hashes the TERMAI-GRID
//! text body. These are intentionally two different canonical forms.

use std::collections::BTreeMap;

use termai_core::grid::{
    Cell, CellPos, Color, CursorState, Damage, GridDelta, GridSnapshot, LinkSpan, RowPayload,
    ScrollOp, ATTR_BLINK, ATTR_BOLD, ATTR_DIM, ATTR_DOUBLE_UNDERLINE, ATTR_HIDDEN, ATTR_ITALIC,
    ATTR_OVERLINE, ATTR_REVERSE, ATTR_STRIKETHROUGH, ATTR_UNDERLINE, LINE_WRAPPED,
};

use crate::backend::Params;
// kernel/03 K-04: the grid owns no width table of its own - it asks the one authority
// (crate::width) so `termai-render` can read the identical rule.
use crate::width::measure_scalar;

/// DEC private mode 25: cursor visible.
pub const MODE_CURSOR_VISIBLE: u64 = 1 << 0;
/// DEC private modes 47 / 1047 / 1049: alternate screen.
pub const MODE_ALT_SCREEN: u64 = 1 << 1;
/// DEC private mode 6: origin mode.
pub const MODE_ORIGIN: u64 = 1 << 2;
/// DEC private mode 7: autowrap.
pub const MODE_AUTOWRAP: u64 = 1 << 3;
/// DEC private mode 2004: bracketed paste.
pub const MODE_BRACKETED_PASTE: u64 = 1 << 4;
/// DEC private mode 1: application cursor keys.
pub const MODE_APP_CURSOR: u64 = 1 << 5;
/// DEC private mode 4: insert mode.
pub const MODE_INSERT: u64 = 1 << 6;
/// ESC = / ESC >: application keypad.
pub const MODE_APP_KEYPAD: u64 = 1 << 7;
/// ANSI mode 20: linefeed/newline mode (LNM). When set, LF, VT and FF also do a CR.
pub const MODE_LINEFEED: u64 = 1 << 8;
/// DEC private mode 45: reverse wraparound. xterm's `CursorBack` also requires DECAWM.
pub const MODE_REVERSE_WRAP: u64 = 1 << 9;
/// DEC private mode 1045: extended reverse wraparound (xterm since patch 383). Also
/// requires DECAWM; at this level both modes move the cursor the same way.
pub const MODE_REVERSE_WRAP2: u64 = 1 << 10;

const FLAG_NAMES: [(u16, &str); 10] = [
    (ATTR_BOLD, "bold"),
    (ATTR_DIM, "dim"),
    (ATTR_ITALIC, "italic"),
    (ATTR_UNDERLINE, "underline"),
    (ATTR_BLINK, "blink"),
    (ATTR_REVERSE, "reverse"),
    (ATTR_HIDDEN, "hidden"),
    (ATTR_STRIKETHROUGH, "strikethrough"),
    (ATTR_DOUBLE_UNDERLINE, "double_underline"),
    (ATTR_OVERLINE, "overline"),
];

/// Current SGR state.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct Sgr {
    fg: Color,
    bg: Color,
    attrs: u16,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct SavedCursor {
    row: u16,
    col: u16,
    sgr: Sgr,
    origin: bool,
}

/// Name of a colour for the golden format.
#[must_use]
pub fn color_name(color: Color) -> String {
    match color {
        Color::Default => "default".to_string(),
        Color::Indexed(i) => format!("idx{i}"),
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
    }
}

/// Parse a colour from the golden format.
#[must_use]
pub fn parse_color(text: &str) -> Option<Color> {
    if text == "default" {
        return Some(Color::Default);
    }
    if let Some(rest) = text.strip_prefix("idx") {
        return rest.parse::<u8>().ok().map(Color::Indexed);
    }
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(hex.get(0..2)?, 16).ok()?;
    let g = u8::from_str_radix(hex.get(2..4)?, 16).ok()?;
    let b = u8::from_str_radix(hex.get(4..6)?, 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

/// Flag names for an attribute word, deterministic order.
#[must_use]
pub fn flag_names(attrs: u16) -> String {
    let mut out: Vec<&str> = Vec::new();
    for (bit, name) in FLAG_NAMES {
        if attrs & bit != 0 {
            out.push(name);
        }
    }
    out.join(",")
}

/// Parse a comma separated flag list.
#[must_use]
pub fn parse_flags(text: &str) -> Option<u16> {
    let mut attrs = 0u16;
    if text.is_empty() {
        return Some(0);
    }
    for part in text.split(',') {
        let mut found = false;
        for (bit, name) in FLAG_NAMES {
            if name == part {
                attrs |= bit;
                found = true;
                break;
            }
        }
        if !found {
            return None;
        }
    }
    Some(attrs)
}

/// Direction of an OSC 52 clipboard request.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClipboardDirection {
    /// Query: read the clipboard (always denied by the kernel).
    Read,
    /// Set: write the clipboard (guarded).
    Write,
}

/// OSC 52 guard decision. The kernel has no clipboard access, so this decision is
/// what gets routed to the clipboard broker (AR-29 item 5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClipboardDecision {
    /// Read/query requests are denied by default and can never be silently enabled.
    DeniedRead,
    /// Single-line, control-free write: allowed without an extra prompt.
    GuardedAllowSingleLine,
    /// Multi-line write: requires explicit confirmation, never a silent multi-line.
    NeedsConfirmMultiLine,
    /// Write containing control characters: requires explicit confirmation.
    NeedsConfirmControlChars,
}

/// Metadata-only verdict for one OSC 52 request. Deliberately holds no clipboard
/// content, no summary and no hash (AR-12 / AR-29 item 6).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ClipboardVerdict {
    /// Read or write direction.
    pub direction: ClipboardDirection,
    /// Guarding decision.
    pub decision: ClipboardDecision,
    /// Decoded payload length in bytes (metadata only).
    pub decoded_len: usize,
    /// Number of lines in the decoded payload (metadata only).
    pub line_count: u32,
}

/// The live grid.
pub struct Grid {
    cols: u16,
    rows: u16,
    cells: Vec<Cell>,
    saved_cells: Vec<Cell>,
    /// Per-row `LineFlags` (ADR-0025 D1), in lockstep with `cells`.
    row_flags: Vec<u16>,
    /// Flags belonging to the inactive main/alt buffer, swapped with `saved_cells`.
    saved_row_flags: Vec<u16>,
    alt: bool,
    cursor_row: u16,
    cursor_col: u16,
    cursor_visible: bool,
    cursor_style: u8,
    sgr: Sgr,
    saved: SavedCursor,
    alt_saved: SavedCursor,
    /// DECSC's saved cursor for the alternate screen; separate from `alt_saved`, which belongs
    /// to `set_alt` (mode 1049's cursor across the switch).
    alt_decsc: SavedCursor,
    modes: u64,
    /// One saved on/off slot per DEC private mode, as `CSI ? Pm s` / `CSI ? Pm r` (xterm's
    /// `save_modes`). Sparse: a mode is present only after it has been saved at least once.
    saved_private_modes: BTreeMap<u16, bool>,
    wrap_pending: bool,
    /// Last graphic character printed, which REP (CSI Ps b) repeats.
    last_graphic: char,
    scroll_top: u16,
    scroll_bottom: u16,
    tab_stops: Vec<bool>,
    charset_g1: bool,
    title: String,
    /// Icon (tab) title. xterm keeps this separate from the window title because OSC 1
    /// and OSC 2 address them independently; OSC 0 sets both.
    icon_title: String,
    cwd: Option<String>,
    cwd_remote: bool,
    links: Vec<LinkSpan>,
    active_link: Option<usize>,
    scrollback_len: u32,
    /// Combining marks keyed by cell index. GridSnapshot/golden/digest cannot carry
    /// them because termai-core Cell.ch is a single char (docs/plan/m0-spec-defects.md SD-08.2).
    combining: BTreeMap<u32, String>,
    backend_label: String,
    damage: Damage,
    rev: u32,
    delivered_rev: u32,
    clipboard_read_requests: u64,
    clipboard_write_requests: u64,
    clipboard_denied_reads: u64,
    clipboard_allowed_single_line: u64,
    clipboard_confirm_multi_line: u64,
    clipboard_confirm_control_chars: u64,
    last_clipboard: Option<ClipboardVerdict>,
    notification_count: u64,
    progress_count: u64,
    rejected_links: u64,
    /// Responses the terminal owes the application (DSR/CPR). Without these a
    /// pseudoconsole client blocks forever waiting for its cursor report.
    responses: Vec<Vec<u8>>,
}

impl Grid {
    /// Create a blank grid.
    #[must_use]
    pub fn new(cols: u16, rows: u16) -> Self {
        let size = usize::from(cols) * usize::from(rows);
        let mut grid = Self {
            cols,
            rows,
            cells: vec![Cell::BLANK; size],
            saved_cells: vec![Cell::BLANK; size],
            row_flags: vec![0; usize::from(rows)],
            saved_row_flags: vec![0; usize::from(rows)],
            alt: false,
            cursor_row: 0,
            cursor_col: 0,
            cursor_visible: true,
            cursor_style: 0,
            sgr: Sgr::default(),
            saved: SavedCursor::default(),
            alt_saved: SavedCursor::default(),
            alt_decsc: SavedCursor::default(),
            modes: MODE_AUTOWRAP | MODE_CURSOR_VISIBLE,
            saved_private_modes: BTreeMap::new(),
            wrap_pending: false,
            last_graphic: ' ',
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            tab_stops: default_tab_stops(cols),
            charset_g1: false,
            title: String::new(),
            icon_title: String::new(),
            cwd: None,
            cwd_remote: false,
            links: Vec::new(),
            active_link: None,
            scrollback_len: 0,
            combining: BTreeMap::new(),
            backend_label: String::new(),
            damage: Damage::default(),
            rev: 0,
            delivered_rev: 0,
            clipboard_read_requests: 0,
            clipboard_write_requests: 0,
            clipboard_denied_reads: 0,
            clipboard_allowed_single_line: 0,
            clipboard_confirm_multi_line: 0,
            clipboard_confirm_control_chars: 0,
            last_clipboard: None,
            notification_count: 0,
            progress_count: 0,
            rejected_links: 0,
            responses: Vec::new(),
        };
        grid.mark_full();
        grid
    }

    /// Set the backend label used in snapshots.
    pub fn set_backend_label(&mut self, label: &str) {
        self.backend_label = label.to_string();
    }

    /// Grid width in columns.
    #[must_use]
    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// Grid height in rows.
    #[must_use]
    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// Cursor position (row, col).
    #[must_use]
    pub fn cursor(&self) -> (u16, u16) {
        (self.cursor_row, self.cursor_col)
    }

    /// Cursor visibility.
    #[must_use]
    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    /// Current mode bitmap.
    #[must_use]
    pub fn modes(&self) -> u64 {
        self.modes
    }

    /// Whether the next printable char starts a new line.
    #[must_use]
    pub fn wrap_pending(&self) -> bool {
        self.wrap_pending
    }

    /// Scroll region as (top, bottom), inclusive.
    #[must_use]
    pub fn scroll_region(&self) -> (u16, u16) {
        (self.scroll_top, self.scroll_bottom)
    }

    /// Cell accessor.
    #[must_use]
    pub fn cell(&self, row: u16, col: u16) -> Option<&Cell> {
        self.cell_index(row, col).and_then(|i| self.cells.get(i))
    }

    /// Logical text of a row (wide halves skipped, trailing blanks trimmed).
    #[must_use]
    pub fn row_text(&self, row: u16) -> String {
        if row >= self.rows {
            return String::new();
        }
        let base = usize::from(row) * usize::from(self.cols);
        let mut text = String::new();
        for col in 0..self.cols {
            let index = base + usize::from(col);
            if let Some(cell) = self.cells.get(index) {
                if !cell.is_wide_continuation() {
                    text.push(cell.ch);
                    if let Some(extra) = self.combining.get(&(index as u32)) {
                        text.push_str(extra);
                    }
                }
            }
        }
        let trimmed = text.trim_end().len();
        text.truncate(trimmed);
        text
    }

    /// Set the window title (already stripped by the caller).
    pub fn set_title(&mut self, title: &str) {
        self.title = title.to_string();
    }

    /// Current window title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Set the icon (tab) title (already stripped by the caller). Kept in-crate: the
    /// snapshot DTO carries only the window title, and no external caller needs it.
    pub(crate) fn set_icon_title(&mut self, title: &str) {
        self.icon_title = title.to_string();
    }

    /// Set the working directory reported by OSC 7 / OSC 633.
    pub fn set_cwd(&mut self, cwd: &str) {
        self.cwd = Some(cwd.to_string());
    }

    /// Current working directory.
    #[must_use]
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    /// Mark whether the cwd belongs to a remote host.
    pub fn set_cwd_remote(&mut self, remote: bool) {
        self.cwd_remote = remote;
    }

    /// Whether the cwd belongs to a remote host.
    #[must_use]
    pub fn cwd_remote(&self) -> bool {
        self.cwd_remote
    }

    /// Number of lines pushed into scrollback.
    #[must_use]
    pub fn scrollback_len(&self) -> u32 {
        self.scrollback_len
    }

    /// Total OSC 52 requests (reads + writes).
    #[must_use]
    pub fn clipboard_requests(&self) -> u64 {
        self.clipboard_read_requests
            .saturating_add(self.clipboard_write_requests)
    }

    /// OSC 52 reads denied. Kept as a stable alias of clipboard_denied_reads().
    #[must_use]
    pub fn clipboard_denied(&self) -> u64 {
        self.clipboard_denied_reads
    }

    /// OSC 52 read/query requests.
    #[must_use]
    pub fn clipboard_read_requests(&self) -> u64 {
        self.clipboard_read_requests
    }

    /// OSC 52 write/set requests.
    #[must_use]
    pub fn clipboard_write_requests(&self) -> u64 {
        self.clipboard_write_requests
    }

    /// OSC 52 reads denied.
    #[must_use]
    pub fn clipboard_denied_reads(&self) -> u64 {
        self.clipboard_denied_reads
    }

    /// Single-line, control-free writes allowed without confirmation.
    #[must_use]
    pub fn clipboard_allowed_single_line(&self) -> u64 {
        self.clipboard_allowed_single_line
    }

    /// Multi-line writes requiring confirmation (never silently allowed).
    #[must_use]
    pub fn clipboard_needs_confirm_multi_line(&self) -> u64 {
        self.clipboard_confirm_multi_line
    }

    /// Writes containing control characters requiring confirmation.
    #[must_use]
    pub fn clipboard_needs_confirm_control_chars(&self) -> u64 {
        self.clipboard_confirm_control_chars
    }

    /// Verdict for the most recent OSC 52 request (metadata only).
    #[must_use]
    pub fn last_clipboard(&self) -> Option<ClipboardVerdict> {
        self.last_clipboard
    }

    /// Number of OSC 9 / 777 notifications recorded.
    #[must_use]
    pub fn notification_count(&self) -> u64 {
        self.notification_count
    }

    /// Number of OSC 9;4 ConEmu progress updates recorded.
    #[must_use]
    pub fn progress_count(&self) -> u64 {
        self.progress_count
    }

    /// Number of OSC 8 links rejected by the scheme whitelist.
    #[must_use]
    /// Drain the responses the terminal owes the application (DSR/CPR today). The caller
    /// writes them back to the pty; they are terminal-generated, not user input.
    pub fn take_responses(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.responses)
    }

    /// DSR (CSI n / CSI ? n). A pseudoconsole client issues ESC[6n during startup and then
    /// blocks until the terminal answers with a cursor position report, so leaving this a
    /// no-op deadlocks the client before it produces any output.
    fn device_status(&mut self, params: &Params, private: bool, ignore: bool) {
        if ignore || private {
            return;
        }
        match params.get(0) {
            // Status report: ready, no malfunction.
            0 | 5 => self.responses.push(b"\x1b[0n".to_vec()),
            6 => {
                let row = self.cursor_row.saturating_add(1);
                let col = self.cursor_col.saturating_add(1);
                self.responses
                    .push(format!("\x1b[{row};{col}R").into_bytes());
            }
            _ => {}
        }
    }

    /// DA1 (`CSI c` / `CSI 0 c`) and DA2 (`CSI > c` / `CSI > 0 c`) device attributes
    /// (ADR-0030 D-2).
    ///
    /// DA1 reports `?1;2` only: VT100 plus the Advanced Video Option, which is exactly the
    /// capability set this layer implements at the declared VT level 1. It deliberately does
    /// not advertise selective erase, locator, colour, or the other options a higher xterm
    /// DA1 would claim. DA2's `314` is TermAI's own self-reported version, chosen as the
    /// lower bound of the range esctest accepts (314..=999) so that it is never mistaken for
    /// an xterm version number.
    fn device_attributes(&mut self, secondary: bool) {
        if secondary {
            self.responses.push(b"\x1b[>0;314;0c".to_vec());
        } else {
            self.responses.push(b"\x1b[?1;2c".to_vec());
        }
    }

    /// XTWINOPS (`CSI Ps t`) window/text-area report subset (SD-19).
    ///
    /// Only the character-cell reports are answered. `14 t` / `15 t` / `16 t` are
    /// deliberately left unanswered: they report **pixel** dimensions, and this layer
    /// holds no font metrics — AR-14 keeps pixels out of the VT layer, so fabricating
    /// an answer here would be a lie. The esctest adapter still synthesises those three
    /// from its fixed window model and records each one as a substitution.
    fn window_op(&mut self, params: &Params) {
        match params.get(0) {
            // 11 t: window state. 1 = normal (never iconified or minimised).
            11 => self.responses.push(b"\x1b[1t".to_vec()),
            // 13 t: window position in pixels. There is no window, so report 0;0.
            13 => self.responses.push(b"\x1b[3;0;0t".to_vec()),
            // 18 t: text-area size in characters -> CSI 8 ; rows ; cols t.
            18 => {
                let rows = self.rows;
                let cols = self.cols;
                self.responses
                    .push(format!("\x1b[8;{rows};{cols}t").into_bytes());
            }
            // 19 t: screen size in characters -> CSI 9 ; rows ; cols t.
            19 => {
                let rows = self.rows;
                let cols = self.cols;
                self.responses
                    .push(format!("\x1b[9;{rows};{cols}t").into_bytes());
            }
            // 20 t / 21 t: report the icon label / window title as OSC L / OSC l. Both are
            // pure state reports, like the character-cell reports above; xterm answers them
            // from the title it already holds.
            20 => self.responses.push(osc_report(b'L', &self.icon_title)),
            21 => self.responses.push(osc_report(b'l', &self.title)),
            _ => {}
        }
    }

    pub fn rejected_links(&self) -> u64 {
        self.rejected_links
    }

    /// Active OSC 8 link spans.
    #[must_use]
    pub fn links(&self) -> &[LinkSpan] {
        &self.links
    }

    /// blake3 digest of the canonical snapshot bytes.
    #[must_use]
    pub fn digest(&self) -> [u8; 32] {
        let bytes = self.snapshot().canonical_bytes();
        *blake3::hash(&bytes).as_bytes()
    }

    /// Build the full snapshot DTO.
    #[must_use]
    pub fn snapshot(&self) -> GridSnapshot {
        GridSnapshot {
            cols: self.cols,
            rows: self.rows,
            cells: self.cells.clone(),
            row_flags: self.row_flags.clone(),
            cursor: self.cursor_state(),
            alt: self.alt,
            wrap_pending: self.wrap_pending,
            origin_mode: self.origin(),
            modes: self.modes,
            title: self.title.clone(),
            links: self.links.clone(),
            scrollback_len: self.scrollback_len,
            scroll: (u32::from(self.scroll_top), u32::from(self.scroll_bottom)),
            backend: self.backend_label.clone(),
        }
    }

    fn cursor_state(&self) -> CursorState {
        CursorState {
            pos: CellPos {
                row: self.cursor_row,
                col: self.cursor_col,
            },
            visible: self.cursor_visible,
            style: self.cursor_style,
        }
    }

    fn origin(&self) -> bool {
        self.modes & MODE_ORIGIN != 0
    }

    fn autowrap(&self) -> bool {
        self.modes & MODE_AUTOWRAP != 0
    }

    fn linefeed_newline(&self) -> bool {
        self.modes & MODE_LINEFEED != 0
    }

    fn insert_mode(&self) -> bool {
        self.modes & MODE_INSERT != 0
    }

    fn cell_index(&self, row: u16, col: u16) -> Option<usize> {
        if row < self.rows && col < self.cols {
            Some(usize::from(row) * usize::from(self.cols) + usize::from(col))
        } else {
            None
        }
    }

    fn mark_row(&mut self, row: u16) {
        self.rev = self.rev.wrapping_add(1);
        if !self.damage.rows.contains(&row) {
            self.damage.rows.push(row);
        }
    }

    fn mark_cursor(&mut self) {
        self.rev = self.rev.wrapping_add(1);
        self.damage.cursor = true;
    }

    fn mark_full(&mut self) {
        self.rev = self.rev.wrapping_add(1);
        self.damage.full = true;
    }

    /// Set `LINE_WRAPPED` on the row that DECAWM just left behind (ADR-0025 D1).
    ///
    /// A flag-only change still has to damage the row: otherwise the wrap link never
    /// reaches a mirror that is following GridDelta (only a full resync would carry it).
    fn mark_line_wrapped(&mut self, row: u16) {
        let changed = match self.row_flags.get_mut(usize::from(row)) {
            Some(flags) if *flags & LINE_WRAPPED == 0 => {
                *flags |= LINE_WRAPPED;
                true
            }
            _ => false,
        };
        if changed {
            self.mark_row(row);
        }
    }

    /// Clear a row's LineFlags: it is no longer a wrapped continuation.
    fn clear_line_flags(&mut self, row: u16) {
        let changed = match self.row_flags.get_mut(usize::from(row)) {
            Some(flags) if *flags != 0 => {
                *flags = 0;
                true
            }
            _ => false,
        };
        if changed {
            self.mark_row(row);
        }
    }

    fn clear_all_line_flags(&mut self) {
        for flags in &mut self.row_flags {
            *flags = 0;
        }
    }

    /// Move row flags in lockstep with the cell rows inside one row-shifting
    /// primitive (scroll / insert_lines / delete_lines). `down` moves content
    /// towards higher row numbers; the rows the move exposes are cleared.
    ///
    /// This lives next to the cell move on purpose: ADR-0025 section 5 negative
    /// item 2 warns that any shifting path missing this call degrades silently to
    /// "all-zero or misaligned flags".
    fn rotate_row_flags(&mut self, top: u16, bottom: u16, n: u16, down: bool) {
        let top = usize::from(top);
        let bottom = usize::from(bottom);
        let n = usize::from(n);
        if n == 0 || top > bottom || bottom >= self.row_flags.len() {
            return;
        }
        let region = bottom - top + 1;
        if n >= region {
            for flags in &mut self.row_flags[top..=bottom] {
                *flags = 0;
            }
            return;
        }
        if down {
            self.row_flags.copy_within(top..=bottom - n, top + n);
            for flags in &mut self.row_flags[top..top + n] {
                *flags = 0;
            }
        } else {
            self.row_flags.copy_within(top + n..=bottom, top);
            for flags in &mut self.row_flags[bottom - n + 1..=bottom] {
                *flags = 0;
            }
        }
    }

    fn set_bit(&mut self, bit: u64, on: bool) {
        if on {
            self.modes |= bit;
        } else {
            self.modes &= !bit;
        }
    }

    /// Drain the damage accumulated since the previous call.
    pub fn take_delta(&mut self, rev: u32) -> Option<GridDelta> {
        if self.damage.is_empty() {
            return None;
        }
        let full = self.damage.full || rev != self.delivered_rev;
        let mut delta = GridDelta {
            rev: self.rev,
            scroll: self.damage.scroll,
            damage: self.damage.clone(),
            rows: Vec::new(),
            cursor: self.cursor_state(),
        };
        if full {
            delta.damage.full = true;
            for row in 0..self.rows {
                delta.rows.push(self.row_payload(row));
            }
        } else {
            let mut rows = self.damage.rows.clone();
            rows.sort_unstable();
            rows.dedup();
            delta.damage.rows = rows.clone();
            for row in rows {
                delta.rows.push(self.row_payload(row));
            }
        }
        self.damage = Damage::default();
        self.delivered_rev = self.rev;
        Some(delta)
    }

    fn row_payload(&self, row: u16) -> RowPayload {
        let base = usize::from(row) * usize::from(self.cols);
        let end = base + usize::from(self.cols);
        RowPayload {
            row,
            cells: self
                .cells
                .get(base..end)
                .map(<[Cell]>::to_vec)
                .unwrap_or_default(),
            flags: self.row_flags.get(usize::from(row)).copied().unwrap_or(0),
        }
    }

    /// Resize the grid, preserving the overlapping top-left region.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        // A resize to the same dimensions is a no-op. It must not disturb the scrolling region,
        // saved cursor or alt buffer; the conformance adapter issues RESIZE after every feed.
        if cols == self.cols && rows == self.rows {
            return;
        }
        let old_cols = self.cols;
        let old_rows = self.rows;
        let mut next = vec![Cell::BLANK; usize::from(cols) * usize::from(rows)];
        for row in 0..old_rows.min(rows) {
            for col in 0..old_cols.min(cols) {
                let src = usize::from(row) * usize::from(old_cols) + usize::from(col);
                let dst = usize::from(row) * usize::from(cols) + usize::from(col);
                if let (Some(cell), Some(slot)) = (self.cells.get(src).copied(), next.get_mut(dst))
                {
                    *slot = cell;
                }
            }
        }
        // Flags follow the rows that survive the resize; new rows start zeroed.
        let mut next_flags = vec![0u16; usize::from(rows)];
        for row in 0..old_rows.min(rows) {
            if let (Some(flag), Some(slot)) = (
                self.row_flags.get(usize::from(row)).copied(),
                next_flags.get_mut(usize::from(row)),
            ) {
                *slot = flag;
            }
        }
        self.cells = next;
        self.row_flags = next_flags;
        self.saved_cells = vec![Cell::BLANK; usize::from(cols) * usize::from(rows)];
        self.saved_row_flags = vec![0; usize::from(rows)];
        self.cols = cols;
        self.rows = rows;
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        self.tab_stops = default_tab_stops(cols);
        self.combining.clear();
        self.mark_full();
    }

    /// Soft reset (DECSTR / CSI ! p): state only, never the screen contents.
    ///
    /// xterm's DECSTR returns the scrolling region to the full screen, homes the cursor and
    /// clears saved state. It deliberately does NOT erase the screen - which is why it cannot
    /// reuse `reset()` below, since that rebuilds the grid via `Grid::new`.
    fn soft_reset(&mut self) {
        self.scroll_top = 0;
        self.scroll_bottom = self.rows.saturating_sub(1);
        // DECSTR resets the SAVED position to home but does not move the cursor itself (esctest's
        // test_SaveRestoreCursor_Reset writes after DECSTR and expects that write to land where the
        // cursor already was).
        self.wrap_pending = false;
        // State that xterm's DECSTR also returns to defaults and that would otherwise leak from one
        // esctest case into the next: character attributes, the saved cursor, the character set,
        // and the modes DECSTR resets (origin mode and insert mode off, autowrap on, cursor shown).
        self.sgr = Sgr::default();
        self.saved = SavedCursor::default();
        self.alt_saved = SavedCursor::default();
        self.alt_decsc = SavedCursor::default();
        self.charset_g1 = false;
        self.last_graphic = ' ';
        self.cursor_visible = true;
        self.modes = MODE_AUTOWRAP | MODE_CURSOR_VISIBLE;
        self.mark_full();
    }

    /// Full reset (RIS / ESC c).
    pub fn reset(&mut self) {
        let cols = self.cols;
        let rows = self.rows;
        let label = self.backend_label.clone();
        *self = Grid::new(cols, rows);
        self.backend_label = label;
        self.mark_full();
    }

    fn put(&mut self, row: u16, col: u16, cell: Cell) {
        if let Some(index) = self.cell_index(row, col) {
            if let Some(slot) = self.cells.get_mut(index) {
                *slot = cell;
                self.mark_row(row);
            }
        }
    }

    fn styled(&self, ch: char) -> Cell {
        Cell {
            ch,
            fg: self.sgr.fg,
            bg: self.sgr.bg,
            attrs: self.sgr.attrs,
            link: self.active_link.map_or(0, |i| (i as u32) + 1),
        }
    }

    fn apply_link(&mut self, row: u16, start: u16, width: u16) {
        let Some(index) = self.active_link else {
            return;
        };
        let link = (index as u32) + 1;
        if let Some(span) = self.links.get_mut(index) {
            if span.row == row {
                let end = start.saturating_add(width);
                if end > span.end_col {
                    span.end_col = end;
                }
            }
        }
        let last = start.saturating_add(width);
        let mut col = start;
        while col < last {
            if let Some(i) = self.cell_index(row, col) {
                if let Some(cell) = self.cells.get_mut(i) {
                    cell.link = link;
                }
            }
            col += 1;
        }
    }

    // -- printing ----------------------------------------------------------

    /// Print one character.
    pub fn print(&mut self, ch: char) {
        let width = measure_scalar(ch);
        if width == 0 {
            self.append_combining(ch);
            return;
        }
        self.last_graphic = ch;
        if self.wrap_pending && self.autowrap() {
            self.line_feed_wrapped();
            self.cursor_col = 0;
        }
        self.wrap_pending = false;
        if self.insert_mode() {
            self.insert_chars(1);
        }
        if width >= 2 && self.cols >= 2 {
            self.print_wide(ch);
        } else {
            self.print_narrow(ch);
        }
    }

    fn print_narrow(&mut self, ch: char) {
        let row = self.cursor_row;
        let col = self.cursor_col;
        let cell = self.styled(ch);
        self.put(row, col, cell);
        self.apply_link(row, col, 1);
        if col + 1 >= self.cols {
            self.cursor_col = self.cols.saturating_sub(1);
            if self.autowrap() {
                self.wrap_pending = true;
            }
        } else {
            self.cursor_col = col + 1;
        }
        self.mark_cursor();
    }

    fn print_wide(&mut self, ch: char) {
        if self.cursor_col + 2 > self.cols {
            if self.autowrap() {
                self.line_feed_wrapped();
                self.cursor_col = 0;
            } else {
                self.cursor_col = self.cols.saturating_sub(2);
            }
        }
        let row = self.cursor_row;
        let col = self.cursor_col;
        let cell = self.styled(ch);
        self.put(row, col, cell);
        if col + 1 < self.cols {
            self.put(row, col + 1, Cell::WIDE_CONTINUATION);
        }
        self.apply_link(row, col, 2);
        if self.cursor_col + 2 >= self.cols {
            self.cursor_col = self.cols.saturating_sub(1);
            if self.autowrap() {
                self.wrap_pending = true;
            }
        } else {
            self.cursor_col += 2;
        }
        self.mark_cursor();
    }

    fn append_combining(&mut self, ch: char) {
        let base_col = if self.wrap_pending {
            Some(self.cursor_col)
        } else if self.cursor_col > 0 {
            Some(self.cursor_col - 1)
        } else {
            None
        };
        let mut col = match base_col {
            Some(col) => col,
            None => return,
        };
        loop {
            let index = match self.cell_index(self.cursor_row, col) {
                Some(index) => index,
                None => return,
            };
            let wide = self
                .cells
                .get(index)
                .map(|cell| cell.is_wide_continuation())
                .unwrap_or(false);
            if !wide {
                let entry = self.combining.entry(index as u32).or_default();
                entry.push(ch);
                self.mark_row(self.cursor_row);
                return;
            }
            if col == 0 {
                return;
            }
            col -= 1;
        }
    }

    // -- C0 and ESC --------------------------------------------------------

    /// Execute a C0/C1 control.
    pub fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => {}
            0x08 => {
                self.cursor_left(1);
            }
            0x09 => {
                self.tab();
            }
            0x0A..=0x0C => {
                // LF/VT/FF. With LNM set (SM 20) the line feed also returns to
                // column 1; without it the column is left where it was.
                self.line_feed_explicit();
                if self.linefeed_newline() {
                    self.cursor_col = 0;
                    self.wrap_pending = false;
                    self.mark_cursor();
                }
            }
            0x0D => {
                self.cursor_col = 0;
                self.wrap_pending = false;
                self.mark_cursor();
            }
            0x0E => {
                self.charset_g1 = true;
            }
            0x0F => {
                self.charset_g1 = false;
            }
            _ => {}
        }
    }

    /// Whether the G1 charset is currently shifted in (SO / SI).
    #[must_use]
    pub fn charset_g1(&self) -> bool {
        self.charset_g1
    }

    /// ESC dispatch.
    pub fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }
        // Intermediates are part of the sequence identity, not decoration: dispatching
        // on the final byte alone turned ESC # 8 (DECALN) into ESC 8 (DECRC) and moved
        // the cursor (G1 finding inv-esc-intermediate-has-no-side-effect).
        if intermediates == [b'#'] {
            if byte == b'8' {
                self.decaln();
            }
            return;
        }
        if !intermediates.is_empty() {
            return;
        }
        match byte {
            b'7' => self.save_cursor(),
            b'8' => self.restore_cursor(),
            b'D' => self.line_feed_explicit(),
            b'E' => {
                self.cursor_col = 0;
                self.line_feed_explicit();
            }
            // HTS (ESC H): set a tab stop at the cursor. Its absence left the escape ignored, so
            // a test that set a custom stop and then tabbed landed on a default one.
            b'H' => self.set_tab_stop(),
            b'M' => self.reverse_index(),
            b'c' => self.reset(),
            b'=' => self.set_bit(MODE_APP_KEYPAD, true),
            b'>' => self.set_bit(MODE_APP_KEYPAD, false),
            _ => {}
        }
    }

    fn tab(&mut self) {
        let mut col = self.cursor_col;
        loop {
            col += 1;
            if col >= self.cols {
                col = self.cols.saturating_sub(1);
                break;
            }
            if self
                .tab_stops
                .get(usize::from(col))
                .copied()
                .unwrap_or(false)
            {
                break;
            }
        }
        self.cursor_col = col;
        self.wrap_pending = false;
        self.mark_cursor();
    }

    /// CBT stepping: move to the previous tab stop, stopping at column 1.
    fn tab_back(&mut self) {
        let mut col = self.cursor_col;
        while col > 0 {
            col -= 1;
            if self
                .tab_stops
                .get(usize::from(col))
                .copied()
                .unwrap_or(false)
            {
                break;
            }
        }
        self.cursor_col = col;
        self.wrap_pending = false;
        self.mark_cursor();
    }

    fn line_feed(&mut self) {
        self.wrap_pending = false;
        if self.cursor_row == self.scroll_bottom {
            self.scroll_up(1);
        } else if self.cursor_row + 1 < self.rows {
            self.cursor_row += 1;
        }
        self.mark_cursor();
    }

    /// LF / IND / NEL: an explicit hard line break. The row we leave is not a
    /// continuation of the row below, so its WRAPPED bit (if any) is cleared
    /// before the row slots move (ADR-0025 D1).
    fn line_feed_explicit(&mut self) {
        self.clear_line_flags(self.cursor_row);
        self.line_feed();
    }

    /// DECAWM autowrap: the row we leave continues on the row below, so mark it
    /// WRAPPED before the row slots move (ADR-0025 D1/D3).
    fn line_feed_wrapped(&mut self) {
        self.mark_line_wrapped(self.cursor_row);
        self.line_feed();
    }

    fn reverse_index(&mut self) {
        self.wrap_pending = false;
        if self.cursor_row == self.scroll_top {
            self.scroll_down(1);
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
        }
        self.mark_cursor();
    }

    // -- scrolling ---------------------------------------------------------

    fn scroll_up(&mut self, n: u16) {
        let top = self.scroll_top;
        let bottom = self.scroll_bottom;
        if top > bottom || bottom >= self.rows || n == 0 {
            return;
        }
        let region = bottom - top + 1;
        let n = n.min(region);
        let cols = usize::from(self.cols);
        let shift = usize::from(n) * cols;
        let start = usize::from(top) * cols;
        let end = (usize::from(bottom) + 1) * cols;
        if n < region {
            if let Some(range) = self.cells.get(start + shift..end) {
                let src: Vec<Cell> = range.to_vec();
                for (offset, cell) in src.into_iter().enumerate() {
                    if let Some(slot) = self.cells.get_mut(start + offset) {
                        *slot = cell;
                    }
                }
            }
            for index in end - shift..end {
                if let Some(slot) = self.cells.get_mut(index) {
                    *slot = Cell::BLANK;
                }
            }
        } else {
            for index in start..end {
                if let Some(slot) = self.cells.get_mut(index) {
                    *slot = Cell::BLANK;
                }
            }
        }
        self.rotate_row_flags(top, bottom, n, false);
        if top == 0 {
            self.scrollback_len = self.scrollback_len.saturating_add(u32::from(n));
        }
        self.damage.scroll = Some(ScrollOp {
            top,
            bottom,
            delta: -(n as i16),
        });
        let mut row = top;
        while row <= bottom {
            self.mark_row(row);
            if row == u16::MAX {
                break;
            }
            row += 1;
        }
    }

    fn scroll_down(&mut self, n: u16) {
        let top = self.scroll_top;
        let bottom = self.scroll_bottom;
        if top > bottom || bottom >= self.rows || n == 0 {
            return;
        }
        let region = bottom - top + 1;
        let n = n.min(region);
        let cols = usize::from(self.cols);
        let shift = usize::from(n) * cols;
        let start = usize::from(top) * cols;
        let end = (usize::from(bottom) + 1) * cols;
        if n < region {
            if let Some(range) = self.cells.get(start..end - shift) {
                let src: Vec<Cell> = range.to_vec();
                for (offset, cell) in src.into_iter().enumerate() {
                    if let Some(slot) = self.cells.get_mut(start + shift + offset) {
                        *slot = cell;
                    }
                }
            }
            for index in start..start + shift {
                if let Some(slot) = self.cells.get_mut(index) {
                    *slot = Cell::BLANK;
                }
            }
        } else {
            for index in start..end {
                if let Some(slot) = self.cells.get_mut(index) {
                    *slot = Cell::BLANK;
                }
            }
        }
        self.rotate_row_flags(top, bottom, n, true);
        self.damage.scroll = Some(ScrollOp {
            top,
            bottom,
            delta: n as i16,
        });
        let mut row = top;
        while row <= bottom {
            self.mark_row(row);
            if row == u16::MAX {
                break;
            }
            row += 1;
        }
    }

    fn insert_lines(&mut self, n: u16) {
        if self.cursor_row < self.scroll_top || self.cursor_row > self.scroll_bottom {
            return;
        }
        let top = self.cursor_row;
        let bottom = self.scroll_bottom;
        let region = bottom - top + 1;
        let n = n.min(region);
        if n == 0 {
            return;
        }
        let cols = usize::from(self.cols);
        let shift = usize::from(n) * cols;
        let start = usize::from(top) * cols;
        let end = (usize::from(bottom) + 1) * cols;
        if n < region {
            if let Some(range) = self.cells.get(start..end - shift) {
                let src: Vec<Cell> = range.to_vec();
                for (offset, cell) in src.into_iter().enumerate() {
                    if let Some(slot) = self.cells.get_mut(start + shift + offset) {
                        *slot = cell;
                    }
                }
            }
            for index in start..start + shift {
                if let Some(slot) = self.cells.get_mut(index) {
                    *slot = Cell::BLANK;
                }
            }
        } else {
            for index in start..end {
                if let Some(slot) = self.cells.get_mut(index) {
                    *slot = Cell::BLANK;
                }
            }
        }
        self.rotate_row_flags(top, bottom, n, true);
        let mut row = top;
        while row <= bottom {
            self.mark_row(row);
            if row == u16::MAX {
                break;
            }
            row += 1;
        }
    }

    fn delete_lines(&mut self, n: u16) {
        if self.cursor_row < self.scroll_top || self.cursor_row > self.scroll_bottom {
            return;
        }
        let top = self.cursor_row;
        let bottom = self.scroll_bottom;
        let region = bottom - top + 1;
        let n = n.min(region);
        if n == 0 {
            return;
        }
        let cols = usize::from(self.cols);
        let shift = usize::from(n) * cols;
        let start = usize::from(top) * cols;
        let end = (usize::from(bottom) + 1) * cols;
        if n < region {
            if let Some(range) = self.cells.get(start + shift..end) {
                let src: Vec<Cell> = range.to_vec();
                for (offset, cell) in src.into_iter().enumerate() {
                    if let Some(slot) = self.cells.get_mut(start + offset) {
                        *slot = cell;
                    }
                }
            }
            for index in end - shift..end {
                if let Some(slot) = self.cells.get_mut(index) {
                    *slot = Cell::BLANK;
                }
            }
        } else {
            for index in start..end {
                if let Some(slot) = self.cells.get_mut(index) {
                    *slot = Cell::BLANK;
                }
            }
        }
        self.rotate_row_flags(top, bottom, n, false);
        let mut row = top;
        while row <= bottom {
            self.mark_row(row);
            if row == u16::MAX {
                break;
            }
            row += 1;
        }
    }

    // -- erase / insert / delete -------------------------------------------

    fn erase_cells(&mut self, row: u16, from: u16, to: u16) {
        let mut col = from;
        while col <= to && col < self.cols {
            self.put(row, col, Cell::BLANK);
            if col == u16::MAX {
                break;
            }
            col += 1;
        }
        // Erasing through the right edge removes the content that a wrap link
        // would have continued on the next row, so the row stops being a wrapped
        // continuation (ADR-0025 D1). Partial erases that keep the edge intact
        // deliberately leave the flag alone.
        if self.cols > 0 && to >= self.cols - 1 {
            self.clear_line_flags(row);
        }
    }

    fn erase_display(&mut self, mode: u16) {
        match mode {
            0 => {
                self.erase_cells(
                    self.cursor_row,
                    self.cursor_col,
                    self.cols.saturating_sub(1),
                );
                let mut row = self.cursor_row + 1;
                while row < self.rows {
                    self.erase_cells(row, 0, self.cols.saturating_sub(1));
                    row += 1;
                }
            }
            1 => {
                self.erase_cells(self.cursor_row, 0, self.cursor_col);
                let mut row = 0;
                while row < self.cursor_row {
                    self.erase_cells(row, 0, self.cols.saturating_sub(1));
                    row += 1;
                }
            }
            2 => {
                let mut row = 0;
                while row < self.rows {
                    self.erase_cells(row, 0, self.cols.saturating_sub(1));
                    row += 1;
                }
            }
            3 => {
                self.scrollback_len = 0;
                self.mark_full();
            }
            _ => {}
        }
    }

    fn erase_line(&mut self, mode: u16) {
        match mode {
            0 => self.erase_cells(
                self.cursor_row,
                self.cursor_col,
                self.cols.saturating_sub(1),
            ),
            1 => self.erase_cells(self.cursor_row, 0, self.cursor_col),
            2 => self.erase_cells(self.cursor_row, 0, self.cols.saturating_sub(1)),
            _ => {}
        }
    }

    fn insert_chars(&mut self, n: u16) {
        let row = self.cursor_row;
        let col = self.cursor_col;
        let avail = self.cols.saturating_sub(col);
        let n = n.min(avail);
        if n == 0 {
            return;
        }
        let cols = usize::from(self.cols);
        let base = usize::from(row) * cols;
        let start = base + usize::from(col);
        let end = base + cols;
        let shift = usize::from(n);
        if let Some(range) = self.cells.get(start..end - shift) {
            let src: Vec<Cell> = range.to_vec();
            for (offset, cell) in src.into_iter().enumerate() {
                if let Some(slot) = self.cells.get_mut(start + shift + offset) {
                    *slot = cell;
                }
            }
        }
        for index in start..start + shift {
            if let Some(slot) = self.cells.get_mut(index) {
                *slot = Cell::BLANK;
            }
        }
        // ICH drops the cells pushed past the right edge, so the wrap link is stale.
        self.clear_line_flags(row);
        self.mark_row(row);
    }

    fn delete_chars(&mut self, n: u16) {
        let row = self.cursor_row;
        let col = self.cursor_col;
        let avail = self.cols.saturating_sub(col);
        let n = n.min(avail);
        if n == 0 {
            return;
        }
        let cols = usize::from(self.cols);
        let base = usize::from(row) * cols;
        let start = base + usize::from(col);
        let end = base + cols;
        let shift = usize::from(n);
        if let Some(range) = self.cells.get(start + shift..end) {
            let src: Vec<Cell> = range.to_vec();
            for (offset, cell) in src.into_iter().enumerate() {
                if let Some(slot) = self.cells.get_mut(start + offset) {
                    *slot = cell;
                }
            }
        }
        for index in end - shift..end {
            if let Some(slot) = self.cells.get_mut(index) {
                *slot = Cell::BLANK;
            }
        }
        // DCH blanks the right edge, so the wrap link is stale.
        self.clear_line_flags(row);
        self.mark_row(row);
    }

    // -- cursor / modes ----------------------------------------------------

    fn cup(&mut self, row: u16, col: u16) {
        let origin = self.origin();
        let row = if origin {
            self.scroll_top.saturating_add(row)
        } else {
            row
        };
        let max_row = if origin {
            self.scroll_bottom
        } else {
            self.rows.saturating_sub(1)
        };
        self.cursor_row = row.min(max_row);
        self.cursor_col = col.min(self.cols.saturating_sub(1));
        self.wrap_pending = false;
        self.mark_cursor();
    }

    fn cursor_up(&mut self, n: u16) {
        let inside = self.cursor_row >= self.scroll_top && self.cursor_row <= self.scroll_bottom;
        let floor = if inside { self.scroll_top } else { 0 };
        let target = self.cursor_row.saturating_sub(n);
        self.cursor_row = target.max(floor);
        self.wrap_pending = false;
        self.mark_cursor();
    }

    fn cursor_down(&mut self, n: u16) {
        let inside = self.cursor_row >= self.scroll_top && self.cursor_row <= self.scroll_bottom;
        let ceil = if inside {
            self.scroll_bottom
        } else {
            self.rows.saturating_sub(1)
        };
        let target = self.cursor_row.saturating_add(n);
        self.cursor_row = target.min(ceil);
        self.wrap_pending = false;
        self.mark_cursor();
    }

    fn cursor_right(&mut self, n: u16) {
        let target = self.cursor_col.saturating_add(n);
        self.cursor_col = target.min(self.cols.saturating_sub(1));
        self.wrap_pending = false;
        self.mark_cursor();
    }

    /// CUB / BS: xterm's CursorBack (cursor.c), ported faithfully rather than approximated.
    ///
    /// Two reverse-wrap modes exist and they are NOT equivalent: private mode 45 crosses
    /// only a boundary whose row carries LINE_WRAPPED and otherwise fails the wrap (the row
    /// is restored and the column lands on the left margin), while mode 1045 wraps
    /// unconditionally and, from the top margin, lands at bottom + 1. Both need DECAWM, and
    /// a pending wrap absorbs one step. Left/right margins (DECLRMM) are not implemented, so
    /// the left margin is always 0.
    fn cursor_left(&mut self, n: u16) {
        let left: i32 = 0;
        let right = i32::from(self.cols.saturating_sub(1));
        let before = i32::from(self.cursor_col);
        let top = i32::from(self.scroll_top);
        let bottom = i32::from(self.scroll_bottom);
        let rev2 = self.autowrap() && self.modes & MODE_REVERSE_WRAP2 != 0;
        let rev = self.autowrap() && self.modes & MODE_REVERSE_WRAP != 0;

        let mut col = before;
        let mut row = i32::from(self.cursor_row);
        let mut count = i32::from(n);
        if count > 0 {
            if (rev || rev2) && self.wrap_pending {
                count -= 1;
            } else {
                col -= 1;
            }
        }

        let mut fetched = false;
        let mut wrapped;
        loop {
            if col < left {
                if rev2 {
                    col = right;
                    if row == top {
                        row = bottom + 1;
                    }
                } else if !rev {
                    col = left;
                    break;
                }
                fetched = false;
                row -= 1;
            }
            if !fetched {
                wrapped = row >= 0
                    && (row as u16) < self.rows
                    && self.row_flags[usize::from(row as u16)] & LINE_WRAPPED != 0;
                fetched = true;
                if row != i32::from(self.cursor_row) {
                    if !rev2 && !wrapped {
                        if row < bottom {
                            row += 1;
                        }
                        col = left;
                        break;
                    }
                    col = right;
                }
            }
            count -= 1;
            if count <= 0 {
                break;
            }
            col -= 1;
        }

        self.cursor_row = row.clamp(0, i32::from(self.rows.saturating_sub(1))) as u16;
        self.cursor_col = col.clamp(left, right) as u16;
        self.wrap_pending = false;
        self.mark_cursor();
    }

    fn set_scroll_region(&mut self, params: &Params) {
        let top = params.get(0);
        let bottom = params.get(1);
        let top = if top == 0 { 1 } else { top };
        let bottom = if bottom == 0 { self.rows } else { bottom };
        let top0 = top.saturating_sub(1).min(self.rows.saturating_sub(1));
        let bottom0 = bottom.saturating_sub(1).min(self.rows.saturating_sub(1));
        if top0 < bottom0 {
            self.scroll_top = top0;
            self.scroll_bottom = bottom0;
        } else {
            self.scroll_top = 0;
            self.scroll_bottom = self.rows.saturating_sub(1);
        }
        self.cup(0, 0);
    }

    /// DECALN (ESC # 8): fill the screen with E, reset the margins and home the cursor.
    fn decaln(&mut self) {
        let cell = self.styled('E');
        for row in 0..self.rows {
            for col in 0..self.cols {
                self.put(row, col, cell);
            }
        }
        self.combining.clear();
        self.clear_all_line_flags();
        self.scroll_top = 0;
        self.scroll_bottom = self.rows.saturating_sub(1);
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.wrap_pending = false;
        self.mark_cursor();
    }
    fn save_cursor(&mut self) {
        // xterm keeps DECSC's saved cursor separately for the main and alternate screens, so that
        // switching back restores the position that screen had.
        let saved = SavedCursor {
            row: self.cursor_row,
            col: self.cursor_col,
            sgr: self.sgr,
            origin: self.origin(),
        };
        if self.alt {
            self.alt_decsc = saved;
        } else {
            self.saved = saved;
        }
    }

    fn restore_cursor(&mut self) {
        let saved = if self.alt { self.alt_decsc } else { self.saved };
        self.cursor_row = saved.row.min(self.rows.saturating_sub(1));
        self.cursor_col = saved.col.min(self.cols.saturating_sub(1));
        self.sgr = saved.sgr;
        self.set_bit(MODE_ORIGIN, saved.origin);
        self.wrap_pending = false;
        self.mark_cursor();
    }
    /// `CSI ? Pm s` (xterm XTERM_SAVE): remember the current on/off state of each listed
    /// DEC private mode. xterm keeps one saved slot per mode, so a later `CSI ? Pm r`
    /// restores only the modes it names.
    fn save_private_modes(&mut self, params: &Params) {
        for i in 0..params.len() {
            let mode = params.get(i);
            if let Some(on) = self.savable_private_mode(mode) {
                self.saved_private_modes.insert(mode, on);
            }
        }
    }

    /// `CSI ? Pm r` (xterm XTERM_RESTORE): restore the modes named, using the value saved
    /// for each. A mode that was never saved is left alone.
    fn restore_private_modes(&mut self, params: &Params) {
        for i in 0..params.len() {
            let mode = params.get(i);
            if let Some(on) = self.saved_private_modes.get(&mode).copied() {
                self.set_private_mode(mode, on);
            }
        }
    }

    /// The DEC private modes whose state `CSI ? Pm s` / `CSI ? Pm r` save and restore.
    /// Restricted to plain on/off modes: the alternate-screen modes (47/1047/1049) are
    /// excluded because restoring them would swap the visible buffer, which xterm does
    /// but which nothing here needs yet.
    fn savable_private_mode(&self, mode: u16) -> Option<bool> {
        match mode {
            1 => Some(self.modes & MODE_APP_CURSOR != 0),
            6 => Some(self.modes & MODE_ORIGIN != 0),
            7 => Some(self.modes & MODE_AUTOWRAP != 0),
            25 => Some(self.cursor_visible),
            _ => None,
        }
    }

    fn set_modes(&mut self, params: &Params, private: bool, enable: bool) {
        let mut i = 0;
        while i < params.len() {
            let mode = params.get(i);
            if private {
                self.set_private_mode(mode, enable);
            } else {
                self.set_standard_mode(mode, enable);
            }
            i += 1;
        }
    }

    fn set_private_mode(&mut self, mode: u16, enable: bool) {
        match mode {
            1 => self.set_bit(MODE_APP_CURSOR, enable),
            6 => {
                self.set_bit(MODE_ORIGIN, enable);
                // Origin mode homes the cursor inside the (possibly changed) region.
                self.cup(0, 0);
            }
            7 => self.set_bit(MODE_AUTOWRAP, enable),
            45 => self.set_bit(MODE_REVERSE_WRAP, enable),
            1045 => self.set_bit(MODE_REVERSE_WRAP2, enable),
            // 1048: save (h) / restore (l) the cursor, like DECSC/DECRC.
            1048 => {
                if enable {
                    self.save_cursor();
                } else {
                    self.restore_cursor();
                }
            }
            25 => {
                self.cursor_visible = enable;
                self.set_bit(MODE_CURSOR_VISIBLE, enable);
                self.mark_cursor();
            }
            47 => self.set_alt(enable, false, false),
            1047 => self.set_alt(enable, true, false),
            1049 => self.set_alt(enable, true, true),
            2004 => self.set_bit(MODE_BRACKETED_PASTE, enable),
            _ => {}
        }
    }

    /// DECRQM (`CSI Ps $ p` / `CSI ? Ps $ p`): report one mode's state as
    /// `CSI Ps ; Pm $ y`, where Pm is 0 not recognised, 1 set, 2 reset
    /// (xterm ctlseqs; oracle = xterm). Reporting 0 for modes we do not track is the
    /// honest answer - claiming a state we do not maintain would be worse than unknown.
    fn decrqm(&mut self, params: &Params, private: bool) {
        let mode = params.get(0);
        let state = self.mode_state(mode, private);
        let prefix = if private { "?" } else { "" };
        self.responses
            .push(format!("\x1b[{prefix}{mode};{state}$y").into_bytes());
    }

    fn mode_state(&self, mode: u16, private: bool) -> u8 {
        fn pm(on: bool) -> u8 {
            if on {
                1
            } else {
                2
            }
        }
        if private {
            return match mode {
                1 => pm(self.modes & MODE_APP_CURSOR != 0),
                6 => pm(self.modes & MODE_ORIGIN != 0),
                7 => pm(self.modes & MODE_AUTOWRAP != 0),
                45 => pm(self.modes & MODE_REVERSE_WRAP != 0),
                1045 => pm(self.modes & MODE_REVERSE_WRAP2 != 0),
                25 => pm(self.cursor_visible),
                47 | 1047 | 1049 => pm(self.alt),
                2004 => pm(self.modes & MODE_BRACKETED_PASTE != 0),
                _ => 0,
            };
        }
        match mode {
            4 => pm(self.modes & MODE_INSERT != 0),
            20 => pm(self.modes & MODE_LINEFEED != 0),
            _ => 0,
        }
    }

    fn set_standard_mode(&mut self, mode: u16, enable: bool) {
        match mode {
            4 => self.set_bit(MODE_INSERT, enable),
            20 => self.set_bit(MODE_LINEFEED, enable),
            _ => {}
        }
    }

    fn set_alt(&mut self, enable: bool, clear: bool, save_cursor: bool) {
        if enable {
            if !self.alt {
                if save_cursor {
                    self.alt_saved = SavedCursor {
                        row: self.cursor_row,
                        col: self.cursor_col,
                        sgr: self.sgr,
                        origin: self.origin(),
                    };
                }
                std::mem::swap(&mut self.cells, &mut self.saved_cells);
                std::mem::swap(&mut self.row_flags, &mut self.saved_row_flags);
                if clear {
                    for cell in &mut self.cells {
                        *cell = Cell::BLANK;
                    }
                    self.clear_all_line_flags();
                }
                self.alt = true;
                self.set_bit(MODE_ALT_SCREEN, true);
                self.mark_full();
            } else if clear {
                for cell in &mut self.cells {
                    *cell = Cell::BLANK;
                }
                self.clear_all_line_flags();
                self.mark_full();
            }
        } else if self.alt {
            std::mem::swap(&mut self.cells, &mut self.saved_cells);
            std::mem::swap(&mut self.row_flags, &mut self.saved_row_flags);
            self.alt = false;
            self.set_bit(MODE_ALT_SCREEN, false);
            if save_cursor {
                let saved = self.alt_saved;
                self.cursor_row = saved.row.min(self.rows.saturating_sub(1));
                self.cursor_col = saved.col.min(self.cols.saturating_sub(1));
                self.sgr = saved.sgr;
                self.set_bit(MODE_ORIGIN, saved.origin);
                self.wrap_pending = false;
            }
            self.combining.clear();
            self.mark_full();
        }
    }

    fn sgr(&mut self, params: &Params) {
        if params.is_empty() {
            self.sgr = Sgr::default();
            return;
        }
        let mut i = 0;
        while i < params.len() {
            let value = params.get(i);
            match value {
                0 => self.sgr = Sgr::default(),
                1 => self.sgr.attrs |= ATTR_BOLD,
                2 => self.sgr.attrs |= ATTR_DIM,
                3 => self.sgr.attrs |= ATTR_ITALIC,
                4 => self.sgr.attrs |= ATTR_UNDERLINE,
                5 => self.sgr.attrs |= ATTR_BLINK,
                7 => self.sgr.attrs |= ATTR_REVERSE,
                8 => self.sgr.attrs |= ATTR_HIDDEN,
                9 => self.sgr.attrs |= ATTR_STRIKETHROUGH,
                21 => self.sgr.attrs |= ATTR_DOUBLE_UNDERLINE,
                22 => self.sgr.attrs &= !(ATTR_BOLD | ATTR_DIM),
                23 => self.sgr.attrs &= !ATTR_ITALIC,
                24 => self.sgr.attrs &= !(ATTR_UNDERLINE | ATTR_DOUBLE_UNDERLINE),
                25 => self.sgr.attrs &= !ATTR_BLINK,
                27 => self.sgr.attrs &= !ATTR_REVERSE,
                28 => self.sgr.attrs &= !ATTR_HIDDEN,
                29 => self.sgr.attrs &= !ATTR_STRIKETHROUGH,
                30..=37 => self.sgr.fg = Color::Indexed((value - 30) as u8),
                39 => self.sgr.fg = Color::Default,
                40..=47 => self.sgr.bg = Color::Indexed((value - 40) as u8),
                49 => self.sgr.bg = Color::Default,
                90..=97 => self.sgr.fg = Color::Indexed((value - 90 + 8) as u8),
                100..=107 => self.sgr.bg = Color::Indexed((value - 100 + 8) as u8),
                38 => i += self.sgr_extended(params, i + 1, true),
                48 => i += self.sgr_extended(params, i + 1, false),
                _ => {}
            }
            i += 1;
        }
    }

    fn sgr_extended(&mut self, params: &Params, start: usize, foreground: bool) -> usize {
        match params.get(start) {
            5 => {
                let index = params.get(start + 1).min(255) as u8;
                self.set_color(foreground, Color::Indexed(index));
                2
            }
            2 => {
                let r = params.get(start + 1).min(255) as u8;
                let g = params.get(start + 2).min(255) as u8;
                let b = params.get(start + 3).min(255) as u8;
                self.set_color(foreground, Color::Rgb(r, g, b));
                4
            }
            _ => 1,
        }
    }

    fn set_color(&mut self, foreground: bool, color: Color) {
        if foreground {
            self.sgr.fg = color;
        } else {
            self.sgr.bg = color;
        }
    }

    // -- CSI ---------------------------------------------------------------

    /// CSI dispatch.
    pub fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        ignore: bool,
        action: u8,
    ) {
        if ignore {
            return;
        }
        let private = intermediates.first() == Some(&b'?');
        match action {
            b'A' => self.cursor_up(def(params.get(0))),
            b'B' => self.cursor_down(def(params.get(0))),
            b'C' => self.cursor_right(def(params.get(0))),
            b'D' => self.cursor_left(def(params.get(0))),
            b'E' => {
                let n = def(params.get(0));
                self.cursor_down(n);
                self.cursor_col = 0;
                self.mark_cursor();
            }
            b'F' => {
                let n = def(params.get(0));
                self.cursor_up(n);
                self.cursor_col = 0;
                self.mark_cursor();
            }
            b'G' => {
                let col = params.get(0).saturating_sub(1);
                self.cursor_col = col.min(self.cols.saturating_sub(1));
                self.wrap_pending = false;
                self.mark_cursor();
            }
            b'H' | b'f' => {
                let row = params.get(0).saturating_sub(1);
                let col = params.get(1).saturating_sub(1);
                self.cup(row, col);
            }
            b'I' => {
                // CHT: cursor forward tabulation (ECMA-48). Ps defaults to 1.
                for _ in 0..def(params.get(0)) {
                    self.tab();
                }
            }
            b'Z' => {
                // CBT: cursor backward tabulation (ECMA-48). Ps defaults to 1.
                for _ in 0..def(params.get(0)) {
                    self.tab_back();
                }
            }
            b'J' => self.erase_display(params.get(0)),
            b'K' => self.erase_line(params.get(0)),
            b'L' => self.insert_lines(def(params.get(0))),
            b'M' => self.delete_lines(def(params.get(0))),
            b'P' => self.delete_chars(def(params.get(0))),
            b'S' => self.scroll_up(def(params.get(0))),
            b'T' => self.scroll_down(def(params.get(0))),
            b'X' => {
                let n = def(params.get(0));
                let end = self.cursor_col.saturating_add(n).saturating_sub(1);
                self.erase_cells(
                    self.cursor_row,
                    self.cursor_col,
                    end.min(self.cols.saturating_sub(1)),
                );
            }
            b'@' => self.insert_chars(def(params.get(0))),
            b'd' => {
                let row = params.get(0).saturating_sub(1);
                self.cursor_row = row.min(self.rows.saturating_sub(1));
                self.wrap_pending = false;
                self.mark_cursor();
            }
            b'e' => self.cursor_down(def(params.get(0))),
            b'\x60' => {
                // HPA: horizontal position absolute (1-based; 0 or absent means column 1).
                let col = params.get(0).saturating_sub(1);
                self.cursor_col = col.min(self.cols.saturating_sub(1));
                self.wrap_pending = false;
                self.mark_cursor();
            }
            b'a' => {
                // HPR: horizontal position relative.
                let col = self.cursor_col.saturating_add(def(params.get(0)));
                self.cursor_col = col.min(self.cols.saturating_sub(1));
                self.wrap_pending = false;
                self.mark_cursor();
            }
            b'b' => {
                // REP: repeat the preceding graphic character, bounded by the screen.
                let ch = self.last_graphic;
                let cap = def(params.get(0)).min(self.cols.saturating_mul(self.rows));
                for _ in 0..cap {
                    self.print(ch);
                }
            }
            // DA1: CSI c / CSI 0 c (ADR-0030 D-2).
            b'c' if intermediates.is_empty() => self.device_attributes(false),
            // DA2: CSI > c / CSI > 0 c (intermediate '>').
            b'c' if intermediates == [b'>'] => self.device_attributes(true),
            // DECSTR (soft reset). esctest issues this before every case, and with no handler
            // the scrolling region leaked from one case into the next.
            b'p' if intermediates == [b'!'] => self.soft_reset(),
            b'p' if intermediates == [b'$'] || intermediates == [b'?', b'$'] => {
                self.decrqm(params, private);
            }
            b'g' => self.clear_tab(params.get(0)),
            b'h' => self.set_modes(params, private, true),
            b'l' => self.set_modes(params, private, false),
            b'm' if intermediates.is_empty() => self.sgr(params),
            b'n' => self.device_status(params, private, ignore),
            b'r' if intermediates.is_empty() => self.set_scroll_region(params),
            // XTERM_SAVE / XTERM_RESTORE: `CSI ? Pm s` / `CSI ? Pm r`, the DEC private-mode
            // counterparts of save/restore cursor (xterm's savemodes/restoremodes).
            b'r' if intermediates == [b'?'] => self.restore_private_modes(params),
            b's' if intermediates.is_empty() => self.save_cursor(),
            b's' if intermediates == [b'?'] => self.save_private_modes(params),
            b'u' if intermediates.is_empty() => self.restore_cursor(),
            b't' if intermediates.is_empty() => self.window_op(params),
            _ => {}
        }
    }

    /// HTS (`ESC H`): set a tab stop at the current column.
    fn set_tab_stop(&mut self) {
        if let Some(slot) = self.tab_stops.get_mut(usize::from(self.cursor_col)) {
            *slot = true;
        }
    }

    fn clear_tab(&mut self, mode: u16) {
        match mode {
            0 => {
                if let Some(slot) = self.tab_stops.get_mut(usize::from(self.cursor_col)) {
                    *slot = false;
                }
            }
            3 => {
                for slot in &mut self.tab_stops {
                    *slot = false;
                }
            }
            _ => {}
        }
    }

    // -- OSC helpers -------------------------------------------------------

    /// OSC 8 hyperlink control.
    pub fn osc8(&mut self, params: &[&[u8]]) {
        let uri = params.get(2).copied().unwrap_or(b"");
        if uri.is_empty() {
            self.active_link = None;
            return;
        }
        if !scheme_allowed(uri) {
            self.rejected_links = self.rejected_links.saturating_add(1);
            self.active_link = None;
            return;
        }
        let id = params
            .get(1)
            .map(|raw| parse_link_id(raw))
            .unwrap_or_default();
        let target = String::from_utf8_lossy(uri).into_owned();
        self.links.push(LinkSpan {
            row: self.cursor_row,
            start_col: self.cursor_col,
            end_col: self.cursor_col,
            id,
            target,
        });
        self.active_link = Some(self.links.len() - 1);
    }

    /// Evaluate an OSC 52 request (AR-29 item 5): reads are denied, writes are
    /// guarded (single-line allowed; multi-line or control-character payloads need
    /// explicit confirmation; never a silent multi-line). Only metadata is recorded
    /// -- length and line count, never the payload (AR-12 / AR-29 item 6). The
    /// returned decision is what gets routed to the clipboard broker.
    pub fn osc52(&mut self, params: &[&[u8]]) -> ClipboardVerdict {
        let payload = params.get(2).copied().unwrap_or(b"");
        if payload == b"?" {
            self.clipboard_read_requests = self.clipboard_read_requests.saturating_add(1);
            self.clipboard_denied_reads = self.clipboard_denied_reads.saturating_add(1);
            let verdict = ClipboardVerdict {
                direction: ClipboardDirection::Read,
                decision: ClipboardDecision::DeniedRead,
                decoded_len: 0,
                line_count: 0,
            };
            self.last_clipboard = Some(verdict);
            return verdict;
        }
        self.clipboard_write_requests = self.clipboard_write_requests.saturating_add(1);
        let decoded = decode_base64(payload).unwrap_or_else(|| payload.to_vec());
        let line_count = count_lines(&decoded);
        let has_multiline = line_count > 1;
        let has_control = decoded.iter().any(|byte| is_control_byte(*byte));
        let decision = if has_multiline {
            self.clipboard_confirm_multi_line = self.clipboard_confirm_multi_line.saturating_add(1);
            ClipboardDecision::NeedsConfirmMultiLine
        } else if has_control {
            self.clipboard_confirm_control_chars =
                self.clipboard_confirm_control_chars.saturating_add(1);
            ClipboardDecision::NeedsConfirmControlChars
        } else {
            self.clipboard_allowed_single_line =
                self.clipboard_allowed_single_line.saturating_add(1);
            ClipboardDecision::GuardedAllowSingleLine
        };
        let verdict = ClipboardVerdict {
            direction: ClipboardDirection::Write,
            decision,
            decoded_len: decoded.len(),
            line_count,
        };
        self.last_clipboard = Some(verdict);
        verdict
    }

    /// Record an OSC 9 / 777 notification.
    pub fn record_notification(&mut self) {
        self.notification_count = self.notification_count.saturating_add(1);
    }

    /// Record an OSC 9;4 ConEmu progress update (not a notification).
    pub fn record_progress(&mut self) {
        self.progress_count = self.progress_count.saturating_add(1);
    }
}

impl Default for Grid {
    fn default() -> Self {
        Self::new(80, 24)
    }
}

fn def(value: u16) -> u16 {
    if value == 0 {
        1
    } else {
        value
    }
}

/// `ESC ] Ps Pt ESC \\`: the OSC form xterm uses to report a title (Ps = L icon, l window).
fn osc_report(code: u8, text: &str) -> Vec<u8> {
    let mut out = vec![0x1b, b']', code];
    out.extend_from_slice(text.as_bytes());
    out.extend_from_slice(b"\x1b\\");
    out
}

fn default_tab_stops(cols: u16) -> Vec<bool> {
    let mut stops = vec![false; usize::from(cols)];
    let mut col = 8usize;
    while col < usize::from(cols) {
        if let Some(slot) = stops.get_mut(col) {
            *slot = true;
        }
        col += 8;
    }
    stops
}

fn scheme_allowed(uri: &[u8]) -> bool {
    let text = match std::str::from_utf8(uri) {
        Ok(text) => text,
        Err(_) => return false,
    };
    let scheme = match text.split_once(':') {
        Some((scheme, _)) => scheme.to_ascii_lowercase(),
        None => return false,
    };
    matches!(
        scheme.as_str(),
        "http" | "https" | "ftp" | "file" | "mailto"
    )
}

fn parse_link_id(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    for part in text.split(':') {
        if let Some(id) = part.strip_prefix("id=") {
            return id.to_string();
        }
    }
    String::new()
}

/// Strip control characters and bidi overrides from a title (anti-spoofing).
#[must_use]
pub fn sanitize_title(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_control() && !is_bidi_override(*c))
        .collect()
}

fn is_bidi_override(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Parse an OSC 7 / OSC 633 cwd payload, returning (path, remote_host).
#[must_use]
pub fn parse_file_uri(uri: &str) -> Option<(String, bool)> {
    let rest = uri.strip_prefix("file://")?;
    let (host, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };
    let decoded = percent_decode(path);
    let remote = !(host.is_empty() || host.eq_ignore_ascii_case("localhost"));
    Some((decoded, remote))
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex_value(bytes[i + 1]);
            let lo = hex_value(bytes[i + 2]);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_control_byte(byte: u8) -> bool {
    byte < 0x20 || byte == 0x7F
}

fn count_lines(bytes: &[u8]) -> u32 {
    let mut lines = 1u32;
    let mut prev_cr = false;
    for &byte in bytes {
        if byte == b'\n' {
            if !prev_cr {
                lines = lines.saturating_add(1);
            }
            prev_cr = false;
        } else if byte == b'\r' {
            lines = lines.saturating_add(1);
            prev_cr = true;
        } else {
            prev_cr = false;
        }
    }
    lines
}

/// Decode standard (or URL-safe) base64. Returns None for malformed input so the
/// caller can fall back to classifying the raw payload bytes.
fn decode_base64(input: &[u8]) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::with_capacity(input.len() / 4 * 3 + 3);
    let mut accumulator: u32 = 0;
    let mut bits: u32 = 0;
    let mut symbols = 0usize;
    for &byte in input {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            break;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        };
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        symbols += 1;
        if bits >= 8 {
            bits -= 8;
            out.push(((accumulator >> bits) & 0xFF) as u8);
        }
    }
    if symbols % 4 == 1 {
        return None;
    }
    Some(out)
}
