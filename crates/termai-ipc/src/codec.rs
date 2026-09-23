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

// ---------------------------------------------------------------------------
// SESSION ATTACH FAMILY (msg 0x0500..=0x0503, ADR-0023 D1)
//
// Structural / low-frequency messages travel as CBOR (ADR-0018 D2); DETACH_NOTICE is
// POD per ADR-0023 D1 and kernel/07 section 3.2. Field names follow kernel/04 section
// 3.4 verbatim so the wire stays auditable against the spec. ATTACH_ACK.state uses
// the kernel/04 section 3.1 six-state vocabulary (kernel/07 section 3.6 tag 1 strings).
// ---------------------------------------------------------------------------

/// Attach mode (kernel/04 section 3.4). termai-ipc cannot depend on termai-session
/// (DC-21 dependency direction), so the wire enum is mirrored here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttachMode {
    ReadOnly,
    Interactive,
}

impl AttachMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            AttachMode::ReadOnly => "read_only",
            AttachMode::Interactive => "interactive",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CodecError> {
        match s {
            "read_only" => Ok(AttachMode::ReadOnly),
            "interactive" => Ok(AttachMode::Interactive),
            _ => Err(CodecError::Bad("attach mode")),
        }
    }
}

/// Wire mirror of the kernel/04 section 3.1 six session states. Mirrored (not reused
/// from termai-session) because termai-ipc may only depend on termai-core.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SessionState {
    Created,
    Running,
    Detached,
    Exited,
    Recovering,
    Dead,
}

impl SessionState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            SessionState::Created => "created",
            SessionState::Running => "running",
            SessionState::Detached => "detached",
            SessionState::Exited => "exited",
            SessionState::Recovering => "recovering",
            SessionState::Dead => "dead",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CodecError> {
        match s {
            "created" => Ok(SessionState::Created),
            "running" => Ok(SessionState::Running),
            "detached" => Ok(SessionState::Detached),
            "exited" => Ok(SessionState::Exited),
            "recovering" => Ok(SessionState::Recovering),
            "dead" => Ok(SessionState::Dead),
            _ => Err(CodecError::Bad("session state")),
        }
    }
}

/// ATTACH_REQ (0x0500, C->S). kernel/04 section 3.4:
/// {proto_range, client_kind, session_id, mode, resume_from?, capabilities[]}.
/// The proto_range shorthand is carried as the explicit proto_min/proto_max pair
/// that the shared negotiation helper consumes (same pair as kernel/07 section 3.3
/// Hello).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AttachRequest {
    pub proto_min: u16,
    pub proto_max: u16,
    pub client_kind: ClientKind,
    pub session_id: termai_core::SessionId,
    pub mode: AttachMode,
    pub resume_from: Option<u64>,
    pub capabilities: Vec<CapId>,
}

/// snapshot_ref (kernel/04 section 3.4). seq is the exclusive upper bound of applied
/// Log records (the next seq the segment writer will use), so a client whose
/// resume_from <= seq is already covered by the snapshot and must not be replayed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SnapshotRef {
    pub segment_id: u32,
    pub seq: u64,
    pub grid_digest: [u8; 32],
}

/// limits in ATTACH_ACK. max_frame is frozen at 8 MiB by kernel/04 section 3.4 /
/// OQ-30; credits is the per-subscription credit grant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AttachLimits {
    pub max_frame: u32,
    pub credits: u32,
}

/// 8 MiB attach frame ceiling (kernel/04 section 3.4 / OQ-30).
pub const ATTACH_MAX_FRAME: u32 = 8 << 20;

/// LeaseInfo (kernel/04 section 3.4 lease message fields; reason is omitted because an
/// ACK only ever carries a granted lease). holder is a stable client id, since is the
/// mono acquisition timestamp in ns, ttl_s the lease TTL in seconds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LeaseInfo {
    pub lease_id: u64,
    pub holder: u128,
    pub since: u64,
    pub ttl_s: u32,
}

/// ATTACH_ACK (0x0501, S->C).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AttachAck {
    pub chosen_ver: u16,
    pub state: SessionState,
    pub snapshot_ref: SnapshotRef,
    pub lease: Option<LeaseInfo>,
    pub limits: AttachLimits,
}

fn u128_bytes(v: u128) -> Value {
    Value::Bytes(v.to_le_bytes().to_vec())
}

fn get_u16_checked(v: &Value, key: &str) -> Result<u16, CodecError> {
    u16::try_from(get_u64(v, key)?).map_err(|_| CodecError::Bad("u16 field out of range"))
}

fn get_u32_checked(v: &Value, key: &str) -> Result<u32, CodecError> {
    u32::try_from(get_u64(v, key)?).map_err(|_| CodecError::Bad("u32 field out of range"))
}

fn get_u128(v: &Value, key: &str) -> Result<u128, CodecError> {
    let b = get_bytes(v, key)?;
    let a: [u8; 16] = b
        .try_into()
        .map_err(|_| CodecError::Bad("u128 field length"))?;
    Ok(u128::from_le_bytes(a))
}

fn get_digest(v: &Value, key: &str) -> Result<[u8; 32], CodecError> {
    let b = get_bytes(v, key)?;
    let a: [u8; 32] = b.try_into().map_err(|_| CodecError::Bad("digest length"))?;
    Ok(a)
}

/// AttachRequest -> CBOR.
#[must_use]
pub fn attach_request_to_value(r: &AttachRequest) -> Value {
    Value::map(vec![
        ("proto_min", u(u64::from(r.proto_min))),
        ("proto_max", u(u64::from(r.proto_max))),
        ("client_kind", t(client_kind_str(r.client_kind))),
        ("session_id", u128_bytes(r.session_id.0)),
        ("mode", t(r.mode.as_str())),
        (
            "resume_from",
            match r.resume_from {
                Some(s) => u(s),
                None => Value::Null,
            },
        ),
        ("capabilities", caps_to_value(&r.capabilities)),
    ])
}

