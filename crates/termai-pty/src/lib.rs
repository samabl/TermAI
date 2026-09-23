#![deny(unsafe_op_in_unsafe_fn)]
//! termai-pty - PTY / platform layer (kernel/02; AR-25; DC-16; ADR-0018 D1).
//!
//! Scope:
//! - the nine-face PtyBackend trait (ADR-0018 D1) is the only surface sessiond sees;
//! - real ConPTY + Job Object on Windows and forkpty on unix;
//! - a best-effort pipe fallback backend for machines where the native path is
//!   unavailable (kernel/02 section 6);
//! - the F0/F1/F2 fidelity labels (AR-25) and the conpty-rules registry (AR-25.3).
//!
//! Non-negotiable boundaries this crate upholds:
//! - the byte buffer layer is never rewritten (K-01, AR-25);
//! - Command is argv only, never a shell string (AR-06 rule 2);
//! - no AI, network or UI dependency (AR-03); portable-pty is refused (AR-28.3).
//!
//! Concurrency: PtyHandle and ProcessTree are cheap Arc clones over
//! interior-mutable platform state. read/write take &PtyHandle, and wait runs on
//! &ProcessTree, so a reader thread can block on read while another thread calls
//! wait. Every handle is released exactly once and close is idempotent.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

mod fidelity;
mod pipe;
mod rules;

#[cfg(windows)]
#[path = "windows/conpty.rs"]
mod conpty;
#[cfg(unix)]
#[path = "unix/pty.rs"]
mod unixpty;

pub use fidelity::{
    fidelity_for_hop, hop_note, Hop, RuleSetId, RULESET_AI_PLUGIN, RULESET_CONPTY,
    RULESET_CONTAINER, RULESET_SSH, RULESET_UI_VISUAL,
};
pub use rules::{
    bundled_rules, is_gate_failure, load_rules, RewriteRule, RuleError, RuleScope, Severity,
};

/// Which stage of spawn failed (kernel/02 section 3.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpawnStage {
    /// Resolving the program / argv.
    Resolve,
    /// Creating the pseudo console / pty pair.
    CreatePty,
    /// Creating the child process.
    Spawn,
    /// Binding pipes / std handles.
    Bind,
    /// Attaching the child to its Job Object.
    JobAttach,
}

/// Structured PTY error (kernel/02 section 3.1; kernel/07 section 3.8).
#[derive(Debug)]
pub enum PtyError {
    /// Spawn failed at a stage; errno is the platform error code (or an HRESULT).
    Spawn { errno: i32, stage: SpawnStage },
    /// An I/O syscall failed.
    Io(std::io::Error),
    /// The referenced pty handle no longer exists.
    NoSuchPty,
    /// The Job Object could not be created or joined; spawn is refused (DC-16).
    JobAttachDenied { code: i32 },
    /// The signal has no platform mapping.
    SignalUnsupported,
    /// A bounded wait elapsed without the process being reaped.
    Timeout { pid: u32, after: Duration },
    /// kill(Force) returned but the tree still has live processes.
    TreeNotEmpty { live: u32 },
    /// The requested capability is not provided by this backend.
    Unsupported(&'static str),
}

impl std::fmt::Display for PtyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PtyError::Spawn { errno, stage } => {
                write!(
                    f,
                    "spawn failed at stage {stage:?} (platform error {errno})"
                )
            }
            PtyError::Io(err) => write!(f, "io error: {err}"),
            PtyError::NoSuchPty => f.write_str("no such pty"),
            PtyError::JobAttachDenied { code } => {
                write!(f, "job object attach denied (platform error {code})")
            }
            PtyError::SignalUnsupported => f.write_str("signal unsupported"),
            PtyError::Timeout { pid, after } => {
                write!(f, "wait for pid {pid} timed out after {after:?}")
            }
            PtyError::TreeNotEmpty { live } => {
                write!(f, "process tree still has {live} live process(es)")
            }
            PtyError::Unsupported(what) => write!(f, "unsupported: {what}"),
        }
    }
}

impl std::error::Error for PtyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PtyError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PtyError {
    fn from(err: std::io::Error) -> Self {
        PtyError::Io(err)
    }
}

/// Window size in cells plus optional pixel dimensions (pixels default to 0).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WinSize {
    pub cols: u16,
    pub rows: u16,
    pub px_w: u16,
    pub px_h: u16,
}

impl WinSize {
    #[must_use]
    pub const fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols,
            rows,
            px_w: 0,
            px_h: 0,
        }
    }

    /// cols and rows must be >= 1 for any backend to apply them (kernel/02 section 3.4).
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.cols >= 1 && self.rows >= 1
    }
}

impl Default for WinSize {
    fn default() -> Self {
        Self::new(80, 24)
    }
}

/// Resize capability of a backend.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResizeCap {
    /// Lossless (POSIX TIOCSWINSZ).
    Exact,
    /// Applied to new content only (ConPTY, C-W3).
    Lossy,
    /// No window size at all (pipe fallback).
    Unsupported,
}

