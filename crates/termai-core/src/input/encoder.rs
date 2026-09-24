//! Stage S3 encoder: Legacy / modifyOtherKeys / kitty keyboard protocols.
//!
//! This is the single producer of PTY bytes (AR-29.3). It never touches the PTY,
//! never reads the clipboard, and never performs I/O.
//!
//! The three protocol columns are kernel/05 §3.3's keyboard table (Legacy / ModifyOtherKeys(2) /
//! Kitty(disambiguate + report_all)); every branch below cites the row(s) it implements.

use super::paste::{paste_gate, PasteCtx, PastePolicy};
use super::{
    ConfirmId, DropReason, EncodeOutcome, InputEncoder, InputError, InputEvent, InputSink, KeyCode,
    KeyEvent, KeyEventType, KeyboardMode, Modifiers, NamedKey, PasteOrigin,
};

/// Default encoder. Application modes are mirrored in from terminal state.
#[derive(Clone, Debug)]
pub struct StandardEncoder {
    /// DECCKM: application cursor keys (SS3 vs CSI).
    pub app_cursor: bool,
    /// DECCKM-equivalent for the keypad (kept for future use).
    pub app_keypad: bool,
    /// Whether SGR mouse reporting (1006) is active.
    pub mouse_sgr: bool,
    /// Bracketed paste enabled by the application.
    pub bracketed_paste: bool,
    /// Paste friction policy (may only strengthen).
    pub paste_policy: PastePolicy,
}

impl Default for StandardEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl StandardEncoder {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            app_cursor: false,
            app_keypad: false,
            mouse_sgr: true,
            bracketed_paste: false,
            paste_policy: PastePolicy::Ask,
        }
    }

    fn emit(out: &mut dyn InputSink, bytes: &[u8]) -> EncodeOutcome {
        match out.write(bytes) {
            Ok(()) => EncodeOutcome::Emitted(bytes.len()),
            Err(InputError::SinkClosed) => EncodeOutcome::Dropped(DropReason::NoLease),
            Err(_) => EncodeOutcome::Dropped(DropReason::PasteBlocked),
        }
    }

    fn encode_key(&self, ev: &KeyEvent, mode: &KeyboardMode) -> Option<Vec<u8>> {
        match mode {
            KeyboardMode::Kitty(flags) => kitty_key(ev, flags),
            KeyboardMode::Legacy => legacy_key(ev, self.app_cursor, 0),
            KeyboardMode::ModifyOtherKeys(level) => legacy_key(ev, self.app_cursor, *level),
        }
    }
}

impl InputEncoder for StandardEncoder {
    fn encode(
        &mut self,
        ev: &InputEvent,
        mode: &KeyboardMode,
        out: &mut dyn InputSink,
    ) -> EncodeOutcome {
        match ev {
            InputEvent::Key(k) => match self.encode_key(k, mode) {
                Some(bytes) => Self::emit(out, &bytes),
                None => {
                    if matches!(k.code, KeyCode::Dead(_)) {
                        EncodeOutcome::Consumed
                    } else {
                        EncodeOutcome::Dropped(DropReason::UnsupportedKey)
                    }
                }
            },
            InputEvent::TextCommit(text) => Self::emit(out, text.as_bytes()),
            InputEvent::Paste(p) => {
                let ctx = PasteCtx {
                    bracketed_mode: self.bracketed_paste || p.bracketed,
                    policy: self.paste_policy,
                };
                let d = paste_gate(p, &ctx);
                if d.needs_confirm {
                    if p.origin == PasteOrigin::Osc52 {
                        return EncodeOutcome::Dropped(DropReason::PasteBlocked);
                    }
                    return EncodeOutcome::NeedsConfirm(ConfirmId(0));
                }
                if !d.allow {
                    return EncodeOutcome::Dropped(DropReason::PasteBlocked);
                }
                if ctx.bracketed_mode {
                    let mut bytes = Vec::with_capacity(d.sanitized.len() + 12);
                    bytes.extend_from_slice(super::paste::BRACKET_START);
                    bytes.extend_from_slice(&d.sanitized);
                    bytes.extend_from_slice(super::paste::BRACKET_END);
                    Self::emit(out, &bytes)
                } else {
                    Self::emit(out, &d.sanitized)
                }
            }
            InputEvent::Mouse(m) => {
                if !self.mouse_sgr {
                    return EncodeOutcome::Dropped(DropReason::UnsupportedKey);
                }
                let btn = m.button
                    + if m.mods.shift { 4 } else { 0 }
                    + if m.mods.alt { 8 } else { 0 }
                    + if m.mods.ctrl { 16 } else { 0 }
                    + match m.kind {
                        super::MouseKind::Motion => 32,
                        super::MouseKind::Wheel => 64,
                        _ => 0,
                    };
                let final_byte = match m.kind {
                    super::MouseKind::Release => 'm',
                    _ => 'M',
                };
                let seq = format!(
                    "\x1b[<{};{};{}{}",
                    btn,
                    u32::from(m.col) + 1,
                    u32::from(m.row) + 1,
                    final_byte
                );
                Self::emit(out, seq.as_bytes())
            }
            InputEvent::FocusGained => Self::emit(out, b"\x1b[I"),
            InputEvent::FocusLost => Self::emit(out, b"\x1b[O"),
            InputEvent::ApiInject(bytes) => Self::emit(out, bytes),
        }
    }
}

