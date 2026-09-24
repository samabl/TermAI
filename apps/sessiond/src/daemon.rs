//! The daemon loop: what makes sessiond a running process instead of a library.
//!
//! Structure (lib.rs): Link (transport) -> Broker (protocol) -> Registry (state + Log +
//! engine). This module drives the first of those and owns the second's other half: the
//! [`SessionHost`](crate::host::SessionHost) that holds the real child processes. `main` is
//! four lines of argument passing into [`run`].
//!
//! Three things this module is responsible for:
//!
//! 1. **A real serve loop.** [`Daemon::serve`] reads the M0 [`Link`], decodes frames with the
//!    frame layer (CRC verified), hands each one to the existing [`Broker`], and writes the
//!    broker's replies back. No message is invented here and no encoding is touched: the wire
//!    contract is whatever `broker.rs` already implements.
//! 2. **Shutdown reaping.** Every exit from the loop - peer EOF, `GO_AWAY`, a frame-layer
//!    refusal, or a transport error - reaches [`Daemon::shutdown`], which ends with
//!    [`SessionHost::close_all`]. That call is the production caller the PTY-owning host
//!    (AR-30 item 2 / PTY-ORPHAN-1) previously lacked, and [`run`] turns its result into the
//!    process exit code. `Drop` runs the same teardown as a safety net for an early return.
//! 3. **A real PTY behind the session.** [`Daemon::open_session`] spawns the child on the
//!    platform backend (ConPTY + Job Object / forkpty), registers a VT-backed engine and a
//!    Session Log for it, and starts one pump thread that feeds every byte the pty produces
//!    into that engine **and writes the engine's answers back to the pty**. The write-back is
//!    not optional: a pseudoconsole application issues `ESC[6n` during startup and blocks on
//!    the reply (kernel/02 section 3.1), so without it the child never gets as far as running
//!    its command.
//!
//! ## How a session is created today, and why that is argv and not a message
//!
//! The broker has **no** session-create `msg_type`: `msg.rs` registers exactly HELLO, PING,
//! GO_AWAY, ERROR, CREDIT_UPDATE, the data/context/audit/attach families and the lease and
//! capability codes, and `Broker::on_frame` has a match arm for each of those - there is no
//! arm that would create a session, and every session in the protocol tests is pre-inserted
//! into the `Registry` by the harness. Adding one is a wire-contract change (a new `msg_type`,
//! a new CBOR body, a `kernel/07` section 3 row and an ADR under "编号即契约"), so this module
//! does not add one. What it does instead is the honest M0 arrangement: the launch contract is
//! **argv** ([`DaemonOptions`]), i.e. the parent process that spawns sessiond on the M0 stdio
//! pipe says which session to own and what to run in it, and sessiond (DC-18: the session
//! truth source) creates that session, hosts its pty and serves the existing protocol for it.
//! When the `SESSION_CREATE`-shaped message is added, this is the module it lands in: the arm
//! in `Broker::on_frame` would have to reach a [`Daemon::open_session`]-shaped call, which is
//! why that call already exists here with a real pty on the other side of it.
//!
//! ## What this module deliberately does NOT do
//!
//! * **No signal/termination handler.** Clean shutdown is reached through the link (EOF or
//!   `GO_AWAY`) and through `Drop`. Installing a SIGINT/SIGTERM handler needs `unsafe` FFI
//!   (or a new dependency), and `#![forbid(unsafe_code)]` is not negotiable for this crate, so
//!   a hard kill of the sessiond process itself has no in-process hook here; the M0 stdio
//!   arrangement is what covers it, because the parent going away closes the pipe and that is
//!   the EOF path.
//! * **No single-session close message.** There is no `SESSION_CLOSE` either, so a session is
//!   torn down by the shutdown path above rather than by a wire message.
//! * **No joined pump thread.** The pump is detached on purpose: the close path releases the
//!   pty handle, and `PtyBackend::close` is contracted to release a read that is already
//!   parked there (ConPTY: `CancelIoEx`; unix: the killed group closes the slave side), so the
//!   pump returns on its own. Joining would let a backend that does not honour that contract
//!   hang the shutdown path, which is the one path that must always finish.

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use termai_core::capability::{CapId, CAP_AUDIT_READ, CAP_SESSION_READ, CAP_STDIN_WRITE};
use termai_core::SessionId;
use termai_ipc::frame::{self, DecodeCfg, FrameHeader, IpcError};
use termai_ipc::handshake::{AuthState, Limits, ServerPolicy};
use termai_pty::{Command, PtyError, SpawnOpts, WinSize};
use termai_session::log::{SegmentWriter, SEG_FLAG_RAW_RING};
use termai_session::state::Trigger;

