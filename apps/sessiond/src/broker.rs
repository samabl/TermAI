//! IPC broker: handshake, lease gate (kernel/07 CAP-1/CAP-2), input, snapshot chunking.
//!
//! Frame-layer errors are returned as Err (the caller closes the link). Message-layer
//! errors are returned as Error frames and do NOT disconnect (kernel/07 section 3.2).

use termai_core::capability::CapId;
use termai_core::grid::GridSnapshot;
use termai_core::time::MonoTime;
use termai_core::SessionId;
use termai_ipc::cbor::Value;
use termai_ipc::codec::{self, CodecError};
use termai_ipc::frame::{flag, FrameHeader, IpcError};
use termai_ipc::handshake::{self, Outcome, RefusedReason, ServerPolicy};
use termai_ipc::msg;
use termai_session::lease::LeaseState;

use crate::registry::{Registry, RegistryError};

/// Snapshot chunk size. Chunks travel as CBOR: a 1 MiB chunk would violate the
/// 4 KiB POD cap (ADR-0018 D2), which is exactly why the POD cap exists.
pub const SNAPSHOT_CHUNK: usize = 64 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConnState {
    Closed,
    Negotiated,
    Ready,
    Draining,
    Failed,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct OutFrame {
    pub msg_type: u16,
    pub flags: u16,
    pub corr_id: u64,
    pub payload: Vec<u8>,
}

impl OutFrame {
    pub fn encode(&self, ver: u16) -> Result<Vec<u8>, IpcError> {
        termai_ipc::encode_versioned(ver, self.msg_type, self.flags, self.corr_id, &self.payload)
    }

    fn cbor(msg_type: u16, flags: u16, corr_id: u64, v: &Value) -> Self {
        Self {
            msg_type,
            flags: flags | flag::ENC,
            corr_id,
            payload: codec::to_bytes(v),
        }
    }

    fn error(corr_id: u64, code: &str, detail: &str) -> Self {
        Self::cbor(
            msg::ERROR,
            flag::ERROR,
            corr_id,
            &codec::error_body(code, detail),
        )
    }
}

/// SessionError::NoSuchSession. kernel/04 section 3.8 registers SessionError as a trait
/// error type (not value-frozen), and kernel/07 section 3.8 has no ipc:: code for a
/// missing session, so the broker keeps the literal it already used for that condition.
const NO_SUCH_SESSION: &str = "NoSuchSession";

/// One live attach subscription. A repeated ATTACH_REQ on the same connection reuses the
/// same handle (kernel/04 section 3.4: duplicate attach returns the same subscription).
#[derive(Clone, PartialEq, Eq, Debug)]
struct AttachState {
    sub_id: u64,
    mode: codec::AttachMode,
    resume_from: Option<u64>,
}

/// The per-connection protocol state machine.
pub struct Broker {
    policy: ServerPolicy,
    registry: Registry,
    session: SessionId,
    state: ConnState,
    chosen_ver: u16,
    caps: Vec<CapId>,
    client: Option<u128>,
    lease: Option<u64>,
    attach: Option<AttachState>,
    next_sub_id: u64,
    now: MonoTime,
    pub input_accepted: u64,
    pub input_rejected: u64,
}

impl std::fmt::Debug for Broker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Broker")
            .field("state", &self.state)
            .field("chosen_ver", &self.chosen_ver)
            .field("session", &self.session)
            .finish()
    }
}

impl Broker {
    #[must_use]
    pub fn new(policy: ServerPolicy, registry: Registry, session: SessionId) -> Self {
        Self {
            policy,
            registry,
            session,
            state: ConnState::Closed,
            chosen_ver: termai_core::PROTO_VERSION,
            caps: Vec::new(),
            client: None,
            lease: None,
            attach: None,
            next_sub_id: 1,
            now: MonoTime::ZERO,
            input_accepted: 0,
            input_rejected: 0,
        }
    }

    /// Inject the monotonic clock (pure, testable: no clock acquisition inside).
    pub fn set_now(&mut self, now: MonoTime) {
        self.now = now;
    }

    #[must_use]
    pub const fn state(&self) -> ConnState {
        self.state
    }

    #[must_use]
    pub const fn chosen_ver(&self) -> u16 {
        self.chosen_ver
    }

    #[must_use]
    pub fn caps(&self) -> &[CapId] {
        &self.caps
    }

    #[must_use]
    pub const fn lease(&self) -> Option<u64> {
        self.lease
    }

    #[must_use]
    pub const fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut Registry {
        &mut self.registry
    }

