//! On-the-wire protocol test: requests are really encoded to bytes, pushed through a
//! Link, decoded with CRC verification, then handed to the broker. Replies are encoded
//! again and decoded back. This proves the M0 daemon contract without a real PTY.

use sessiond::broker::{chunk_tail_replay, reassemble_snapshot, Broker, ConnState, OutFrame};
use sessiond::link::{Link, MemoryLink};
use sessiond::registry::{testing::TextEngine, Registry};

use termai_core::capability::{CAP_AUDIT_READ, CAP_SESSION_READ, CAP_STDIN_WRITE};
use termai_core::SessionId;
use termai_ipc::cbor::Value;
use termai_ipc::codec;
use termai_ipc::frame::{self, flag, DecodeCfg, FrameHeader};
use termai_ipc::handshake::{AuthState, ClientKind, Hello, Limits, ServerPolicy};
use termai_ipc::msg;
use termai_session::log::{read_segment, Record, SegmentWriter};

const VER: u16 = termai_core::PROTO_VERSION;

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

fn hello() -> Hello {
    Hello {
        proto_min: 1,
        proto_max: 3,
        client_kind: ClientKind::Cli,
        caps: vec![CAP_SESSION_READ, CAP_STDIN_WRITE, CAP_AUDIT_READ],
        required: vec![CAP_SESSION_READ],
        session_claim: SessionId(1),
        feature_bits: 0,
        nonce: [5u8; 16],
    }
}

struct Harness {
    broker: Broker,
    session: SessionId,
    link: MemoryLink,
}

impl Harness {
    fn new(tag: &str) -> Self {
        Self::new_from(tag, 0)
    }

    /// A session whose Log starts at `first_seq`, i.e. older records were archived
    /// (AR-26: P0 metadata is durable but may be trimmed).
    fn new_from(tag: &str, first_seq: u64) -> Self {
        let mut dir = std::env::temp_dir();
        dir.push(format!("termai-wire-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let id = SessionId(1);
        let mut reg = Registry::new();
        let w = SegmentWriter::create(&dir, 0, first_seq, [0u8; 8], 1, 0).unwrap();
        reg.insert(id, Box::new(TextEngine::new(80, 24)), w, 1)
            .unwrap();
        Self {
            broker: Broker::new(policy(), reg, id),
            session: id,
            link: MemoryLink::new(),
        }
    }

    /// Encode a request into the link, then decode it back out and hand it to the
    /// broker. Returns the broker replies re-encoded and re-decoded, i.e. real bytes
    /// both ways. Ok(None) means the frame layer rejected the bytes.
    fn send(
        &mut self,
        msg_type: u16,
        flags: u16,
        corr_id: u64,
        payload: &[u8],
    ) -> Result<Vec<OutFrame>, termai_ipc::IpcError> {
        let wire = frame::encode(msg_type, flags, corr_id, payload)?;
        self.link.write_all(&wire).expect("link write");
        let mut buf = self.link.take_outbound();
        let (header, payload) = {
            let f = match frame::decode(&buf, DecodeCfg::verifying())? {
                Some(f) => f,
                None => return Err(termai_ipc::IpcError::NeedMore),
            };
            (f.header, f.payload.to_vec())
        };
        let total = frame::FRAME_HEADER_LEN + header.len as usize;
        assert_eq!(total, buf.len(), "exactly one frame per exchange");
        buf.drain(..total);
        assert!(buf.is_empty());

        let replies = self.broker.on_frame(&header, &payload)?;
        // Verify each reply really survives an encode/decode round trip.
        for r in &replies {
            let bytes = r.encode(VER).expect("encode reply");
            let back = frame::decode(&bytes, DecodeCfg::verifying())
                .expect("decode reply")
                .expect("a whole reply frame");
            assert_eq!(back.header.msg_type, r.msg_type);
            assert_eq!(back.payload, r.payload.as_slice());
        }
        Ok(replies)
    }

    fn hello(&mut self) -> Vec<OutFrame> {
        self.send(
            msg::HELLO,
            flag::ENC,
            1,
            &codec::to_bytes(&codec::hello_to_value(&hello())),
        )
        .expect("hello")
    }

    fn attach_payload_range(
        &self,
        mode: codec::AttachMode,
        resume_from: Option<u64>,
        min: u16,
        max: u16,
    ) -> Vec<u8> {
        codec::to_bytes(&codec::attach_request_to_value(&codec::AttachRequest {
            proto_min: min,
            proto_max: max,
            client_kind: ClientKind::Cli,
            session_id: self.session,
            mode,
            resume_from,
            capabilities: vec![CAP_SESSION_READ, CAP_STDIN_WRITE],
        }))
    }

    fn attach_payload(&self, mode: codec::AttachMode, resume_from: Option<u64>) -> Vec<u8> {
        self.attach_payload_range(mode, resume_from, 1, 3)
    }
}

/// Decode the single ATTACH_ACK frame produced by a successful attach.
fn decode_attach_ack(frames: &[OutFrame]) -> codec::AttachAck {
    assert_eq!(frames.len(), 1, "ATTACH_ACK is a single frame");
    assert_eq!(frames[0].msg_type, msg::ATTACH_ACK);
    codec::attach_ack_from_value(&codec::from_bytes(&frames[0].payload).unwrap()).unwrap()
}

#[test]
fn full_wire_sequence_handshake_lease_input_snapshot() {
    let mut h = Harness::new("seq");

    // 1. Handshake.
    let out = h.hello();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].msg_type, msg::HELLO_ACK);
    let ack = codec::ack_from_value(&codec::from_bytes(&out[0].payload).unwrap()).unwrap();
    assert_eq!(ack.chosen_ver, 3, "the highest common version wins");
    assert!(!ack.degraded);
    assert_eq!(h.broker.state(), ConnState::Negotiated);

