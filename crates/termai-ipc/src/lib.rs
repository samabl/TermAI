//! `termai-ipc` - binary framing, handshake and CBOR payloads (DC-22 / kernel/07).
//!
//! No JSON, no gRPC on any hot path (AR-04). Depends only on termai-core.
#![forbid(unsafe_code)]

pub mod cbor;
pub mod codec;
pub mod crc32c;
pub mod frame;
pub mod handshake;
pub mod msg;

pub use termai_core::PROTO_VERSION;

/// Frame header length on the wire, re-exported for consumers.
pub const FRAME_HEADER_LEN: usize = frame::FRAME_HEADER_LEN;

/// 24-byte frame header.
pub use frame::FrameHeader;
/// Frame-level error type.
pub use frame::IpcError;
/// Parse a frame from a byte buffer.
pub use frame::{decode, DecodeCfg, Frame};
/// Encode a message into a wire frame.
pub use frame::{encode, encode_versioned};
