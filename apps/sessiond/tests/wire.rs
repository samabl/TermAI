//! On-the-wire protocol test: requests are really encoded to bytes, pushed through a
//! Link, decoded with CRC verification, then handed to the broker. Replies are encoded
//! again and decoded back. This proves the M0 daemon contract without a real PTY.

use sessiond::broker::{reassemble_snapshot, Broker, ConnState, OutFrame};
use sessiond::link::{Link, MemoryLink};
use sessiond::registry::{testing::TextEngine, Registry};

use termai_core::capability::{CAP_AUDIT_READ, CAP_SESSION_READ, CAP_STDIN_WRITE};
use termai_core::SessionId;
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
        let mut dir = std::env::temp_dir();
        dir.push(format!("termai-wire-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let id = SessionId(1);
        let mut reg = Registry::new();
        let w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, 0).unwrap();
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

    // 2. Read-only attach succeeds before any lease (AR-26/ADR-0009 semantics).
    let chunks = h.send(msg::GRID_SNAPSHOT, 0, 2, &[]).unwrap();
    assert!(!chunks.is_empty());
    let (bytes, digest) = reassemble_snapshot(&chunks).unwrap();
    assert_eq!(digest, *blake3::hash(&bytes).as_bytes());
    let snap = codec::grid_snapshot_from_value(&codec::from_bytes(&bytes).unwrap()).unwrap();
    assert_eq!(snap, h.broker.registry().snapshot(h.session).unwrap());
    assert!(h.broker.read_only_attached());
    assert_eq!(h.broker.state(), ConnState::Negotiated);

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
