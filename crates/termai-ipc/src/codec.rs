//! Wire codecs for handshake (CBOR) and hot-path payloads (POD).
//!
//! ADR-0018 D2: POD is capped at 4 KiB and used for hot-path messages; everything
//! larger (grid snapshots, context events, audit, handshake metadata) travels as CBOR.
//! Large snapshots are explicitly NOT squeezed into a single POD frame.

use termai_core::capability::CapId;
use termai_core::grid::{Cell, CellPos, Color, CursorState, GridSnapshot, LinkSpan};

use crate::cbor::{decode, encode, CborError, Value};
use crate::frame::POD_MAX_LEN;
use crate::handshake::{AuthState, ClientKind, Hello, HelloAck, Limits, RefusedReason, ShmOffer};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CodecError {
    Cbor(CborError),
    Missing(&'static str),
    Bad(&'static str),
}

impl From<CborError> for CodecError {
    fn from(e: CborError) -> Self {
        CodecError::Cbor(e)
    }
}

fn u(v: u64) -> Value {
    Value::U64(v)
}
fn t(s: &str) -> Value {
    Value::Text(s.to_string())
}

fn get_u64(v: &Value, key: &str) -> Result<u64, CodecError> {
    v.get(key)
        .and_then(Value::as_u64)
        .ok_or(CodecError::Missing("u64 field"))
}

fn get_text<'a>(v: &'a Value, key: &str) -> Result<&'a str, CodecError> {
    v.get(key)
        .and_then(Value::as_text)
        .ok_or(CodecError::Missing("text field"))
}

fn get_array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, CodecError> {
    match v.get(key) {
        Some(Value::Array(a)) => Ok(a),
        _ => Err(CodecError::Missing("array field")),
    }
}

fn get_bytes<'a>(v: &'a Value, key: &str) -> Result<&'a [u8], CodecError> {
    match v.get(key) {
        Some(Value::Bytes(b)) => Ok(b),
        _ => Err(CodecError::Missing("bytes field")),
    }
}

fn client_kind_str(k: ClientKind) -> &'static str {
    k.as_str()
}

fn parse_client_kind(s: &str) -> Result<ClientKind, CodecError> {
    match s {
        "ui" => Ok(ClientKind::Ui),
        "agent" => Ok(ClientKind::Agent),
        "plugin_host" => Ok(ClientKind::PluginHost),
        "cli" => Ok(ClientKind::Cli),
        "ide" => Ok(ClientKind::Ide),
        "ci" => Ok(ClientKind::Ci),
        _ => Err(CodecError::Bad("client_kind")),
    }
}

fn parse_auth(s: &str) -> Result<AuthState, CodecError> {
    match s {
        "anonymous" => Ok(AuthState::Anonymous),
        "local_peer" => Ok(AuthState::LocalPeer),
        "entitled" => Ok(AuthState::Entitled),
        "degraded" => Ok(AuthState::Degraded),
        _ => Err(CodecError::Bad("auth_state")),
    }
}

fn auth_str(a: AuthState) -> &'static str {
    match a {
        AuthState::Anonymous => "anonymous",
        AuthState::LocalPeer => "local_peer",
        AuthState::Entitled => "entitled",
        AuthState::Degraded => "degraded",
    }
}

fn caps_to_value(caps: &[CapId]) -> Value {
    Value::Array(caps.iter().map(|c| u(u64::from(c.0))).collect())
}

fn caps_from_value(v: &Value, key: &str) -> Result<Vec<CapId>, CodecError> {
    Ok(get_array(v, key)?
        .iter()
        .filter_map(Value::as_u64)
        .map(|x| CapId(x as u32))
        .collect())
}

/// Hello -> CBOR.
#[must_use]
pub fn hello_to_value(h: &Hello) -> Value {
    let nonce = Value::Bytes(h.nonce.to_vec());
    Value::map(vec![
        ("proto_min", u(u64::from(h.proto_min))),
        ("proto_max", u(u64::from(h.proto_max))),
        ("client_kind", t(client_kind_str(h.client_kind))),
        ("caps", caps_to_value(&h.caps)),
        ("required", caps_to_value(&h.required)),
        (
            "session_claim",
            Value::Bytes(h.session_claim.0.to_le_bytes().to_vec()),
        ),
        (
            "feature_bits",
            Value::Bytes(h.feature_bits.to_le_bytes().to_vec()),
        ),
        ("nonce", nonce),
    ])
}