    // 2. Read-only attach succeeds before any lease (kernel/04 section 3.4; the attach
    //    family is the only attach path since ADR-0023 D1).
    let payload = h.attach_payload(codec::AttachMode::ReadOnly, None);
    let out = h.send(msg::ATTACH_REQUEST, flag::ENC, 2, &payload).unwrap();
    let ack = decode_attach_ack(&out);
    assert_eq!(ack.chosen_ver, 3);
    assert_eq!(ack.limits.max_frame, codec::ATTACH_MAX_FRAME);
    assert_eq!(
        ack.snapshot_ref.grid_digest,
        h.broker.registry().digest(h.session).unwrap()
    );
    assert!(h.broker.read_only_attached());
    assert_eq!(h.broker.state(), ConnState::Negotiated, "no lease yet");

    // 2b. The snapshot is the transport leg of the attach flow.
    let chunks = h.send(msg::GRID_SNAPSHOT, 0, 20, &[]).unwrap();
    assert!(!chunks.is_empty());
    let (bytes, digest) = reassemble_snapshot(&chunks).unwrap();
    assert_eq!(digest, *blake3::hash(&bytes).as_bytes());
    let snap = codec::grid_snapshot_from_value(&codec::from_bytes(&bytes).unwrap()).unwrap();
    assert_eq!(snap, h.broker.registry().snapshot(h.session).unwrap());

    // 3. Writer acquisition.
    let out = h
        .send(msg::LEASE_ACQUIRE, 0, 3, &9u128.to_le_bytes())
        .unwrap();
    assert_eq!(out[0].msg_type, msg::LEASE_GRANT);
    assert_eq!(h.broker.state(), ConnState::Ready);

    // 4. Input is accepted, flagged auditable, and never logged in plaintext.
    let input = codec::InputPayload {
        seq: 1,
        kind: 0,
        flags: 0,
        payload: b"echo m0".to_vec(),
    }
    .encode()
    .unwrap();
    let out = h.send(msg::INPUT, 0, 4, &input).unwrap();
    assert_eq!(out[0].msg_type, msg::CREDIT_UPDATE);
    assert_ne!(out[0].flags & flag::AUDIT_REQUIRED, 0);
    assert_eq!(h.broker.input_accepted, 1);

    // 5. PTY output arrives and the snapshot reflects it.
    h.broker
        .registry_mut()
        .feed_pty_out(h.session, b"m0-output", 5)
        .unwrap();
    let chunks = h.send(msg::GRID_SNAPSHOT, 0, 6, &[]).unwrap();
    let (bytes, _) = reassemble_snapshot(&chunks).unwrap();
    let snap = codec::grid_snapshot_from_value(&codec::from_bytes(&bytes).unwrap()).unwrap();
    assert_eq!(snap.row_text(0), "m0-output");

