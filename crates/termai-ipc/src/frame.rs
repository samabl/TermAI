//! termai-ipc frame layer: 24-byte header, CRC-32C, reserved-sentinel rejection.
//!
//! WIRE LAYOUT (authoritative). kernel/07 section 3.1 shows a repr(C) struct, but
//! that layout cannot be 24 bytes: placing corr_id (u64) at offset 12 forces 4 bytes
//! of padding and an 8-byte tail alignment, giving 32 bytes. The contract that all
//! other statements depend on is the explicit one: FRAME_HEADER_LEN = 24 and
//! crc32c covers header[0..20] || payload. This module therefore encodes the header
//! byte-wise little-endian, exactly as the field table specifies:
//!
//!   off  0..4  len:u32      payload length (excludes header and the crc field)
//!   off  4..6  ver:u16      0xMMmm
//!   off  6..8  msg_type:u16
//!   off  8..10 flags:u16
//!   off 10..12 rsv:u16      must be 0 -> any non-zero rejects the frame
//!   off 12..20 corr_id:u64
//!   off 20..24 crc32c:u32
//!
//! See docs/plan/m0-spec-defects.md (SD-01).

use crate::crc32c::Crc32c;

/// Header length on the wire.
pub const FRAME_HEADER_LEN: usize = 24;
/// Maximum payload length (8 MiB, OQ-30).
pub const MAX_FRAME_LEN: usize = 8 << 20;
/// Upper bound for ENC=POD payloads; larger payloads must use CBOR or chunking.
pub const POD_MAX_LEN: usize = 4096;

/// Frame encoding classes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Encoding {
    /// repr(C) fixed layout.
    #[default]
    Pod,
    /// CBOR (context events, audit, control handshake).
    Cbor,
}

/// Frame channel (flags bits 2:1).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum Chan {
    #[default]
    Control = 0,
    Data = 1,
    Context = 2,
    Audit = 3,
}

impl Chan {
    #[must_use]
    pub const fn from_bits(v: u16) -> Chan {
        match (v >> 1) & 0x3 {
            1 => Chan::Data,
            2 => Chan::Context,
            3 => Chan::Audit,
            _ => Chan::Control,
        }
    }
}

/// Header flag bit positions (kernel/07 section 3.1.2).
pub mod flag {
    pub const ENC: u16 = 1 << 0;
    pub const CHAN_MASK: u16 = 0b11 << 1;
    pub const RESPONSE: u16 = 1 << 3;
    pub const ERROR: u16 = 1 << 4;
    pub const MORE_CHUNK: u16 = 1 << 5;
    pub const LAST_CHUNK: u16 = 1 << 6;
    pub const REPLAY: u16 = 1 << 7;
    pub const AUDIT_REQUIRED: u16 = 1 << 8;
    pub const DROP_NOTICE: u16 = 1 << 9;
    pub const SEALED: u16 = 1 << 10;
    pub const DEGRADED: u16 = 1 << 11;
    /// Bits 12..15 are reserved and must be zero.
    pub const RESERVED_MASK: u16 = 0xF000;
}

/// Frame-level errors. These disconnect; message-level errors do not (section 3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IpcError {
    /// buf.len() < 24 or buf.len() < 24 + len.
    NeedMore,
    /// rsv != 0 -> reject + audit ipc.reserved_nonzero.
    ReservedNonZero,
    FrameTooLarge,
    CrcMismatch,
    EncodingMismatch,
    Corrupt,
    UnsupportedMsg,
    ReservedMsgType,
}