pub fn hello_from_value(v: &Value) -> Result<Hello, CodecError> {
    let mut nonce = [0u8; 16];
    let nb = get_bytes(v, "nonce")?;
    if nb.len() != 16 {
        return Err(CodecError::Bad("nonce length"));
    }
    nonce.copy_from_slice(nb);
    let fb = get_bytes(v, "feature_bits")?;
    if fb.len() != 16 {
        return Err(CodecError::Bad("feature_bits length"));
    }
    let mut fba = [0u8; 16];
    fba.copy_from_slice(fb);
    let sc = get_bytes(v, "session_claim")?;
    if sc.len() != 16 {
        return Err(CodecError::Bad("session_claim length"));
    }
    let mut sca = [0u8; 16];
    sca.copy_from_slice(sc);
    Ok(Hello {
        proto_min: get_u64(v, "proto_min")? as u16,
        proto_max: get_u64(v, "proto_max")? as u16,
        client_kind: parse_client_kind(get_text(v, "client_kind")?)?,
        caps: caps_from_value(v, "caps")?,
        required: caps_from_value(v, "required")?,
        session_claim: termai_core::SessionId(u128::from_le_bytes(sca)),
        feature_bits: u128::from_le_bytes(fba),
        nonce,
    })
}

/// HelloAck -> CBOR.
#[must_use]
pub fn ack_to_value(a: &HelloAck) -> Value {
    let shm = match &a.limits.shm {
        Some(s) => Value::map(vec![
            ("slot_shift", u(u64::from(s.slot_shift))),
            ("slot_count_shift", u(u64::from(s.slot_count_shift))),
        ]),
        None => Value::Null,
    };
    let limits = Value::map(vec![
        ("max_frame", u(u64::from(a.limits.max_frame))),
        ("credits", u(u64::from(a.limits.credits))),
        ("max_chunks", u(u64::from(a.limits.max_chunks))),
        ("shm", shm),
        ("audit_queue", u(u64::from(a.limits.audit_queue))),
    ]);
    Value::map(vec![
        ("chosen_ver", u(u64::from(a.chosen_ver))),
        ("caps_inter", caps_to_value(&a.caps_inter)),
        ("limits", limits),
        ("auth_state", t(auth_str(a.auth_state))),
        (
            "server_feature_bits",
            Value::Bytes(a.server_feature_bits.to_le_bytes().to_vec()),
        ),
        ("degraded", Value::Bool(a.degraded)),
    ])
}

pub fn ack_from_value(v: &Value) -> Result<HelloAck, CodecError> {
    let limits_v = v.get("limits").ok_or(CodecError::Missing("limits"))?;
    let shm = match limits_v.get("shm") {
        Some(Value::Map(_)) => Some(ShmOffer {
            slot_shift: get_u64(limits_v.get("shm").unwrap(), "slot_shift")? as u8,
            slot_count_shift: get_u64(limits_v.get("shm").unwrap(), "slot_count_shift")? as u8,
        }),
        _ => None,
    };
    let fb = get_bytes(v, "server_feature_bits")?;
    if fb.len() != 16 {
        return Err(CodecError::Bad("server_feature_bits length"));
    }
    let mut fba = [0u8; 16];
    fba.copy_from_slice(fb);
    Ok(HelloAck {
        chosen_ver: get_u64(v, "chosen_ver")? as u16,
        caps_inter: caps_from_value(v, "caps_inter")?,
        limits: Limits {
            max_frame: get_u64(limits_v, "max_frame")? as u32,
            credits: get_u64(limits_v, "credits")? as u32,
            max_chunks: get_u64(limits_v, "max_chunks")? as u16,
            shm,
            audit_queue: get_u64(limits_v, "audit_queue")? as u32,
        },
        auth_state: parse_auth(get_text(v, "auth_state")?)?,
        server_feature_bits: u128::from_le_bytes(fba),
        degraded: matches!(v.get("degraded"), Some(Value::Bool(true))),
    })
}

/// Refusal payload (HelloNack).
#[must_use]
pub fn refusal_to_value(r: RefusedReason) -> Value {
    Value::map(vec![
        ("reason", t(r.code())),
        ("retryable", Value::Bool(r.retryable())),
        ("cli_exit", u(r.cli_exit() as u64)),
    ])
}

