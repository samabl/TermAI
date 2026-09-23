//! termai-vt public parsing contract (kernel/01 section 3.2).
//!
//! AR-18: the ESC state machine is provided by the vte crate, but it lives strictly
//! *behind* the trait boundary defined here. No vte type appears in any public
//! signature of this crate (kernel/01 section 3.2, K-07). The concrete adapter is
//! private to vte_adapter.rs.

#![allow(clippy::module_name_repetitions)]

/// Default OSC payload limit (bytes). Conservative M0 value; the divergence from
/// kernel/01 OQ-VT-01 (1 MiB) is registered in docs/plan/m0-spec-defects.md SD-08.1.
pub const OSC_LEN_LIMIT_DEFAULT: u32 = 64 * 1024;
/// Default DCS/APC payload limit (bytes). SOS/PM/APC strings share this cap.
/// Conservative M0 value; see docs/plan/m0-spec-defects.md SD-08.1.
pub const DCS_LEN_LIMIT_DEFAULT: u32 = 1024 * 1024;
/// Maximum number of parameters/subparameters carried by Params.
pub const MAX_PARAMS: usize = 16;

/// A fixed-capacity parameter list. Owns a [u16; MAX_PARAMS] and a length; no heap
/// allocation and no vte type inside, so it can appear in public signatures.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Params {
    data: [u16; MAX_PARAMS],
    len: usize,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            data: [0; MAX_PARAMS],
            len: 0,
        }
    }
}

impl Params {
    /// Create an empty parameter list.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append one value, ignoring overflow beyond MAX_PARAMS.
    pub(crate) fn push(&mut self, value: u16) {
        if self.len < MAX_PARAMS {
            self.data[self.len] = value;
            self.len += 1;
        }
    }

    /// Borrow the parameters as a slice.
    #[must_use]
    pub fn as_slice(&self) -> &[u16] {
        &self.data[..self.len]
    }

    /// Number of parameters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Parameter at i, or 0 when missing (VT default-value convention).
    #[must_use]
    pub fn get(&self, i: usize) -> u16 {
        self.data.get(i).copied().unwrap_or(0)
    }

    /// Iterate the parameters in order.
    pub fn iter(&self) -> impl Iterator<Item = u16> + '_ {
        self.as_slice().iter().copied()
    }
}

/// Parser state reported with ParseError. Mirrors kernel/01 section 3.2.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ParserState {
    Ground,
    Escape,
    CsiEntry,
    CsiParam,
    CsiIntermediate,
    CsiIgnore,
    OscString,
    DcsEntry,
    DcsPassthrough,
    SosPmApc,
}

impl ParserState {
    /// Stable key for diagnostics.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            ParserState::Ground => "ground",
            ParserState::Escape => "escape",
            ParserState::CsiEntry => "csi_entry",
            ParserState::CsiParam => "csi_param",
            ParserState::CsiIntermediate => "csi_intermediate",
            ParserState::CsiIgnore => "csi_ignore",
            ParserState::OscString => "osc_string",
            ParserState::DcsEntry => "dcs_entry",
            ParserState::DcsPassthrough => "dcs_passthrough",
            ParserState::SosPmApc => "sos_pm_apc",
        }
    }
}

/// The 14 counter keys of kernel/01 section 3.4.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ParseErrorKind {
    CsiUnknown,
    CsiMalformed,
    EscUnknown,
    OscUnknown,
    OscAborted,
    OscOverflow,
    DcsUnknown,
    DcsOverflow,
    DcsBelInData,
    StringCancelled,
    StStray,
    C1EightBitInUtf8,
    C1EightBitUsed,
    InvalidUtf8,
}

impl ParseErrorKind {
    /// All keys, in declaration order.
    pub const ALL: [ParseErrorKind; 14] = [
        ParseErrorKind::CsiUnknown,
        ParseErrorKind::CsiMalformed,
        ParseErrorKind::EscUnknown,
        ParseErrorKind::OscUnknown,
        ParseErrorKind::OscAborted,
        ParseErrorKind::OscOverflow,
        ParseErrorKind::DcsUnknown,
        ParseErrorKind::DcsOverflow,
        ParseErrorKind::DcsBelInData,
        ParseErrorKind::StringCancelled,
        ParseErrorKind::StStray,
        ParseErrorKind::C1EightBitInUtf8,
        ParseErrorKind::C1EightBitUsed,
        ParseErrorKind::InvalidUtf8,
    ];