    // 6. The Log keeps the exact output bytes (F0) and only a digest of the input.
    h.broker.registry_mut().flush(h.session).unwrap();
    let path = h.broker.registry().log_path(h.session).unwrap();
    let read = read_segment(&path).unwrap();
    assert!(!read.tail_truncated);
    let ptyout = read
        .records
        .iter()
        .find_map(|r| match &r.record {
            Record::PtyOut { bytes, .. } => Some(bytes.clone()),
            _ => None,
        })
        .expect("a PtyOut record");
    assert_eq!(ptyout, b"m0-output", "F0: the Log stores the exact bytes");
    let raw = std::fs::read(&path).unwrap();
    assert!(
        !raw.windows(4).any(|w| w == b"echo"),
        "input plaintext must never reach the Log"
    );
}

#[test]
fn the_link_rejects_a_corrupted_frame_before_the_broker_sees_it() {
    let h = Harness::new("crc");
    let mut wire = frame::encode(msg::PING, 0, 1, &[]).unwrap();
    let n = wire.len() - 1;
    wire[n] ^= 0xFF;
    assert_eq!(
        frame::decode(&wire, DecodeCfg::verifying()),
        Err(termai_ipc::IpcError::CrcMismatch)
    );
    assert_eq!(h.broker.state(), ConnState::Closed, "the broker never ran");
}

#[test]
fn a_reserved_flag_bit_is_rejected_as_a_frame_error() {
    let mut wire = frame::encode(msg::PING, 0, 1, &[]).unwrap();
    wire[9] |= 0xF0;
    assert_eq!(
        frame::decode(&wire, DecodeCfg::verifying()),
        Err(termai_ipc::IpcError::ReservedNonZero)
    );
    assert!(termai_ipc::IpcError::ReservedNonZero.disconnects());
}

#[test]
fn unknown_message_in_a_known_section_keeps_the_link_alive() {
    let mut h = Harness::new("unknown");
    h.hello();
    let out = h.send(0x01FF, 0, 9, &[]).unwrap();
    assert_eq!(out[0].msg_type, msg::ERROR);
    assert_eq!(h.broker.state(), ConnState::Negotiated, "link survives");
    let body = codec::from_bytes(&out[0].payload).unwrap();
    assert_eq!(
        body.get("code").and_then(|v| v.as_text()),
        Some(termai_core::error::ipc::UNSUPPORTED_MSG)
    );
}

#[test]
fn go_away_moves_the_connection_to_draining() {
    let mut h = Harness::new("goaway");
    h.hello();
    let out = h.send(msg::GO_AWAY, 0, 8, &[]).unwrap();
    assert!(out.is_empty());
    assert_eq!(h.broker.state(), ConnState::Draining);
}

#[test]
fn the_frame_header_is_24_bytes_on_the_wire() {
    let wire = frame::encode(msg::PING, 0, 1, &[]).unwrap();
    assert_eq!(wire.len(), 24);
    assert_eq!(
        FrameHeader::decode(&wire[..24].try_into().unwrap()).msg_type,
        msg::PING
    );
}