pub fn attach_request_from_value(v: &Value) -> Result<AttachRequest, CodecError> {
    Ok(AttachRequest {
        proto_min: get_u16_checked(v, "proto_min")?,
        proto_max: get_u16_checked(v, "proto_max")?,
        client_kind: parse_client_kind(get_text(v, "client_kind")?)?,
        session_id: termai_core::SessionId(get_u128(v, "session_id")?),
        mode: AttachMode::parse(get_text(v, "mode")?)?,
        resume_from: match v.get("resume_from") {
            None | Some(Value::Null) => None,
            Some(x) => Some(x.as_u64().ok_or(CodecError::Bad("resume_from"))?),
        },
        capabilities: caps_from_value(v, "capabilities")?,
    })
}

/// AttachAck -> CBOR.
#[must_use]
pub fn attach_ack_to_value(a: &AttachAck) -> Value {
    let snapshot_ref = Value::map(vec![
        ("segment_id", u(u64::from(a.snapshot_ref.segment_id))),
        ("seq", u(a.snapshot_ref.seq)),
        (
            "grid_digest",
            Value::Bytes(a.snapshot_ref.grid_digest.to_vec()),
        ),
    ]);
    let lease = match &a.lease {
        Some(l) => Value::map(vec![
            ("lease_id", u(l.lease_id)),
            ("holder", u128_bytes(l.holder)),
            ("since", u(l.since)),
            ("ttl_s", u(u64::from(l.ttl_s))),
        ]),
        None => Value::Null,
    };
    let limits = Value::map(vec![
        ("max_frame", u(u64::from(a.limits.max_frame))),
        ("credits", u(u64::from(a.limits.credits))),
    ]);
    Value::map(vec![
        ("chosen_ver", u(u64::from(a.chosen_ver))),
        ("state", t(a.state.as_str())),
        ("snapshot_ref", snapshot_ref),
        ("lease", lease),
        ("limits", limits),
    ])
}

pub fn attach_ack_from_value(v: &Value) -> Result<AttachAck, CodecError> {
    let snap = v
        .get("snapshot_ref")
        .ok_or(CodecError::Missing("snapshot_ref"))?;
    let lease = match v.get("lease") {
        Some(lv @ Value::Map(_)) => Some(LeaseInfo {
            lease_id: get_u64(lv, "lease_id")?,
            holder: get_u128(lv, "holder")?,
            since: get_u64(lv, "since")?,
            ttl_s: get_u32_checked(lv, "ttl_s")?,
        }),
        _ => None,
    };
    let limits = v.get("limits").ok_or(CodecError::Missing("limits"))?;
    Ok(AttachAck {
        chosen_ver: get_u16_checked(v, "chosen_ver")?,
        state: SessionState::parse(get_text(v, "state")?)?,
        snapshot_ref: SnapshotRef {
            segment_id: get_u32_checked(snap, "segment_id")?,
            seq: get_u64(snap, "seq")?,
            grid_digest: get_digest(snap, "grid_digest")?,
        },
        lease,
        limits: AttachLimits {
            max_frame: get_u32_checked(limits, "max_frame")?,
            credits: get_u32_checked(limits, "credits")?,
        },
    })
}

/// DETACH_NOTICE (0x0503) POD payload: one byte, 0x00 = keep the lease, 0x01 = release
/// it (ADR-0023 D1 / kernel/04 section 3.4). Any other shape is malformed.
#[must_use]
pub fn detach_notice_encode(lease_release: bool) -> Vec<u8> {
    vec![u8::from(lease_release)]
}

pub fn detach_notice_decode(b: &[u8]) -> Result<bool, CodecError> {
    match b {
        [0] => Ok(false),
        [1] => Ok(true),
        _ => Err(CodecError::Bad("DetachNotice payload")),
    }
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
    // ADR-0025 D1: per-row LineFlags travel as rows * 2 bytes, u16 LE row-major.
    let mut row_flags = Vec::with_capacity(usize::from(s.rows) * 2);
    for row in 0..s.rows {
        let flags = s.row_flags.get(usize::from(row)).copied().unwrap_or(0);
        row_flags.extend_from_slice(&flags.to_le_bytes());
    }
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
        ("row_flags", Value::Bytes(row_flags)),
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
    // ADR-0025 D1: a minor-1 document has no row_flags field; decode it as all
    // zero rather than failing (DC-40 N-2 forward/backward compatibility).
    let row_flags = match v.get("row_flags") {
        Some(Value::Bytes(b)) => {
            if b.len() != usize::from(rows) * 2 {
                return Err(CodecError::Bad("row_flags length"));
            }
            b.chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect()
        }
        _ => vec![0u16; usize::from(rows)],
    };
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
        row_flags,
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

// ---------------------------------------------------------------------------
// TAIL_REPLAY (0x0502) - the P0 tail of the Session Log (ADR-0023 D1,
// kernel/04 section 3.4)
//
// Wire shape: {from_seq: u64, events: [ReplayEvent]}. from_seq is EXCLUSIVE: the server
// only ever sends events with seq > from_seq (kernel/04 section 3.4), which is what
// makes a repeated request idempotent (resume_from = last_applied_seq).
//
// Each event is a kernel/07 section 3.6 Context event. tag is the oneof discriminant
// ("tag 即契约") and every payload field name is taken VERBATIM from that table. The
// envelope adds what section 3.6 fixes outside the oneof: seq (the only ordering /
// dedup key), ts_mono_ns (the Log carries exactly one timestamp, see ReplayEvent), and
// pane (kernel/04 section 3.2.3 gives every record a pane; it is optional because
// StateChange carries none).
//
// TAIL_REPLAY is "仅 P0 事件" (kernel/04 section 3.4): the P1 volatile raw ring (PtyOut)
// is never replayed. A gap in events[].seq therefore means "a record of another
// retention class was filtered out", NOT loss; loss is expressed by refusing the
// request outright (see sessiond::broker and AR-26).
// ---------------------------------------------------------------------------

/// kernel/07 section 3.6 event tags. Values are contract: never reused.
pub mod context_tag {
    pub const SESSION_LIFECYCLE: u64 = 1;
    pub const COMMAND_BOUNDARY: u64 = 2;
    pub const EXIT_STATUS: u64 = 3;
    pub const CWD_CHANGED: u64 = 4;
    pub const TITLE_CHANGED: u64 = 5;
    pub const RESIZE: u64 = 6;
    pub const ERROR_FRAGMENT: u64 = 7;
    pub const MODE_CHANGED: u64 = 8;
    pub const TRANSPORT_STATE: u64 = 9;
    pub const TRUNCATION_NOTICE: u64 = 10;
}

/// kernel/07 section 3.6 tag 7: an error fragment is untrusted data by contract.
pub const CONTEXT_TRUST_UNTRUSTED: &str = "untrusted";

/// kernel/07 section 3.6 tag 2 phase.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommandPhase {
    PromptStart,
    CmdStart,
    CmdEnd,
}

