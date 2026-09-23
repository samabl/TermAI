//! vte-backed VtBackend (AR-18) plus a private pre-scanner.
//!
//! AR-18 keeps vte behind the trait boundary: this module is the only place that
//! names a vte type, and every public item here is a plain termai-vt type.
//!
//! The pre-scanner mirrors vte's byte-level state machine for the cases that vte
//! does not surface through Perform:
//!   * SOS / PM / APC strings (vte silently swallows them),
//!   * OSC and DCS length limits (Overflow),
//!   * the csi_malformed classification for illegal bytes that drive vte into
//!     CsiIgnore (where vte dispatches nothing).
//!
//! Every byte is still fed to vte, so the two state machines stay in lockstep.

use crate::backend::{
    AdvanceReport, BackendCaps, BackendId, EscapeSink, Params, ParseError, ParseErrorKind,
    ParserState, StringKind, StringTerm, VtBackend, DCS_LEN_LIMIT_DEFAULT, MAX_PARAMS,
    OSC_LEN_LIMIT_DEFAULT,
};
use crate::counters::VtCounters;

/// vte-backed parser. The only vte usage in the crate.
pub struct VteBackend {
    parser: vte::Parser,
    pre: PreState,
    eight_bit_c1: bool,
    osc_limit: u32,
    dcs_limit: u32,
}

impl Default for VteBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl VteBackend {
    /// Create the default (UTF-8, default limits) vte backend.
    #[must_use]
    pub fn new() -> Self {
        Self {
            parser: vte::Parser::new(),
            pre: PreState::default(),
            eight_bit_c1: false,
            osc_limit: OSC_LEN_LIMIT_DEFAULT,
            dcs_limit: DCS_LEN_LIMIT_DEFAULT,
        }
    }

    /// Override the payload limits (used by tests and by capability negotiation).
    #[must_use]
    pub fn with_limits(mut self, osc_len_limit: u32, dcs_len_limit: u32) -> Self {
        self.osc_limit = osc_len_limit;
        self.dcs_limit = dcs_len_limit;
        self
    }

    /// Select whether raw 8-bit C1 bytes are treated as controls. OQ-VT-14 keeps the
    /// default false (UTF-8 mode).
    #[must_use]
    pub fn with_eight_bit_c1(mut self, enabled: bool) -> Self {
        self.eight_bit_c1 = enabled;
        self
    }
}

impl VtBackend for VteBackend {
    fn advance(&mut self, bytes: &[u8], sink: &mut dyn EscapeSink) -> AdvanceReport {
        let mut ctx = AdvCtx::default();
        let osc_limit = self.osc_limit;
        let dcs_limit = self.dcs_limit;
        for (i, &byte) in bytes.iter().enumerate() {
            ctx.current_offset = i as u32;
            let step = self.pre.step(byte, osc_limit, dcs_limit, &mut ctx);
            if let Some(emit) = step.emit {
                apply_emit(emit, &mut *sink, &mut ctx);
            }
            if step.reset_parser {
                self.parser = vte::Parser::new();
            }
            if step.feed {
                let mut adapter = VteAdapter {
                    sink: &mut *sink,
                    ctx: &mut ctx,
                    pre: &mut self.pre,
                    eight_bit_c1: self.eight_bit_c1,
                };
                self.parser.advance(&mut adapter, &[byte]);
            }
            ctx.suppress_st = false;
            ctx.suppress_unhook = false;
        }
        AdvanceReport {
            consumed: bytes.len(),
            first_error: ctx.first_error,
            counters: ctx.counters,
        }
    }

    fn reset(&mut self) {
        self.parser = vte::Parser::new();
        self.pre = PreState::default();
    }

    fn id(&self) -> BackendId {
        BackendId::Vte { version: "0.15" }
    }

    fn caps(&self) -> BackendCaps {
        BackendCaps {
            eight_bit_c1: self.eight_bit_c1,
            byte_offsets: false,
            osc_len_limit: self.osc_limit,
            dcs_len_limit: self.dcs_limit,
        }
    }
}