/// ADR-0023 D1 allocated the 0x05xx attach family; WS-05a implements its handshake.
/// Before WS-05a this test asserted ATTACH_REQUEST answered UnsupportedMsg. It is
/// renamed (not deleted) and now asserts the implemented behaviour: a valid request
/// gets ATTACH_ACK, 0x0502 answers with a real TAIL_REPLAY, and 0x05FF (unassigned
/// inside the known section) still answers UnsupportedMsg -- so the ADR-0023 D1
/// boundary stays covered.
#[test]
fn the_attach_family_is_implemented_per_adr_0023_d1() {
    let mut h = Harness::new("attach");
    h.hello();

    // resume_from is what makes a tail replay answerable at all: TAIL_REPLAY's floor is
    // exclusive, so the client must state its last applied seq (WS-05b).
    let payload = h.attach_payload(codec::AttachMode::ReadOnly, Some(0));
    let out = h
        .send(msg::ATTACH_REQUEST, flag::ENC, 11, &payload)
        .unwrap();
    let ack = decode_attach_ack(&out);
    assert_eq!(ack.state, codec::SessionState::Created);
    assert_eq!(
        ack.snapshot_ref.grid_digest,
        h.broker.registry().digest(h.session).unwrap()
    );
    assert_eq!(
        h.broker.state(),
        ConnState::Negotiated,
        "link survives, no lease"
    );

    // 0x05FF is unassigned inside the known 0x05xx section (ADR-0023 D1 reserves
    // 0x0504-0x05FF): it still answers UnsupportedMsg and keeps the link.
    let out = h.send(0x05FF, 0, 13, &[]).unwrap();
    assert_eq!(out[0].msg_type, msg::ERROR);
    assert_eq!(h.broker.state(), ConnState::Negotiated, "link survives");
    let body = codec::from_bytes(&out[0].payload).unwrap();
    assert_eq!(
        body.get("code").and_then(|v| v.as_text()),
        Some(termai_core::error::ipc::UNSUPPORTED_MSG)
    );

    // 0x0502 TAIL_REPLAY became implemented in WS-05b. This connection attached with
    // resume_from = 0 and nothing was appended, so the legitimate answer is a TAIL_REPLAY
    // frame carrying an EMPTY event set -- an error would be wrong, and so would be a
    // stream leg that never arrives.
    let out = h.send(msg::TAIL_REPLAY, 0, 12, &[]).unwrap();
    assert_eq!(out[0].msg_type, msg::TAIL_REPLAY);
    assert_eq!(out[0].flags & flag::LAST_CHUNK, flag::LAST_CHUNK);
    let replay =
        codec::tail_replay_from_value(&codec::from_bytes(&out[0].payload).unwrap()).unwrap();
    assert_eq!(replay.from_seq, 0);
    assert!(replay.events.is_empty(), "nothing was appended yet");
    assert_eq!(h.broker.state(), ConnState::Negotiated, "link survives");
}

/// The next unallocated section still answers ReservedMsgType, so D1 did not turn
/// every unknown type into a known one.
#[test]
fn a_still_unallocated_section_still_answers_reserved_msg_type() {
    let mut h = Harness::new("reserved");
    h.hello();
    let out = h.send(0x0600, 0, 13, &[]).unwrap();
    assert_eq!(out[0].msg_type, msg::ERROR);
    assert_eq!(h.broker.state(), ConnState::Negotiated, "link survives");
    let body = codec::from_bytes(&out[0].payload).unwrap();
    assert_eq!(
        body.get("code").and_then(|v| v.as_text()),
        Some(termai_core::error::ipc::RESERVED_MSG_TYPE)
    );
}

/// Read-only attach succeeds and the ACK's snapshot_ref matches the registry: the grid
/// digest is the live digest and segment_id/seq come from the real Log segment.
#[test]
fn read_only_attach_reports_the_log_position_and_registry_grid_digest() {
    let mut h = Harness::new("attach-digest");
    h.hello();
    h.broker
        .registry_mut()
        .feed_pty_out(h.session, b"attached", 5)
        .unwrap();

    let payload = h.attach_payload(codec::AttachMode::ReadOnly, None);
    let out = h
        .send(msg::ATTACH_REQUEST, flag::ENC, 20, &payload)
        .unwrap();
    let ack = decode_attach_ack(&out);
    assert_eq!(
        ack.snapshot_ref.grid_digest,
        h.broker.registry().digest(h.session).unwrap()
    );
    assert_eq!(
        (ack.snapshot_ref.segment_id, ack.snapshot_ref.seq),
        h.broker.registry().log_position(h.session).unwrap()
    );
    assert_eq!(ack.snapshot_ref.seq, 1, "one PtyOut record was appended");
    assert!(ack.lease.is_none(), "a read-only attach never has a lease");
    assert!(h.broker.read_only_attached());
}