/// Control byte for Ctrl+<char> (xterm table).
#[must_use]
pub fn ctrl_byte(c: char) -> Option<u8> {
    match c {
        ' ' | '@' => Some(0x00),
        'a'..='z' => Some(c as u8 - b'a' + 1),
        'A'..='Z' => Some(c as u8 - b'A' + 1),
        '[' => Some(0x1B),
        '\\' => Some(0x1C),
        ']' => Some(0x1D),
        '^' => Some(0x1E),
        '_' => Some(0x1F),
        '?' => Some(0x7F),
        _ => None,
    }
}

/// kernel/05 §3.3, ModifyOtherKeys(2) column: `CSI 27;{1+shift+2*alt+4*ctrl};{codepoint}~`.
fn mok_form(code: u32, mods: Modifiers) -> Vec<u8> {
    let m = mods.xterm_param().unwrap_or(1);
    format!("\x1b[27;{m};{code}~").into_bytes()
}

/// kernel/05 §3.3, ModifyOtherKeys(2) column: does the Legacy encoding of this key + modifier
/// combination drop a modifier bit, so that [`mok_form`] must replace it?
///
/// The §3.3 rows this is read from, verbatim (Legacy column -> ModifyOtherKeys(2) column):
///
/// ```text
/// | Ctrl+a       | `0x01`              | `0x01`         |
/// | Ctrl+Shift+A | `0x01`（丢 Shift）   | `CSI 27;6;65~` |
/// | Alt+x        | `ESC x`             | `ESC x`        |
/// ```
///
/// Legacy carries Alt in the `ESC` prefix (§3.2「Alt + 可打印」row) and Ctrl in the C0 byte
/// (§3.2「Ctrl + 字母」row), but once Ctrl has consumed the key it can no longer carry Shift
/// (`Ctrl+Shift+A`), and for a character with no C0 mapping it cannot carry Ctrl at all
/// (`Ctrl+1` -> `CSI 27;5;49~`, corpus case M02). `Ctrl+a` and `Alt+x` keep their Legacy bytes.
fn mok_replaces_legacy(c: char, mods: Modifiers) -> bool {
    mods.ctrl && (ctrl_byte(c).is_none() || mods.shift)
}

/// kitty's `:{event}` event-type suffix (2 = repeat, 3 = release) — §3.1 `KittyFlags.report_events`,
/// corpus cases K09/K10 — or nothing when the event type is not reported.
fn event_suffix(event: Option<u8>) -> String {
    event.map_or_else(String::new, |e| format!(":{e}"))
}