impl CommandPhase {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PromptStart => "prompt_start",
            Self::CmdStart => "cmd_start",
            Self::CmdEnd => "cmd_end",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CodecError> {
        match s {
            "prompt_start" => Ok(Self::PromptStart),
            "cmd_start" => Ok(Self::CmdStart),
            "cmd_end" => Ok(Self::CmdEnd),
            _ => Err(CodecError::Bad("command phase")),
        }
    }
}

/// kernel/07 section 3.6 tag 5 scope.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TitleScope {
    Icon,
    Window,
}

impl TitleScope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Icon => "icon",
            Self::Window => "window",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CodecError> {
        match s {
            "icon" => Ok(Self::Icon),
            "window" => Ok(Self::Window),
            _ => Err(CodecError::Bad("title scope")),
        }
    }
}

/// kernel/07 section 3.6 tag 9 kind.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransportKind {
    Local,
    Ssh,
    Container,
    Wsl,
}

impl TransportKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Ssh => "ssh",
            Self::Container => "container",
            Self::Wsl => "wsl",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CodecError> {
        match s {
            "local" => Ok(Self::Local),
            "ssh" => Ok(Self::Ssh),
            "container" => Ok(Self::Container),
            "wsl" => Ok(Self::Wsl),
            _ => Err(CodecError::Bad("transport kind")),
        }
    }
}

/// kernel/07 section 3.6 tag 9 state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransportStatus {
    Connecting,
    Ready,
    Lost,
}

impl TransportStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Ready => "ready",
            Self::Lost => "lost",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CodecError> {
        match s {
            "connecting" => Ok(Self::Connecting),
            "ready" => Ok(Self::Ready),
            "lost" => Ok(Self::Lost),
            _ => Err(CodecError::Bad("transport state")),
        }
    }
}

fn get_bool(v: &Value, key: &str) -> Result<bool, CodecError> {
    match v.get(key) {
        Some(Value::Bool(b)) => Ok(*b),
        _ => Err(CodecError::Missing("bool field")),
    }
}

fn get_f32(v: &Value, key: &str) -> Result<f32, CodecError> {
    v.get(key)
        .and_then(Value::as_f32)
        .ok_or(CodecError::Missing("f32 field"))
}

fn get_opt_u64(v: &Value, key: &str) -> Result<Option<u64>, CodecError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(x) => Ok(Some(x.as_u64().ok_or(CodecError::Bad("u64 field"))?)),
    }
}

fn get_opt_i32(v: &Value, key: &str) -> Result<Option<i32>, CodecError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::I64(x)) => i32::try_from(*x)
            .map(Some)
            .map_err(|_| CodecError::Bad("i32 field out of range")),
        Some(Value::U64(x)) => i32::try_from(*x)
            .map(Some)
            .map_err(|_| CodecError::Bad("i32 field out of range")),
        _ => Err(CodecError::Bad("i32 field")),
    }
}

fn get_opt_u8(v: &Value, key: &str) -> Result<Option<u8>, CodecError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(x) => u8::try_from(x.as_u64().ok_or(CodecError::Bad("u8 field"))?)
            .map(Some)
            .map_err(|_| CodecError::Bad("u8 field out of range")),
    }
}

fn get_u16_list(v: &Value, key: &str) -> Result<Vec<u16>, CodecError> {
    get_array(v, key)?
        .iter()
        .map(|x| {
            u16::try_from(x.as_u64().ok_or(CodecError::Bad("u16 list"))?)
                .map_err(|_| CodecError::Bad("u16 list out of range"))
        })
        .collect()
}

fn get_opt_pair(v: &Value, key: &str) -> Result<Option<(u32, u32)>, CodecError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) if a.len() == 2 => {
            let w = u32::try_from(a[0].as_u64().ok_or(CodecError::Bad("pair"))?)
                .map_err(|_| CodecError::Bad("pair out of range"))?;
            let h = u32::try_from(a[1].as_u64().ok_or(CodecError::Bad("pair"))?)
                .map_err(|_| CodecError::Bad("pair out of range"))?;
            Ok(Some((w, h)))
        }
        _ => Err(CodecError::Bad("pair")),
    }
}