/// Signal capability of a backend.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SignalCap {
    /// POSIX signals to the foreground process group.
    Posix,
    /// Windows console control events (best effort, K-03).
    ConsoleEvent,
    /// Only a raw 0x03 byte can be written (K-03).
    ByteFallback,
}

/// Byte-fidelity label of a hop (AR-25 second clause).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fidelity {
    /// Bytes are unchanged.
    F0,
    /// Only the metadata layer may rewrite; carries the rule-set id.
    F1(RuleSetId),
    /// The render layer may rewrite; carries the rule-set id.
    F2(RuleSetId),
}

/// Capability snapshot; constant for the lifetime of a backend instance.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PtyCapabilities {
    pub resize: ResizeCap,
    pub signals: SignalCap,
    pub graphics_passthrough: bool,
    pub byte_fidelity: Fidelity,
    pub job_control: bool,
}

/// Semantic signal; callers must never hardcode integers (kernel/02 section 3.5).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Sig {
    Int,
    Term,
    Hup,
    Quit,
    Winch,
    Usr1,
    Usr2,
    Kill,
    Stop,
    Cont,
}

/// Result of a signal request.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SignalOutcome {
    /// The platform event/signal was generated.
    Delivered,
    /// The signal fell back to writing this byte to the pty input (K-03).
    ByteFallback(u8),
    /// The platform has no mapping; the caller must not pretend otherwise (AR-20).
    Unsupported,
}

/// Effect actually achieved by a resize request (K-07).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResizeEffect {
    /// Remote/local size changed and the application is expected to redraw.
    Applied,
    /// Applied to new content only (ConPTY, C-W3).
    AppliedLossy,
    /// Only the local grid changed; the target size is unchanged.
    LocalOnly,
}

/// Environment policy for a spawned command.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EnvPolicy {
    /// Inherit the parent environment.
    Inherit,
    /// Start with an empty environment.
    Clean,
    /// Use exactly these key/value pairs.
    Explicit(Vec<(String, String)>),
}

/// Whether a child may leave its process tree (05-spec section 3.3 rule 6).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DetachPolicy {
    Allow,
    Deny,
}

/// How hard to kill a process tree.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KillMode {
    /// Send a graceful request, then force after the duration.
    Graceful(Duration),
    /// TerminateJobObject / SIGKILL.
    Force,
}

/// How long to wait for a process tree.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WaitTimeout {
    /// Non-blocking poll.
    Zero,
    /// Wait up to this many milliseconds.
    Millis(u64),
    /// Wait forever.
    Infinite,
}

/// A program plus argv. Never a shell string (AR-06 rule 2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Command {
    pub program: String,
    pub args: Vec<String>,
}

impl Command {
    #[must_use]
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_args(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }
}

/// Options for a spawn.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SpawnOpts {
    pub env: EnvPolicy,
    pub cwd: Option<PathBuf>,
    pub login: bool,
    pub latency_budget: Duration,
    pub detach_policy: DetachPolicy,
}

impl Default for SpawnOpts {
    fn default() -> Self {
        Self {
            env: EnvPolicy::Inherit,
            cwd: None,
            login: false,
            latency_budget: Duration::from_secs(5),
            detach_policy: DetachPolicy::Deny,
        }
    }
}

/// Exit description. reaped is true on every successful wait (kernel/02 section 3.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ExitInfo {
    pub code: Option<i32>,
    pub signal: Option<Sig>,
    pub reaped: bool,
    pub live_children: u32,
    pub wall: Duration,
}

/// One process in a session tree.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ProcEntry {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub start_time: u64,
    pub cpu_ms: u64,
}

/// Platform operations owned by a PtyHandle.
pub(crate) trait HandleOps: Send + Sync {
    fn write(&self, data: &[u8]) -> Result<usize, PtyError>;
    fn read(&self, buf: &mut [u8]) -> Result<usize, PtyError>;
    fn resize(&self, sz: WinSize) -> Result<ResizeEffect, PtyError>;
    fn signal(&self, sig: Sig) -> Result<SignalOutcome, PtyError>;
    fn close(&self) -> Result<(), PtyError>;
    /// Original console output code page, when the backend has one (C-W5).
    fn original_code_page(&self) -> Option<u32> {
        None
    }
}

/// Platform operations owned by a ProcessTree.
pub(crate) trait TreeOps: Send + Sync {
    fn snapshot(&self) -> Result<Vec<ProcEntry>, PtyError>;
    fn live_children(&self) -> u32;
    fn kill(&self, mode: KillMode) -> Result<ExitInfo, PtyError>;
    fn wait(&self, to: WaitTimeout) -> Result<ExitInfo, PtyError>;
}

/// Opaque, cheaply cloneable pty handle.
///
/// The handle carries two Arcs: one for byte/size/signal operations and one for
/// the process tree, so a cloned handle can keep a tree alive after another clone
/// is closed. Cloning never duplicates platform handles; close releases them once.
pub struct PtyHandle {
    pub(crate) ops: Arc<dyn HandleOps>,
    pub(crate) tree: Arc<dyn TreeOps>,
}