/// Interactive mode expresses intent, not write rights. Without an explicit
/// LEASE_ACQUIRE the client is still denied and the CAP-1 gate is untouched.
#[test]
fn interactive_attach_without_a_lease_does_not_grant_write_rights() {
    let mut h = Harness::new("attach-interactive");
    h.hello();

    let payload = h.attach_payload(codec::AttachMode::Interactive, None);
    let out = h
        .send(msg::ATTACH_REQUEST, flag::ENC, 20, &payload)
        .unwrap();
    let ack = decode_attach_ack(&out);
    assert!(
        ack.lease.is_none(),
        "Interactive attach must not self-grant a lease"
    );
    assert_eq!(
        h.broker.state(),
        ConnState::Negotiated,
        "still not the writer"
    );

    let input = codec::InputPayload {
        seq: 1,
        kind: 0,
        flags: 0,
        payload: b"id".to_vec(),
    }
    .encode()
    .unwrap();
    let out = h.send(msg::INPUT, 0, 21, &input).unwrap();
    assert_eq!(out[0].msg_type, msg::CAP_DENIED);
    assert_eq!(h.broker.input_accepted, 0);
    assert_eq!(h.broker.input_rejected, 1);
}

/// A version range with no overlap is a structured refusal; being a message-layer error
/// it keeps the link (kernel/07 section 3.2).
#[test]
fn disjoint_attach_version_range_is_refused_and_the_link_survives() {
    let mut h = Harness::new("attach-ver");
    h.hello();
    let payload = h.attach_payload_range(codec::AttachMode::ReadOnly, None, 9, 10);
    let out = h
        .send(msg::ATTACH_REQUEST, flag::ENC, 20, &payload)
        .unwrap();
    assert_eq!(out[0].msg_type, msg::ERROR);
    assert_eq!(h.broker.state(), ConnState::Negotiated, "no disconnect");
    let body = codec::from_bytes(&out[0].payload).unwrap();
    assert_eq!(
        body.get("code").and_then(|v| v.as_text()),
        Some(termai_core::error::handshake::VER_UNSUPPORTED)
    );
    assert!(h.broker.attach_subscription().is_none());
}

/// Duplicate attach returns the same subscription handle, and resume_from at or before
/// the applied watermark asks for no replay (WS-05a emits none).
#[test]
fn duplicate_attach_reuses_the_subscription_and_resume_from_is_idempotent() {
    let mut h = Harness::new("attach-idem");
    h.hello();

    let payload = h.attach_payload(codec::AttachMode::ReadOnly, Some(0));
    let first = h
        .send(msg::ATTACH_REQUEST, flag::ENC, 20, &payload)
        .unwrap();
    let ack1 = decode_attach_ack(&first);
    let sub1 = h.broker.attach_subscription().unwrap();
    assert_eq!(ack1.snapshot_ref.seq, 0, "no records appended yet");
    assert!(
        first.iter().all(|f| f.msg_type == msg::ATTACH_ACK),
        "WS-05a emits no TAIL_REPLAY"
    );

    let second = h
        .send(msg::ATTACH_REQUEST, flag::ENC, 21, &payload)
        .unwrap();
    let ack2 = decode_attach_ack(&second);
    assert_eq!(h.broker.attach_subscription(), Some(sub1), "same handle");
    assert_eq!(ack1.snapshot_ref, ack2.snapshot_ref);
    assert_eq!(h.broker.attach_resume_from(), Some(0));
}

/// DETACH_NOTICE releases the read subscription. Basis for refusing the later snapshot:
/// kernel/04 section 3.4 makes GRID_SNAPSHOT the transport leg of the attach flow
/// (ATTACH_REQ -> ATTACH_ACK -> GRID_SNAPSHOT), so with no subscription there is no
/// consumer to deliver to.
#[test]
fn detach_releases_the_subscription_and_later_snapshot_is_refused() {
    let mut h = Harness::new("attach-detach");
    h.hello();
    let payload = h.attach_payload(codec::AttachMode::ReadOnly, None);
    h.send(msg::ATTACH_REQUEST, flag::ENC, 20, &payload)
        .unwrap();
    assert!(h.broker.read_only_attached());
    let snap = h.send(msg::GRID_SNAPSHOT, 0, 21, &[]).unwrap();
    assert!(!snap.is_empty(), "snapshot works while attached");

    let out = h
        .send(
            msg::DETACH_NOTICE,
            0,
            22,
            &codec::detach_notice_encode(false),
        )
        .unwrap();
    assert!(out.is_empty(), "DETACH_NOTICE has no reply");
    assert!(!h.broker.read_only_attached());
    assert!(h.broker.attach_subscription().is_none());

    let out = h.send(msg::GRID_SNAPSHOT, 0, 23, &[]).unwrap();
    assert_eq!(out[0].msg_type, msg::ERROR);
    let body = codec::from_bytes(&out[0].payload).unwrap();
    assert_eq!(
        body.get("code").and_then(|v| v.as_text()),
        Some("AttachStateInvalid"),
        "SD-18: detached is an attach-state refusal, not frame damage"
    );
    assert_eq!(h.broker.state(), ConnState::Negotiated, "link survives");
}