// ---------------------------------------------------------------------------
// Pre-scanner
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum StrKind {
    Osc,
    Dcs,
    Sos,
    Pm,
    Apc,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CsiPhase {
    Entry,
    Param,
    Intermediate,
    Ignore,
}

struct StringCtx {
    kind: StrKind,
    len: u32,
    buf: Vec<u8>,
    esc_seen: bool,
}

impl StringCtx {
    fn new(kind: StrKind) -> Self {
        Self {
            kind,
            len: 0,
            buf: Vec::new(),
            esc_seen: false,
        }
    }
}

#[derive(Default)]
struct PreState {
    prev_esc: bool,
    string: Option<StringCtx>,
    csi: Option<CsiPhase>,
    dcs_active: bool,
}

#[derive(Default)]
struct AdvCtx {
    counters: VtCounters,
    first_error: Option<ParseError>,
    suppress_st: bool,
    suppress_unhook: bool,
    current_offset: u32,
    seq_start: u32,
}

struct PreStep {
    feed: bool,
    reset_parser: bool,
    emit: Option<Emit>,
}

impl PreStep {
    fn feed() -> Self {
        Self {
            feed: true,
            reset_parser: false,
            emit: None,
        }
    }
}

enum Emit {
    Osc {
        params: Vec<Vec<u8>>,
        term: StringTerm,
    },
    SosPm {
        kind: StringKind,
        params: Vec<Vec<u8>>,
        term: StringTerm,
    },
    DcsUnhook {
        term: StringTerm,
    },
}

impl PreState {
    fn step(&mut self, byte: u8, osc_limit: u32, dcs_limit: u32, ctx: &mut AdvCtx) -> PreStep {
        if self.string.is_some() {
            return self.string_step(byte, osc_limit, dcs_limit, ctx);
        }
        if self.csi.is_some() {
            return self.csi_step(byte, ctx);
        }
        self.ground_step(byte, ctx)
    }

    fn ground_step(&mut self, byte: u8, ctx: &mut AdvCtx) -> PreStep {
        if self.prev_esc {
            self.prev_esc = false;
            match byte {
                b'[' => {
                    self.csi = Some(CsiPhase::Entry);
                    ctx.seq_start = ctx.current_offset.saturating_sub(1);
                }
                b']' => {
                    self.string = Some(StringCtx::new(StrKind::Osc));
                    ctx.seq_start = ctx.current_offset.saturating_sub(1);
                }
                b'P' => {
                    self.string = Some(StringCtx::new(StrKind::Dcs));
                    ctx.seq_start = ctx.current_offset.saturating_sub(1);
                }
                b'X' => {
                    self.string = Some(StringCtx::new(StrKind::Sos));
                    ctx.seq_start = ctx.current_offset.saturating_sub(1);
                }
                b'^' => {
                    self.string = Some(StringCtx::new(StrKind::Pm));
                    ctx.seq_start = ctx.current_offset.saturating_sub(1);
                }
                b'_' => {
                    self.string = Some(StringCtx::new(StrKind::Apc));
                    ctx.seq_start = ctx.current_offset.saturating_sub(1);
                }
                _ => {}
            }
            if byte == 0x1B {
                self.prev_esc = true;
                ctx.seq_start = ctx.current_offset;
            }
        } else if byte == 0x1B {
            self.prev_esc = true;
            ctx.seq_start = ctx.current_offset;
        }
        PreStep::feed()
    }

    #[allow(clippy::too_many_lines)]
    fn csi_step(&mut self, byte: u8, ctx: &mut AdvCtx) -> PreStep {
        match byte {
            0x00..=0x17 | 0x19 | 0x1C..=0x1F => return PreStep::feed(),
            0x1B => {
                self.csi = None;
                self.prev_esc = true;
                ctx.seq_start = ctx.current_offset;
                return PreStep::feed();
            }
            0x18 | 0x1A => {
                self.csi = None;
                return PreStep::feed();
            }
            _ => {}
        }
        let phase = match self.csi {
            Some(phase) => phase,
            None => return PreStep::feed(),
        };
        match phase {
            CsiPhase::Entry => match byte {
                0x20..=0x2F => self.csi = Some(CsiPhase::Intermediate),
                0x30..=0x3F => self.csi = Some(CsiPhase::Param),
                0x40..=0x7E => self.csi = None,
                _ => {}
            },
            CsiPhase::Param => match byte {
                0x20..=0x2F => self.csi = Some(CsiPhase::Intermediate),
                0x30..=0x3B => self.csi = Some(CsiPhase::Param),
                0x3C..=0x3F => {
                    self.csi = Some(CsiPhase::Ignore);
                    ctx.counters.bump(ParseErrorKind::CsiMalformed);
                    record(
                        ctx,
                        ParseErrorKind::CsiMalformed,
                        ParserState::CsiParam,
                        byte,
                        ctx.seq_start,
                    );
                }
                0x40..=0x7E => self.csi = None,
                _ => {}
            },
            CsiPhase::Intermediate => match byte {
                0x20..=0x2F => self.csi = Some(CsiPhase::Intermediate),
                0x30..=0x3F => {
                    self.csi = Some(CsiPhase::Ignore);
                    ctx.counters.bump(ParseErrorKind::CsiMalformed);
                    record(
                        ctx,
                        ParseErrorKind::CsiMalformed,
                        ParserState::CsiIntermediate,
                        byte,
                        ctx.seq_start,
                    );
                }
                0x40..=0x7E => self.csi = None,
                _ => {}
            },
            CsiPhase::Ignore => match byte {
                0x20..=0x3F => {}
                0x40..=0x7E => self.csi = None,
                _ => {}
            },
        }
        PreStep::feed()
    }

    fn string_step(
        &mut self,
        byte: u8,
        osc_limit: u32,
        dcs_limit: u32,
        ctx: &mut AdvCtx,
    ) -> PreStep {
        let kind = match self.string.as_ref() {
            Some(st) => st.kind,
            None => return self.ground_step(byte, ctx),
        };
        let esc_seen = self.string.as_ref().map(|st| st.esc_seen).unwrap_or(false);
        if esc_seen {
            if let Some(st) = self.string.as_mut() {
                st.esc_seen = false;
            }
            if byte == b'\\' {
                let emit = self.finish_string(StringTerm::St);
                ctx.suppress_st = true;
                return PreStep {
                    feed: true,
                    reset_parser: false,
                    emit,
                };
            }
            let emit = self.finish_string(StringTerm::Aborted);
            self.prev_esc = true;
            let mut ground = self.ground_step(byte, ctx);
            if ground.emit.is_none() {
                ground.emit = emit;
            }
            return ground;
        }
        match byte {
            0x18 | 0x1A => {
                if kind == StrKind::Dcs {
                    ctx.suppress_unhook = true;
                }
                let emit = self.finish_string(StringTerm::Cancelled);
                return PreStep {
                    feed: true,
                    reset_parser: false,
                    emit,
                };
            }
            0x1B => {
                if kind == StrKind::Dcs {
                    ctx.suppress_unhook = true;
                }
                if let Some(st) = self.string.as_mut() {
                    st.esc_seen = true;
                }
                return PreStep::feed();
            }
            _ => {}
        }
        if kind == StrKind::Osc && byte == 0x07 {
            let emit = self.finish_string(StringTerm::Bel);
            return PreStep {
                feed: true,
                reset_parser: false,
                emit,
            };
        }
        if kind == StrKind::Dcs && byte == 0x9C {
            ctx.suppress_unhook = true;
            let emit = self.finish_string(StringTerm::St);
            return PreStep {
                feed: true,
                reset_parser: false,
                emit,
            };
        }
        if kind == StrKind::Dcs && byte == 0x07 {
            ctx.counters.bump(ParseErrorKind::DcsBelInData);
        }
        let limit = if kind == StrKind::Osc {
            osc_limit
        } else {
            dcs_limit
        };
        let store = kind != StrKind::Dcs;
        let overflow = match self.string.as_mut() {
            Some(st) => {
                st.len = st.len.saturating_add(1);
                if st.len > limit {
                    true
                } else {
                    if store {
                        st.buf.push(byte);
                    }
                    false
                }
            }
            None => false,
        };
        if overflow {
            let emit = self.finish_string(StringTerm::Overflow);
            return PreStep {
                feed: false,
                reset_parser: true,
                emit,
            };
        }
        PreStep::feed()
    }

    fn finish_string(&mut self, term: StringTerm) -> Option<Emit> {
        let st = self.string.take()?;
        match st.kind {
            StrKind::Osc => Some(Emit::Osc {
                params: split_params(&st.buf),
                term,
            }),
            StrKind::Sos => Some(Emit::SosPm {
                kind: StringKind::Sos,
                params: split_params(&st.buf),
                term,
            }),
            StrKind::Pm => Some(Emit::SosPm {
                kind: StringKind::Pm,
                params: split_params(&st.buf),
                term,
            }),
            StrKind::Apc => Some(Emit::SosPm {
                kind: StringKind::Apc,
                params: split_params(&st.buf),
                term,
            }),
            StrKind::Dcs => {
                if self.dcs_active {
                    self.dcs_active = false;
                    Some(Emit::DcsUnhook { term })
                } else {
                    None
                }
            }
        }
    }
}

fn split_params(buf: &[u8]) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    for &b in buf {
        if b == b';' && out.len() < MAX_PARAMS - 1 {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(b);
        }
    }
    out.push(cur);
    out.truncate(MAX_PARAMS);
    out
}

