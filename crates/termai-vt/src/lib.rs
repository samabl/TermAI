//! termai-vt - VT/ANSI semantics (kernel/01).
//!
//! AR-18: the ESC state machine is vte, but it lives strictly behind the trait
//! boundary re-exported here. No vte type appears in any public signature.
#![forbid(unsafe_code)]

pub mod backend;
pub mod counters;
pub mod golden;
pub mod grid;
pub mod lane;
pub mod replay;
pub mod shell;
pub mod terminal;
mod vte_adapter;

pub use backend::{
    AdvanceReport, BackendCaps, BackendId, EscapeSink, Params, ParseError, ParseErrorKind,
    ParserState, StringKind, StringTerm, VtBackend, DCS_LEN_LIMIT_DEFAULT, MAX_PARAMS,
    OSC_LEN_LIMIT_DEFAULT,
};
pub use counters::VtCounters;
pub use golden::{golden_hash, parse_golden, write_golden, GoldenDoc, GoldenError};
pub use grid::{ClipboardDecision, ClipboardDirection, ClipboardVerdict, Grid};
pub use lane::{lane_verdict, Lane, Verdict};
pub use replay::{parse_trec, run, ReplayError, ReplayReport, ReplayScript, Step};
pub use shell::{CommandBlock, ShellIntegration};
pub use terminal::Terminal;
pub use vte_adapter::VteBackend;