    /// Handle one inbound frame. Ok(frames) means continue; Err closes the link.
    pub fn on_frame(&mut self, h: &FrameHeader, payload: &[u8]) -> Result<Vec<OutFrame>, IpcError> {
        if self.state == ConnState::Failed {
            return Err(IpcError::Corrupt);
        }
        match h.msg_type {
            msg::HELLO => self.on_hello(h.corr_id, payload),
            msg::PING => Ok(vec![OutFrame {
                msg_type: msg::PONG,
                flags: 0,
                corr_id: h.corr_id,
                payload: Vec::new(),
            }]),
            msg::GO_AWAY => {
                self.state = ConnState::Draining;
                Ok(Vec::new())
            }
            msg::LEASE_ACQUIRE => self.on_lease_acquire(h.corr_id, payload),
            msg::INPUT => self.on_input(h.corr_id, payload),
            msg::PASTE => self.on_paste(h.corr_id, payload),
            msg::RESIZE => self.on_resize(h.corr_id, payload),
            msg::SIGNAL => Ok(vec![OutFrame::error(h.corr_id, "Unsupported", "signal")]),
            msg::GRID_SNAPSHOT => self.on_snapshot_request(h.corr_id, payload),
            msg::ATTACH_REQUEST => self.on_attach_request(h.corr_id, payload),
            msg::DETACH_NOTICE => self.on_detach_notice(h.corr_id, payload),
            other => Ok(vec![match msg::classify(other) {
                msg::MsgClass::ReservedSection => OutFrame::error(
                    h.corr_id,
                    termai_core::error::ipc::RESERVED_MSG_TYPE,
                    "reserved section",
                ),
                _ => OutFrame::error(
                    h.corr_id,
                    termai_core::error::ipc::UNSUPPORTED_MSG,
                    "unsupported message type",
                ),
            }]),
        }
    }

    fn on_hello(&mut self, corr_id: u64, payload: &[u8]) -> Result<Vec<OutFrame>, IpcError> {
        if self.state != ConnState::Closed {
            return Ok(vec![OutFrame::error(
                corr_id,
                termai_core::error::ipc::CORRUPT,
                "hello already sent",
            )]);
        }
        let value = match codec::from_bytes(payload) {
            Ok(v) => v,
            Err(_) => {
                self.state = ConnState::Failed;
                return Ok(vec![OutFrame::cbor(
                    msg::HELLO_NACK,
                    0,
                    corr_id,
                    &codec::refusal_to_value(RefusedReason::PeerDenied),
                )]);
            }
        };
        let hello = match codec::hello_from_value(&value) {
            Ok(h) => h,
            Err(_) => {
                self.state = ConnState::Failed;
                return Ok(vec![OutFrame::cbor(
                    msg::HELLO_NACK,
                    0,
                    corr_id,
                    &codec::refusal_to_value(RefusedReason::PeerDenied),
                )]);
            }
        };
        match handshake::negotiate(&hello, &self.policy) {
            Outcome::Negotiated { ack, .. } => {
                self.chosen_ver = ack.chosen_ver;
                self.caps = ack.caps_inter.clone();
                self.state = ConnState::Negotiated;
                self.policy.seen_nonces.push(hello.nonce);
                Ok(vec![OutFrame::cbor(
                    msg::HELLO_ACK,
                    flag::RESPONSE,
                    corr_id,
                    &codec::ack_to_value(&ack),
                )])
            }
            Outcome::Refused(reason) => {
                self.state = ConnState::Failed;
                Ok(vec![OutFrame::cbor(
                    msg::HELLO_NACK,
                    0,
                    corr_id,
                    &codec::refusal_to_value(reason),
                )])
            }
        }
    }

    fn on_lease_acquire(
        &mut self,
        corr_id: u64,
        payload: &[u8],
    ) -> Result<Vec<OutFrame>, IpcError> {
        if self.state != ConnState::Negotiated && self.state != ConnState::Ready {
            return Ok(vec![OutFrame::error(
                corr_id,
                termai_core::error::ipc::CORRUPT,
                "handshake required",
            )]);
        }
        if payload.len() != 16 {
            return Ok(vec![OutFrame::error(
                corr_id,
                codec_error_code(CodecError::Bad("client id length")),
                "client id must be 16 bytes",
            )]);
        }
        let mut a = [0u8; 16];
        a.copy_from_slice(payload);
        let client = u128::from_le_bytes(a);
        match self.registry.acquire_lease(
            self.session,
            termai_session::state::ClientId(client),
            self.now,
        ) {
            Ok(id) => {
                self.client = Some(client);
                self.lease = Some(id.0);
                self.state = ConnState::Ready;
                Ok(vec![OutFrame {
                    msg_type: msg::LEASE_GRANT,
                    flags: flag::RESPONSE,
                    corr_id,
                    payload: id.0.to_le_bytes().to_vec(),
                }])
            }
            Err(_) => Ok(vec![OutFrame::cbor(
                msg::CAP_DENIED,
                flag::ERROR,
                corr_id,
                &codec::error_body(
                    termai_core::error::authz::MISSING,
                    "lease held by another client",
                ),
            )]),
        }
    }