fn apply_emit(emit: Emit, sink: &mut dyn EscapeSink, ctx: &mut AdvCtx) {
    match emit {
        Emit::Osc { params, term } => {
            match term {
                StringTerm::Overflow => ctx.counters.bump(ParseErrorKind::OscOverflow),
                StringTerm::Aborted => ctx.counters.bump(ParseErrorKind::OscAborted),
                StringTerm::Cancelled => ctx.counters.bump(ParseErrorKind::StringCancelled),
                StringTerm::Bel | StringTerm::St => {
                    if let Some(num) = params.first().and_then(|p| parse_u32(p)) {
                        if !osc_known(num) {
                            ctx.counters.bump_osc_unknown(num);
                            record(
                                ctx,
                                ParseErrorKind::OscUnknown,
                                ParserState::OscString,
                                0x07,
                                ctx.seq_start,
                            );
                        }
                    }
                }
            }
            let refs: Vec<&[u8]> = params.iter().map(Vec::as_slice).collect();
            sink.osc_dispatch(&refs, term);
        }
        Emit::SosPm { kind, params, term } => {
            match term {
                StringTerm::Cancelled => ctx.counters.bump(ParseErrorKind::StringCancelled),
                StringTerm::Overflow => ctx.counters.bump(ParseErrorKind::DcsOverflow),
                StringTerm::Bel | StringTerm::St | StringTerm::Aborted => {}
            }
            let refs: Vec<&[u8]> = params.iter().map(Vec::as_slice).collect();
            sink.sos_pm_apc(kind, &refs, term);
        }
        Emit::DcsUnhook { term } => {
            match term {
                StringTerm::Overflow => ctx.counters.bump(ParseErrorKind::DcsOverflow),
                StringTerm::Cancelled => ctx.counters.bump(ParseErrorKind::StringCancelled),
                StringTerm::Bel | StringTerm::St | StringTerm::Aborted => {}
            }
            sink.dcs_unhook(term);
        }
    }
}