use crate::broker::{Broker, ConnState};
use crate::engine::VtEngine;
use crate::host::{CloseOutcome, HostError, PtySession, SessionHost};
use crate::link::{Link, StdioLink};
use crate::registry::{Registry, RegistryError};

/// Bytes read from the link per `Link::read`.
const LINK_READ_CHUNK: usize = 16 * 1024;
/// Bytes read from a pty per pump iteration (the same chunk `apps/termai` reads).
const PTY_READ_CHUNK: usize = 16 * 1024;

/// Clean shutdown and every hosted tree reaped.
pub const EXIT_OK: i32 = 0;
/// A hosted process tree was NOT reaped: the orphan risk AR-30 item 2 measures (AR-20: a
/// failed reap is reported, never rounded up to success).
pub const EXIT_NOT_REAPED: i32 = 1;
/// A usage error, or the transport/frame layer failed.
pub const EXIT_TRANSPORT: i32 = 2;
/// The session could not be opened (spawn refused, or its Log could not be created).
pub const EXIT_OPEN_FAILED: i32 = 3;
/// The handshake was refused; the NACK went back to the peer and the connection is over.
pub const EXIT_REFUSED: i32 = 4;

/// Why the serve loop stopped. Every variant is a shutdown path, and every one of them runs
/// the reaping teardown - that is the point of [`Daemon::serve`] returning instead of `?`ing
/// out of the loop.
#[derive(Debug)]
pub enum ShutdownReason {
    /// The peer closed the link. On the M0 stdio transport this is stdin EOF, i.e. the parent
    /// process is gone.
    PeerEof,
    /// The peer sent `GO_AWAY`; the broker moved to `Draining` and the daemon stopped serving.
    GoAway,
    /// The handshake was refused (version range, capabilities, replay). The NACK was written
    /// to the peer before the loop stopped.
    Refused,
    /// The peer sent bytes the frame layer refuses (CRC mismatch, reserved bits, oversize).
    Frame(IpcError),
    /// The transport itself failed.
    Transport(std::io::Error),
}

impl ShutdownReason {
    /// True when the peer and this daemon agreed how to stop - the clean ends of a connection.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        matches!(self, ShutdownReason::PeerEof | ShutdownReason::GoAway)
    }
}

/// What one serve loop did, including the reaping result of its shutdown path.
#[derive(Debug)]
pub struct ServeOutcome {
    /// Frames accepted from the peer (a frame the frame layer refused is not counted).
    pub frames_in: u64,
    /// Frames written back to the peer.
    pub frames_out: u64,
    pub shutdown: ShutdownReason,
    /// One entry per hosted session, in session-id order: the shutdown path's reap result.
    pub closes: Vec<(SessionId, Result<CloseOutcome, HostError>)>,
}

impl ServeOutcome {
    /// True when every hosted session really came down: each close reported zero live
    /// processes. A session that was already closed counts - the wanted end state holds.
    #[must_use]
    pub fn reaped_all(&self) -> bool {
        self.closes
            .iter()
            .all(|(_, result)| matches!(result, Ok(outcome) if outcome.live_children == 0))
    }

    /// True when the connection ended the way the lifecycle expects it to.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.shutdown.is_clean()
    }

    /// How many sessions failed to come down. Zero on a healthy shutdown.
    #[must_use]
    pub fn failures(&self) -> usize {
        self.closes
            .iter()
            .filter(|(_, result)| !matches!(result, Ok(outcome) if outcome.live_children == 0))
            .count()
    }

    /// The process exit code. A tree that was not reaped outranks the shutdown reason: the
    /// orphan is the failure that matters (AR-30 item 2).
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        if !self.reaped_all() {
            return EXIT_NOT_REAPED;
        }
        match &self.shutdown {
            ShutdownReason::PeerEof | ShutdownReason::GoAway => EXIT_OK,
            ShutdownReason::Refused => EXIT_REFUSED,
            ShutdownReason::Frame(_) | ShutdownReason::Transport(_) => EXIT_TRANSPORT,
        }
    }
}

