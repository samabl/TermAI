//! sessiond - the session truth daemon (kernel/04, DC-18).
//!
//! Structure: Link (transport) -> Broker (protocol) -> Registry (state + Log + engine).
//! The terminal engine is a trait, so the protocol is testable without a real PTY.
#![forbid(unsafe_code)]

pub mod broker;
pub mod engine;
pub mod link;
pub mod registry;
