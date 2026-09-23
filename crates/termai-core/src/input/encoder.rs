//! Stage S3 encoder: Legacy / modifyOtherKeys / kitty keyboard protocols.
//!
//! This is the single producer of PTY bytes (AR-29.3). It never touches the PTY,
//! never reads the clipboard, and never performs I/O.

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

fn tilde(code: u8, m: Option<u8>) -> Vec<u8> {
    match m {
        Some(v) => format!("\x1b[{code};{v}~").into_bytes(),
        None => format!("\x1b[{code}~").into_bytes(),
    }
}

fn cursor_like(final_byte: u8, m: Option<u8>, app_cursor: bool) -> Vec<u8> {
    match m {
        Some(v) => format!("\x1b[1;{v}{}", final_byte as char).into_bytes(),
        None if app_cursor => vec![0x1B, b'O', final_byte],
        None => vec![0x1B, b'[', final_byte],
    }
}

fn legacy_named(key: NamedKey, mods: Modifiers, app_cursor: bool) -> Option<Vec<u8>> {
    let m = mods.xterm_param();
    Some(match key {
        NamedKey::Enter => b"\r".to_vec(),
        NamedKey::Tab => {
            if mods.shift {
                b"\x1b[Z".to_vec()
            } else {
                b"\t".to_vec()
            }
        }
        NamedKey::Backspace => vec![0x7F],
        NamedKey::Escape => vec![0x1B],
        NamedKey::Space => b" ".to_vec(),
        NamedKey::Up => cursor_like(b'A', m, app_cursor),
        NamedKey::Down => cursor_like(b'B', m, app_cursor),
        NamedKey::Right => cursor_like(b'C', m, app_cursor),
        NamedKey::Left => cursor_like(b'D', m, app_cursor),
        NamedKey::Home => cursor_like(b'H', m, app_cursor),
        NamedKey::End => cursor_like(b'F', m, app_cursor),
        NamedKey::Insert => tilde(2, m),
        NamedKey::Delete => tilde(3, m),
        NamedKey::PageUp => tilde(5, m),
        NamedKey::PageDown => tilde(6, m),
        NamedKey::F(n) => match n {
            1..=4 => {
                let f = b"PQRS"[usize::from(n - 1)];
                match m {
                    Some(v) => format!("\x1b[1;{v}{}", f as char).into_bytes(),
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
                tilde(code, m)
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
        KeyCode::Named(n) => legacy_named(n, ev.mods, app_cursor),
        KeyCode::Char(c) => {
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
            if mok_level >= 2 && (ev.mods.ctrl || ev.mods.alt) {
                let cp = u32::from(c);
                let m = u32::from(ev.mods.xterm_param().unwrap_or(1));
                return Some(format!("\x1b[27;{m};{cp}~").into_bytes());
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

fn kitty_key(ev: &KeyEvent, flags: &super::KittyFlags) -> Option<Vec<u8>> {
    if !flags.report_events && matches!(ev.ty, KeyEventType::Release | KeyEventType::Repeat) {
        return None;
    }
    let code = match ev.code {
        KeyCode::Char(c) => u32::from(c),
        KeyCode::Named(n) => n.codepoint(),
        KeyCode::Dead(_) => return None,
    };
    let m = ev.mods.kitty_param();
    let event_type = match ev.ty {
        KeyEventType::Press => 1,
        KeyEventType::Repeat => 2,
        KeyEventType::Release => 3,
    };
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
        assert_eq!(
            enc(&key(KeyCode::Named(NamedKey::Up), Modifiers::NONE), &mode),
            b"\x1b[57352u"
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