/// Why the daemon could not open a session. Structured - no panic crosses this boundary
/// (AGENTS section 6).
#[derive(Debug)]
pub enum DaemonError {
    /// The platform layer or the host refused (spawn, already open, unreapable).
    Host(HostError),
    /// The registry refused the session (duplicate id, or its Log could not be created).
    Registry(RegistryError),
    /// The host did not report back the session it had just accepted. This cannot happen
    /// today; it is a refusal instead of an `expect`, because a panic here would skip the
    /// error path that releases the child (AGENTS section 6).
    SessionVanished(SessionId),
}

impl std::fmt::Display for DaemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DaemonError::Host(err) => write!(f, "{err}"),
            // RegistryError carries no Display of its own (it is a copyable enum); the Debug
            // form names the variant and the Log error without inventing a message.
            DaemonError::Registry(err) => write!(f, "registry refused the session: {err:?}"),
            DaemonError::SessionVanished(id) => {
                write!(f, "the host did not report session {id:?} back")
            }
        }
    }
}

impl std::error::Error for DaemonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DaemonError::Host(err) => Some(err),
            _ => None,
        }
    }
}

impl From<HostError> for DaemonError {
    fn from(err: HostError) -> Self {
        DaemonError::Host(err)
    }
}

impl From<RegistryError> for DaemonError {
    fn from(err: RegistryError) -> Self {
        DaemonError::Registry(err)
    }
}

/// The M0 launch contract: what the parent that spawns sessiond on the stdio pipe says.
///
/// This is deliberately **not** a wire message. The protocol has no session-create
/// `msg_type` (see the module docs), so the only honest place for "which session, running
/// what" today is the process arguments.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DaemonOptions {
    /// The session this process owns and serves. One daemon process, one session id: the
    /// broker is session-scoped (kernel/04 section 3.4).
    pub session: SessionId,
    pub cols: u16,
    pub rows: u16,
    /// Root of the Session Log; one directory per session under it.
    pub log_dir: PathBuf,
    /// The program to run. `None` means the platform's default shell.
    pub command: Option<Command>,
}

impl Default for DaemonOptions {
    fn default() -> Self {
        Self {
            session: SessionId(1),
            cols: 80,
            rows: 24,
            log_dir: PathBuf::from(".termai/sessions"),
            command: None,
        }
    }
}

/// What [`parse_args`] decided.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Launch {
    /// Serve one session over the M0 stdio link.
    Serve(DaemonOptions),
    Version,
    Help,
}

/// The M0 usage text. Printed on stdout only for `--help` and for a usage error, before any
/// frame is written (stdout is the IPC channel once serving starts).
pub const USAGE: &str = "\
sessiond - session truth daemon (kernel/04, DC-18)

USAGE:
  sessiond [OPTIONS] [-- PROGRAM [ARGS...]]
      --session ID      session id this process owns and serves (default 1)
      --cols N          initial columns (default 80)
      --rows N          initial rows (default 24)
      --log-dir DIR     Session Log root; one directory per session
                        (default .termai/sessions)
      --version         print the version and exit
      --help            print this text and exit

The daemon opens the session's real pty, then speaks the kernel/07 IPC protocol on
stdio (the M0 transport: a binary frame stream, never text). Every hosted process
tree is force-reaped before the process exits (AR-30 item 2 / PTY-ORPHAN-1).

Exit codes: 0 clean shutdown, every tree reaped; 1 a tree was NOT reaped;
            2 usage error or transport failure; 3 the session could not be opened;
            4 handshake refused.
";