/// lease_release = true frees the lease through the existing explicit revoke path, so
/// write rights disappear with it and a re-acquire is required.
#[test]
fn detach_with_lease_release_frees_the_lease_and_locks_writes_again() {
    let mut h = Harness::new("attach-release");
    h.hello();
    let payload = h.attach_payload(codec::AttachMode::Interactive, None);
    h.send(msg::ATTACH_REQUEST, flag::ENC, 20, &payload)
        .unwrap();

    let out = h
        .send(msg::LEASE_ACQUIRE, 0, 21, &9u128.to_le_bytes())
        .unwrap();
    assert_eq!(out[0].msg_type, msg::LEASE_GRANT);
    let input = codec::InputPayload {
        seq: 1,
        kind: 0,
        flags: 0,
        payload: b"x".to_vec(),
    }
    .encode()
    .unwrap();
    assert_eq!(
        h.send(msg::INPUT, 0, 22, &input).unwrap()[0].msg_type,
        msg::CREDIT_UPDATE
    );

    let out = h
        .send(
            msg::DETACH_NOTICE,
            0,
            23,
            &codec::detach_notice_encode(true),
        )
        .unwrap();
    assert!(out.is_empty());
    assert_eq!(h.broker.lease(), None);
    assert_eq!(h.broker.state(), ConnState::Negotiated);

    let input = codec::InputPayload {
        seq: 2,
        kind: 0,
        flags: 0,
        payload: b"y".to_vec(),
    }
    .encode()
    .unwrap();
    assert_eq!(
        h.send(msg::INPUT, 0, 24, &input).unwrap()[0].msg_type,
        msg::CAP_DENIED
    );
}

/// Attach before the handshake is refused; a payload missing required fields is rejected
/// rather than silently defaulted, and the link survives both.
#[test]
fn attach_requires_a_handshake_and_a_complete_payload() {
    let mut h = Harness::new("attach-pre");
    let payload = h.attach_payload(codec::AttachMode::ReadOnly, None);
    let out = h.send(msg::ATTACH_REQUEST, flag::ENC, 1, &payload).unwrap();
    assert_eq!(out[0].msg_type, msg::ERROR);
    assert_eq!(h.broker.state(), ConnState::Closed);

    h.hello();
    let bad = codec::to_bytes(&Value::map(vec![("proto_min", Value::U64(1))]));
    let out = h.send(msg::ATTACH_REQUEST, flag::ENC, 2, &bad).unwrap();
    assert_eq!(out[0].msg_type, msg::ERROR);
    assert_eq!(h.broker.state(), ConnState::Negotiated, "link survives");
    assert!(h.broker.attach_subscription().is_none());
}

/// Unknown optional fields are ignored for forward compatibility (kernel/07 section 3.6
/// V3), so a newer client minor does not break an older broker.
#[test]
fn attach_ignores_unknown_optional_fields() {
    let mut h = Harness::new("attach-future");
    h.hello();
    let mut v = codec::attach_request_to_value(&codec::AttachRequest {
        proto_min: 1,
        proto_max: 3,
        client_kind: ClientKind::Cli,
        session_id: h.session,
        mode: codec::AttachMode::ReadOnly,
        resume_from: None,
        capabilities: vec![CAP_SESSION_READ],
    });
    if let Value::Map(ref mut m) = v {
        m.push((Value::Text("future_field".into()), Value::U64(1)));
    }
    let payload = codec::to_bytes(&v);
    let out = h
        .send(msg::ATTACH_REQUEST, flag::ENC, 20, &payload)
        .unwrap();
    let ack = decode_attach_ack(&out);
    assert_eq!(ack.chosen_ver, 3);
}