fn record(ctx: &mut AdvCtx, kind: ParseErrorKind, state: ParserState, final_byte: u8, offset: u32) {
    if ctx.first_error.is_none() {
        ctx.first_error = Some(ParseError {
            offset,
            state,
            kind,
            final_byte,
        });
    }
}

fn parse_u32(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() || bytes.len() > 10 {
        return None;
    }
    let mut value: u32 = 0;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.saturating_mul(10).saturating_add(u32::from(b - b'0'));
    }
    Some(value)
}

fn osc_known(num: u32) -> bool {
    matches!(num, 0 | 2 | 7 | 8 | 9 | 52 | 133 | 633 | 777)
}

fn csi_known(intermediates: &[u8], action: char) -> bool {
    if intermediates.is_empty() {
        return matches!(
            action,
            'A' | 'B'
                | 'C'
                | 'D'
                | 'E'
                | 'F'
                | 'G'
                | 'H'
                | 'f'
                | 'J'
                | 'K'
                | 'L'
                | 'M'
                | 'P'
                | 'S'
                | 'T'
                | 'X'
                | 'd'
                | 'e'
                | 'g'
                | 'h'
                | 'l'
                | 'm'
                | 'n'
                | 'r'
                | 's'
                | 't'
                | 'u'
                | '@'
                | 'c'
        );
    }
    match action {
        'h' | 'l' => intermediates == [b'?'],
        'u' => intermediates == [b'>'] || intermediates == [b'='] || intermediates == [b'<'],
        _ => false,
    }
}

fn esc_known(intermediates: &[u8], byte: u8) -> bool {
    intermediates.is_empty()
        && matches!(byte, b'7' | b'8' | b'D' | b'E' | b'M' | b'c' | b'=' | b'>')
}