/// Parse the launch contract. Errors are user-facing strings with an actionable hint.
pub fn parse_args(args: &[String]) -> Result<Launch, String> {
    let mut options = DaemonOptions::default();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => return Ok(Launch::Help),
            "--version" | "-V" => return Ok(Launch::Version),
            "--" => {
                let Some((program, rest)) = args[i + 1..].split_first() else {
                    return Err("-- needs a program to run".to_string());
                };
                options.command = Some(Command::with_args(program.clone(), rest.to_vec()));
                break;
            }
            "--session" => {
                let value = args.get(i + 1).ok_or("--session needs a value")?;
                options.session = SessionId(
                    value
                        .parse()
                        .map_err(|_| format!("--session: not a session id: {value}"))?,
                );
                i += 2;
            }
            "--cols" => {
                let value = args.get(i + 1).ok_or("--cols needs a value")?;
                options.cols = value
                    .parse()
                    .map_err(|_| format!("--cols: not a number: {value}"))?;
                i += 2;
            }
            "--rows" => {
                let value = args.get(i + 1).ok_or("--rows needs a value")?;
                options.rows = value
                    .parse()
                    .map_err(|_| format!("--rows: not a number: {value}"))?;
                i += 2;
            }
            "--log-dir" => {
                let value = args.get(i + 1).ok_or("--log-dir needs a value")?;
                options.log_dir = PathBuf::from(value);
                i += 2;
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option: {other}"));
            }
            // A bare program is the caller's argv, so `sessiond ls -la` means `-- ls -la`.
            other => {
                options.command = Some(Command::with_args(other, args[i + 1..].to_vec()));
                break;
            }
        }
    }
    if options.cols == 0 || options.rows == 0 {
        return Err("--cols/--rows must be greater than zero".to_string());
    }
    Ok(Launch::Serve(options))
}

/// The M0 server policy for the daemon's own connection: a local peer (DC-24 places the
/// transport on local IPC), the three P0 capability ids, and the frame layer's own limits.
#[must_use]
pub fn local_policy() -> ServerPolicy {
    let caps: Vec<CapId> = vec![CAP_SESSION_READ, CAP_STDIN_WRITE, CAP_AUDIT_READ];
    ServerPolicy {
        proto_min: 1,
        proto_max: termai_core::PROTO_VERSION,
        caps,
        required: vec![CAP_SESSION_READ],
        limits: Limits::default(),
        auth_state: AuthState::LocalPeer,
        feature_bits: 0,
        seen_nonces: Vec::new(),
    }
}

/// The process entry point: parse the launch contract, open the session's real pty, serve the
/// M0 stdio link, and reap on the way out. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let options = match parse_args(args) {
        Ok(Launch::Help) => {
            print!("{USAGE}");
            return EXIT_OK;
        }
        Ok(Launch::Version) => {
            println!("sessiond {}", env!("CARGO_PKG_VERSION"));
            return EXIT_OK;
        }
        Ok(Launch::Serve(options)) => options,
        Err(err) => {
            eprintln!("sessiond: {err}");
            eprint!("{USAGE}");
            return EXIT_TRANSPORT;
        }
    };

    let command = options
        .command
        .clone()
        .unwrap_or_else(default_shell_command);
    let size = WinSize::new(options.cols, options.rows);
    let mut daemon = Daemon::probed(local_policy(), options.session, options.log_dir.clone());
    if let Err(err) = daemon.open_session(options.session, &command, size) {
        // AGENTS section 6 and AR-12: the program name is recorded, never the argv - argv is
        // where tokens and paths live.
        eprintln!(
            "sessiond: cannot open session {}: {err} (program {})",
            options.session.0, command.program
        );
        return EXIT_OPEN_FAILED;
    }
    eprintln!(
        "sessiond: serving session {} on stdio ({}x{}, log root {})",
        options.session.0,
        options.cols,
        options.rows,
        options.log_dir.display()
    );

    let mut link = StdioLink::new();
    let outcome = daemon.serve(&mut link);
    eprintln!(
        "sessiond: shutdown {:?} frames_in={} frames_out={} sessions={} not_reaped={} clean={}",
        outcome.shutdown,
        outcome.frames_in,
        outcome.frames_out,
        outcome.closes.len(),
        outcome.failures(),
        outcome.is_clean()
    );
    outcome.exit_code()
}

/// The platform default shell, as a `Command` (argv only, never a shell string - AR-06).
fn default_shell_command() -> Command {
    if cfg!(windows) {
        Command::new(std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()))
    } else {
        Command::new(std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()))
    }
}

/// The session truth daemon: one broker, one host, one Log root.
///
/// The broker sits behind a mutex shared with the per-session pump threads. That sharing is
/// what lets a pump feed the session's engine and take its DSR/CPR answers without a second
/// copy of the session state; the cost is that a pump waits for the lock while the serve loop
/// is inside `Broker::on_frame` (the longest case is a tail replay reading Log segments from
/// disk). Both critical sections are short, and a pty write is never issued while either lock
/// is held, so there is no lock order to deadlock on.
pub struct Daemon {
    broker: Arc<Mutex<Broker>>,
    host: SessionHost,
    session: SessionId,
    log_dir: PathBuf,
}