    fn require_writer(&mut self, corr_id: u64) -> Option<Vec<OutFrame>> {
        let Some(client) = self.client else {
            self.input_rejected += 1;
            return Some(vec![OutFrame::cbor(
                msg::CAP_DENIED,
                flag::ERROR,
                corr_id,
                &codec::error_body(termai_core::error::authz::MISSING, "no lease"),
            )]);
        };
        if self.lease.is_none() {
            self.input_rejected += 1;
            return Some(vec![OutFrame::cbor(
                msg::CAP_DENIED,
                flag::ERROR,
                corr_id,
                &codec::error_body(termai_core::error::authz::MISSING, "no lease"),
            )]);
        }
        // Side-effect-free lease probe: it must never append a record.
        let e = self.registry.authorize_stdin(
            self.session,
            termai_session::state::ClientId(client),
            self.now,
        );
        match e {
            Ok(_) => None,
            Err(termai_session::lease::LeaseError::HeldByOther { .. }) => {
                self.input_rejected += 1;
                Some(vec![OutFrame::cbor(
                    msg::CAP_DENIED,
                    flag::ERROR,
                    corr_id,
                    &codec::error_body(termai_core::error::authz::SCOPE, "not the writer"),
                )])
            }
            Err(_) => {
                self.input_rejected += 1;
                Some(vec![OutFrame::cbor(
                    msg::CAP_DENIED,
                    flag::ERROR,
                    corr_id,
                    &codec::error_body(termai_core::error::authz::EXPIRED, "lease expired"),
                )])
            }
        }
    }

    fn on_input(&mut self, corr_id: u64, payload: &[u8]) -> Result<Vec<OutFrame>, IpcError> {
        let decoded = match codec::InputPayload::decode(payload) {
            Ok(p) => p,
            Err(e) => {
                return Ok(vec![OutFrame::error(
                    corr_id,
                    codec_error_code(e),
                    "bad input",
                )])
            }
        };
        if let Some(deny) = self.require_writer(corr_id) {
            return Ok(deny);
        }
        let client = self.client.unwrap_or(0);
        if self
            .registry
            .write_stdin(
                self.session,
                termai_session::state::ClientId(client),
                &decoded.payload,
                termai_session::log::Source::Human,
                decoded.seq,
                self.now,
            )
            .is_err()
        {
            self.input_rejected += 1;
            return Ok(vec![OutFrame::cbor(
                msg::CAP_DENIED,
                flag::ERROR,
                corr_id,
                &codec::error_body(termai_core::error::authz::SCOPE, "not the writer"),
            )]);
        }
        self.input_accepted += 1;
        // Acting frames must be auditable; the flag makes the audit obligation explicit.
        Ok(vec![OutFrame {
            msg_type: msg::CREDIT_UPDATE,
            flags: flag::AUDIT_REQUIRED,
            corr_id,
            payload: Vec::new(),
        }])
    }

    fn on_paste(&mut self, corr_id: u64, payload: &[u8]) -> Result<Vec<OutFrame>, IpcError> {
        let chunk = match codec::PasteChunk::decode(payload) {
            Ok(c) => c,
            Err(e) => {
                return Ok(vec![OutFrame::error(
                    corr_id,
                    codec_error_code(e),
                    "bad paste",
                )])
            }
        };
        if chunk.chunk_total > 1 {
            // Multi-chunk assembly lands with the shm/P1 window; refusing is honest.
            return Ok(vec![OutFrame::error(
                corr_id,
                "Unsupported",
                "chunked_paste_pending_p1",
            )]);
        }
        if let Some(deny) = self.require_writer(corr_id) {
            return Ok(deny);
        }
        let origin = match chunk.origin {
            0 => termai_core::input::PasteOrigin::LocalClipboard,
            1 => termai_core::input::PasteOrigin::Osc52,
            2 => termai_core::input::PasteOrigin::ApiInject,
            _ => termai_core::input::PasteOrigin::PluginInject,
        };
        let p = termai_core::input::PastePayload {
            data: chunk.data.clone(),
            origin,
            bracketed: false,
        };
        let d = termai_core::input::paste_gate(&p, &termai_core::input::PasteCtx::default());
        if !d.allow {
            return Ok(vec![OutFrame::cbor(
                msg::CAP_DENIED,
                flag::ERROR,
                corr_id,
                &codec::error_body(termai_core::error::authz::APPROVAL, d.reason),
            )]);
        }
        let client = self.client.unwrap_or(0);
        self.registry
            .write_stdin(
                self.session,
                termai_session::state::ClientId(client),
                &d.sanitized,
                termai_session::log::Source::Human,
                chunk.seq,
                self.now,
            )
            .map_err(|_| IpcError::Corrupt)?;
        self.input_accepted += 1;
        Ok(vec![OutFrame {
            msg_type: msg::CREDIT_UPDATE,
            flags: flag::AUDIT_REQUIRED,
            corr_id,
            payload: Vec::new(),
        }])
    }