impl IpcError {
    /// Frame-layer errors tear the connection down; message-layer errors do not.
    #[must_use]
    pub const fn disconnects(self) -> bool {
        matches!(
            self,
            IpcError::ReservedNonZero
                | IpcError::FrameTooLarge
                | IpcError::CrcMismatch
                | IpcError::EncodingMismatch
        )
    }

    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            IpcError::NeedMore => termai_core::error::ipc::NEED_MORE,
            IpcError::ReservedNonZero => termai_core::error::ipc::RESERVED_NONZERO,
            IpcError::FrameTooLarge => termai_core::error::ipc::FRAME_TOO_LARGE,
            IpcError::CrcMismatch => termai_core::error::ipc::CRC_MISMATCH,
            IpcError::EncodingMismatch => termai_core::error::ipc::ENCODING_MISMATCH,
            IpcError::Corrupt => termai_core::error::ipc::CORRUPT,
            IpcError::UnsupportedMsg => termai_core::error::ipc::UNSUPPORTED_MSG,
            IpcError::ReservedMsgType => termai_core::error::ipc::RESERVED_MSG_TYPE,
        }
    }
}

/// The 24-byte frame header.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FrameHeader {
    pub len: u32,
    pub ver: u16,
    pub msg_type: u16,
    pub flags: u16,
    pub rsv: u16,
    pub corr_id: u64,
    pub crc32c: u32,
}

impl FrameHeader {
    #[must_use]
    pub fn encode(&self) -> [u8; FRAME_HEADER_LEN] {
        let mut b = [0u8; FRAME_HEADER_LEN];
        b[0..4].copy_from_slice(&self.len.to_le_bytes());
        b[4..6].copy_from_slice(&self.ver.to_le_bytes());
        b[6..8].copy_from_slice(&self.msg_type.to_le_bytes());
        b[8..10].copy_from_slice(&self.flags.to_le_bytes());
        b[10..12].copy_from_slice(&self.rsv.to_le_bytes());
        b[12..20].copy_from_slice(&self.corr_id.to_le_bytes());
        b[20..24].copy_from_slice(&self.crc32c.to_le_bytes());
        b
    }

    #[must_use]
    pub fn decode(b: &[u8; FRAME_HEADER_LEN]) -> Self {
        let u16_at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
        let u32_at = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        let mut c = [0u8; 8];
        c.copy_from_slice(&b[12..20]);
        Self {
            len: u32_at(0),
            ver: u16_at(4),
            msg_type: u16_at(6),
            flags: u16_at(8),
            rsv: u16_at(10),
            corr_id: u64::from_le_bytes(c),
            crc32c: u32_at(20),
        }
    }

    #[must_use]
    pub const fn encoding(&self) -> Encoding {
        if self.flags & flag::ENC == 0 {
            Encoding::Pod
        } else {
            Encoding::Cbor
        }
    }

    #[must_use]
    pub const fn chan(&self) -> Chan {
        Chan::from_bits(self.flags)
    }

    #[must_use]
    pub const fn is_response(&self) -> bool {
        self.flags & flag::RESPONSE != 0
    }

    #[must_use]
    pub const fn is_error(&self) -> bool {
        self.flags & flag::ERROR != 0
    }

    #[must_use]
    pub const fn is_sealed(&self) -> bool {
        self.flags & flag::SEALED != 0
    }

    #[must_use]
    pub const fn is_degraded(&self) -> bool {
        self.flags & flag::DEGRADED != 0
    }

    #[must_use]
    pub const fn needs_resync(&self) -> bool {
        self.flags & flag::DROP_NOTICE != 0
    }
}

/// A borrowed frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Frame<'a> {
    pub header: FrameHeader,
    pub payload: &'a [u8],
}

/// Compute the CRC over header[0..20] || payload.
#[must_use]
pub fn frame_crc(header_prefix: &[u8], payload: &[u8]) -> u32 {
    let mut c = Crc32c::new();
    c.update(header_prefix);
    c.update(payload);
    c.finish()
}