impl Daemon {
    /// Build a daemon whose registry starts **empty**: the sessions it serves are the ones
    /// [`Daemon::open_session`] opens, not the ones a test harness pre-inserts.
    #[must_use]
    pub fn new(
        policy: ServerPolicy,
        host: SessionHost,
        session: SessionId,
        log_dir: PathBuf,
    ) -> Self {
        Self {
            broker: Arc::new(Mutex::new(Broker::new(policy, Registry::new(), session))),
            host,
            session,
            log_dir,
        }
    }

    /// The same, on the best backend this machine has (kernel/02 section 6: ConPTY / forkpty,
    /// or the declared pipe fallback when the native path is unavailable).
    #[must_use]
    pub fn probed(policy: ServerPolicy, session: SessionId, log_dir: PathBuf) -> Self {
        Self::new(policy, SessionHost::probed(), session, log_dir)
    }

    /// The broker, for diagnostics and for the acceptance tests' assertions.
    pub fn broker(&self) -> MutexGuard<'_, Broker> {
        lock(&self.broker)
    }

    /// The PTY-owning host: the component whose `close_all` is the shutdown path.
    #[must_use]
    pub fn host(&self) -> &SessionHost {
        &self.host
    }

    /// The session this process owns and serves.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session
    }

    /// The hosted pty for `id`, or `None` once it has been closed. Cloning the returned
    /// handle (or its tree) before the shutdown is how a caller polls the tree afterwards.
    #[must_use]
    pub fn session(&self, id: SessionId) -> Option<Arc<PtySession>> {
        self.host.get(id)
    }

    /// The Log directory of a session: one directory per session, so a segment path and a
    /// rebuild scan stay per-session (kernel/04 section 3.2, `restore.rs`).
    #[must_use]
    pub fn session_log_dir(&self, id: SessionId) -> PathBuf {
        self.log_dir.join(format!("{:032x}", id.0))
    }

    /// Open the session's real child process, register its Log and VT engine, and start its
    /// pump. This is the daemon-side answer to "sessiond holds the PTY": after this call the
    /// session has a real process tree that [`Daemon::shutdown`] can reap.
    ///
    /// Every failure path releases the pty it may have just spawned - a daemon that cannot
    /// set a session up must not leave the child behind (kernel/02 section 3.3).
    pub fn open_session(
        &mut self,
        id: SessionId,
        command: &Command,
        size: WinSize,
    ) -> Result<(), DaemonError> {
        if self.host.get(id).is_some() {
            return Err(DaemonError::Host(HostError::AlreadyOpen(id)));
        }
        if lock(&self.broker).registry().get(id).is_some() {
            return Err(DaemonError::Registry(RegistryError::AlreadyExists));
        }
        // `SpawnOpts::default()` is `DetachPolicy::Deny`: a descendant must not be able to
        // leave the Job Object / process group, or the close path could not reap the whole
        // tree (kernel/02 section 3.3).
        self.host
            .open(id, command, size, SpawnOpts::default())
            .map_err(DaemonError::Host)?;
        // The host accepted the session, so `get` answers Some. Refusing instead of unwrapping
        // keeps a panic out of this boundary (AGENTS section 6); the child goes below.
        let Some(pty) = self.host.get(id) else {
            abandon(&mut self.host, id);
            return Err(DaemonError::SessionVanished(id));
        };

        let now = now_ns();
        let path = self.session_log_dir(id);
        let writer = match SegmentWriter::create(&path, 0, 0, [0u8; 8], now, SEG_FLAG_RAW_RING) {
            Ok(writer) => writer,
            Err(err) => {
                abandon(&mut self.host, id);
                return Err(DaemonError::Registry(RegistryError::Log(err)));
            }
        };
        let mut broker = lock(&self.broker);
        if let Err(err) = broker.registry_mut().insert(
            id,
            Box::new(VtEngine::new(size.cols, size.rows)),
            writer,
            now,
        ) {
            drop(broker);
            abandon(&mut self.host, id);
            return Err(DaemonError::Registry(err));
        }
        // kernel/04 section 3.1: spawn ok is what moves created -> running, and the transition
        // is persisted before it is visible (invariant 4).
        if let Err(err) = broker.registry_mut().apply(id, Trigger::SpawnOk, now) {
            drop(broker);
            abandon(&mut self.host, id);
            return Err(DaemonError::Registry(err));
        }
        drop(broker);

        self.start_pump(id, pty);
        Ok(())
    }

    /// One pump thread per session: read the pty, feed the session's engine (which records
    /// the bytes in the Log first - `Registry::feed_pty_out`), and write the engine's answers
    /// back to the pty.
    fn start_pump(&mut self, id: SessionId, pty: Arc<PtySession>) {
        let broker = Arc::clone(&self.broker);
        std::thread::spawn(move || {
            let mut buf = [0u8; PTY_READ_CHUNK];
            loop {
                let read = match pty.read(&mut buf) {
                    // EOF: the child side is gone.
                    Ok(0) => break,
                    Ok(read) => read,
                    // The close path released the pty: this pump is over.
                    Err(_) => break,
                };
                let responses = {
                    let mut broker = lock(&broker);
                    // The Log record and the engine feed are one call, in that order
                    // (kernel/04 section 3.2: raw output is a ring record, not a screen).
                    if broker
                        .registry_mut()
                        .feed_pty_out(id, &buf[..read], now_ns())
                        .is_err()
                    {
                        // The session is gone from the registry: nothing left to feed.
                        break;
                    }
                    broker.registry_mut().take_responses(id)
                };
                for response in responses {
                    if write_all(&pty, &response).is_err() {
                        // The pty is gone; the next read reports that too.
                        break;
                    }
                }
            }
        });
    }

    /// Serve the M0 link until the connection ends, then shut down.
    ///
    /// The loop is single-threaded on purpose: the frame layer is the only place bytes are
    /// interpreted, replies are written in the order the broker produced them, and the pump
    /// threads run the pty side independently. Every exit from the loop - peer EOF, `GO_AWAY`,
    /// a refused handshake, a frame the frame layer rejects, a transport error - reaches
    /// [`Daemon::shutdown`] exactly once, and its result travels back in the outcome.
    pub fn serve(&mut self, link: &mut dyn Link) -> ServeOutcome {
        let mut frames_in = 0u64;
        let mut frames_out = 0u64;
        let mut buffer: Vec<u8> = Vec::new();
        let shutdown = 'serving: loop {
            // Every complete frame already buffered, in order.
            loop {
                let (header, payload) = match frame::decode(&buffer, DecodeCfg::verifying()) {
                    Ok(Some(decoded)) => (decoded.header, decoded.payload.to_vec()),
                    Ok(None) => break, // NeedMore: read again below.
                    Err(err) => break 'serving ShutdownReason::Frame(err),
                };
                buffer.drain(..frame::FRAME_HEADER_LEN + header.len as usize);
                frames_in += 1;
                match self.dispatch(&header, &payload, link) {
                    Ok((written, Some(reason))) => {
                        frames_out += written;
                        break 'serving reason;
                    }
                    Ok((written, None)) => frames_out += written,
                    Err(reason) => break 'serving reason,
                }
            }
            let mut chunk = [0u8; LINK_READ_CHUNK];
            match link.read(&mut chunk) {
                // A clean end of the connection. Anything left in `buffer` is a partial frame
                // the peer never finished; the peer is gone, so there is nothing to answer.
                Ok(0) => break 'serving ShutdownReason::PeerEof,
                Ok(read) => buffer.extend_from_slice(&chunk[..read]),
                Err(err) if err.kind() == ErrorKind::Interrupted => {}
                Err(err) => break 'serving ShutdownReason::Transport(err),
            }
        };
        let closes = self.shutdown();
        ServeOutcome {
            frames_in,
            frames_out,
            shutdown,
            closes,
        }
    }

    /// One frame in, its replies out. Returns the frames written and, when the connection is
    /// over, why: a refused handshake (`Failed`) or the peer's `GO_AWAY` (`Draining`).
    fn dispatch(
        &self,
        header: &FrameHeader,
        payload: &[u8],
        link: &mut dyn Link,
    ) -> Result<(u64, Option<ShutdownReason>), ShutdownReason> {
        let version;
        let replies;
        let stop;
        {
            let mut broker = lock(&self.broker);
            replies = broker
                .on_frame(header, payload)
                .map_err(ShutdownReason::Frame)?;
            version = broker.chosen_ver();
            stop = match broker.state() {
                ConnState::Draining => Some(ShutdownReason::GoAway),
                ConnState::Failed => Some(ShutdownReason::Refused),
                _ => None,
            };
        }
        let mut written = 0u64;
        for reply in &replies {
            let bytes = reply.encode(version).map_err(ShutdownReason::Frame)?;
            link.write_all(&bytes).map_err(ShutdownReason::Transport)?;
            written += 1;
        }
        Ok((written, stop))
    }

    /// The daemon's one teardown path.
    ///
    /// It persists the terminal lifecycle state of every hosted session and then reaps every
    /// process tree with [`SessionHost::close_all`], which for each session runs the fixed
    /// `kill(Force)` -> bounded wait -> bounded liveness poll -> release order of `host.rs`
    /// (AR-30 item 2 gives 2 s; `REAP_BUDGET` sits inside that).
    ///
    /// A state transition the machine refuses (for example a session that already reached a
    /// terminal state) is left as it is: the acceptance item here is the reap, and the fact
    /// that matters - the process tree is gone - is the one below.
    pub fn shutdown(&mut self) -> Vec<(SessionId, Result<CloseOutcome, HostError>)> {
        let now = now_ns();
        for id in self.host.ids() {
            let mut broker = lock(&self.broker);
            // The reader is over and the tree is about to be reaped: exited, then dead
            // (kernel/04 section 3.1: `EofReaped` then `Reaped`, reason `reaped`).
            let _ = broker.registry_mut().apply(id, Trigger::EofReaped, now);
            let _ = broker.registry_mut().apply(id, Trigger::Reaped, now);
        }
        self.host.close_all()
    }
}