/// `CSI {code}[;{m}{event}]~`. The caller prints the parameter even when it is 1 by passing
/// `Some(1)` (kitty `report_all` / a reported event type).
fn tilde(code: u8, m: Option<u8>, event: &str) -> Vec<u8> {
    match m {
        Some(v) => format!("\x1b[{code};{v}{event}~").into_bytes(),
        None => format!("\x1b[{code}~").into_bytes(),
    }
}

/// `CSI 1[;{m}{event}]{final}` / `SS3 {final}` / `CSI {final}` (DECCKM, xterm table).
fn cursor_like(final_byte: u8, m: Option<u8>, app_cursor: bool, event: &str) -> Vec<u8> {
    match m {
        Some(v) => format!("\x1b[1;{v}{event}{}", final_byte as char).into_bytes(),
        None if app_cursor => vec![0x1B, b'O', final_byte],
        None => vec![0x1B, b'[', final_byte],
    }
}

/// The legacy / functional sequence of a named key: the Legacy and ModifyOtherKeys columns of
/// kernel/05 §3.3, plus the *functional* cells of its Kitty column. §3.3's `Left` row is
/// `CSI D` / `SS3 D`（DECCKM） in Legacy, 同 Legacy in ModifyOtherKeys(2) and `CSI 1;1D` in Kitty —
/// i.e. in kitty mode the functional sequence is still used, printed *with* its modifier parameter
/// (1 when no modifier is held) under `report_all`, instead of being replaced by `CSI 57354u`.
///
/// `m` is the calling protocol's modifier parameter (`None` = print no parameter); `event` is
/// kitty's `:{event}` suffix. A reported event type can only travel in the parameter list, so the
/// parameter is printed whenever `event` is set (kitty's `CSI 1;1:3D`).
fn functional_named(
    key: NamedKey,
    m: Option<u8>,
    app_cursor: bool,
    event: Option<u8>,
) -> Option<Vec<u8>> {
    let m = if event.is_some() {
        Some(m.unwrap_or(1))
    } else {
        m
    };
    let suffix = event_suffix(event);
    let suffix = suffix.as_str();
    Some(match key {
        NamedKey::Enter => b"\r".to_vec(),
        NamedKey::Tab => b"\t".to_vec(),
        NamedKey::Backspace => vec![0x7F],
        NamedKey::Escape => vec![0x1B],
        NamedKey::Space => b" ".to_vec(),
        NamedKey::Up => cursor_like(b'A', m, app_cursor, suffix),
        NamedKey::Down => cursor_like(b'B', m, app_cursor, suffix),
        NamedKey::Right => cursor_like(b'C', m, app_cursor, suffix),
        NamedKey::Left => cursor_like(b'D', m, app_cursor, suffix),
        NamedKey::Home => cursor_like(b'H', m, app_cursor, suffix),
        NamedKey::End => cursor_like(b'F', m, app_cursor, suffix),
        NamedKey::Insert => tilde(2, m, suffix),
        NamedKey::Delete => tilde(3, m, suffix),
        NamedKey::PageUp => tilde(5, m, suffix),
        NamedKey::PageDown => tilde(6, m, suffix),
        NamedKey::F(n) => match n {
            1..=4 => {
                let f = b"PQRS"[usize::from(n - 1)];
                match m {
                    Some(v) => format!("\x1b[1;{v}{suffix}{}", f as char).into_bytes(),
                    None => vec![0x1B, b'O', f],
                }
            }
            5..=12 => {
                let code = match n {
                    5 => 15,
                    6 => 17,
                    7 => 18,
                    8 => 19,
                    9 => 20,
                    10 => 21,
                    11 => 23,
                    _ => 24,
                };
                tilde(code, m, suffix)
            }
            _ => return None,
        },
    })
}