/// One kernel/07 section 3.6 Context event body. Field names on the wire are the
/// table's keyword-column names verbatim; the in-crate CBOR codec carries confidence as
/// a real IEEE-754 binary32 (cbor.rs major type 7 / ai 26).
#[derive(Clone, PartialEq, Debug)]
pub enum ContextEventBody {
    /// tag 1. state uses the kernel/04 section 3.1 six-state vocabulary.
    SessionLifecycle {
        state: SessionState,
        reason: Option<String>,
    },
    /// tag 2.
    CommandBoundary {
        cmd_id: u64,
        phase: CommandPhase,
        osc: u16,
        confidence: f32,
    },
    /// tag 3.
    ExitStatus {
        cmd_id: u64,
        code: Option<i32>,
        signal: Option<u8>,
    },
    /// tag 4. cwd is a PathRef: length-prefixed UTF-8 (a CBOR byte string is
    /// length-prefixed), plus the lossy marker.
    CwdChanged { cwd: String, lossy: bool },
    /// tag 5.
    TitleChanged { title: String, scope: TitleScope },
    /// tag 6.
    Resize {
        rows: u16,
        cols: u16,
        px: Option<(u32, u32)>,
    },
    /// tag 7. trust is the fixed literal "untrusted" on the wire and is validated on
    /// decode, so it needs no Rust field.
    ErrorFragment {
        cmd_id: Option<u64>,
        redacted: String,
        rule_ids: Vec<u16>,
    },
    /// tag 8.
    ModeChanged { modes: u64 },
    /// tag 9.
    TransportState {
        kind: TransportKind,
        state: TransportStatus,
    },
    /// tag 10. kernel/07 section 3.6 declares reason: enum without enumerating its
    /// members, so the field travels as an opaque text value (no member is invented
    /// here); the member table is a registered spec gap.
    TruncationNotice { dropped_bytes: u64, reason: String },
    /// kernel/07 section 3.6 V3: a reader must ignore an unknown tag and count it,
    /// never panic and never disconnect. Preserving the tag and payload verbatim is
    /// what lets a consumer do that without losing data.
    Unknown { tag: u64, payload: Value },
}

impl ContextEventBody {
    #[must_use]
    pub const fn tag(&self) -> u64 {
        match self {
            Self::SessionLifecycle { .. } => context_tag::SESSION_LIFECYCLE,
            Self::CommandBoundary { .. } => context_tag::COMMAND_BOUNDARY,
            Self::ExitStatus { .. } => context_tag::EXIT_STATUS,
            Self::CwdChanged { .. } => context_tag::CWD_CHANGED,
            Self::TitleChanged { .. } => context_tag::TITLE_CHANGED,
            Self::Resize { .. } => context_tag::RESIZE,
            Self::ErrorFragment { .. } => context_tag::ERROR_FRAGMENT,
            Self::ModeChanged { .. } => context_tag::MODE_CHANGED,
            Self::TransportState { .. } => context_tag::TRANSPORT_STATE,
            Self::TruncationNotice { .. } => context_tag::TRUNCATION_NOTICE,
            Self::Unknown { tag, .. } => *tag,
        }
    }

    #[must_use]
    pub fn to_payload(&self) -> Value {
        match self {
            Self::SessionLifecycle { state, reason } => Value::map(vec![
                ("state", t(state.as_str())),
                ("reason", reason.as_deref().map_or(Value::Null, t)),
            ]),
            Self::CommandBoundary {
                cmd_id,
                phase,
                osc,
                confidence,
            } => Value::map(vec![
                ("cmd_id", u(*cmd_id)),
                ("phase", t(phase.as_str())),
                ("osc", u(u64::from(*osc))),
                ("confidence", Value::F32(confidence.to_bits())),
            ]),
            Self::ExitStatus {
                cmd_id,
                code,
                signal,
            } => Value::map(vec![
                ("cmd_id", u(*cmd_id)),
                (
                    "code",
                    code.map_or(Value::Null, |c| Value::I64(i64::from(c))),
                ),
                ("signal", signal.map_or(Value::Null, |s| u(u64::from(s)))),
            ]),
            Self::CwdChanged { cwd, lossy } => Value::map(vec![
                ("cwd", Value::Bytes(cwd.as_bytes().to_vec())),
                ("lossy", Value::Bool(*lossy)),
            ]),
            Self::TitleChanged { title, scope } => {
                Value::map(vec![("title", t(title)), ("scope", t(scope.as_str()))])
            }
            Self::Resize { rows, cols, px } => Value::map(vec![
                ("rows", u(u64::from(*rows))),
                ("cols", u(u64::from(*cols))),
                (
                    "px",
                    px.map_or(Value::Null, |(w, h)| {
                        Value::Array(vec![u(u64::from(w)), u(u64::from(h))])
                    }),
                ),
            ]),
            Self::ErrorFragment {
                cmd_id,
                redacted,
                rule_ids,
            } => Value::map(vec![
                ("cmd_id", cmd_id.map_or(Value::Null, u)),
                ("redacted", t(redacted)),
                (
                    "rule_ids",
                    Value::Array(rule_ids.iter().map(|r| u(u64::from(*r))).collect()),
                ),
                ("trust", t(CONTEXT_TRUST_UNTRUSTED)),
            ]),
            Self::ModeChanged { modes } => Value::map(vec![("modes", u(*modes))]),
            Self::TransportState { kind, state } => Value::map(vec![
                ("kind", t(kind.as_str())),
                ("state", t(state.as_str())),
            ]),
            Self::TruncationNotice {
                dropped_bytes,
                reason,
            } => Value::map(vec![
                ("dropped_bytes", u(*dropped_bytes)),
                ("reason", t(reason)),
            ]),
            Self::Unknown { payload, .. } => payload.clone(),
        }
    }