/// Encode a frame. Returns Err only for contract violations (too large / bad POD size).
pub fn encode(
    msg_type: u16,
    flags: u16,
    corr_id: u64,
    payload: &[u8],
) -> Result<Vec<u8>, IpcError> {
    if payload.len() > MAX_FRAME_LEN {
        return Err(IpcError::FrameTooLarge);
    }
    if flags & flag::ENC == 0 && payload.len() > POD_MAX_LEN {
        return Err(IpcError::EncodingMismatch);
    }
    let mut header = FrameHeader {
        len: payload.len() as u32,
        ver: crate::PROTO_VERSION,
        msg_type,
        flags,
        rsv: 0,
        corr_id,
        crc32c: 0,
    };
    let prefix = header.encode();
    header.crc32c = frame_crc(&prefix[0..20], payload);
    let mut out = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
    out.extend_from_slice(&header.encode());
    out.extend_from_slice(payload);
    Ok(out)
}

/// Encode a frame with an explicit version (handshake peers may differ).
pub fn encode_versioned(
    ver: u16,
    msg_type: u16,
    flags: u16,
    corr_id: u64,
    payload: &[u8],
) -> Result<Vec<u8>, IpcError> {
    if payload.len() > MAX_FRAME_LEN {
        return Err(IpcError::FrameTooLarge);
    }
    let mut header = FrameHeader {
        len: payload.len() as u32,
        ver,
        msg_type,
        flags,
        rsv: 0,
        corr_id,
        crc32c: 0,
    };
    let prefix = header.encode();
    header.crc32c = frame_crc(&prefix[0..20], payload);
    let mut out = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
    out.extend_from_slice(&header.encode());
    out.extend_from_slice(payload);
    Ok(out)
}

/// Decode options.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct DecodeCfg {
    /// Verify CRC. Never disabled on a real transport; kept for throughput tests.
    pub verify_crc: bool,
    /// Accept payloads larger than MAX_FRAME_LEN (tests only).
    pub allow_oversize: bool,
}

impl DecodeCfg {
    /// Production configuration: CRC always verified.
    #[must_use]
    pub const fn verifying() -> Self {
        Self {
            verify_crc: true,
            allow_oversize: false,
        }
    }
}

/// Parse one frame. Ok(None) means NeedMore (borrow semantics, no copy).
pub fn decode<'a>(buf: &'a [u8], cfg: DecodeCfg) -> Result<Option<Frame<'a>>, IpcError> {
    if buf.len() < FRAME_HEADER_LEN {
        return Ok(None);
    }
    let mut hb = [0u8; FRAME_HEADER_LEN];
    hb.copy_from_slice(&buf[0..FRAME_HEADER_LEN]);
    let header = FrameHeader::decode(&hb);

    if header.rsv != 0 || header.flags & flag::RESERVED_MASK != 0 {
        return Err(IpcError::ReservedNonZero);
    }
    if !cfg.allow_oversize && header.len as usize > MAX_FRAME_LEN {
        return Err(IpcError::FrameTooLarge);
    }
    let total = FRAME_HEADER_LEN + header.len as usize;
    if buf.len() < total {
        return Ok(None);
    }
    if cfg.verify_crc && frame_crc(&hb[0..20], &buf[FRAME_HEADER_LEN..total]) != header.crc32c {
        return Err(IpcError::CrcMismatch);
    }
    if header.encoding() == Encoding::Pod && header.len as usize > POD_MAX_LEN {
        return Err(IpcError::EncodingMismatch);
    }
    Ok(Some(Frame {
        header,
        payload: &buf[FRAME_HEADER_LEN..total],
    }))
}