pub fn refusal_from_value(v: &Value) -> Result<RefusedReason, CodecError> {
    match get_text(v, "reason")? {
        "VerUnsupported" => Ok(RefusedReason::VerUnsupported),
        "CapUnknown" => Ok(RefusedReason::CapUnknown),
        "PeerDenied" => Ok(RefusedReason::PeerDenied),
        "HandshakeReplay" => Ok(RefusedReason::HandshakeReplay),
        "HandshakeTimeout" => Ok(RefusedReason::HandshakeTimeout),
        _ => Err(CodecError::Bad("refusal reason")),
    }
}

/// Error body payload.
#[must_use]
pub fn error_body(code: &str, detail: &str) -> Value {
    Value::map(vec![("code", t(code)), ("detail", t(detail))])
}

/// Grid snapshot as CBOR. The cell array travels as one byte string, 20 bytes/cell.
#[must_use]
pub fn grid_snapshot_to_value(s: &GridSnapshot) -> Value {
    let mut cells = Vec::with_capacity(s.cells.len() * 20);
    for c in &s.cells {
        cells.extend_from_slice(&u32::from(c.ch).to_le_bytes());
        cells.extend_from_slice(&c.attrs.to_le_bytes());
        cells.extend_from_slice(&0u16.to_le_bytes());
        cells.extend_from_slice(&encode_color(c.fg).to_le_bytes());
        cells.extend_from_slice(&encode_color(c.bg).to_le_bytes());
        cells.extend_from_slice(&c.link.to_le_bytes());
    }
    let links = Value::Array(
        s.links
            .iter()
            .map(|l| {
                Value::map(vec![
                    ("row", u(u64::from(l.row))),
                    ("start", u(u64::from(l.start_col))),
                    ("end", u(u64::from(l.end_col))),
                    ("id", t(&l.id)),
                    ("target", t(&l.target)),
                ])
            })
            .collect(),
    );
    let cursor = Value::map(vec![
        ("row", u(u64::from(s.cursor.pos.row))),
        ("col", u(u64::from(s.cursor.pos.col))),
        ("visible", Value::Bool(s.cursor.visible)),
        ("style", u(u64::from(s.cursor.style))),
    ]);
    Value::map(vec![
        ("cols", u(u64::from(s.cols))),
        ("rows", u(u64::from(s.rows))),
        ("cells", Value::Bytes(cells)),
        ("cursor", cursor),
        ("alt", Value::Bool(s.alt)),
        ("wrap", Value::Bool(s.wrap_pending)),
        ("origin", Value::Bool(s.origin_mode)),
        ("modes", Value::Bytes(s.modes.to_le_bytes().to_vec())),
        ("title", t(&s.title)),
        ("links", links),
        ("scrollback_len", u(u64::from(s.scrollback_len))),
        ("scroll0", u(u64::from(s.scroll.0))),
        ("scroll1", u(u64::from(s.scroll.1))),
        ("backend", t(&s.backend)),
    ])
}

