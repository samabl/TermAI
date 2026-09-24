//! tools/bench/input-bytes-driver.rs — the H12 bench driver (kernel/05 §5 IN-AC byte equality).
//!
//! This is NOT a second encoder. It links against the real `termai_core` rlib and calls
//! `StandardEncoder` (the single S3 encoder of AR-29 item 3 / kernel/05 K-01) once per corpus
//! case, so every byte it reports was produced by the production encoder. The JS producer
//! `tools/bench/input-bytes.mjs` builds it with:
//!
//!   cargo build --release -p termai-core
//!   rustc --edition 2021 tools/bench/input-bytes-driver.rs \
//!         --extern termai_core=target/release/libtermai_core.rlib -o target/bench-input-bytes/...
//!
//! `termai-core` has no dependencies of its own (crates/termai-core/Cargo.toml), which is what
//! makes that rustc line sufficient; no crate needs to be edited to drive it.
//!
//! Line protocol (one case per line, tab separated; `#` and blank lines are skipped):
//!
//!   <caseId> \t <mode> \t <opts> \t <event>
//!
//!   mode   : legacy | mok:<0..255> | kitty:<bits>
//!            kitty bits are kernel/05 §3.1's KittyFlags order, bit0 first:
//!            disambiguate=1 report_events=2 report_alt_keys=4 report_all=8
//!            report_text=16 report_associated_text=32
//!   opts   : `-` for StandardEncoder::new(), else a comma list of
//!            app_cursor | app_keypad | no_sgr | bracketed | always
//!            (no_sgr mirrors "the application has not enabled SGR mouse reporting",
//!             bracketed mirrors DECSET 2004, always is PastePolicy::Always)
//!   event  : key;<code>;<mods>;<type>;<texthex|->
//!              code = c:<codepoint> | n:<NamedKey> | d:<codepoint>
//!              mods = bitmask shift=1 alt=2 ctrl=4 meta=8 super=16 hyper=32
//!              type = p (Press) | r (Repeat) | l (Release)
//!              texthex = UTF-8 hex of KeyEvent::text, or `-` for None
//!            commit;<utf8hex>
//!            paste;<local|osc52|api|plugin>;<0|1>;<hex>
//!            mouse;<press|release|motion|wheel>;<button>;<col>;<row>;<mods>
//!            focus;<gain|lost>
//!            inject;<hex>
//!
//! Output (written to <out-file>, never parsed from a pipe):
//!
//!   <caseId> \t <hex of the bytes that reached the InputSink> \t <outcome>
//!
//! `outcome` is the real `EncodeOutcome`, printed so a case that is *supposed* to produce zero
//! bytes (dropped / consumed / needs-confirm) can be told apart from an encoder that simply
//! produced nothing: "Emitted(n)" | "Consumed" | "Dropped(NoLease)" |
//! "Dropped(UnsupportedKey)" | "Dropped(PasteBlocked)" | "Dropped(ReadOnlyClient)" |
//! "NeedsConfirm(0)".

use std::env;
use std::fs;
use std::process::ExitCode;

use termai_core::input::{
    EncodeOutcome, InputEncoder, InputError, InputEvent, InputSink, KeyCode, KeyEvent,
    KeyEventType, KeyboardMode, KittyFlags, Modifiers, MouseEvent, MouseKind, NamedKey,
    PasteOrigin, PastePayload, PastePolicy, StandardEncoder,
};

/// Collects everything the encoder emits. `sessiond` owns the real sink behind a lease
/// (kernel/05 §3.1 invariant P-1); this one is the bench stand-in and only records bytes.
#[derive(Default)]
struct Buf {
    bytes: Vec<u8>,
}