/// Total wire size of a frame whose header has been read.
#[must_use]
pub const fn frame_total_len(header: &FrameHeader) -> usize {
    FRAME_HEADER_LEN + header.len as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_exactly_24_bytes() {
        assert_eq!(core::mem::size_of::<[u8; FRAME_HEADER_LEN]>(), 24);
        let h = FrameHeader {
            len: 5,
            ver: 0x0001,
            msg_type: 0x0100,
            flags: flag::ENC,
            rsv: 0,
            corr_id: 0x1122_3344_5566_7788,
            crc32c: 0xDEAD_BEEF,
        };
        let enc = h.encode();
        assert_eq!(enc.len(), 24);
        assert_eq!(FrameHeader::decode(&enc), h);
        assert_eq!(&enc[12..20], &0x1122_3344_5566_7788u64.to_le_bytes());
        assert_eq!(&enc[20..24], &0xDEAD_BEEFu32.to_le_bytes());
    }

    #[test]
    fn roundtrip_encode_decode() {
        let payload = b"hello";
        let wire = encode(0x0100, flag::ENC | (1 << 1), 7, payload).unwrap();
        assert_eq!(wire.len(), 24 + 5);
        let f = decode(&wire, DecodeCfg::verifying()).unwrap().unwrap();
        assert_eq!(f.payload, payload);
        assert_eq!(f.header.msg_type, 0x0100);
        assert_eq!(f.header.corr_id, 7);
        assert_eq!(f.header.chan(), Chan::Data);
    }

    #[test]
    fn partial_frames_report_need_more() {
        let wire = encode(0x0100, 0, 0, b"abcdef").unwrap();
        assert_eq!(decode(&wire[..10], DecodeCfg::verifying()), Ok(None));
        assert_eq!(decode(&wire[..24], DecodeCfg::verifying()), Ok(None));
        assert!(decode(&wire, DecodeCfg::verifying()).unwrap().is_some());
    }

    #[test]
    fn nonzero_reserved_rejects_frame() {
        let mut wire = encode(0x0100, 0, 0, b"x").unwrap();
        wire[10] = 1;
        assert_eq!(
            decode(&wire, DecodeCfg::verifying()),
            Err(IpcError::ReservedNonZero)
        );
        assert!(IpcError::ReservedNonZero.disconnects());
    }

    #[test]
    fn reserved_flag_bits_reject_frame() {
        let mut wire = encode(0x0100, 0, 0, b"x").unwrap();
        wire[9] |= 0xF0;
        assert_eq!(
            decode(&wire, DecodeCfg::verifying()),
            Err(IpcError::ReservedNonZero)
        );
    }

    #[test]
    fn corrupted_payload_fails_crc() {
        let mut wire = encode(0x0100, 0, 0, b"payload").unwrap();
        let n = wire.len() - 1;
        wire[n] ^= 0xFF;
        assert_eq!(
            decode(&wire, DecodeCfg::verifying()),
            Err(IpcError::CrcMismatch)
        );
        // Without CRC verification the frame is still structurally readable.
        assert!(decode(&wire, DecodeCfg::default()).unwrap().is_some());
    }

    #[test]
    fn oversize_length_is_rejected_before_allocation() {
        let mut wire = encode(0x0100, 0, 0, b"x").unwrap();
        wire[0..4].copy_from_slice(&(u32::MAX).to_le_bytes());
        assert_eq!(
            decode(&wire, DecodeCfg::verifying()),
            Err(IpcError::FrameTooLarge)
        );
    }

    #[test]
    fn pod_frames_over_4096_must_use_cbor() {
        assert_eq!(
            encode(0x0100, 0, 0, &vec![0u8; POD_MAX_LEN + 1]),
            Err(IpcError::EncodingMismatch)
        );
        assert!(encode(0x0100, flag::ENC, 0, &vec![0u8; POD_MAX_LEN + 1]).is_ok());
    }

    #[test]
    fn flags_helpers() {
        let h = FrameHeader {
            flags: flag::SEALED | flag::DEGRADED | flag::DROP_NOTICE | flag::ENC | flag::ERROR,
            ..Default::default()
        };
        assert!(h.is_sealed() && h.is_degraded() && h.needs_resync() && h.is_error());
        assert_eq!(h.encoding(), Encoding::Cbor);
        assert_eq!(h.chan(), Chan::Control);
    }
}
