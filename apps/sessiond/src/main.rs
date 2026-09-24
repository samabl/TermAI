//! sessiond - session truth daemon (kernel/04, DC-18).
//!
//! The binary is the process-level shell around [`sessiond::daemon::run`]: it parses the M0
//! launch contract (which session to own, what to run in it), opens that session's real pty,
//! serves the kernel/07 IPC protocol on the M0 stdio link, and force-reaps every hosted
//! process tree before it exits (AR-30 item 2 / PTY-ORPHAN-1).
//!
//! `--help` and `--version` print to stdout before serving starts; everything else the daemon
//! has to say goes to stderr, because once serving starts stdout is the frame stream and is
//! never text.
#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = sessiond::daemon::run(&args);
    // The documented codes are 0..=4; a wider value cannot be reported through ExitCode, so it
    // is reported as a failure rather than silently truncated.
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