/// A re-attach on a connection that has acquired the lease reports it in ATTACH_ACK,
/// so a client can tell it is the writer without a separate round trip.
#[test]
fn reattach_after_lease_reports_the_granted_lease() {
    let mut h = Harness::new("attach-lease-info");
    h.hello();
    let payload = h.attach_payload(codec::AttachMode::Interactive, None);
    let out = h
        .send(msg::ATTACH_REQUEST, flag::ENC, 20, &payload)
        .unwrap();
    assert!(decode_attach_ack(&out).lease.is_none());

    h.send(msg::LEASE_ACQUIRE, 0, 21, &9u128.to_le_bytes())
        .unwrap();
    let out = h
        .send(msg::ATTACH_REQUEST, flag::ENC, 22, &payload)
        .unwrap();
    let lease = decode_attach_ack(&out).lease.expect("lease is reported");
    assert_eq!(lease.lease_id, 1);
    assert_eq!(lease.holder, 9);
    assert_eq!(
        lease.since, 0,
        "mono clock is injected and zero in this harness"
    );
    assert_eq!(lease.ttl_s, 30, "kernel/04 section 3.4 default TTL");
}

// ---------------------------------------------------------------------------
// TAIL_REPLAY (0x0502, WS-05b): the last leg of E-P0-4 "screen 可恢复".
// ---------------------------------------------------------------------------

fn resize_record(cols: u16) -> Record {
    Record::Resize {
        pane: 0,
        cols,
        rows: 24,
        px_w: 0,
        px_h: 0,
    }
}

fn cmd_end_record(cmd_id: u64) -> Record {
    Record::CmdEnd {
        pane: 0,
        cmd_id,
        exit_code: 7,
        duration_ms: 3,
        cwd: "/tmp".into(),
        confidence: 10,
    }
}

fn append_record(h: &mut Harness, rec: Record) -> u64 {
    h.broker
        .registry_mut()
        .get_mut(h.session)
        .unwrap()
        .writer
        .append(&rec, 7)
        .unwrap()
        .seq
}

fn decode_replay(frames: &[OutFrame]) -> codec::TailReplay {
    assert_eq!(frames.len(), 1, "a small replay is a single frame");
    assert_eq!(frames[0].msg_type, msg::TAIL_REPLAY);
    codec::tail_replay_from_value(&codec::from_bytes(&frames[0].payload).unwrap()).unwrap()
}

/// The attach flow of kernel/04 section 3.4: ATTACH_REQ -> ATTACH_ACK -> GRID_SNAPSHOT ->
/// TAIL_REPLAY(seq0..head). The replay only carries events with seq greater than from_seq.
#[test]
fn tail_replay_after_attach_carries_the_p0_tail_only() {
    let mut h = Harness::new("tail-replay");
    h.hello();
    let payload = h.attach_payload(codec::AttachMode::ReadOnly, Some(0));
    let out = h
        .send(msg::ATTACH_REQUEST, flag::ENC, 20, &payload)
        .unwrap();
    let ack = decode_attach_ack(&out);
    assert_eq!(
        ack.snapshot_ref.seq, 0,
        "nothing appended before the attach"
    );

    // The records that land between the snapshot and the replay are the tail. The
    // subscription floor is the client's resume_from = 0, so seq 0 (already applied by
    // the client's own claim) is not sent again: the floor is EXCLUSIVE.
    assert_eq!(append_record(&mut h, resize_record(120)), 0);
    h.broker
        .registry_mut()
        .feed_pty_out(h.session, b"tail", 1)
        .unwrap(); // seq 1: P1 raw ring
    assert_eq!(append_record(&mut h, resize_record(121)), 2);
    assert_eq!(append_record(&mut h, cmd_end_record(5)), 3);

    let out = h.send(msg::GRID_SNAPSHOT, 0, 21, &[]).unwrap();
    let (bytes, _) = reassemble_snapshot(&out).unwrap();
    assert!(codec::from_bytes(&bytes).is_ok());

    let out = h.send(msg::TAIL_REPLAY, 0, 22, &[]).unwrap();
    assert_eq!(out[0].flags & flag::LAST_CHUNK, flag::LAST_CHUNK);
    let replay = decode_replay(&out);
    assert_eq!(replay.from_seq, 0);
    assert_eq!(
        replay.events.iter().map(|e| e.seq).collect::<Vec<_>>(),
        vec![2, 3],
        "seq 0 is at/below the floor and seq 1 is the P1 raw ring"
    );
    assert_eq!(
        replay.events[0].body,
        codec::ContextEventBody::Resize {
            rows: 24,
            cols: 121,
            px: None
        }
    );
    assert_eq!(
        replay.events[1].body,
        codec::ContextEventBody::ExitStatus {
            cmd_id: 5,
            code: Some(7),
            signal: None
        }
    );

    // Idempotent: the same request yields identical frames and consumes nothing.
    let again = h.send(msg::TAIL_REPLAY, 0, 22, &[]).unwrap();
    assert_eq!(again, out);
    assert_eq!(
        h.broker.registry().log_position(h.session).unwrap().1,
        4,
        "no record was consumed or appended"
    );
}

