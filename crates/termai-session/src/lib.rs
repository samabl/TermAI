//! `termai-session` - the session truth layer (kernel/04).
//!
//! The kernel never depends on AI, network or UI (AR-03). This crate depends only on
//! termai-core (leaf contract) and termai-ipc (frame/crc primitives), which kernel/04
//! section 4 explicitly lists as inputs it receives.
//!
//! VT and PTY are reached through the traits in this crate (GridReplay, ByteTransport),
//! implemented by the app layer. That keeps the session core testable without a real PTY
//! and keeps the dependency direction one-way (DC-21).
#![forbid(unsafe_code)]

pub mod bus;
pub mod checkpoint;
pub mod lease;
pub mod log;
pub mod state;
pub mod supervisor;

pub use bus::{
    DropNotice, DropPolicy, EventBus, EventClass, PublishOutcome, SubscribeError, SubscribeSpec,
    SubscriptionHandle,
};
pub use checkpoint::{recover_session, Checkpoint, GridReplay, MetaIndex, RecoverOutcome};
pub use lease::{LeaseAction, LeaseError, LeaseEvent, LeaseId, LeaseManager, LeaseState};
pub use log::{
    read_segment, FlushMode, LogError, LoggedRecord, Record, RecordId, RotationPolicy,
    SegmentHeader, SegmentRead, SegmentWriter, Source, RECORD_HEADER_LEN, SEGMENT_HEADER_LEN,
    SEGMENT_MAGIC, SEGMENT_MAX_BYTES,
};
pub use state::{
    transition, Actor, ActorKind, AttachMode, ClientId, CloseReason, Machine, SessionState,
    Transition, Trigger,
};
pub use supervisor::{Backoff, RestartDecision, RestartPolicy};

/// Bytes read from the PTY and written back to it. Implemented by the app layer
/// (apps/sessiond) over termai-pty; kept as a trait so this crate stays PTY-free.
pub trait ByteTransport: Send {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize>;
}