pub fn grid_snapshot_from_value(v: &Value) -> Result<GridSnapshot, CodecError> {
    let cols = get_u64(v, "cols")? as u16;
    let rows = get_u64(v, "rows")? as u16;
    let raw = get_bytes(v, "cells")?;
    let expected = usize::from(cols) * usize::from(rows) * 20;
    if raw.len() != expected {
        return Err(CodecError::Bad("cell array length"));
    }
    let mut cells = Vec::with_capacity(usize::from(cols) * usize::from(rows));
    for chunk in raw.chunks_exact(20) {
        let ch = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let attrs = u16::from_le_bytes([chunk[4], chunk[5]]);
        let fg = decode_color(u32::from_le_bytes([
            chunk[8], chunk[9], chunk[10], chunk[11],
        ]));
        let bg = decode_color(u32::from_le_bytes([
            chunk[12], chunk[13], chunk[14], chunk[15],
        ]));
        let link = u32::from_le_bytes([chunk[16], chunk[17], chunk[18], chunk[19]]);
        cells.push(Cell {
            ch: char::from_u32(ch).unwrap_or('\u{fffd}'),
            fg,
            bg,
            attrs,
            link,
        });
    }
    let cv = v.get("cursor").ok_or(CodecError::Missing("cursor"))?;
    let modes_b = get_bytes(v, "modes")?;
    if modes_b.len() != 8 {
        return Err(CodecError::Bad("modes length"));
    }
    let mut mb = [0u8; 8];
    mb.copy_from_slice(modes_b);
    let links = get_array(v, "links")?
        .iter()
        .filter_map(|l| {
            Some(LinkSpan {
                row: l.get("row")?.as_u64()? as u16,
                start_col: l.get("start")?.as_u64()? as u16,
                end_col: l.get("end")?.as_u64()? as u16,
                id: l.get("id")?.as_text()?.to_string(),
                target: l.get("target")?.as_text()?.to_string(),
            })
        })
        .collect();
    Ok(GridSnapshot {
        cols,
        rows,
        cells,
        cursor: CursorState {
            pos: CellPos {
                row: get_u64(cv, "row")? as u16,
                col: get_u64(cv, "col")? as u16,
            },
            visible: matches!(cv.get("visible"), Some(Value::Bool(true))),
            style: get_u64(cv, "style").unwrap_or(0) as u8,
        },
        alt: matches!(v.get("alt"), Some(Value::Bool(true))),
        wrap_pending: matches!(v.get("wrap"), Some(Value::Bool(true))),
        origin_mode: matches!(v.get("origin"), Some(Value::Bool(true))),
        modes: u64::from_le_bytes(mb),
        title: get_text(v, "title").unwrap_or("").to_string(),
        links,
        scrollback_len: get_u64(v, "scrollback_len").unwrap_or(0) as u32,
        scroll: (
            get_u64(v, "scroll0").unwrap_or(0) as u32,
            get_u64(v, "scroll1").unwrap_or(0) as u32,
        ),
        backend: get_text(v, "backend").unwrap_or("").to_string(),
    })
}

#[must_use]
pub fn encode_color(c: Color) -> u32 {
    match c {
        Color::Default => 0,
        Color::Indexed(i) => 0x0100_0000 | u32::from(i),
        Color::Rgb(r, g, b) => {
            0x0200_0000 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
        }
    }
}

#[must_use]
pub fn decode_color(v: u32) -> Color {
    match v >> 24 {
        1 => Color::Indexed((v & 0xFF) as u8),
        2 => Color::Rgb(
            ((v >> 16) & 0xFF) as u8,
            ((v >> 8) & 0xFF) as u8,
            (v & 0xFF) as u8,
        ),
        _ => Color::Default,
    }
}

/// Convenience: CBOR-encode a value into a frame payload.
#[must_use]
pub fn to_bytes(v: &Value) -> Vec<u8> {
    encode(v)
}

/// Convenience: decode a CBOR frame payload.
pub fn from_bytes(b: &[u8]) -> Result<Value, CodecError> {
    Ok(decode(b)?)
}

// ---------------------------------------------------------------------------
// HOT PATH POD payloads (<= POD_MAX_LEN)
// ---------------------------------------------------------------------------

/// Input payload (msg 0x0100). kind: 0 keyboard, 1 mouse, 2 ime commit, 3 api inject, 4 text.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InputPayload {
    pub seq: u64,
    pub kind: u8,
    pub flags: u16,
    pub payload: Vec<u8>,
}

impl InputPayload {
    pub fn encode(&self) -> Result<Vec<u8>, CodecError> {
        let mut out = Vec::with_capacity(19 + self.payload.len());
        out.extend_from_slice(&self.seq.to_le_bytes());
        out.push(self.kind);
        out.extend_from_slice(&self.flags.to_le_bytes());
        out.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.payload);
        if out.len() > POD_MAX_LEN {
            return Err(CodecError::Bad("Input exceeds POD_MAX_LEN; use Paste"));
        }
        Ok(out)
    }

    pub fn decode(b: &[u8]) -> Result<Self, CodecError> {
        if b.len() < 15 {
            return Err(CodecError::Bad("Input payload too short"));
        }
        let seq = u64::from_le_bytes(b[0..8].try_into().unwrap());
        let kind = b[8];
        let flags = u16::from_le_bytes([b[9], b[10]]);
        let n = u32::from_le_bytes([b[11], b[12], b[13], b[14]]) as usize;
        if b.len() != 15 + n {
            return Err(CodecError::Bad("Input payload length"));
        }
        Ok(Self {
            seq,
            kind,
            flags,
            payload: b[15..].to_vec(),
        })
    }
}

