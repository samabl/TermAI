//! Input contract (kernel/05). Stage S3 is the ONLY stage allowed to produce
//! PTY bytes (AR-29.3, invariant P-1). Leaf contract: no I/O, no PTY, no network.

pub mod encoder;
pub mod paste;

pub use encoder::StandardEncoder;
pub use paste::{paste_gate, sanitize_paste, PasteCtx, PasteDecision, PastePolicy};

/// Named (non-text) keys.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum NamedKey {
    Enter,
    Tab,
    Backspace,
    Escape,
    Space,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    /// F1..F24.
    F(u8),
}

impl NamedKey {
    /// Kitty functional key codepoints (kitty keyboard protocol).
    #[must_use]
    pub const fn codepoint(self) -> u32 {
        match self {
            NamedKey::Enter => 13,
            NamedKey::Tab => 9,
            NamedKey::Backspace => 127,
            NamedKey::Escape => 27,
            NamedKey::Space => 32,
            NamedKey::Up => 57352,
            NamedKey::Down => 57353,
            NamedKey::Left => 57354,
            NamedKey::Right => 57355,
            NamedKey::PageUp => 57356,
            NamedKey::PageDown => 57357,
            NamedKey::Home => 57358,
            NamedKey::End => 57359,
            NamedKey::Insert => 57360,
            NamedKey::Delete => 57361,
            NamedKey::F(n) => 57363 + n as u32,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            NamedKey::Enter => "Enter",
            NamedKey::Tab => "Tab",
            NamedKey::Backspace => "Backspace",
            NamedKey::Escape => "Escape",
            NamedKey::Space => "Space",
            NamedKey::Up => "Up",
            NamedKey::Down => "Down",
            NamedKey::Left => "Left",
            NamedKey::Right => "Right",
            NamedKey::Home => "Home",
            NamedKey::End => "End",
            NamedKey::PageUp => "PageUp",
            NamedKey::PageDown => "PageDown",
            NamedKey::Insert => "Insert",
            NamedKey::Delete => "Delete",
            NamedKey::F(_) => "F",
        }
    }
}

/// Modifier set. Mod abstract: Cmd on macOS, Ctrl+Shift on Windows/Linux (AR-29.2/29.7).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub meta: bool,
    pub super_: bool,
    pub hyper: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers {
        shift: false,
        alt: false,
        ctrl: false,
        meta: false,
        super_: false,
        hyper: false,
    };

    #[must_use]
    pub const fn is_plain(self) -> bool {
        !self.shift && !self.alt && !self.ctrl && !self.meta && !self.super_ && !self.hyper
    }

    /// xterm modifier parameter: 1 + shift(1) + alt(2) + ctrl(4) + meta(8). None when 1.
    #[must_use]
    pub const fn xterm_param(self) -> Option<u8> {
        let v = 1
            + (self.shift as u8)
            + ((self.alt as u8) << 1)
            + ((self.ctrl as u8) << 2)
            + ((self.meta as u8) << 3);
        if v == 1 {
            None
        } else {
            Some(v)
        }
    }

    /// kitty modifier parameter: 1 + bits (shift=1, alt=2, ctrl=4, super=8, hyper=16, meta=32).
    #[must_use]
    pub const fn kitty_param(self) -> u8 {
        1 + (self.shift as u8)
            + ((self.alt as u8) << 1)
            + ((self.ctrl as u8) << 2)
            + ((self.super_ as u8) << 3)
            + ((self.hyper as u8) << 4)
            + ((self.meta as u8) << 5)
    }

    /// Modifiers that xterm-style encoding cannot express.
    #[must_use]
    pub const fn unsupported_for_legacy(self) -> bool {
        self.super_ || self.hyper
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum KeyCode {
    Char(char),
    Named(NamedKey),
    Dead(u32),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum KeyEventType {
    Press,
    Repeat,
    Release,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub mods: Modifiers,
    pub ty: KeyEventType,
    /// Associated text produced by the platform (IME commit / dead-key composition).
    pub text: Option<String>,
    pub scancode: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct KittyFlags {
    pub disambiguate: bool,
    pub report_events: bool,
    pub report_alt_keys: bool,
    pub report_all: bool,
    pub report_text: bool,
    pub report_associated_text: bool,
}

/// Keyboard mode. v1 default is Legacy (OQ-06: kitty is opt-in per application).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyboardMode {
    Legacy,
    ModifyOtherKeys(u8),
    Kitty(KittyFlags),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MouseKind {
    Press,
    Release,
    Motion,
    Wheel,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MouseEvent {
    pub kind: MouseKind,
    pub button: u8,
    pub col: u16,
    pub row: u16,
    pub mods: Modifiers,
}

/// Where a paste came from. S5 uses this for friction level and audit (invariant P-2).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PasteOrigin {
    LocalClipboard,
    Osc52,
    ApiInject,
    PluginInject,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PastePayload {
    pub data: Vec<u8>,
    pub origin: PasteOrigin,
    /// Whether the application enabled bracketed paste (ESC 200~).
    pub bracketed: bool,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum InputEvent {
    Key(KeyEvent),
    /// IME commit or composed text. Preedit never appears here (IN-AC-05).
    TextCommit(String),
    Paste(PastePayload),
    Mouse(MouseEvent),
    FocusGained,
    FocusLost,
    /// API/plugin injection: bytes still produced by the encoder (AR-29.3).
    ApiInject(Vec<u8>),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputError {
    SinkClosed,
    TooLarge,
    Denied,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DropReason {
    NoLease,
    UnsupportedKey,
    PasteBlocked,
    ReadOnlyClient,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ConfirmId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EncodeOutcome {
    Emitted(usize),
    Consumed,
    Dropped(DropReason),
    NeedsConfirm(ConfirmId),
}

/// Only sessiond implements this, and only inside a held lease (invariant P-1).
pub trait InputSink {
    fn write(&mut self, bytes: &[u8]) -> Result<(), InputError>;
}

/// Stage S3: the single encoder. Any other module writing to the PTY is a defect.
pub trait InputEncoder {
    fn encode(
        &mut self,
        ev: &InputEvent,
        mode: &KeyboardMode,
        out: &mut dyn InputSink,
    ) -> EncodeOutcome;
}