fn dcs_known(intermediates: &[u8], action: char) -> bool {
    if intermediates.is_empty() {
        return matches!(action, 'q' | 't');
    }
    (intermediates == [b'$'] || intermediates == [b'+']) && action == 'q'
}

// ---------------------------------------------------------------------------
// vte adapter
// ---------------------------------------------------------------------------

struct VteAdapter<'a> {
    sink: &'a mut dyn EscapeSink,
    ctx: &'a mut AdvCtx,
    pre: &'a mut PreState,
    eight_bit_c1: bool,
}

impl VteAdapter<'_> {
    fn params(params: &vte::Params) -> Params {
        let mut out = Params::new();
        for slice in params.iter() {
            for &value in slice {
                out.push(value);
            }
        }
        out
    }
}

impl vte::Perform for VteAdapter<'_> {
    fn print(&mut self, c: char) {
        if c == '\u{FFFD}' {
            self.ctx.counters.bump(ParseErrorKind::InvalidUtf8);
            record(
                self.ctx,
                ParseErrorKind::InvalidUtf8,
                ParserState::Ground,
                0,
                self.ctx.current_offset,
            );
        }
        self.sink.print(c);
    }

    fn execute(&mut self, byte: u8) {
        if (0x80..=0x9F).contains(&byte) {
            if self.eight_bit_c1 {
                self.ctx.counters.bump(ParseErrorKind::C1EightBitUsed);
                self.sink.execute(byte);
                return;
            }
            self.ctx.counters.bump(ParseErrorKind::C1EightBitInUtf8);
            self.ctx.counters.bump(ParseErrorKind::InvalidUtf8);
            record(
                self.ctx,
                ParseErrorKind::C1EightBitInUtf8,
                ParserState::Ground,
                byte,
                self.ctx.current_offset,
            );
            self.sink.print('\u{FFFD}');
            return;
        }
        self.sink.execute(byte);
    }

    fn hook(&mut self, params: &vte::Params, intermediates: &[u8], ignore: bool, action: char) {
        let params = Self::params(params);
        if !dcs_known(intermediates, action) {
            self.ctx.counters.bump(ParseErrorKind::DcsUnknown);
            record(
                self.ctx,
                ParseErrorKind::DcsUnknown,
                ParserState::DcsEntry,
                action as u8,
                self.ctx.seq_start,
            );
        }
        self.pre.dcs_active = true;
        self.sink
            .dcs_hook(&params, intermediates, ignore, action as u8);
    }

    fn put(&mut self, byte: u8) {
        self.sink.dcs_put(byte);
    }

    fn unhook(&mut self) {
        if self.ctx.suppress_unhook {
            self.ctx.suppress_unhook = false;
            return;
        }
        self.pre.dcs_active = false;
        self.sink.dcs_unhook(StringTerm::St);
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {
        // OSC is fully owned by the pre-scanner so that St vs Aborted can be told
        // apart (vte only reports a boolean).
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        let params = Self::params(params);
        if ignore {
            self.ctx.counters.bump(ParseErrorKind::CsiMalformed);
            record(
                self.ctx,
                ParseErrorKind::CsiMalformed,
                ParserState::CsiParam,
                action as u8,
                self.ctx.seq_start,
            );
        } else if !csi_known(intermediates, action) {
            self.ctx.counters.bump(ParseErrorKind::CsiUnknown);
            record(
                self.ctx,
                ParseErrorKind::CsiUnknown,
                ParserState::CsiEntry,
                action as u8,
                self.ctx.seq_start,
            );
        }
        self.sink
            .csi_dispatch(&params, intermediates, ignore, action as u8);
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if self.ctx.suppress_st {
            self.ctx.suppress_st = false;
            if byte == b'\\' && intermediates.is_empty() {
                return;
            }
        }
        if byte == b'\\' && intermediates.is_empty() {
            self.ctx.counters.bump(ParseErrorKind::StStray);
            record(
                self.ctx,
                ParseErrorKind::StStray,
                ParserState::Escape,
                byte,
                self.ctx.seq_start,
            );
        } else if !esc_known(intermediates, byte) {
            self.ctx.counters.bump(ParseErrorKind::EscUnknown);
            record(
                self.ctx,
                ParseErrorKind::EscUnknown,
                ParserState::Escape,
                byte,
                self.ctx.seq_start,
            );
        }
        self.sink.esc_dispatch(intermediates, ignore, byte);
    }
}