/// Paste payload (msg 0x0101), chunked, with an explicit origin for friction/audit.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PasteChunk {
    pub seq: u64,
    pub origin: u8,
    pub chunk_idx: u16,
    pub chunk_total: u16,
    pub total_len: u32,
    pub data: Vec<u8>,
}

impl PasteChunk {
    pub fn encode(&self) -> Result<Vec<u8>, CodecError> {
        let mut out = Vec::with_capacity(21 + self.data.len());
        out.extend_from_slice(&self.seq.to_le_bytes());
        out.push(self.origin);
        out.extend_from_slice(&self.chunk_idx.to_le_bytes());
        out.extend_from_slice(&self.chunk_total.to_le_bytes());
        out.extend_from_slice(&self.total_len.to_le_bytes());
        out.extend_from_slice(&(self.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.data);
        if out.len() > POD_MAX_LEN {
            return Err(CodecError::Bad("Paste chunk exceeds POD_MAX_LEN"));
        }
        Ok(out)
    }

    pub fn decode(b: &[u8]) -> Result<Self, CodecError> {
        if b.len() < 21 {
            return Err(CodecError::Bad("Paste payload too short"));
        }
        let seq = u64::from_le_bytes(b[0..8].try_into().unwrap());
        let origin = b[8];
        let chunk_idx = u16::from_le_bytes([b[9], b[10]]);
        let chunk_total = u16::from_le_bytes([b[11], b[12]]);
        let total_len = u32::from_le_bytes([b[13], b[14], b[15], b[16]]);
        let n = u32::from_le_bytes([b[17], b[18], b[19], b[20]]) as usize;
        if b.len() != 21 + n {
            return Err(CodecError::Bad("Paste payload length"));
        }
        Ok(Self {
            seq,
            origin,
            chunk_idx,
            chunk_total,
            total_len,
            data: b[21..].to_vec(),
        })
    }
}

/// Resize payload (msg 0x0102).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ResizePayload {
    pub cols: u16,
    pub rows: u16,
    pub px_w: u16,
    pub px_h: u16,
}

impl ResizePayload {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8);
        out.extend_from_slice(&self.cols.to_le_bytes());
        out.extend_from_slice(&self.rows.to_le_bytes());
        out.extend_from_slice(&self.px_w.to_le_bytes());
        out.extend_from_slice(&self.px_h.to_le_bytes());
        out
    }

    pub fn decode(b: &[u8]) -> Result<Self, CodecError> {
        if b.len() != 8 {
            return Err(CodecError::Bad("Resize payload length"));
        }
        Ok(Self {
            cols: u16::from_le_bytes([b[0], b[1]]),
            rows: u16::from_le_bytes([b[2], b[3]]),
            px_w: u16::from_le_bytes([b[4], b[5]]),
            px_h: u16::from_le_bytes([b[6], b[7]]),
        })
    }
}

/// Signal payload (msg 0x0103): Sig semantic index, never a raw OS integer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SignalPayload {
    pub sig: u8,
}

impl SignalPayload {
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        vec![self.sig]
    }
}