    /// The exact counter key of kernel/01 section 3.4.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            ParseErrorKind::CsiUnknown => "csi_unknown",
            ParseErrorKind::CsiMalformed => "csi_malformed",
            ParseErrorKind::EscUnknown => "esc_unknown",
            ParseErrorKind::OscUnknown => "osc_unknown",
            ParseErrorKind::OscAborted => "osc_aborted",
            ParseErrorKind::OscOverflow => "osc_overflow",
            ParseErrorKind::DcsUnknown => "dcs_unknown",
            ParseErrorKind::DcsOverflow => "dcs_overflow",
            ParseErrorKind::DcsBelInData => "dcs_bel_in_data",
            ParseErrorKind::StringCancelled => "string_cancelled",
            ParseErrorKind::StStray => "st_stray",
            ParseErrorKind::C1EightBitInUtf8 => "c1_8bit_in_utf8",
            ParseErrorKind::C1EightBitUsed => "c1_8bit_used",
            ParseErrorKind::InvalidUtf8 => "invalid_utf8",
        }
    }

    /// Index used by the counter block.
    #[must_use]
    pub(crate) const fn index(self) -> usize {
        match self {
            ParseErrorKind::CsiUnknown => 0,
            ParseErrorKind::CsiMalformed => 1,
            ParseErrorKind::EscUnknown => 2,
            ParseErrorKind::OscUnknown => 3,
            ParseErrorKind::OscAborted => 4,
            ParseErrorKind::OscOverflow => 5,
            ParseErrorKind::DcsUnknown => 6,
            ParseErrorKind::DcsOverflow => 7,
            ParseErrorKind::DcsBelInData => 8,
            ParseErrorKind::StringCancelled => 9,
            ParseErrorKind::StStray => 10,
            ParseErrorKind::C1EightBitInUtf8 => 11,
            ParseErrorKind::C1EightBitUsed => 12,
            ParseErrorKind::InvalidUtf8 => 13,
        }
    }
}

/// First unrecognised / malformed sequence seen in an AdvanceReport.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ParseError {
    /// Byte offset inside the slice passed to the offending advance call.
    pub offset: u32,
    /// Parser state in which the error was classified.
    pub state: ParserState,
    /// Counter key.
    pub kind: ParseErrorKind,
    /// Final byte that triggered the classification.
    pub final_byte: u8,
}

/// Backend capability declaration (kernel/01 section 3.2). Capability differences
/// must be explicit, never silent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BackendCaps {
    /// Whether raw 8-bit C1 bytes are recognised as controls (UTF-8 mode = false).
    pub eight_bit_c1: bool,
    /// Whether the backend natively reports byte offsets in ParseError.
    pub byte_offsets: bool,
    /// OSC payload cap in bytes.
    pub osc_len_limit: u32,
    /// DCS/APC/SOS/PM payload cap in bytes.
    pub dcs_len_limit: u32,
}

impl Default for BackendCaps {
    fn default() -> Self {
        Self {
            eight_bit_c1: false,
            byte_offsets: false,
            osc_len_limit: OSC_LEN_LIMIT_DEFAULT,
            dcs_len_limit: DCS_LEN_LIMIT_DEFAULT,
        }
    }
}

/// Backend identity. Kept minimal so a clean-room parser can replace vte behind the
/// same trait without touching callers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BackendId {
    /// The vte-backed parser (AR-18 phase 0-2).
    Vte {
        /// Exact pinned version.
        version: &'static str,
    },
    /// A future clean-room parser revision.
    CleanRoom {
        /// Parser revision.
        rev: u32,
    },
}

impl BackendId {
    /// Stable label used in golden snapshots (for example vte-0.15).
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            BackendId::Vte { version } => format!("vte-{version}"),
            BackendId::CleanRoom { rev } => format!("cleanroom-r{rev}"),
        }
    }
}

/// How a string control (OSC/DCS/SOS/PM/APC) ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StringTerm {
    /// BEL (0x07).
    Bel,
    /// String terminator ESC backslash.
    St,
    /// ESC followed by a byte other than backslash: the string is aborted and the ESC
    /// is processed as the start of a new escape sequence.
    Aborted,
    /// Payload exceeded the configured length limit.
    Overflow,
    /// CAN (0x18) or SUB (0x1A).
    Cancelled,
}

/// Which of the SOS / PM / APC controls a string belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StringKind {
    Sos,
    Pm,
    Apc,
}

/// Result of one VtBackend::advance call.
#[derive(Clone, Debug)]
pub struct AdvanceReport {
    /// Bytes consumed (always the full slice for this backend).
    pub consumed: usize,
    /// First unrecognised / malformed sequence seen during this call.
    pub first_error: Option<ParseError>,
    /// Counters accumulated during this call (a delta, not a running total).
    pub counters: crate::counters::VtCounters,
}

/// Consumer of decoded VT actions. This is the only extension point; the live grid
/// lives in grid.rs and is reached through terminal.rs.
pub trait EscapeSink {
    /// A printable character (including U+FFFD replacements).
    fn print(&mut self, ch: char);
    /// Execute a C0/C1 control.
    fn execute(&mut self, byte: u8);
    /// ESC dispatch.
    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8);
    /// CSI dispatch.
    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8);
    /// OSC dispatch.
    fn osc_dispatch(&mut self, params: &[&[u8]], term: StringTerm);
    /// DCS hook (start of a device control string).
    fn dcs_hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8);
    /// DCS payload byte.
    fn dcs_put(&mut self, byte: u8);
    /// DCS end.
    fn dcs_unhook(&mut self, term: StringTerm);
    /// SOS / PM / APC dispatch.
    fn sos_pm_apc(&mut self, kind: StringKind, params: &[&[u8]], term: StringTerm);
}

/// The one and only parsing contract exposed by this crate (AR-18).
pub trait VtBackend: Send {
    /// Feed bytes to the parser, dispatching actions to the sink.
    fn advance(&mut self, bytes: &[u8], sink: &mut dyn EscapeSink) -> AdvanceReport;
    /// Reset all parser state.
    fn reset(&mut self);
    /// Backend identity.
    fn id(&self) -> BackendId;
    /// Backend capabilities.
    fn caps(&self) -> BackendCaps;
}