impl std::fmt::Debug for Daemon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = lock(&self.broker).state();
        f.debug_struct("Daemon")
            .field("session", &self.session)
            .field("state", &state)
            .field("host", &self.host)
            .field("log_dir", &self.log_dir)
            .finish()
    }
}

/// The safety net for the paths that never reach [`Daemon::serve`]'s shutdown: an early
/// `return` in `run`, or an unwinding panic in a caller. Closing twice is a no-op
/// (`CloseOutcome::already_closed`), so this cannot corrupt a completed teardown.
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.host.close_all();
    }
}

/// Release a session that failed to register. The primary failure is the one the caller has
/// already been given; a reap that also fails is an orphan risk and is reported (AR-20: never
/// silently), naming the session but never its command or its output.
fn abandon(host: &mut SessionHost, id: SessionId) {
    if let Ok(outcome) = host.close(id) {
        if outcome.live_children == 0 {
            return;
        }
    }
    eprintln!(
        "sessiond: session {} could not be reaped after a setup failure; a live process tree \
         may remain (AR-30 item 2)",
        id.0
    );
}

/// Write every byte, honouring partial writes (kernel/02 section 3.1).
fn write_all(pty: &PtySession, mut data: &[u8]) -> Result<(), PtyError> {
    while !data.is_empty() {
        let written = pty.write(data)?;
        if written == 0 {
            return Err(PtyError::Io(std::io::Error::from(ErrorKind::WriteZero)));
        }
        data = &data[written..];
    }
    Ok(())
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}