impl PtyHandle {
    pub(crate) fn new(ops: Arc<dyn HandleOps>, tree: Arc<dyn TreeOps>) -> Self {
        Self { ops, tree }
    }

    /// Original console output code page recorded at spawn (C-W5). None on
    /// backends without a console code page.
    #[must_use]
    pub fn original_code_page(&self) -> Option<u32> {
        self.ops.original_code_page()
    }
}

impl Clone for PtyHandle {
    fn clone(&self) -> Self {
        Self {
            ops: Arc::clone(&self.ops),
            tree: Arc::clone(&self.tree),
        }
    }
}

impl std::fmt::Debug for PtyHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PtyHandle")
    }
}

/// Opaque process tree for a session.
pub struct ProcessTree {
    pub(crate) ops: Arc<dyn TreeOps>,
}

impl ProcessTree {
    pub(crate) fn new(ops: Arc<dyn TreeOps>) -> Self {
        Self { ops }
    }

    /// Number of processes still live in the tree.
    #[must_use]
    pub fn live_children(&self) -> u32 {
        self.ops.live_children()
    }
}

impl Clone for ProcessTree {
    fn clone(&self) -> Self {
        Self {
            ops: Arc::clone(&self.ops),
        }
    }
}

impl std::fmt::Debug for ProcessTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProcessTree")
    }
}

/// The nine-face PTY backend (ADR-0018 D1). Only capabilities and spawn are
/// backend-specific; the remaining faces delegate to the opaque handles so that a
/// single platform implementation serves every call shape.
pub trait PtyBackend: Send + Sync {
    /// Pure query; constant across calls for the backend instance.
    fn capabilities(&self) -> PtyCapabilities;

    /// Spawn a command on a new pty of the given size.
    fn spawn(&self, cmd: &Command, sz: WinSize, o: SpawnOpts) -> Result<PtyHandle, PtyError>;

    /// Write to the pty input; partial writes are the caller's loop.
    fn write(&self, pty: &PtyHandle, data: &[u8]) -> Result<usize, PtyError> {
        pty.ops.write(data)
    }

    /// Read from the pty output; 0 means EOF and keeps returning 0.
    fn read(&self, pty: &PtyHandle, buf: &mut [u8]) -> Result<usize, PtyError> {
        pty.ops.read(buf)
    }

    /// Resize the terminal.
    fn resize(&self, pty: &PtyHandle, sz: WinSize) -> Result<ResizeEffect, PtyError> {
        pty.ops.resize(sz)
    }

    /// Deliver a semantic signal.
    fn signal(&self, pty: &PtyHandle, sig: Sig) -> Result<SignalOutcome, PtyError> {
        pty.ops.signal(sig)
    }

    /// Kill a process tree; Force must leave no live children (PTY-ORPHAN-1).
    fn kill(&self, tree: &ProcessTree, mode: KillMode) -> Result<ExitInfo, PtyError> {
        tree.ops.kill(mode)
    }

    /// Wait for a tree; every Ok carries reaped == true.
    fn wait(&self, tree: &ProcessTree, to: WaitTimeout) -> Result<ExitInfo, PtyError> {
        tree.ops.wait(to)
    }

    /// Take the process tree of a handle.
    fn process_tree(&self, pty: &PtyHandle) -> Result<ProcessTree, PtyError> {
        Ok(ProcessTree::new(Arc::clone(&pty.tree)))
    }

    /// Snapshot the current process tree.
    fn tree_snapshot(&self, tree: &ProcessTree) -> Result<Vec<ProcEntry>, PtyError> {
        tree.ops.snapshot()
    }

    /// Release pty resources. Idempotent; does not implicitly kill the tree.
    fn close(&self, pty: PtyHandle) -> Result<(), PtyError> {
        pty.ops.close()
    }
}

/// The real platform backend: ConPTY on Windows, forkpty on unix.
#[must_use]
pub fn native_backend() -> Box<dyn PtyBackend> {
    #[cfg(windows)]
    {
        Box::new(conpty::ConPtyBackend::new())
    }
    #[cfg(unix)]
    {
        Box::new(unixpty::ForkPtyBackend::new())
    }
    #[cfg(not(any(windows, unix)))]
    {
        compile_error!("termai-pty has no native backend for this target")
    }
}

/// Backend for tests and for the degraded path: ordinary pipes via std::process,
/// with unsupported features declared honestly. It never claims a window size or
/// POSIX signal support.
#[must_use]
pub fn pipe_backend() -> Box<dyn PtyBackend> {
    Box::new(pipe::PipeBackend::new())
}

/// Probe for the best available backend, falling back to pipe_backend when the
/// native path is unavailable (kernel/02 section 6, degrade to PipeFallback).
#[must_use]
pub fn probe_backend() -> Box<dyn PtyBackend> {
    #[cfg(windows)]
    {
        if conpty::probe_available() {
            native_backend()
        } else {
            pipe_backend()
        }
    }
    #[cfg(unix)]
    {
        native_backend()
    }
    #[cfg(not(any(windows, unix)))]
    {
        compile_error!("termai-pty has no native backend for this target")
    }
}