fn legacy_key(ev: &KeyEvent, app_cursor: bool, mok_level: u8) -> Option<Vec<u8>> {
    // Release events carry no bytes in legacy modes.
    if ev.ty == KeyEventType::Release {
        return None;
    }
    if ev.mods.unsupported_for_legacy() {
        return None;
    }
    match ev.code {
        KeyCode::Dead(_) => None,
        KeyCode::Named(n) => {
            // §3.3「Esc」row, ModifyOtherKeys(2) column = `CSI 27;1;27~`: the bare `0x1B` byte is
            // ambiguous with the start of a sequence, so the modifier-reporting mode replaces it —
            // the same reason that row's Kitty column is `CSI 27u`. `Enter` keeps `CR` and the
            // functional keys keep the Legacy encoding (§3.3「Enter」/「Left」rows, ModifyOtherKeys(2)
            // column = `CR` / 同 Legacy).
            if mok_level >= 2 && n == NamedKey::Escape {
                return Some(mok_form(27, ev.mods));
            }
            // §3.3 has no cell restating `Shift+Tab = CSI Z`, so the Legacy cell (xterm ctlseqs,
            // case L19) is kept in every mode whose table column is 同 Legacy.
            if n == NamedKey::Tab && ev.mods.shift {
                return Some(b"\x1b[Z".to_vec());
            }
            functional_named(n, ev.mods.xterm_param(), app_cursor, None)
        }
        KeyCode::Char(c) => {
            // §3.3, ModifyOtherKeys(2) column: `mok_form` replaces the Legacy encoding exactly when
            // the Legacy encoding would drop a modifier bit (see `mok_replaces_legacy`).
            if mok_level >= 2 && mok_replaces_legacy(c, ev.mods) {
                return Some(mok_form(u32::from(c), ev.mods));
            }
            if ev.mods.ctrl {
                if let Some(byte) = ctrl_byte(c) {
                    let mut v = Vec::with_capacity(2);
                    if ev.mods.alt {
                        v.push(0x1B);
                    }
                    v.push(byte);
                    return Some(v);
                }
            }
            let text = ev
                .text
                .as_deref()
                .map_or_else(|| c.to_string(), std::string::ToString::to_string);
            if ev.mods.alt {
                let mut v = Vec::with_capacity(text.len() + 1);
                v.push(0x1B);
                v.extend_from_slice(text.as_bytes());
                Some(v)
            } else {
                Some(text.into_bytes())
            }
        }
    }
}

fn kitty_event_type(ty: KeyEventType) -> u8 {
    match ty {
        KeyEventType::Press => 1,
        KeyEventType::Repeat => 2,
        KeyEventType::Release => 3,
    }
}

/// The kernel/05 §3.3 Kitty-column encoding of a named key, or `None` when that key takes the
/// `CSI {code};{mod}u` form (or when the functional table has no sequence for it, e.g. `F13..F24`).
///
/// Verbatim §3.3 rows:
///
/// ```text
/// | Esc   | `0x1B`                     | `CSI 27;1;27~` | `CSI 27u`  |
/// | Enter | `CR`                       | `CR`           | `CR`       |
/// | Left  | `CSI D` / `SS3 D`（DECCKM） | 同 Legacy      | `CSI 1;1D` |
/// ```
fn kitty_functional(n: NamedKey, ev: &KeyEvent, flags: &super::KittyFlags) -> Option<Vec<u8>> {
    match n {
        // §3.3「Esc」row, Kitty column = `CSI 27u`: removing that ambiguity is exactly what
        // `disambiguate` exists for, so Escape never keeps its legacy byte in kitty mode.
        NamedKey::Escape => None,
        // §3.3「Enter」row, Kitty column = `CR`. A C0 byte carries neither a modifier parameter nor
        // an event type, so it is kept for an unmodified press only; a combination it cannot express
        // takes the `CSI {code};{mod}u` form (the rule the character cells of the table follow).
        // Tab = `HT` and Backspace = `DEL` are that same class of C0 key (xterm ctlseqs, L18/L20).
        NamedKey::Enter | NamedKey::Tab | NamedKey::Backspace
            if !(ev.mods.is_plain() && ev.ty == KeyEventType::Press) =>
        {
            None
        }
        // `NamedKey::Space` has no §3.3 cell at all (corpus `omitted`): its existing `CSI 32u` form
        // is kept rather than inventing a cell for it.
        NamedKey::Space => None,
        _ => {
            let event = if flags.report_events && ev.ty != KeyEventType::Press {
                Some(kitty_event_type(ev.ty))
            } else {
                None
            };
            // §3.3「Left」row, Kitty column = `CSI 1;1D`: under `report_all` the functional sequence
            // is printed with its parameter even when it is 1, which is what distinguishes that cell
            // from the Legacy `CSI D`.
            let kp = ev.mods.kitty_param();
            let m = if kp == 1 && !flags.report_all && event.is_none() {
                None
            } else {
                Some(kp)
            };
            // DECCKM (`SS3 D`) is a Legacy-column alternative, not a Kitty-column one: §3.3's Kitty
            // cell for that row is the CSI form.
            functional_named(n, m, false, event)
        }
    }
}