/// A poisoned lock is recovered instead of panicking across this boundary
/// (AGENTS section 6: a pump thread must never take the daemon down).
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use termai_pty::{pipe_backend, PtyBackend};

    fn test_daemon() -> Daemon {
        // A backend instance, not a real pty: these tests cover the launch contract and the
        // empty teardown, and must not spawn anything.
        let backend: Arc<dyn PtyBackend> = Arc::from(pipe_backend());
        Daemon::new(
            local_policy(),
            SessionHost::new(backend),
            SessionId(1),
            PathBuf::from("."),
        )
    }

    #[test]
    fn usage_lists_every_documented_exit_code() {
        for line in [
            "0 clean",
            "1 a tree",
            "2 usage",
            "3 the session",
            "4 handshake",
        ] {
            assert!(USAGE.contains(line), "usage must document {line}");
        }
    }

    #[test]
    fn defaults_are_the_documented_ones() {
        let Ok(Launch::Serve(options)) = parse_args(&[]) else {
            panic!("no arguments must mean serve");
        };
        assert_eq!(options.session, SessionId(1));
        assert_eq!((options.cols, options.rows), (80, 24));
        assert_eq!(options.log_dir, PathBuf::from(".termai/sessions"));
        assert!(options.command.is_none(), "None means the default shell");
    }

    #[test]
    fn the_launch_contract_carries_the_session_and_the_program() {
        let Ok(Launch::Serve(options)) = parse_args(&[
            "--session".into(),
            "42".into(),
            "--cols".into(),
            "120".into(),
            "--rows".into(),
            "40".into(),
            "--log-dir".into(),
            "logs".into(),
            "--".into(),
            "ls".into(),
            "-la".into(),
        ]) else {
            panic!("valid argv must mean serve");
        };
        assert_eq!(options.session, SessionId(42));
        assert_eq!((options.cols, options.rows), (120, 40));
        assert_eq!(options.log_dir, PathBuf::from("logs"));
        assert_eq!(
            options.command,
            Some(Command::with_args("ls", vec!["-la".to_string()]))
        );
    }

    #[test]
    fn a_bare_program_is_accepted_and_help_version_short_circuit() {
        let Ok(Launch::Serve(options)) = parse_args(&["cmd.exe".into()]) else {
            panic!("a bare program must mean serve");
        };
        assert_eq!(options.command, Some(Command::new("cmd.exe")));
        assert_eq!(parse_args(&["--help".into()]), Ok(Launch::Help));
        assert_eq!(parse_args(&["-h".into()]), Ok(Launch::Help));
        assert_eq!(parse_args(&["--version".into()]), Ok(Launch::Version));
    }

    #[test]
    fn bad_arguments_are_refused_with_a_hint() {
        assert!(parse_args(&["--session".into()]).is_err());
        assert!(parse_args(&["--session".into(), "not-a-number".into()]).is_err());
        assert!(parse_args(&["--cols".into(), "0".into()]).is_err());
        assert!(parse_args(&["--rows".into(), "abc".into()]).is_err());
        assert!(parse_args(&["--log-dir".into()]).is_err());
        assert!(parse_args(&["--wat".into()]).is_err());
        assert!(parse_args(&["--".into()]).is_err());
    }

    #[test]
    fn the_local_policy_offers_the_p0_capabilities_and_the_protocol_version() {
        let policy = local_policy();
        assert!(policy.caps.contains(&CAP_SESSION_READ));
        assert!(policy.caps.contains(&CAP_STDIN_WRITE));
        assert!(policy.caps.contains(&CAP_AUDIT_READ));
        assert_eq!(policy.required, vec![CAP_SESSION_READ]);
        assert_eq!(policy.proto_max, termai_core::PROTO_VERSION);
        assert_eq!(policy.auth_state, AuthState::LocalPeer);
    }

    #[test]
    fn a_daemon_with_no_hosted_session_shuts_down_empty_and_clean() {
        // The trivial end of the shutdown path: no hosted session is not an error, and the
        // outcome still reports a zero exit code.
        let mut daemon = test_daemon();
        let outcome = daemon.serve(&mut crate::link::MemoryLink::new());
        assert!(matches!(outcome.shutdown, ShutdownReason::PeerEof));
        assert!(outcome.closes.is_empty());
        assert!(outcome.reaped_all(), "{outcome:?}");
        assert!(outcome.is_clean(), "{outcome:?}");
        assert_eq!(outcome.exit_code(), EXIT_OK);
        assert_eq!(outcome.frames_in, 0);
        assert!(daemon.host().is_empty());
    }

    #[test]
    fn session_log_dirs_are_per_session_and_fixed_width() {
        let daemon = test_daemon();
        let dir = daemon.session_log_dir(SessionId(0xabc));
        assert_eq!(dir.parent(), Some(std::path::Path::new(".")));
        assert_eq!(
            dir.file_name().and_then(|n| n.to_str()),
            Some("00000000000000000000000000000abc")
        );
    }

    #[test]
    fn a_second_teardown_is_a_no_op_and_drop_is_the_safety_net() {
        let mut daemon = test_daemon();
        assert!(daemon.shutdown().is_empty());
        assert!(daemon.shutdown().is_empty());
        assert_eq!(daemon.session_id(), SessionId(1));
        // Drop runs close_all again; an empty host makes that a no-op, not an error.
        drop(daemon);
    }
}