    fn on_resize(&mut self, corr_id: u64, payload: &[u8]) -> Result<Vec<OutFrame>, IpcError> {
        let r = match codec::ResizePayload::decode(payload) {
            Ok(r) => r,
            Err(e) => {
                return Ok(vec![OutFrame::error(
                    corr_id,
                    codec_error_code(e),
                    "bad resize",
                )])
            }
        };
        if let Some(deny) = self.require_writer(corr_id) {
            return Ok(deny);
        }
        if let Some(e) = self.registry.get_mut(self.session) {
            e.engine.resize(r.cols, r.rows);
            let _ = e.writer.append(
                &termai_session::log::Record::Resize {
                    pane: 0,
                    cols: r.cols,
                    rows: r.rows,
                    px_w: r.px_w,
                    px_h: r.px_h,
                },
                0,
            );
        }
        Ok(vec![OutFrame {
            msg_type: msg::RESIZE,
            flags: flag::RESPONSE,
            corr_id,
            payload: Vec::new(),
        }])
    }

    fn on_snapshot_request(
        &mut self,
        corr_id: u64,
        payload: &[u8],
    ) -> Result<Vec<OutFrame>, IpcError> {
        if self.state != ConnState::Negotiated && self.state != ConnState::Ready {
            return Ok(vec![OutFrame::error(
                corr_id,
                termai_core::error::ipc::CORRUPT,
                "handshake required",
            )]);
        }
        if !payload.is_empty() {
            return Ok(vec![OutFrame::error(
                corr_id,
                codec_error_code(CodecError::Bad("snapshot request payload")),
                "snapshot request must be empty",
            )]);
        }
        // ADR-0023 D1 voids the M0 idiom where an empty GRID_SNAPSHOT was itself the
        // attach: the snapshot is now the transport leg of the attach flow (kernel/04
        // section 3.4: ATTACH_REQ -> ATTACH_ACK -> GRID_SNAPSHOT), so it needs a live
        // subscription. After DETACH_NOTICE releases it, snapshot requests are refused.
        if self.attach.is_none() {
            return Ok(vec![OutFrame::error(
                corr_id,
                termai_core::error::ipc::CORRUPT,
                "attach required",
            )]);
        }
        let snapshot = match self.registry.snapshot(self.session) {
            Ok(s) => s,
            Err(RegistryError::NoSuchSession) => {
                return Ok(vec![OutFrame::error(
                    corr_id,
                    "NoSuchSession",
                    "session not found",
                )])
            }
            Err(_) => {
                return Ok(vec![OutFrame::error(
                    corr_id,
                    termai_core::error::ipc::CORRUPT,
                    "snapshot failed",
                )])
            }
        };
        Ok(self.chunk_snapshot(corr_id, &snapshot))
    }

    /// ATTACH_REQUEST (0x0500): validate version range, session, mode semantics and
    /// idempotency, then answer ATTACH_ACK. Interactive mode does NOT acquire stdin
    /// write rights; that still requires an explicit LEASE_ACQUIRE (WS-05a).
    fn on_attach_request(
        &mut self,
        corr_id: u64,
        payload: &[u8],
    ) -> Result<Vec<OutFrame>, IpcError> {
        if self.state != ConnState::Negotiated && self.state != ConnState::Ready {
            return Ok(vec![OutFrame::error(
                corr_id,
                termai_core::error::ipc::CORRUPT,
                "handshake required",
            )]);
        }
        let req =
            match codec::from_bytes(payload).and_then(|v| codec::attach_request_from_value(&v)) {
                Ok(r) => r,
                Err(e) => {
                    return Ok(vec![OutFrame::error(
                        corr_id,
                        codec_error_code(e),
                        "bad attach request",
                    )])
                }
            };
        // Version range: reuse the connection handshake intersection, never a second
        // implementation (kernel/07 section 3.3).
        let Some(chosen_ver) = handshake::chosen_version(
            req.proto_min,
            req.proto_max,
            self.policy.proto_min,
            self.policy.proto_max,
        ) else {
            return Ok(vec![OutFrame::cbor(
                msg::ERROR,
                flag::ERROR,
                corr_id,
                &codec::error_body(
                    termai_core::error::handshake::VER_UNSUPPORTED,
                    "attach version range has no overlap",
                ),
            )]);
        };
        // The connection is session-scoped: only the bound session may be attached.
        if req.session_id != self.session || self.registry.get(req.session_id).is_none() {
            return Ok(vec![OutFrame::error(
                corr_id,
                NO_SUCH_SESSION,
                "session not found",
            )]);
        }
        let Some((segment_id, seq)) = self.registry.log_position(self.session) else {
            return Ok(vec![OutFrame::error(
                corr_id,
                NO_SUCH_SESSION,
                "session not found",
            )]);
        };
        let grid_digest = match self.registry.digest(self.session) {
            Ok(d) => d,
            Err(_) => {
                return Ok(vec![OutFrame::error(
                    corr_id,
                    NO_SUCH_SESSION,
                    "session not found",
                )])
            }
        };
        let state = self
            .registry
            .state(self.session)
            .and_then(|s| codec::SessionState::parse(s.as_str()).ok())
            .unwrap_or(codec::SessionState::Dead);
        // Duplicate attach returns the same subscription handle.
        let sub_id = match &self.attach {
            Some(a) => a.sub_id,
            None => {
                let id = self.next_sub_id;
                self.next_sub_id += 1;
                id
            }
        };
        self.attach = Some(AttachState {
            sub_id,
            mode: req.mode,
            resume_from: req.resume_from,
        });
        let ack = codec::AttachAck {
            chosen_ver,
            state,
            snapshot_ref: codec::SnapshotRef {
                segment_id,
                seq,
                grid_digest,
            },
            lease: self.current_lease_info(),
            limits: codec::AttachLimits {
                max_frame: codec::ATTACH_MAX_FRAME,
                credits: self.policy.limits.credits,
            },
        };
        Ok(vec![OutFrame::cbor(
            msg::ATTACH_ACK,
            flag::RESPONSE,
            corr_id,
            &codec::attach_ack_to_value(&ack),
        )])
    }