/// AR-26 boundary honesty: when the requested window is gone the server refuses with a
/// structured error and tells the client to take a full snapshot. It must never answer a
/// short replay that would make the screen look consistent when it is not.
#[test]
fn tail_replay_below_the_window_is_refused_over_the_wire() {
    let mut h = Harness::new_from("tail-window", 5);
    h.hello();
    assert_eq!(
        append_record(&mut h, resize_record(120)),
        5,
        "oldest seq is 5"
    );
    let payload = h.attach_payload(codec::AttachMode::ReadOnly, Some(0));
    h.send(msg::ATTACH_REQUEST, flag::ENC, 20, &payload)
        .unwrap();

    let out = h.send(msg::TAIL_REPLAY, 0, 21, &[]).unwrap();
    assert_eq!(out[0].msg_type, msg::ERROR);
    assert_ne!(out[0].flags & flag::DROP_NOTICE, 0);
    let body = codec::from_bytes(&out[0].payload).unwrap();
    assert_eq!(
        body.get("code").and_then(|v| v.as_text()),
        Some("AttachStateInvalid")
    );
    let detail = body.get("detail").and_then(|v| v.as_text()).unwrap();
    assert!(detail.contains("tail_replay_below_window"), "{detail}");
    assert!(detail.contains("GRID_SNAPSHOT"), "{detail}");
    assert_eq!(h.broker.state(), ConnState::Negotiated, "link survives");
}

/// Every chunk of a chunked replay is a complete TAIL_REPLAY, so reassembly is an ordered
/// concatenation and no second representation appears on the wire.
#[test]
fn chunked_tail_replay_reassembles_in_order_through_the_frame_layer() {
    let events: Vec<codec::ReplayEvent> = (0..20)
        .map(|i| codec::ReplayEvent {
            seq: i,
            ts_mono_ns: i * 10,
            pane: Some(0),
            body: codec::ContextEventBody::TitleChanged {
                title: format!("t{i}"),
                scope: codec::TitleScope::Window,
            },
        })
        .collect();
    let frames = chunk_tail_replay(77, 0, &events, 128).unwrap();
    assert!(frames.len() > 1, "the replay must be chunked at this cap");

    let mut link = MemoryLink::new();
    for f in &frames {
        link.write_all(&f.encode(VER).unwrap()).unwrap();
    }
    let mut buf = link.take_outbound();
    let mut seen = Vec::new();
    let mut flags = Vec::new();
    let mut first_floor = None;
    while !buf.is_empty() {
        let f = frame::decode(&buf, DecodeCfg::verifying())
            .unwrap()
            .unwrap();
        let total = frame::FRAME_HEADER_LEN + f.header.len as usize;
        let chunk_flags = f.header.flags & (flag::MORE_CHUNK | flag::LAST_CHUNK);
        flags.push(chunk_flags);
        let r = codec::tail_replay_from_value(&codec::from_bytes(f.payload).unwrap()).unwrap();
        if first_floor.is_none() {
            first_floor = Some(r.from_seq);
        }
        seen.extend(r.events.iter().map(|e| e.seq));
        buf.drain(..total);
    }
    assert_eq!(first_floor, Some(0));
    assert_eq!(seen, (0..20).collect::<Vec<u64>>(), "no reordering");
    assert_eq!(
        *flags.last().unwrap() & flag::LAST_CHUNK,
        flag::LAST_CHUNK,
        "the last chunk is flagged"
    );
    assert!(
        flags[..flags.len() - 1]
            .iter()
            .all(|f| f & flag::MORE_CHUNK != 0),
        "every earlier chunk is flagged as partial"
    );
}
