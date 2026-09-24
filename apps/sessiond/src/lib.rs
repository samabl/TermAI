//! sessiond - the session truth daemon (kernel/04, DC-18).
//!
//! Structure: Link (transport) -> Broker (protocol) -> Registry (state + Log + engine).
//! The terminal engine is a trait, so the protocol is testable without a real PTY.
//! The Host owns the real child process: sessiond holds the PTY and is the component that
//! reaps its tree on close (kernel/02 section 3.3, AR-30 item 2).
//!
//! [`daemon`] is the production driver of the three layers: it serves the broker over a
//! [`link::Link`], owns a [`host::SessionHost`], and runs [`host::SessionHost::close_all`] on
//! every shutdown path.
#![forbid(unsafe_code)]

pub mod broker;
pub mod daemon;
pub mod engine;
pub mod host;
pub mod link;
pub mod registry;
pub mod restore;