impl InputSink for Buf {
    fn write(&mut self, bytes: &[u8]) -> Result<(), InputError> {
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn unhex(text: &str) -> Result<Vec<u8>, String> {
    if text.len() % 2 != 0 {
        return Err(format!("hex string has an odd length: {text:?}"));
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let hi = (bytes[i] as char)
            .to_digit(16)
            .ok_or_else(|| format!("not hex: {text:?}"))?;
        let lo = (bytes[i + 1] as char)
            .to_digit(16)
            .ok_or_else(|| format!("not hex: {text:?}"))?;
        out.push(((hi << 4) | lo) as u8);
        i += 2;
    }
    Ok(out)
}

fn parse_named(name: &str) -> Result<NamedKey, String> {
    let key = match name {
        "Enter" => NamedKey::Enter,
        "Tab" => NamedKey::Tab,
        "Backspace" => NamedKey::Backspace,
        "Escape" => NamedKey::Escape,
        "Space" => NamedKey::Space,
        "Up" => NamedKey::Up,
        "Down" => NamedKey::Down,
        "Left" => NamedKey::Left,
        "Right" => NamedKey::Right,
        "Home" => NamedKey::Home,
        "End" => NamedKey::End,
        "PageUp" => NamedKey::PageUp,
        "PageDown" => NamedKey::PageDown,
        "Insert" => NamedKey::Insert,
        "Delete" => NamedKey::Delete,
        other => {
            if let Some(rest) = other.strip_prefix('F') {
                let n: u8 = rest.parse().map_err(|_| format!("unknown NamedKey {other:?}"))?;
                NamedKey::F(n)
            } else {
                return Err(format!("unknown NamedKey {other:?}"));
            }
        }
    };
    Ok(key)
}

fn parse_mods(value: &str) -> Result<Modifiers, String> {
    let bits: u8 = value.parse().map_err(|_| format!("mods must be 0..63, got {value:?}"))?;
    if bits > 63 {
        return Err(format!("mods out of range: {bits}"));
    }
    Ok(Modifiers {
        shift: bits & 1 != 0,
        alt: bits & 2 != 0,
        ctrl: bits & 4 != 0,
        meta: bits & 8 != 0,
        super_: bits & 16 != 0,
        hyper: bits & 32 != 0,
    })
}

fn parse_origin(value: &str) -> Result<PasteOrigin, String> {
    Ok(match value {
        "local" => PasteOrigin::LocalClipboard,
        "osc52" => PasteOrigin::Osc52,
        "api" => PasteOrigin::ApiInject,
        "plugin" => PasteOrigin::PluginInject,
        other => return Err(format!("unknown paste origin {other:?}")),
    })
}

fn parse_kind(value: &str) -> Result<MouseKind, String> {
    Ok(match value {
        "press" => MouseKind::Press,
        "release" => MouseKind::Release,
        "motion" => MouseKind::Motion,
        "wheel" => MouseKind::Wheel,
        other => return Err(format!("unknown mouse kind {other:?}")),
    })
}

fn parse_mode(value: &str) -> Result<KeyboardMode, String> {
    if value == "legacy" {
        return Ok(KeyboardMode::Legacy);
    }
    if let Some(rest) = value.strip_prefix("mok:") {
        let level: u8 = rest.parse().map_err(|_| format!("bad mok level {rest:?}"))?;
        return Ok(KeyboardMode::ModifyOtherKeys(level));
    }
    if let Some(rest) = value.strip_prefix("kitty:") {
        let bits: u8 = rest.parse().map_err(|_| format!("bad kitty flag bits {rest:?}"))?;
        return Ok(KeyboardMode::Kitty(KittyFlags {
            disambiguate: bits & 1 != 0,
            report_events: bits & 2 != 0,
            report_alt_keys: bits & 4 != 0,
            report_all: bits & 8 != 0,
            report_text: bits & 16 != 0,
            report_associated_text: bits & 32 != 0,
        }));
    }
    Err(format!("unknown keyboard mode {value:?}"))
}

fn parse_event(value: &str) -> Result<InputEvent, String> {
    let f: Vec<&str> = value.split(';').collect();
    match f.first().copied() {
        Some("key") => {
            if f.len() != 5 {
                return Err(format!("key needs 5 fields, got {}", f.len()));
            }
            let code = if let Some(cp) = f[1].strip_prefix("c:") {
                let cp: u32 = cp.parse().map_err(|_| format!("bad codepoint {cp:?}"))?;
                KeyCode::Char(char::from_u32(cp).ok_or_else(|| format!("not a char: {cp}"))?)
            } else if let Some(name) = f[1].strip_prefix("n:") {
                KeyCode::Named(parse_named(name)?)
            } else if let Some(cp) = f[1].strip_prefix("d:") {
                KeyCode::Dead(cp.parse().map_err(|_| format!("bad dead-key codepoint {cp:?}"))?)
            } else {
                return Err(format!("bad key code {:?}", f[1]));
            };
            let mods = parse_mods(f[2])?;
            let ty = match f[3] {
                "p" => KeyEventType::Press,
                "r" => KeyEventType::Repeat,
                "l" => KeyEventType::Release,
                other => return Err(format!("bad key event type {other:?}")),
            };
            let text = if f[4] == "-" {
                None
            } else {
                Some(String::from_utf8(unhex(f[4])?).map_err(|e| format!("text is not UTF-8: {e}"))?)
            };
            Ok(InputEvent::Key(KeyEvent {
                code,
                mods,
                ty,
                text,
                scancode: 0,
            }))
        }
        Some("commit") => {
            if f.len() != 2 {
                return Err(format!("commit needs 2 fields, got {}", f.len()));
            }
            let text = String::from_utf8(unhex(f[1])?).map_err(|e| format!("commit is not UTF-8: {e}"))?;
            Ok(InputEvent::TextCommit(text))
        }
        Some("paste") => {
            if f.len() != 4 {
                return Err(format!("paste needs 4 fields, got {}", f.len()));
            }
            Ok(InputEvent::Paste(PastePayload {
                origin: parse_origin(f[1])?,
                bracketed: match f[2] {
                    "0" => false,
                    "1" => true,
                    other => return Err(format!("paste bracketed must be 0|1, got {other:?}")),
                },
                data: unhex(f[3])?,
            }))
        }
        Some("mouse") => {
            if f.len() != 6 {
                return Err(format!("mouse needs 6 fields, got {}", f.len()));
            }
            Ok(InputEvent::Mouse(MouseEvent {
                kind: parse_kind(f[1])?,
                button: f[2].parse().map_err(|_| format!("bad button {:?}", f[2]))?,
                col: f[3].parse().map_err(|_| format!("bad col {:?}", f[3]))?,
                row: f[4].parse().map_err(|_| format!("bad row {:?}", f[4]))?,
                mods: parse_mods(f[5])?,
            }))
        }
        Some("focus") => match f.get(1).copied() {
            Some("gain") => Ok(InputEvent::FocusGained),
            Some("lost") => Ok(InputEvent::FocusLost),
            other => Err(format!("focus needs gain|lost, got {other:?}")),
        },
        Some("inject") => {
            if f.len() != 2 {
                return Err(format!("inject needs 2 fields, got {}", f.len()));
            }
            Ok(InputEvent::ApiInject(unhex(f[1])?))
        }
        other => Err(format!("unknown event kind {other:?}")),
    }
}

fn fmt_outcome(outcome: EncodeOutcome) -> String {
    match outcome {
        EncodeOutcome::Emitted(n) => format!("Emitted({n})"),
        EncodeOutcome::Consumed => "Consumed".to_string(),
        EncodeOutcome::Dropped(reason) => format!("Dropped({reason:?})"),
        EncodeOutcome::NeedsConfirm(id) => format!("NeedsConfirm({})", id.0),
    }
}

fn run_case(line: &str) -> Result<String, String> {
    let parts: Vec<&str> = line.split('\t').collect();
    if parts.len() != 4 {
        return Err(format!("expected 4 tab-separated fields, got {}", parts.len()));
    }
    let mut enc = StandardEncoder::new();
    for token in parts[2].split(',') {
        match token {
            "" | "-" => {}
            "app_cursor" => enc.app_cursor = true,
            "app_keypad" => enc.app_keypad = true,
            "no_sgr" => enc.mouse_sgr = false,
            "bracketed" => enc.bracketed_paste = true,
            "always" => enc.paste_policy = PastePolicy::Always,
            other => return Err(format!("unknown opt {other:?}")),
        }
    }
    let mode = parse_mode(parts[1])?;
    let event = parse_event(parts[3])?;
    let mut sink = Buf::default();
    let outcome = enc.encode(&event, &mode, &mut sink);
    Ok(format!(
        "{}\t{}\t{}",
        parts[0],
        hex(&sink.bytes),
        fmt_outcome(outcome)
    ))
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: input-bytes-driver <cases-file> <out-file>");
        return ExitCode::from(2);
    }
    let text = match fs::read_to_string(&args[1]) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("input-bytes-driver: cannot read {}: {err}", args[1]);
            return ExitCode::from(2);
        }
    };
    let mut out = String::new();
    let mut cases = 0usize;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match run_case(line) {
            Ok(record) => {
                out.push_str(&record);
                out.push('\n');
                cases += 1;
            }
            Err(err) => {
                eprintln!("input-bytes-driver: {err}\n  case line: {line}");
                return ExitCode::from(2);
            }
        }
    }
    if let Err(err) = fs::write(&args[2], out) {
        eprintln!("input-bytes-driver: cannot write {}: {err}", args[2]);
        return ExitCode::from(2);
    }
    println!("cases={cases}");
    ExitCode::SUCCESS
}