fn kitty_key(ev: &KeyEvent, flags: &super::KittyFlags) -> Option<Vec<u8>> {
    if !flags.report_events && matches!(ev.ty, KeyEventType::Release | KeyEventType::Repeat) {
        return None;
    }
    // §3.3's Kitty column keeps a legacy / functional sequence for the cells that say so (Enter ->
    // `CR`, arrows -> `CSI 1;1D`); `CSI {code};{mod}u` is reserved for the cells §3.3 spells that
    // way (`Esc` -> `CSI 27u`, and the character cells -> `CSI 97;5u`, ...).
    if let KeyCode::Named(n) = ev.code {
        if let Some(bytes) = kitty_functional(n, ev, flags) {
            return Some(bytes);
        }
    }
    let code = match ev.code {
        KeyCode::Char(c) => u32::from(c),
        KeyCode::Named(n) => n.codepoint(),
        KeyCode::Dead(_) => return None,
    };
    let m = ev.mods.kitty_param();
    let event_type = kitty_event_type(ev.ty);
    let mut params = code.to_string();
    if m != 1 || event_type != 1 || flags.report_text {
        params.push(';');
        params.push_str(&m.to_string());
        if event_type != 1 {
            params.push(':');
            params.push_str(&event_type.to_string());
        }
    }
    if flags.report_text {
        if let Some(text) = ev.text.as_deref() {
            if !text.is_empty() {
                params.push(';');
                let mut first = true;
                for ch in text.chars() {
                    if !first {
                        params.push(':');
                    }
                    params.push_str(&u32::from(ch).to_string());
                    first = false;
                }
            }
        }
    }
    Some(format!("\x1b[{params}u").into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Modifiers;

    #[derive(Default)]
    struct Buf {
        bytes: Vec<u8>,
        closed: bool,
    }

    impl InputSink for Buf {
        fn write(&mut self, bytes: &[u8]) -> Result<(), InputError> {
            if self.closed {
                return Err(InputError::SinkClosed);
            }
            self.bytes.extend_from_slice(bytes);
            Ok(())
        }
    }

    fn key(code: KeyCode, mods: Modifiers) -> InputEvent {
        InputEvent::Key(KeyEvent {
            code,
            mods,
            ty: KeyEventType::Press,
            text: None,
            scancode: 0,
        })
    }

    fn enc(ev: &InputEvent, mode: &KeyboardMode) -> Vec<u8> {
        let mut e = StandardEncoder::new();
        let mut buf = Buf::default();
        e.encode(ev, mode, &mut buf);
        buf.bytes
    }

    #[test]
    fn ctrl_letter_maps_to_control_byte() {
        let mut mods = Modifiers::NONE;
        mods.ctrl = true;
        assert_eq!(
            enc(&key(KeyCode::Char('c'), mods), &KeyboardMode::Legacy),
            vec![0x03]
        );
        assert_eq!(
            enc(&key(KeyCode::Char('a'), mods), &KeyboardMode::Legacy),
            vec![0x01]
        );
        assert_eq!(
            enc(&key(KeyCode::Char('['), mods), &KeyboardMode::Legacy),
            vec![0x1B]
        );
    }

    #[test]
    fn alt_prefixes_with_esc() {
        let mut mods = Modifiers::NONE;
        mods.alt = true;
        assert_eq!(
            enc(&key(KeyCode::Char('a'), mods), &KeyboardMode::Legacy),
            b"\x1ba"
        );
    }

    #[test]
    fn plain_text_is_utf8() {
        assert_eq!(
            enc(
                &key(KeyCode::Char('x'), Modifiers::NONE),
                &KeyboardMode::Legacy
            ),
            b"x"
        );
        let mut mods = Modifiers::NONE;
        mods.shift = true;
        assert_eq!(
            enc(&key(KeyCode::Char('A'), mods), &KeyboardMode::Legacy),
            b"A"
        );
    }

    #[test]
    fn named_keys_follow_xterm_tables() {
        let none = Modifiers::NONE;
        assert_eq!(
            enc(
                &key(KeyCode::Named(NamedKey::Enter), none),
                &KeyboardMode::Legacy
            ),
            b"\r"
        );
        assert_eq!(
            enc(
                &key(KeyCode::Named(NamedKey::Backspace), none),
                &KeyboardMode::Legacy
            ),
            vec![0x7F]
        );
        assert_eq!(
            enc(
                &key(KeyCode::Named(NamedKey::Up), none),
                &KeyboardMode::Legacy
            ),
            b"\x1b[A"
        );
        assert_eq!(
            enc(
                &key(KeyCode::Named(NamedKey::F(5)), none),
                &KeyboardMode::Legacy
            ),
            b"\x1b[15~"
        );
        let mut sh = Modifiers::NONE;
        sh.shift = true;
        assert_eq!(
            enc(
                &key(KeyCode::Named(NamedKey::Tab), sh),
                &KeyboardMode::Legacy
            ),
            b"\x1b[Z"
        );
    }

    #[test]
    fn app_cursor_mode_uses_ss3() {
        let mut e = StandardEncoder::new();
        e.app_cursor = true;
        let mut buf = Buf::default();
        e.encode(
            &key(KeyCode::Named(NamedKey::Up), Modifiers::NONE),
            &KeyboardMode::Legacy,
            &mut buf,
        );
        assert_eq!(buf.bytes, b"\x1bOA");
    }

    #[test]
    fn modified_arrows_use_csi_param() {
        let mut mods = Modifiers::NONE;
        mods.ctrl = true;
        assert_eq!(
            enc(
                &key(KeyCode::Named(NamedKey::Right), mods),
                &KeyboardMode::Legacy
            ),
            b"\x1b[1;5C"
        );
    }

    #[test]
    fn super_and_hyper_are_dropped_not_mangled() {
        let mut mods = Modifiers::NONE;
        mods.super_ = true;
        let mut e = StandardEncoder::new();
        let mut buf = Buf::default();
        let out = e.encode(
            &key(KeyCode::Char('k'), mods),
            &KeyboardMode::Legacy,
            &mut buf,
        );
        assert_eq!(out, EncodeOutcome::Dropped(DropReason::UnsupportedKey));
        assert!(buf.bytes.is_empty());
    }

    #[test]
    fn kitty_encodes_csi_u_with_modifiers() {
        let mut mods = Modifiers::NONE;
        mods.ctrl = true;
        let mode = KeyboardMode::Kitty(crate::input::KittyFlags::default());
        assert_eq!(enc(&key(KeyCode::Char('c'), mods), &mode), b"\x1b[99;5u");
    }

    /// kernel/05 §3.3 keyboard table, quoted verbatim (Legacy / ModifyOtherKeys(2) / Kitty
    /// columns; the Kitty column is headed Kitty(disambiguate + report_all)):
    ///
    /// ```text
    /// | Ctrl+a       | `0x01`                     | `0x01`         | `CSI 97;5u` |
    /// | Ctrl+Shift+A | `0x01`（丢 Shift）          | `CSI 27;6;65~` | `CSI 65;6u` |
    /// | Alt+x        | `ESC x`                    | `ESC x`        | `CSI 120;3u`|
    /// | Esc          | `0x1B`                     | `CSI 27;1;27~` | `CSI 27u`   |
    /// | Enter        | `CR`                       | `CR`           | `CR`        |
    /// | Left         | `CSI D` / `SS3 D`（DECCKM） | 同 Legacy      | `CSI 1;1D`  |
    /// ```
    #[test]
    fn modify_other_keys_level2_follows_the_section_3_3_table() {
        let mode = KeyboardMode::ModifyOtherKeys(2);
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        let ctrl_shift = Modifiers {
            shift: true,
            ..ctrl
        };
        let alt = Modifiers {
            alt: true,
            ..Modifiers::NONE
        };
        // Ctrl+a: the C0 byte carries Ctrl and nothing else is held -> still `0x01`.
        assert_eq!(enc(&key(KeyCode::Char('a'), ctrl), &mode), vec![0x01]);
        // Ctrl+Shift+A: the C0 byte would drop Shift -> `CSI 27;6;65~`.
        assert_eq!(
            enc(&key(KeyCode::Char('A'), ctrl_shift), &mode),
            b"\x1b[27;6;65~"
        );
        // Alt+x: Alt is carried by the ESC prefix -> still `ESC x`.
        assert_eq!(enc(&key(KeyCode::Char('x'), alt), &mode), b"\x1bx");
        // Esc: the bare byte is ambiguous -> `CSI 27;1;27~`.
        assert_eq!(
            enc(
                &key(KeyCode::Named(NamedKey::Escape), Modifiers::NONE),
                &mode
            ),
            b"\x1b[27;1;27~"
        );
        // Enter keeps `CR` and Left stays on the Legacy cell (同 Legacy).
        assert_eq!(
            enc(
                &key(KeyCode::Named(NamedKey::Enter), Modifiers::NONE),
                &mode
            ),
            b"\r"
        );
        assert_eq!(
            enc(&key(KeyCode::Named(NamedKey::Left), Modifiers::NONE), &mode),
            b"\x1b[D"
        );
    }

    /// Same §3.3 table, Kitty(disambiguate + report_all) column: the functional cells are kept
    /// (`Enter` -> `CR`, arrows -> `CSI 1;1D`) and only the cells the table spells `CSI {code};{mod}u`
    /// (here `Esc` -> `CSI 27u`) use the CSI-u form.
    #[test]
    fn kitty_functional_cells_keep_their_section_3_3_sequences() {
        let table = KeyboardMode::Kitty(crate::input::KittyFlags {
            disambiguate: true,
            report_all: true,
            ..crate::input::KittyFlags::default()
        });
        assert_eq!(
            enc(
                &key(KeyCode::Named(NamedKey::Escape), Modifiers::NONE),
                &table
            ),
            b"\x1b[27u"
        );
        assert_eq!(
            enc(
                &key(KeyCode::Named(NamedKey::Enter), Modifiers::NONE),
                &table
            ),
            b"\r"
        );
        assert_eq!(
            enc(
                &key(KeyCode::Named(NamedKey::Left), Modifiers::NONE),
                &table
            ),
            b"\x1b[1;1D"
        );
        // Without `report_all` there is no `CSI 1;1D` cell to match: the same functional sequence is
        // printed without its parameter, i.e. the Legacy `CSI D` shape.
        let plain = KeyboardMode::Kitty(crate::input::KittyFlags::default());
        assert_eq!(
            enc(&key(KeyCode::Named(NamedKey::Up), Modifiers::NONE), &plain),
            b"\x1b[A"
        );
        assert_eq!(
            enc(
                &key(KeyCode::Named(NamedKey::Left), Modifiers::NONE),
                &plain
            ),
            b"\x1b[D"
        );
    }

    #[test]
    fn kitty_release_is_dropped_without_report_events() {
        let mut e = StandardEncoder::new();
        let mut buf = Buf::default();
        let ev = InputEvent::Key(KeyEvent {
            code: KeyCode::Char('a'),
            mods: Modifiers::NONE,
            ty: KeyEventType::Release,
            text: None,
            scancode: 0,
        });
        let out = e.encode(
            &ev,
            &KeyboardMode::Kitty(crate::input::KittyFlags::default()),
            &mut buf,
        );
        assert_eq!(out, EncodeOutcome::Dropped(DropReason::UnsupportedKey));
        let mode = KeyboardMode::Kitty(crate::input::KittyFlags {
            report_events: true,
            ..Default::default()
        });
        let mut e2 = StandardEncoder::new();
        let mut buf2 = Buf::default();
        e2.encode(&ev, &mode, &mut buf2);
        assert_eq!(buf2.bytes, b"\x1b[97;1:3u");
    }

    #[test]
    fn modify_other_keys_level2_encodes_printable_ctrl() {
        let mut mods = Modifiers::NONE;
        mods.ctrl = true;
        assert_eq!(
            enc(
                &key(KeyCode::Char('1'), mods),
                &KeyboardMode::ModifyOtherKeys(2)
            ),
            b"\x1b[27;5;49~"
        );
    }

    #[test]
    fn text_commit_is_utf8() {
        let mut e = StandardEncoder::new();
        let mut buf = Buf::default();
        let out = e.encode(
            &InputEvent::TextCommit("\u{4f60}\u{597d}".to_string()),
            &KeyboardMode::Legacy,
            &mut buf,
        );
        assert_eq!(out, EncodeOutcome::Emitted(6));
        assert_eq!(String::from_utf8(buf.bytes).unwrap(), "\u{4f60}\u{597d}");
    }

    #[test]
    fn multiline_paste_never_silently_emitted() {
        let mut e = StandardEncoder::new();
        let mut buf = Buf::default();
        let ev = InputEvent::Paste(crate::input::PastePayload {
            data: b"a\nb".to_vec(),
            origin: PasteOrigin::LocalClipboard,
            bracketed: false,
        });
        let out = e.encode(&ev, &KeyboardMode::Legacy, &mut buf);
        assert!(matches!(out, EncodeOutcome::NeedsConfirm(_)));
        assert!(buf.bytes.is_empty());
    }

    #[test]
    fn bracketed_paste_is_wrapped() {
        let mut e = StandardEncoder::new();
        e.bracketed_paste = true;
        let mut buf = Buf::default();
        let ev = InputEvent::Paste(crate::input::PastePayload {
            data: b"a\nb\x1b[201~evil".to_vec(),
            origin: PasteOrigin::LocalClipboard,
            bracketed: true,
        });
        e.encode(&ev, &KeyboardMode::Legacy, &mut buf);
        assert!(buf.bytes.starts_with(b"\x1b[200~"));
        assert!(buf.bytes.ends_with(b"\x1b[201~"));
        // The embedded terminator was sanitised away: exactly one remains, ours.
        assert_eq!(
            buf.bytes.windows(6).filter(|w| *w == b"\x1b[201~").count(),
            1
        );
    }

    #[test]
    fn closed_sink_reports_no_lease() {
        let mut e = StandardEncoder::new();
        let mut buf = Buf {
            bytes: Vec::new(),
            closed: true,
        };
        let out = e.encode(
            &key(KeyCode::Char('a'), Modifiers::NONE),
            &KeyboardMode::Legacy,
            &mut buf,
        );
        assert_eq!(out, EncodeOutcome::Dropped(DropReason::NoLease));
    }

    #[test]
    fn osc52_paste_is_denied_by_default() {
        let mut e = StandardEncoder::new();
        let mut buf = Buf::default();
        let ev = InputEvent::Paste(crate::input::PastePayload {
            data: b"a\nb".to_vec(),
            origin: PasteOrigin::Osc52,
            bracketed: false,
        });
        assert_eq!(
            e.encode(&ev, &KeyboardMode::Legacy, &mut buf),
            EncodeOutcome::Dropped(DropReason::PasteBlocked)
        );
    }

    #[test]
    fn focus_events_use_xterm_sequences() {
        let mut e = StandardEncoder::new();
        let mut buf = Buf::default();
        e.encode(&InputEvent::FocusGained, &KeyboardMode::Legacy, &mut buf);
        e.encode(&InputEvent::FocusLost, &KeyboardMode::Legacy, &mut buf);
        assert_eq!(buf.bytes, b"\x1b[I\x1b[O");
    }
}