    pub fn from_payload(tag: u64, p: &Value) -> Result<Self, CodecError> {
        Ok(match tag {
            context_tag::SESSION_LIFECYCLE => Self::SessionLifecycle {
                state: SessionState::parse(get_text(p, "state")?)?,
                reason: match p.get("reason") {
                    None | Some(Value::Null) => None,
                    Some(Value::Text(s)) => Some(s.clone()),
                    _ => return Err(CodecError::Bad("reason")),
                },
            },
            context_tag::COMMAND_BOUNDARY => {
                let osc = get_u16_checked(p, "osc")?;
                if osc != 133 && osc != 633 {
                    return Err(CodecError::Bad("osc"));
                }
                Self::CommandBoundary {
                    cmd_id: get_u64(p, "cmd_id")?,
                    phase: CommandPhase::parse(get_text(p, "phase")?)?,
                    osc,
                    confidence: get_f32(p, "confidence")?,
                }
            }
            context_tag::EXIT_STATUS => Self::ExitStatus {
                cmd_id: get_u64(p, "cmd_id")?,
                code: get_opt_i32(p, "code")?,
                signal: get_opt_u8(p, "signal")?,
            },
            context_tag::CWD_CHANGED => Self::CwdChanged {
                cwd: match p.get("cwd") {
                    Some(Value::Bytes(b)) => {
                        String::from_utf8(b.clone()).map_err(|_| CodecError::Bad("cwd utf8"))?
                    }
                    _ => return Err(CodecError::Missing("cwd")),
                },
                lossy: get_bool(p, "lossy")?,
            },
            context_tag::TITLE_CHANGED => Self::TitleChanged {
                title: get_text(p, "title")?.to_string(),
                scope: TitleScope::parse(get_text(p, "scope")?)?,
            },
            context_tag::RESIZE => Self::Resize {
                rows: get_u16_checked(p, "rows")?,
                cols: get_u16_checked(p, "cols")?,
                px: get_opt_pair(p, "px")?,
            },
            context_tag::ERROR_FRAGMENT => {
                if get_text(p, "trust")? != CONTEXT_TRUST_UNTRUSTED {
                    return Err(CodecError::Bad("trust"));
                }
                Self::ErrorFragment {
                    cmd_id: get_opt_u64(p, "cmd_id")?,
                    redacted: get_text(p, "redacted")?.to_string(),
                    rule_ids: get_u16_list(p, "rule_ids")?,
                }
            }
            context_tag::MODE_CHANGED => Self::ModeChanged {
                modes: get_u64(p, "modes")?,
            },
            context_tag::TRANSPORT_STATE => Self::TransportState {
                kind: TransportKind::parse(get_text(p, "kind")?)?,
                state: TransportStatus::parse(get_text(p, "state")?)?,
            },
            context_tag::TRUNCATION_NOTICE => Self::TruncationNotice {
                dropped_bytes: get_u64(p, "dropped_bytes")?,
                reason: get_text(p, "reason")?.to_string(),
            },
            other => Self::Unknown {
                tag: other,
                payload: p.clone(),
            },
        })
    }
}

/// One replayed event. seq is the Log record's sequence number and the only ordering /
/// dedup key (kernel/07 section 3.6). ts_mono_ns is the Log record timestamp: kernel/04
/// section 3.2.2 stores exactly one ts_ns per record and does not separate a monotonic
/// from a wall clock, so no ts_wall_ns is invented here.
#[derive(Clone, PartialEq, Debug)]
pub struct ReplayEvent {
    pub seq: u64,
    pub ts_mono_ns: u64,
    pub pane: Option<u16>,
    pub body: ContextEventBody,
}

impl ReplayEvent {
    #[must_use]
    pub fn to_value(&self) -> Value {
        Value::map(vec![
            ("seq", u(self.seq)),
            ("ts_mono_ns", u(self.ts_mono_ns)),
            ("pane", self.pane.map_or(Value::Null, |p| u(u64::from(p)))),
            ("tag", u(self.body.tag())),
            ("payload", self.body.to_payload()),
        ])
    }

    pub fn from_value(v: &Value) -> Result<Self, CodecError> {
        let tag = get_u64(v, "tag")?;
        let payload = v.get("payload").ok_or(CodecError::Missing("payload"))?;
        Ok(Self {
            seq: get_u64(v, "seq")?,
            ts_mono_ns: get_u64(v, "ts_mono_ns")?,
            pane: match v.get("pane") {
                None | Some(Value::Null) => None,
                Some(_) => Some(get_u16_checked(v, "pane")?),
            },
            body: ContextEventBody::from_payload(tag, payload)?,
        })
    }
}

/// TAIL_REPLAY (0x0502, S->C) payload. from_seq is the EXCLUSIVE replay floor.
#[derive(Clone, PartialEq, Debug)]
pub struct TailReplay {
    pub from_seq: u64,
    pub events: Vec<ReplayEvent>,
}

/// TailReplay -> CBOR.
#[must_use]
pub fn tail_replay_to_value(r: &TailReplay) -> Value {
    Value::map(vec![
        ("from_seq", u(r.from_seq)),
        (
            "events",
            Value::Array(r.events.iter().map(ReplayEvent::to_value).collect()),
        ),
    ])
}