    /// The lease this connection currently holds, as ATTACH_ACK.lease. None when the
    /// connection is not the writer, so a read-only client learns it has no write rights.
    fn current_lease_info(&self) -> Option<codec::LeaseInfo> {
        let client = self.client?;
        let lease_id = self.lease?;
        let entry = self.registry.get(self.session)?;
        match entry.lease.state() {
            LeaseState::Held {
                holder,
                acquired_at,
                ..
            } if holder.0 == client => Some(codec::LeaseInfo {
                lease_id,
                holder: holder.0,
                since: acquired_at.as_nanos(),
                ttl_s: entry.lease.ttl().as_secs() as u32,
            }),
            _ => None,
        }
    }

    /// DETACH_NOTICE (0x0503): release the read subscription and, on request, the lease.
    /// Idempotent, and it never bypasses the lease authority: release goes through
    /// Registry::release_lease, which requires the caller to be the holder (CAP gate).
    fn on_detach_notice(
        &mut self,
        corr_id: u64,
        payload: &[u8],
    ) -> Result<Vec<OutFrame>, IpcError> {
        if self.state != ConnState::Negotiated && self.state != ConnState::Ready {
            return Ok(vec![OutFrame::error(
                corr_id,
                termai_core::error::ipc::CORRUPT,
                "handshake required",
            )]);
        }
        let lease_release = match codec::detach_notice_decode(payload) {
            Ok(v) => v,
            Err(e) => {
                return Ok(vec![OutFrame::error(
                    corr_id,
                    codec_error_code(e),
                    "bad detach notice",
                )])
            }
        };
        // Detaching without an attach is a no-op (idempotent); DETACH_NOTICE has no reply
        // in kernel/04 section 3.4.
        self.attach = None;
        if lease_release {
            if let (Some(client), Some(lease_id)) = (self.client, self.lease) {
                let _ = self.registry.release_lease(
                    self.session,
                    termai_session::lease::LeaseId(lease_id),
                    termai_session::state::ClientId(client),
                    self.now,
                );
            }
            self.lease = None;
            if self.state == ConnState::Ready {
                self.state = ConnState::Negotiated;
            }
        }
        Ok(Vec::new())
    }

    /// Chunk a snapshot into CBOR frames with MORE_CHUNK / LAST_CHUNK flags.
    #[must_use]
    pub fn chunk_snapshot(&self, corr_id: u64, s: &GridSnapshot) -> Vec<OutFrame> {
        let bytes = codec::to_bytes(&codec::grid_snapshot_to_value(s));
        let digest = *blake3::hash(&bytes).as_bytes();
        let total = bytes.len().div_ceil(SNAPSHOT_CHUNK).max(1);
        let mut out = Vec::with_capacity(total);
        for (idx, part) in bytes.chunks(SNAPSHOT_CHUNK).enumerate() {
            let last = idx + 1 == total;
            let v = Value::map(vec![
                ("grid_digest", Value::Bytes(digest.to_vec())),
                ("chunk_idx", Value::U64(idx as u64)),
                ("chunk_total", Value::U64(total as u64)),
                ("data", Value::Bytes(part.to_vec())),
            ]);
            out.push(OutFrame::cbor(
                msg::GRID_SNAPSHOT,
                flag::ENC
                    | if last {
                        flag::LAST_CHUNK
                    } else {
                        flag::MORE_CHUNK
                    },
                corr_id,
                &v,
            ));
        }
        out
    }

    /// True when a ReadOnly attach subscription is live.
    #[must_use]
    pub fn read_only_attached(&self) -> bool {
        matches!(&self.attach, Some(a) if a.mode == codec::AttachMode::ReadOnly)
    }

    /// The subscription handle of the live attach, if any. A repeated attach reuses it.
    #[must_use]
    pub fn attach_subscription(&self) -> Option<u64> {
        self.attach.as_ref().map(|a| a.sub_id)
    }