/// Context event payload (msg 0x0202) as CBOR.
#[must_use]
pub fn context_event_to_value(pane: u16, kind: u16, payload: &[u8]) -> Value {
    Value::map(vec![
        ("pane", u(u64::from(pane))),
        ("kind", u(u64::from(kind))),
        ("payload", Value::Bytes(payload.to_vec())),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handshake::{ClientKind, Hello};

    fn sample_hello() -> Hello {
        Hello {
            proto_min: 1,
            proto_max: 3,
            client_kind: ClientKind::Cli,
            caps: vec![CapId(1), CapId(2)],
            required: vec![CapId(1)],
            session_claim: termai_core::SessionId(0xDEAD_BEEF),
            feature_bits: 0x0123_4567_89AB_CDEF_0123_4567_89AB_CDEF,
            nonce: [9u8; 16],
        }
    }

    #[test]
    fn hello_round_trips_through_cbor() {
        let h = sample_hello();
        let bytes = to_bytes(&hello_to_value(&h));
        let v = from_bytes(&bytes).unwrap();
        assert_eq!(hello_from_value(&v).unwrap(), h);
    }

    #[test]
    fn ack_round_trips_through_cbor() {
        let a = HelloAck {
            chosen_ver: 3,
            caps_inter: vec![CapId(1)],
            limits: Limits {
                max_frame: 8 << 20,
                credits: 64,
                max_chunks: 1024,
                shm: Some(ShmOffer {
                    slot_shift: 16,
                    slot_count_shift: 8,
                }),
                audit_queue: 256,
            },
            auth_state: AuthState::LocalPeer,
            server_feature_bits: 7,
            degraded: true,
        };
        let bytes = to_bytes(&ack_to_value(&a));
        assert_eq!(ack_from_value(&from_bytes(&bytes).unwrap()).unwrap(), a);
    }

    #[test]
    fn refusal_round_trips_and_maps_to_exit_codes() {
        for r in [
            RefusedReason::VerUnsupported,
            RefusedReason::CapUnknown,
            RefusedReason::PeerDenied,
            RefusedReason::HandshakeReplay,
            RefusedReason::HandshakeTimeout,
        ] {
            let v = from_bytes(&to_bytes(&refusal_to_value(r))).unwrap();
            assert_eq!(refusal_from_value(&v).unwrap(), r);
        }
        assert_eq!(RefusedReason::HandshakeTimeout.cli_exit(), 2);
        assert_eq!(RefusedReason::PeerDenied.cli_exit(), 3);
        assert_eq!(RefusedReason::VerUnsupported.cli_exit(), 4);
    }

    #[test]
    fn grid_snapshot_round_trips_including_wide_cells_and_colors() {
        let mut s = GridSnapshot::new(4, 2);
        s.cells[0].ch = 'X';
        s.cells[0].fg = Color::Rgb(1, 2, 3);
        s.cells[0].bg = Color::Indexed(200);
        s.cells[0].attrs = termai_core::grid::ATTR_BOLD;
        s.cells[1] = Cell::WIDE_CONTINUATION;
        s.cursor = CursorState {
            pos: CellPos { row: 1, col: 2 },
            visible: true,
            style: 2,
        };
        s.title = "term".into();
        s.backend = "vte-0.15.0".into();
        s.links.push(LinkSpan {
            row: 0,
            start_col: 0,
            end_col: 4,
            id: "h1".into(),
            target: "https://example.com/".into(),
        });
        let bytes = to_bytes(&grid_snapshot_to_value(&s));
        let back = grid_snapshot_from_value(&from_bytes(&bytes).unwrap()).unwrap();
        assert_eq!(back, s);
        assert!(back.cells[1].is_wide_continuation());
    }

    #[test]
    fn pod_payloads_round_trip_and_respect_the_cap() {
        let i = InputPayload {
            seq: 7,
            kind: 2,
            flags: 1,
            payload: b"abc".to_vec(),
        };
        assert_eq!(InputPayload::decode(&i.encode().unwrap()).unwrap(), i);
        let too_big = InputPayload {
            seq: 0,
            kind: 0,
            flags: 0,
            payload: vec![0u8; POD_MAX_LEN],
        };
        assert_eq!(
            too_big.encode(),
            Err(CodecError::Bad("Input exceeds POD_MAX_LEN; use Paste"))
        );

        let p = PasteChunk {
            seq: 1,
            origin: 0,
            chunk_idx: 1,
            chunk_total: 3,
            total_len: 9000,
            data: b"xy".to_vec(),
        };
        assert_eq!(PasteChunk::decode(&p.encode().unwrap()).unwrap(), p);

        let r = ResizePayload {
            cols: 120,
            rows: 40,
            px_w: 0,
            px_h: 0,
        };
        assert_eq!(ResizePayload::decode(&r.encode()).unwrap(), r);
        assert!(ResizePayload::decode(&[0u8; 3]).is_err());
    }

    #[test]
    fn color_encoding_round_trips() {
        for c in [
            Color::Default,
            Color::Indexed(0),
            Color::Indexed(255),
            Color::Rgb(0, 128, 255),
        ] {
            assert_eq!(decode_color(encode_color(c)), c);
        }
    }

    #[test]
    fn malformed_hello_is_rejected_not_guessed() {
        let v = Value::map(vec![("proto_min", u(1))]);
        assert!(hello_from_value(&v).is_err());
    }
}