pub fn tail_replay_from_value(v: &Value) -> Result<TailReplay, CodecError> {
    Ok(TailReplay {
        from_seq: get_u64(v, "from_seq")?,
        events: get_array(v, "events")?
            .iter()
            .map(ReplayEvent::from_value)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

/// The request direction of 0x0502: the same map with no replays in it. A client asks
/// for the tail after from_seq; an absent field means "use the subscription resume
/// point" and is handled by the broker, so it is not decoded here.
pub fn tail_replay_request_from_value(v: &Value) -> Result<u64, CodecError> {
    get_u64(v, "from_seq")
}

/// Tests for the TAIL_REPLAY wire (0x0502) and the kernel/07 section 3.6 oneof.
#[cfg(test)]
mod replay_tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        let b = s.as_bytes();
        (0..b.len() / 2)
            .map(|i| {
                let hi = (b[2 * i] as char).to_digit(16).unwrap() as u8;
                let lo = (b[2 * i + 1] as char).to_digit(16).unwrap() as u8;
                (hi << 4) | lo
            })
            .collect()
    }

    // ---- TAIL_REPLAY (0x0502) and the kernel/07 section 3.6 event schema ----

    fn ev(seq: u64, pane: Option<u16>, body: ContextEventBody) -> ReplayEvent {
        ReplayEvent {
            seq,
            ts_mono_ns: seq * 10,
            pane,
            body,
        }
    }

    fn sample_events() -> Vec<ReplayEvent> {
        vec![
            ev(
                1,
                Some(0),
                ContextEventBody::SessionLifecycle {
                    state: SessionState::Running,
                    reason: Some("closed".into()),
                },
            ),
            ev(
                2,
                None,
                ContextEventBody::CommandBoundary {
                    cmd_id: 9,
                    phase: CommandPhase::PromptStart,
                    osc: 633,
                    confidence: 0.5,
                },
            ),
            ev(
                3,
                Some(0),
                ContextEventBody::ExitStatus {
                    cmd_id: 9,
                    code: Some(-2),
                    signal: Some(9),
                },
            ),
            ev(
                4,
                Some(0),
                ContextEventBody::CwdChanged {
                    cwd: "C:/tmp".into(),
                    lossy: false,
                },
            ),
            ev(
                5,
                Some(0),
                ContextEventBody::TitleChanged {
                    title: "term".into(),
                    scope: TitleScope::Icon,
                },
            ),
            ev(
                6,
                Some(0),
                ContextEventBody::Resize {
                    rows: 24,
                    cols: 80,
                    px: Some((800, 600)),
                },
            ),
            ev(
                7,
                Some(0),
                ContextEventBody::ErrorFragment {
                    cmd_id: None,
                    redacted: "<redacted>".into(),
                    rule_ids: vec![1, 2],
                },
            ),
            ev(8, Some(0), ContextEventBody::ModeChanged { modes: 0b1011 }),
            ev(
                9,
                Some(0),
                ContextEventBody::TransportState {
                    kind: TransportKind::Wsl,
                    state: TransportStatus::Lost,
                },
            ),
            ev(
                10,
                Some(0),
                ContextEventBody::TruncationNotice {
                    dropped_bytes: 7,
                    reason: "p1_ring_overflow".into(),
                },
            ),
        ]
    }

    /// Frozen golden vector: any field rename / reorder / type change breaks this.
    #[test]
    fn tail_replay_matches_frozen_golden_bytes() {
        let r = TailReplay {
            from_seq: 5,
            events: vec![ev(
                6,
                Some(0),
                ContextEventBody::ExitStatus {
                    cmd_id: 3,
                    code: Some(1),
                    signal: None,
                },
            )],
        };
        let bytes = to_bytes(&tail_replay_to_value(&r));
        let expected = hex(
            "A26866726F6D5F73657105666576656E747381A563736571066A74735F6D6F6E6F5F6E73183C64\
             70616E65006374616703677061796C6F6164A366636D645F69640364636F646501667369676E616CF6",
        );
        assert_eq!(bytes, expected);
        assert_eq!(
            tail_replay_from_value(&from_bytes(&bytes).unwrap()).unwrap(),
            r
        );
    }

    #[test]
    fn tail_replay_round_trips_every_section_3_6_event() {
        let r = TailReplay {
            from_seq: 0,
            events: sample_events(),
        };
        let bytes = to_bytes(&tail_replay_to_value(&r));
        let back = tail_replay_from_value(&from_bytes(&bytes).unwrap()).unwrap();
        assert_eq!(back, r);
        assert_eq!(
            back.events.iter().map(|e| e.body.tag()).collect::<Vec<_>>(),
            (1..=10).collect::<Vec<u64>>(),
            "each event keeps its section 3.6 tag"
        );
    }

    #[test]
    fn context_event_confidence_is_a_real_binary32() {
        let body = ContextEventBody::CommandBoundary {
            cmd_id: 1,
            phase: CommandPhase::CmdEnd,
            osc: 133,
            confidence: 0.9,
        };
        let bytes = to_bytes(&body.to_payload());
        assert!(
            bytes
                .windows(5)
                .any(|w| w == [0xFA, 0x3F, 0x66, 0x66, 0x66]),
            "confidence must be CBOR binary32 (0xFA), not a byte-string carrier"
        );
        let back = ContextEventBody::from_payload(
            context_tag::COMMAND_BOUNDARY,
            &from_bytes(&bytes).unwrap(),
        )
        .unwrap();
        assert_eq!(back, body);
    }

    #[test]
    fn tail_replay_rejects_missing_fields_and_bad_enums() {
        // No payload at all.
        let no_payload = Value::map(vec![("seq", u(1)), ("ts_mono_ns", u(2)), ("tag", u(3))]);
        assert!(ReplayEvent::from_value(&no_payload).is_err());
        // tag 3 without the required cmd_id is not a complete ExitStatus (code and
        // signal really are optional and may be null).
        let partial = Value::map(vec![
            ("seq", u(1)),
            ("ts_mono_ns", u(2)),
            ("tag", u(3)),
            ("payload", Value::map(vec![("code", Value::Null)])),
        ]);
        assert!(ReplayEvent::from_value(&partial).is_err());
        // tag 2 with an osc outside the frozen {133, 633} set.
        let bad_osc = Value::map(vec![
            ("seq", u(1)),
            ("ts_mono_ns", u(2)),
            ("tag", u(2)),
            (
                "payload",
                Value::map(vec![
                    ("cmd_id", u(1)),
                    ("phase", Value::Text("cmd_end".into())),
                    ("osc", u(99)),
                    ("confidence", Value::F32(0.5f32.to_bits())),
                ]),
            ),
        ]);
        assert!(ReplayEvent::from_value(&bad_osc).is_err());
        // tag 7 whose trust is not the frozen literal.
        let bad_trust = Value::map(vec![
            ("seq", u(1)),
            ("ts_mono_ns", u(2)),
            ("tag", u(7)),
            (
                "payload",
                Value::map(vec![
                    ("cmd_id", Value::Null),
                    ("redacted", Value::Text("x".into())),
                    ("rule_ids", Value::Array(vec![])),
                    ("trust", Value::Text("trusted".into())),
                ]),
            ),
        ]);
        assert!(ReplayEvent::from_value(&bad_trust).is_err());
        // A request without from_seq.
        assert!(tail_replay_request_from_value(&Value::map(vec![(
            "events",
            Value::Array(vec![])
        )]))
        .is_err());
    }

    #[test]
    fn tail_replay_ignores_unknown_fields_for_forward_compat() {
        // kernel/07 section 3.6 V3 / section 3.7: a newer minor adds optional fields.
        let mut v = tail_replay_to_value(&TailReplay {
            from_seq: 0,
            events: sample_events(),
        });
        if let Value::Map(ref mut m) = v {
            m.push((Value::Text("future_field".into()), Value::U64(1)));
            if let Some((_, Value::Array(events))) = m.last_mut() {
                if let Some(Value::Map(e)) = events.first_mut() {
                    e.push((Value::Text("future_event_field".into()), Value::U64(2)));
                }
            }
        }
        let back = tail_replay_from_value(&from_bytes(&to_bytes(&v)).unwrap()).unwrap();
        assert_eq!(back.events.len(), 10);
        assert_eq!(back.events[0].seq, 1);
    }

    /// kernel/07 section 3.6 V3 / N-6: an unknown event tag must decode, never panic.
    #[test]
    fn tail_replay_unknown_tag_is_preserved_not_fatal() {
        let r = TailReplay {
            from_seq: 1,
            events: vec![ev(
                2,
                Some(0),
                ContextEventBody::Unknown {
                    tag: 42,
                    payload: Value::map(vec![("future", Value::Text("x".into()))]),
                },
            )],
        };
        let bytes = to_bytes(&tail_replay_to_value(&r));
        let back = tail_replay_from_value(&from_bytes(&bytes).unwrap()).unwrap();
        assert_eq!(back, r);
        assert_eq!(back.events[0].body.tag(), 42);
        assert_eq!(to_bytes(&tail_replay_to_value(&back)), bytes);
    }

    #[test]
    fn tail_replay_request_is_the_same_map_without_events() {
        let v = tail_replay_to_value(&TailReplay {
            from_seq: 7,
            events: vec![],
        });
        assert_eq!(tail_replay_request_from_value(&v).unwrap(), 7);
        assert_eq!(
            tail_replay_from_value(&v).unwrap(),
            TailReplay {
                from_seq: 7,
                events: vec![]
            }
        );
    }
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
        // ADR-0025 D1: prove a non-zero flag survives the CBOR round trip.
        s.row_flags[1] = termai_core::grid::LINE_WRAPPED;
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
        assert_ne!(back.row_flags[1] & termai_core::grid::LINE_WRAPPED, 0);
    }

    #[test]
    fn grid_snapshot_without_row_flags_decodes_as_zero() {
        // A minor-1 producer on the wire omits the field; a minor-2 reader must
        // still decode it (ADR-0025 D1 / DC-40).
        let mut s = GridSnapshot::new(3, 2);
        s.row_flags[0] = termai_core::grid::LINE_WRAPPED;
        let mut v = grid_snapshot_to_value(&s);
        if let Value::Map(entries) = &mut v {
            entries.retain(|(k, _)| k.as_text() != Some("row_flags"));
        }
        let back = grid_snapshot_from_value(&v).unwrap();
        assert_eq!(back.row_flags, vec![0, 0]);
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

    // ---- session attach family (0x05xx, ADR-0023 D1) ----

    fn hex(s: &str) -> Vec<u8> {
        let b = s.as_bytes();
        (0..b.len() / 2)
            .map(|i| {
                let hi = (b[2 * i] as char).to_digit(16).unwrap() as u8;
                let lo = (b[2 * i + 1] as char).to_digit(16).unwrap() as u8;
                (hi << 4) | lo
            })
            .collect()
    }

    fn sample_attach_request() -> AttachRequest {
        AttachRequest {
            proto_min: 1,
            proto_max: 3,
            client_kind: ClientKind::Cli,
            session_id: termai_core::SessionId(1),
            mode: AttachMode::ReadOnly,
            resume_from: Some(5),
            capabilities: vec![CapId(1), CapId(2)],
        }
    }

    fn sample_attach_ack() -> AttachAck {
        AttachAck {
            chosen_ver: 3,
            state: SessionState::Running,
            snapshot_ref: SnapshotRef {
                segment_id: 0,
                seq: 1,
                grid_digest: [0xAB; 32],
            },
            lease: Some(LeaseInfo {
                lease_id: 9,
                holder: 1,
                since: 0,
                ttl_s: 30,
            }),
            limits: AttachLimits {
                max_frame: ATTACH_MAX_FRAME,
                credits: 64,
            },
        }
    }

    /// Frozen golden vector: any field rename / reorder / type change breaks this.
    #[test]
    fn attach_request_matches_frozen_golden_bytes() {
        let bytes = to_bytes(&attach_request_to_value(&sample_attach_request()));
        let expected = hex(
            "A76970726F746F5F6D696E016970726F746F5F6D6178036B636C69656E745F6B696E6463636C69\
             6A73657373696F6E5F69645001000000000000000000000000000000646D6F646569726561645F\
             6F6E6C796B726573756D655F66726F6D056C6361706162696C6974696573820102",
        );
        assert_eq!(bytes, expected);
        assert_eq!(
            attach_request_from_value(&from_bytes(&bytes).unwrap()).unwrap(),
            sample_attach_request()
        );
        assert_eq!(ATTACH_MAX_FRAME, 8 << 20);
    }

    /// Frozen golden vector for ATTACH_ACK (with a granted lease).
    #[test]
    fn attach_ack_matches_frozen_golden_bytes() {
        let bytes = to_bytes(&attach_ack_to_value(&sample_attach_ack()));
        let mut expected = hex(
            "A56A63686F73656E5F766572036573746174656772756E6E696E676C736E617073686F745F726566\
             A36A7365676D656E745F69640063736571016B677269645F6469676573745820",
        );
        expected.extend_from_slice(&[0xAB; 32]);
        expected.extend_from_slice(&hex("656C65617365A4686C656173655F69640966686F6C64657250"));
        expected.extend_from_slice(&1u128.to_le_bytes());
        expected.extend_from_slice(&hex(
            "6573696E6365006574746C5F73181E666C696D697473A2696D61785F6672616D651A0080000067\
             637265646974731840",
        ));
        assert_eq!(bytes, expected);
        assert_eq!(
            attach_ack_from_value(&from_bytes(&bytes).unwrap()).unwrap(),
            sample_attach_ack()
        );
    }

    #[test]
    fn attach_request_round_trips_with_and_without_resume_from() {
        let mut r = sample_attach_request();
        r.mode = AttachMode::Interactive;
        r.resume_from = None;
        r.client_kind = ClientKind::Ide;
        let v = from_bytes(&to_bytes(&attach_request_to_value(&r))).unwrap();
        assert_eq!(attach_request_from_value(&v).unwrap(), r);
    }

    #[test]
    fn attach_ack_round_trips_without_a_lease() {
        let mut a = sample_attach_ack();
        a.lease = None;
        a.state = SessionState::Detached;
        let v = from_bytes(&to_bytes(&attach_ack_to_value(&a))).unwrap();
        assert_eq!(attach_ack_from_value(&v).unwrap(), a);
    }

    #[test]
    fn attach_missing_required_fields_are_rejected_not_defaulted() {
        assert!(attach_request_from_value(&Value::map(vec![("proto_min", u(1))])).is_err());
        assert!(attach_ack_from_value(&Value::map(vec![("chosen_ver", u(1))])).is_err());
        // snapshot_ref present, but grid_digest missing.
        let partial = Value::map(vec![
            ("chosen_ver", u(1)),
            ("state", Value::Text("running".into())),
            (
                "snapshot_ref",
                Value::map(vec![("segment_id", u(0)), ("seq", u(0))]),
            ),
            (
                "limits",
                Value::map(vec![("max_frame", u(0)), ("credits", u(0))]),
            ),
        ]);
        assert!(attach_ack_from_value(&partial).is_err());
    }

    #[test]
    fn attach_unknown_optional_fields_are_ignored_for_forward_compat() {
        let mut v = attach_request_to_value(&sample_attach_request());
        if let Value::Map(ref mut m) = v {
            m.push((Value::Text("future_field".into()), Value::U64(1)));
        }
        assert_eq!(
            attach_request_from_value(&v).unwrap(),
            sample_attach_request()
        );
    }

    #[test]
    fn attach_rejects_a_bad_mode_and_out_of_range_version() {
        let mut v = attach_request_to_value(&sample_attach_request());
        if let Value::Map(ref mut m) = v {
            for (k, val) in m.iter_mut() {
                if k.as_text() == Some("mode") {
                    *val = Value::Text("readwrite".into());
                }
            }
        }
        assert!(attach_request_from_value(&v).is_err());

        let mut v = attach_request_to_value(&sample_attach_request());
        if let Value::Map(ref mut m) = v {
            for (k, val) in m.iter_mut() {
                if k.as_text() == Some("proto_max") {
                    *val = Value::U64(0x1_0000);
                }
            }
        }
        assert!(attach_request_from_value(&v).is_err());
    }

    #[test]
    fn detach_notice_is_a_one_byte_pod() {
        assert_eq!(detach_notice_encode(false), vec![0]);
        assert_eq!(detach_notice_encode(true), vec![1]);
        assert_eq!(detach_notice_decode(&[0]), Ok(false));
        assert_eq!(detach_notice_decode(&[1]), Ok(true));
        assert_eq!(
            detach_notice_decode(&[]),
            Err(CodecError::Bad("DetachNotice payload"))
        );
        assert_eq!(
            detach_notice_decode(&[2]),
            Err(CodecError::Bad("DetachNotice payload"))
        );
        assert_eq!(
            detach_notice_decode(&[0, 1]),
            Err(CodecError::Bad("DetachNotice payload"))
        );
    }
}