    /// The live attach mode, if any.
    #[must_use]
    pub fn attach_mode(&self) -> Option<codec::AttachMode> {
        self.attach.as_ref().map(|a| a.mode)
    }

    /// The resume_from the client last claimed, if any.
    #[must_use]
    pub fn attach_resume_from(&self) -> Option<u64> {
        self.attach.as_ref().and_then(|a| a.resume_from)
    }
}

/// Map a codec failure onto an already-registered IPC error code (no new codes).
#[must_use]
pub fn codec_error_code(_e: CodecError) -> &'static str {
    termai_core::error::ipc::CORRUPT
}

/// Reassemble chunked snapshot frames. Returns the payload and the digest.
pub fn reassemble_snapshot(frames: &[OutFrame]) -> Result<(Vec<u8>, [u8; 32]), CodecError> {
    let mut buf = Vec::new();
    let mut digest = [0u8; 32];
    let mut expected_total = 0usize;
    for f in frames {
        if f.msg_type != msg::GRID_SNAPSHOT {
            continue;
        }
        let v = codec::from_bytes(&f.payload)?;
        let d = match v.get("grid_digest") {
            Some(Value::Bytes(b)) if b.len() == 32 => {
                let mut a = [0u8; 32];
                a.copy_from_slice(b);
                a
            }
            _ => return Err(CodecError::Missing("grid_digest")),
        };
        if digest == [0u8; 32] {
            digest = d;
        }
        let total = v
            .get("chunk_total")
            .and_then(Value::as_u64)
            .ok_or(CodecError::Missing("chunk_total"))? as usize;
        if expected_total == 0 {
            expected_total = total;
        }
        match v.get("data") {
            Some(Value::Bytes(b)) => buf.extend_from_slice(b),
            _ => return Err(CodecError::Missing("data")),
        }
    }
    if expected_total == 0 {
        return Err(CodecError::Missing("no snapshot chunks"));
    }
    Ok((buf, digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::testing::TextEngine;
    use std::path::PathBuf;
    use termai_core::capability::{CAP_AUDIT_READ, CAP_SESSION_READ, CAP_STDIN_WRITE};
    use termai_ipc::handshake::{AuthState, ClientKind, Hello, Limits};
    use termai_session::log::SegmentWriter;

    fn tmp_dir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "termai-broker-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn policy() -> ServerPolicy {
        ServerPolicy {
            proto_min: 1,
            proto_max: 3,
            caps: vec![CAP_SESSION_READ, CAP_STDIN_WRITE, CAP_AUDIT_READ],
            required: vec![CAP_SESSION_READ],
            limits: Limits::default(),
            auth_state: AuthState::LocalPeer,
            feature_bits: 0,
            seen_nonces: Vec::new(),
        }
    }

    fn broker(tag: &str) -> (Broker, SessionId) {
        let dir = tmp_dir(tag);
        let id = SessionId(7);
        let mut reg = Registry::new();
        let w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, 0).unwrap();
        reg.insert(id, Box::new(TextEngine::new(80, 24)), w, 1)
            .unwrap();
        (Broker::new(policy(), reg, id), id)
    }

    fn hello() -> Hello {
        Hello {
            proto_min: 1,
            proto_max: 3,
            client_kind: ClientKind::Cli,
            caps: vec![CAP_SESSION_READ, CAP_STDIN_WRITE, CAP_AUDIT_READ],
            required: vec![CAP_SESSION_READ],
            session_claim: SessionId(7),
            feature_bits: 0,
            nonce: [3u8; 16],
        }
    }

    fn hdr(msg_type: u16, flags: u16, corr_id: u64) -> FrameHeader {
        FrameHeader {
            len: 0,
            ver: termai_core::PROTO_VERSION,
            msg_type,
            flags,
            rsv: 0,
            corr_id,
            crc32c: 0,
        }
    }

    fn do_hello(b: &mut Broker) -> Vec<OutFrame> {
        let payload = codec::to_bytes(&codec::hello_to_value(&hello()));
        b.on_frame(&hdr(msg::HELLO, flag::ENC, 1), &payload)
            .unwrap()
    }

    #[test]
    fn handshake_is_answered_with_ack_and_negotiated_state() {
        let (mut b, _) = broker("hello");
        let out = do_hello(&mut b);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].msg_type, msg::HELLO_ACK);
        assert_eq!(b.state(), ConnState::Negotiated);
        assert_eq!(b.chosen_ver(), 3);
        let ack = codec::ack_from_value(&codec::from_bytes(&out[0].payload).unwrap()).unwrap();
        assert_eq!(ack.caps_inter.len(), 3);
        assert!(!ack.degraded);
    }

    #[test]
    fn version_mismatch_is_refused_with_a_nack_and_exit_code() {
        let (mut b, _) = broker("ver");
        let mut h = hello();
        h.proto_min = 9;
        h.proto_max = 10;
        let payload = codec::to_bytes(&codec::hello_to_value(&h));
        let out = b
            .on_frame(&hdr(msg::HELLO, flag::ENC, 1), &payload)
            .unwrap();
        assert_eq!(out[0].msg_type, msg::HELLO_NACK);
        let r = codec::refusal_from_value(&codec::from_bytes(&out[0].payload).unwrap()).unwrap();
        assert_eq!(r, RefusedReason::VerUnsupported);
        assert_eq!(r.cli_exit(), 4);
        assert_eq!(b.state(), ConnState::Failed);
    }

    #[test]
    fn input_without_a_lease_is_denied_with_cap_denied() {
        let (mut b, _) = broker("nolease");
        do_hello(&mut b);
        let payload = codec::InputPayload {
            seq: 1,
            kind: 0,
            flags: 0,
            payload: b"ls".to_vec(),
        }
        .encode()
        .unwrap();
        let out = b.on_frame(&hdr(msg::INPUT, 0, 2), &payload).unwrap();
        assert_eq!(out[0].msg_type, msg::CAP_DENIED);
        assert_eq!(b.input_rejected, 1);
        assert_eq!(b.input_accepted, 0);
    }

    #[test]
    fn lease_then_input_is_accepted_and_audited() {
        let (mut b, id) = broker("lease");
        do_hello(&mut b);
        let client = 0xABCu128;
        let out = b
            .on_frame(&hdr(msg::LEASE_ACQUIRE, 0, 2), &client.to_le_bytes())
            .unwrap();
        assert_eq!(out[0].msg_type, msg::LEASE_GRANT);
        assert_eq!(b.state(), ConnState::Ready);

        let payload = codec::InputPayload {
            seq: 1,
            kind: 0,
            flags: 0,
            payload: b"echo hi".to_vec(),
        }
        .encode()
        .unwrap();
        let out = b.on_frame(&hdr(msg::INPUT, 0, 3), &payload).unwrap();
        assert_eq!(out[0].msg_type, msg::CREDIT_UPDATE);
        assert_ne!(
            out[0].flags & flag::AUDIT_REQUIRED,
            0,
            "acting frames must be auditable"
        );
        assert_eq!(b.input_accepted, 1);
        assert_eq!(
            b.registry().state(id),
            Some(termai_session::state::SessionState::Created)
        );
    }

    #[test]
    fn the_lease_probe_never_appends_a_spurious_record() {
        let (mut b, id) = broker("noprobe");
        do_hello(&mut b);
        b.on_frame(&hdr(msg::LEASE_ACQUIRE, 0, 2), &1u128.to_le_bytes())
            .unwrap();
        let payload = codec::InputPayload {
            seq: 1,
            kind: 0,
            flags: 0,
            payload: b"one".to_vec(),
        }
        .encode()
        .unwrap();
        b.on_frame(&hdr(msg::INPUT, 0, 3), &payload).unwrap();
        b.registry_mut().flush(id).unwrap();
        let path = b.registry().log_path(id).unwrap();
        let read = termai_session::log::read_segment(&path).unwrap();
        let ptyin = read
            .records
            .iter()
            .filter(|r| matches!(r.record, termai_session::log::Record::PtyIn { .. }))
            .count();
        assert_eq!(ptyin, 1, "exactly one PtyIn record per accepted input");
    }

    #[test]
    fn second_client_cannot_take_the_lease() {
        let (mut b, _) = broker("contend");
        do_hello(&mut b);
        b.on_frame(&hdr(msg::LEASE_ACQUIRE, 0, 2), &1u128.to_le_bytes())
            .unwrap();
        let out = b
            .on_frame(&hdr(msg::LEASE_ACQUIRE, 0, 3), &2u128.to_le_bytes())
            .unwrap();
        assert_eq!(out[0].msg_type, msg::CAP_DENIED);
    }

    #[test]
    fn multiline_paste_requires_confirmation_and_is_refused_not_silently_sent() {
        let (mut b, _) = broker("paste");
        do_hello(&mut b);
        b.on_frame(&hdr(msg::LEASE_ACQUIRE, 0, 2), &1u128.to_le_bytes())
            .unwrap();
        let chunk = codec::PasteChunk {
            seq: 1,
            origin: 0,
            chunk_idx: 0,
            chunk_total: 1,
            total_len: 3,
            data: b"a\nb".to_vec(),
        };
        let out = b
            .on_frame(&hdr(msg::PASTE, 0, 3), &chunk.encode().unwrap())
            .unwrap();
        assert_eq!(out[0].msg_type, msg::CAP_DENIED);
        let body = codec::from_bytes(&out[0].payload).unwrap();
        assert_eq!(
            body.get("detail").and_then(Value::as_text),
            Some("multiline_unbracketed")
        );
    }

    #[test]
    fn chunked_oversize_paste_is_refused_explicitly() {
        let (mut b, _) = broker("bigpaste");
        do_hello(&mut b);
        let chunk = codec::PasteChunk {
            seq: 1,
            origin: 0,
            chunk_idx: 0,
            chunk_total: 3,
            total_len: 100000,
            data: b"x".to_vec(),
        };
        let out = b
            .on_frame(&hdr(msg::PASTE, 0, 2), &chunk.encode().unwrap())
            .unwrap();
        assert_eq!(out[0].msg_type, msg::ERROR);
        let body = codec::from_bytes(&out[0].payload).unwrap();
        assert_eq!(
            body.get("detail").and_then(Value::as_text),
            Some("chunked_paste_pending_p1")
        );
    }

    fn attach_payload(mode: codec::AttachMode) -> Vec<u8> {
        codec::to_bytes(&codec::attach_request_to_value(&codec::AttachRequest {
            proto_min: 1,
            proto_max: 3,
            client_kind: ClientKind::Cli,
            session_id: SessionId(7),
            mode,
            resume_from: None,
            capabilities: vec![CAP_SESSION_READ, CAP_STDIN_WRITE],
        }))
    }

    #[test]
    fn snapshot_without_attach_is_refused_since_adr_0023_d1() {
        let (mut b, _) = broker("snapshot-noattach");
        do_hello(&mut b);
        let out = b.on_frame(&hdr(msg::GRID_SNAPSHOT, 0, 2), &[]).unwrap();
        assert_eq!(out[0].msg_type, msg::ERROR);
        assert_eq!(b.state(), ConnState::Negotiated, "link survives");
        assert!(!b.read_only_attached());
    }

    #[test]
    fn snapshot_request_returns_chunks_that_reassemble_exactly() {
        let (mut b, id) = broker("snapshot");
        do_hello(&mut b);
        let ack = b
            .on_frame(
                &hdr(msg::ATTACH_REQUEST, flag::ENC, 2),
                &attach_payload(codec::AttachMode::ReadOnly),
            )
            .unwrap();
        assert_eq!(ack[0].msg_type, msg::ATTACH_ACK);
        b.registry_mut()
            .feed_pty_out(id, b"hello world", 5)
            .unwrap();
        let out = b.on_frame(&hdr(msg::GRID_SNAPSHOT, 0, 3), &[]).unwrap();
        assert!(!out.is_empty());
        let (bytes, digest) = reassemble_snapshot(&out).unwrap();
        assert_eq!(digest, *blake3::hash(&bytes).as_bytes());
        let snap = codec::grid_snapshot_from_value(&codec::from_bytes(&bytes).unwrap()).unwrap();
        assert_eq!(snap, b.registry().snapshot(id).unwrap());
        assert_eq!(
            out.last().unwrap().flags & flag::LAST_CHUNK,
            flag::LAST_CHUNK
        );
    }

    #[test]
    fn unknown_type_in_a_known_section_gets_error_but_keeps_the_link() {
        let (mut b, _) = broker("unknown");
        do_hello(&mut b);
        let out = b.on_frame(&hdr(0x01FF, 0, 9), &[]).unwrap();
        assert_eq!(out[0].msg_type, msg::ERROR);
        assert_eq!(b.state(), ConnState::Negotiated, "link must survive");
        let body = codec::from_bytes(&out[0].payload).unwrap();
        assert_eq!(
            body.get("code").and_then(Value::as_text),
            Some(termai_core::error::ipc::UNSUPPORTED_MSG)
        );
    }

    #[test]
    fn reserved_section_is_reported_as_reserved() {
        let (mut b, _) = broker("reserved");
        do_hello(&mut b);
        let out = b.on_frame(&hdr(0x0600, 0, 9), &[]).unwrap();
        let body = codec::from_bytes(&out[0].payload).unwrap();
        assert_eq!(
            body.get("code").and_then(Value::as_text),
            Some(termai_core::error::ipc::RESERVED_MSG_TYPE)
        );
    }

    #[test]
    fn signal_is_refused_with_the_registered_unsupported_code() {
        let (mut b, _) = broker("signal");
        do_hello(&mut b);
        let out = b
            .on_frame(
                &hdr(msg::SIGNAL, 0, 2),
                &codec::SignalPayload { sig: 0 }.encode(),
            )
            .unwrap();
        assert_eq!(out[0].msg_type, msg::ERROR);
        let body = codec::from_bytes(&out[0].payload).unwrap();
        assert_eq!(
            body.get("code").and_then(Value::as_text),
            Some("Unsupported")
        );
    }

    #[test]
    fn ping_is_answered_with_the_same_corr_id() {
        let (mut b, _) = broker("ping");
        do_hello(&mut b);
        let out = b.on_frame(&hdr(msg::PING, 0, 42), &[]).unwrap();
        assert_eq!(out[0].msg_type, msg::PONG);
        assert_eq!(out[0].corr_id, 42);
    }

    #[test]
    fn reassembling_nothing_is_an_error() {
        assert_eq!(
            reassemble_snapshot(&[]),
            Err(CodecError::Missing("no snapshot chunks"))
        );
    }
}
