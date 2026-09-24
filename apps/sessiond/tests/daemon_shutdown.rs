//! sessiond's daemon shutdown path with real processes: the production caller of the
//! PTY-owning [`SessionHost`] (AR-30 item 2 / PTY-ORPHAN-1), driven through the M0 link the
//! way `apps/sessiond/tests/wire.rs` drives the broker.
//!
//! What is measured, and how:
//!
//! * the daemon is built in-process (`Daemon::new`) on the **native** backend, so no test here
//!   can be satisfied by the declared pipe fallback;
//! * every session is a real child process with a grandchild (Windows: `cmd.exe` waiting on
//!   `ping`; unix: `/bin/sh` waiting on `sleep 300`). On Windows the descendant is a **named**
//!   assertion, and it is only reachable because the daemon's own pump answers the `ESC[6n`
//!   the pseudoconsole sends at startup - the same reason `pty_lifecycle.rs` runs a DSR reader;
//! * the trigger is always a real end of the connection (EOF, `GO_AWAY`, a refused handshake,
//!   a frame the frame layer rejects), never a direct call to `close_all`;
//! * the trees are polled, never sampled once, and each session's close wall time is checked
//!   against the 2 s AR-30 item 2 bound that `host.rs` already asserts per `close()`.

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use sessiond::daemon::{local_policy, Daemon, ServeOutcome, ShutdownReason};
use sessiond::host::{PtySession, SessionHost};
use sessiond::link::MemoryLink;

use termai_core::capability::{CAP_AUDIT_READ, CAP_SESSION_READ, CAP_STDIN_WRITE};
use termai_core::SessionId;
use termai_ipc::codec;
use termai_ipc::frame::{self, flag, DecodeCfg};
use termai_ipc::handshake::{ClientKind, Hello};
use termai_ipc::msg;
use termai_pty::{Command, ProcEntry, ProcessTree, PtyBackend, WinSize};

/// AR-30 item 2 and the HARNESS section 8.2 reliability row: session close to a clean process
/// tree is 2 s. `host.rs`'s `REAP_BUDGET` sits below it.
const ORPHAN_BUDGET: Duration = Duration::from_secs(2);

/// How long a freshly spawned session gets to show its descendant. This is setup, not the
/// measurement, so it is deliberately generous (the same 10 s `pty_lifecycle.rs` uses).
const SETUP_LIMIT: Duration = Duration::from_secs(10);

/// Processes a live two-process tree is expected to list on Windows/ConPTY, which enumerates
/// the whole Job Object: `cmd.exe` plus the `ping` it waits on.
#[cfg(windows)]
const WINDOWS_TREE_PROCESSES: usize = 2;
/// On unix the tree ops report the process-group leader only (see
/// `expected_tree_processes`), so the honest expectation there is exactly one entry.
#[cfg(unix)]
const UNIX_TREE_PROCESSES: usize = 1;
/// The grandchild's name in a Windows job listing.
#[cfg(windows)]
const GRANDCHILD_NAME: &str = "ping";

/// The platform's honest expectation for a live session tree.
///
/// **Windows/ConPTY** enumerates the whole Job Object, so the shell and the descendant it is
/// waiting on are both listed.
///
/// **unix** lists the leader only: `TreeOps::snapshot` for `UnixTree`
/// (`crates/termai-pty/src/unix/pty.rs:417-436`) does `waitpid(WNOHANG)` on the single pid
/// `forkpty` returned and answers exactly one `ProcEntry`, or none once it is gone. The whole
/// group is still signalled and probed (`kill(-pgid, ...)`, same file lines 102-111 and
/// 438-447), so `live_children() == 0` after a close is evidence about the group, grandchild
/// included - it just cannot be *named* there. That is what the unix arm asserts instead of a
/// silent skip.
fn expected_tree_processes() -> usize {
    #[cfg(windows)]
    {
        WINDOWS_TREE_PROCESSES
    }
    #[cfg(unix)]
    {
        UNIX_TREE_PROCESSES
    }
}

/// A shell that waits on a long-lived descendant: the session tree really holds a child and a
/// grandchild, which is the case a root-only reaper would miss (kernel/02 section 3.3).
fn shell_waiting_on_a_grandchild() -> Command {
    #[cfg(windows)]
    {
        Command::with_args(
            "cmd.exe",
            vec!["/C".to_string(), "ping -n 60 127.0.0.1".to_string()],
        )
    }
    #[cfg(unix)]
    {
        Command::with_args(
            "/bin/sh",
            vec!["-c".to_string(), "sleep 300 & wait".to_string()],
        )
    }
}

/// The text the pump test waits for in the session's grid. On Windows it is what `ping` prints
/// first, which is the same statement as "the pseudoconsole got as far as running the command"
/// (it only gets there after the daemon answers its `ESC[6n`). On unix nothing is printed
/// unless the shell is told to, so the marker command says it.
#[cfg(windows)]
const MARKER: &str = "127.0.0.1";
#[cfg(unix)]
const MARKER: &str = "termai-pump-marker";

fn marker_command() -> Command {
    #[cfg(windows)]
    {
        shell_waiting_on_a_grandchild()
    }
    #[cfg(unix)]
    {
        Command::with_args(
            "/bin/sh",
            vec!["-c".to_string(), format!("echo {MARKER}; sleep 300")],
        )
    }
}

fn size() -> WinSize {
    WinSize::new(80, 24)
}

fn tmp_dir(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "termai-daemon-{tag}-{}-{:?}",
        std::process::id(),
        thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("temp log root");
    path
}

/// A daemon on the native backend, plus that backend for polling trees from the test side.
fn daemon(tag: &str) -> (Daemon, Arc<dyn PtyBackend>) {
    let backend: Arc<dyn PtyBackend> = Arc::from(termai_pty::native_backend());
    let daemon = Daemon::new(
        local_policy(),
        SessionHost::new(Arc::clone(&backend)),
        SessionId(1),
        tmp_dir(tag),
    );
    (daemon, backend)
}

/// What the test keeps from a hosted session so it can poll it after the daemon's shutdown has
/// already removed its own entry: the process tree handle and the pty handle.
struct Watched {
    tree: ProcessTree,
    pty: Arc<PtySession>,
}

/// Open a real session and keep its handles.
fn open_watched(daemon: &mut Daemon, id: SessionId) -> Watched {
    daemon
        .open_session(id, &shell_waiting_on_a_grandchild(), size())
        .unwrap_or_else(|err| panic!("open session {id:?}: {err}"));
    let session = daemon
        .session(id)
        .expect("the daemon must report the session it just opened");
    assert!(
        session.root_pid().is_some(),
        "a daemon-hosted session must own a real child process"
    );
    Watched {
        tree: session.tree().clone(),
        pty: session,
    }
}

/// Wait (setup, not measurement) until a freshly opened session's tree holds the child and the
/// grandchild. Returns what was seen, so a failure prints the real listing.
fn wait_until_live(backend: &Arc<dyn PtyBackend>, watched: &Watched, what: &str) -> Vec<ProcEntry> {
    let entries = wait_for_count(
        backend,
        &watched.tree,
        expected_tree_processes(),
        SETUP_LIMIT,
    );
    assert!(
        entries.len() >= expected_tree_processes(),
        "{what} must own a real tree before the measurement: {entries:?}"
    );
    entries
}

fn hello_payload(claim: SessionId) -> Vec<u8> {
    codec::to_bytes(&codec::hello_to_value(&Hello {
        proto_min: 1,
        proto_max: termai_core::PROTO_VERSION,
        client_kind: ClientKind::Cli,
        caps: vec![CAP_SESSION_READ, CAP_STDIN_WRITE, CAP_AUDIT_READ],
        required: vec![CAP_SESSION_READ],
        session_claim: claim,
        feature_bits: 0,
        nonce: [9u8; 16],
    }))
}

/// Decode every frame in `bytes`, failing the test on a damaged stream: a reply that cannot be
/// decoded is not evidence of a working serve loop.
fn decode_msg_types(bytes: &[u8]) -> Vec<u16> {
    let mut out = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let decoded = frame::decode(rest, DecodeCfg::verifying())
            .expect("the daemon's own replies must pass CRC verification")
            .expect("a whole frame per reply");
        out.push(decoded.header.msg_type);
        rest = &rest[frame::FRAME_HEADER_LEN + decoded.header.len as usize..];
    }
    out
}

/// Read the tree, failing the test on an error: an unreadable tree must never be mistaken for
/// an empty one.
fn snapshot(backend: &Arc<dyn PtyBackend>, tree: &ProcessTree) -> Vec<ProcEntry> {
    backend
        .tree_snapshot(tree)
        .expect("tree snapshot of a live tree handle")
}

/// Poll the tree until it lists at least `count` processes, or the limit passes.
fn wait_for_count(
    backend: &Arc<dyn PtyBackend>,
    tree: &ProcessTree,
    count: usize,
    limit: Duration,
) -> Vec<ProcEntry> {
    let deadline = Instant::now() + limit;
    loop {
        let entries = backend.tree_snapshot(tree).unwrap_or_default();
        if entries.len() >= count || Instant::now() >= deadline {
            return entries;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Poll the tree until it is empty, or the limit passes.
fn wait_for_empty(
    backend: &Arc<dyn PtyBackend>,
    tree: &ProcessTree,
    limit: Duration,
) -> Vec<ProcEntry> {
    let deadline = Instant::now() + limit;
    loop {
        let entries = backend.tree_snapshot(tree).unwrap_or_default();
        if entries.is_empty() || Instant::now() >= deadline {
            return entries;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Assert one session really came down.
///
/// Two independent statements, both required by AR-30 item 2 / PTY-ORPHAN-1:
/// the daemon's own close reported zero live processes inside the 2 s bound, and the tree the
/// test holds is empty when polled afterwards (the daemon's entry is gone by then, so the test
/// polls its own handles - a tree that only the daemon could read would be no evidence).
fn assert_reaped(
    watched: &Watched,
    backend: &Arc<dyn PtyBackend>,
    id: SessionId,
    outcome: &ServeOutcome,
) {
    let (_, close) = outcome
        .closes
        .iter()
        .find(|(closed, _)| *closed == id)
        .unwrap_or_else(|| panic!("the shutdown path must report session {id:?}: {outcome:?}"));
    let close = close
        .as_ref()
        .unwrap_or_else(|err| panic!("session {id:?} was not reaped: {err}"));
    assert!(!close.already_closed, "session {id:?} must really close");
    assert!(
        close.reaped,
        "session {id:?} must report the reap: {close:?}"
    );
    assert_eq!(
        close.live_children, 0,
        "PTY-ORPHAN-1: session {id:?} still had live processes at the end of the close"
    );
    assert!(
        close.wall <= ORPHAN_BUDGET,
        "session {id:?} took {:?} to close; AR-30 item 2 allows 2 s",
        close.wall
    );
    let left = wait_for_empty(backend, &watched.tree, ORPHAN_BUDGET);
    assert!(
        left.is_empty(),
        "PTY-ORPHAN-1 / AR-30 item 2: session {id:?} still holds {left:?}"
    );
    assert!(
        snapshot(backend, &watched.tree).is_empty(),
        "the closed session's tree must be readable and empty"
    );
    assert_eq!(
        watched.pty.live_children(),
        0,
        "session {id:?} still reports a live child through its own pty handle"
    );
    assert!(
        watched.pty.is_closed(),
        "session {id:?} must have released its pty"
    );
}

/// The acceptance test: a clean end of the connection - here peer EOF, which on the M0 stdio
/// transport is the parent closing the pipe - reaps **every** hosted tree, and the serve loop
/// reports success.
///
/// The control is session B: nothing in this test closes it, and it is asserted live right
/// before the trigger, so the empty tree measured afterwards can only come from the daemon's
/// own shutdown path.
#[test]
fn a_clean_shutdown_reaps_every_hosted_tree_and_reports_success() {
    let (mut daemon, backend) = daemon("shutdown-eof");
    let (a, b) = (SessionId(1), SessionId(2));
    let watched_a = open_watched(&mut daemon, a);
    let watched_b = open_watched(&mut daemon, b);

    let before_a = wait_until_live(&backend, &watched_a, "session A");
    let before_b = wait_until_live(&backend, &watched_b, "the control session B");
    #[cfg(windows)]
    assert!(
        before_b
            .iter()
            .any(|entry| entry.name.to_ascii_lowercase().contains(GRANDCHILD_NAME)),
        "control B must hold its grandchild, not a root-only listing: {before_b:?}"
    );
    assert!(
        !before_a.is_empty() && !before_b.is_empty(),
        "both trees are live before the trigger"
    );

    // The trigger: a real M0 connection that ends. The daemon must serve the existing broker
    // over it (HELLO then PING are answered), and the EOF behind them is the shutdown path.
    let mut link = MemoryLink::new();
    link.push_inbound(&frame::encode(msg::HELLO, flag::ENC, 1, &hello_payload(a)).expect("hello"));
    link.push_inbound(&frame::encode(msg::PING, 0, 2, &[]).expect("ping"));
    let outcome = daemon.serve(&mut link);

    // (b) the serve loop's own result.
    assert!(
        matches!(outcome.shutdown, ShutdownReason::PeerEof),
        "peer EOF must be the shutdown reason: {outcome:?}"
    );
    assert!(outcome.is_clean(), "EOF is a clean end: {outcome:?}");
    assert!(outcome.reaped_all(), "{outcome:?}");
    assert_eq!(outcome.exit_code(), 0, "a clean reaping shutdown exits 0");
    assert_eq!(outcome.frames_in, 2, "HELLO and PING were served");
    assert_eq!(outcome.frames_out, 2, "HELLO_ACK and PONG were written");
    assert_eq!(
        outcome.closes.len(),
        2,
        "close_all must cover every hosted session, not just the bound one: {outcome:?}"
    );
    assert!(
        daemon.host().is_empty(),
        "the shutdown path must leave no hosted session behind"
    );

    // The replies really went out on the link, CRC-verified on the way back in.
    let replies = decode_msg_types(&link.take_outbound());
    assert_eq!(replies, vec![msg::HELLO_ACK, msg::PONG]);

    // (a) every child is gone within the AR-30 item 2 bound; (c) the control B, which no other
    // part of this test ever closed, is gone by the same path.
    assert_reaped(&watched_a, &backend, a, &outcome);
    assert_reaped(&watched_b, &backend, b, &outcome);
}

/// `GO_AWAY` is the other clean shutdown path (kernel/07 section 3.1). It must stop the loop
/// where it is - a frame queued behind it is never served - and still reap.
#[test]
fn go_away_stops_serving_immediately_and_still_reaps() {
    let (mut daemon, backend) = daemon("shutdown-goaway");
    let id = SessionId(5);
    let watched = open_watched(&mut daemon, id);
    wait_until_live(&backend, &watched, "the session");

    let mut link = MemoryLink::new();
    link.push_inbound(&frame::encode(msg::HELLO, flag::ENC, 1, &hello_payload(id)).expect("hello"));
    link.push_inbound(&frame::encode(msg::GO_AWAY, 0, 2, &[]).expect("go away"));
    // Queued behind GO_AWAY on purpose: serving stops at the draining state, so this frame must
    // never be counted (a PING would have answered PONG).
    link.push_inbound(&frame::encode(msg::PING, 0, 3, &[]).expect("ping"));

    let outcome = daemon.serve(&mut link);
    assert!(
        matches!(outcome.shutdown, ShutdownReason::GoAway),
        "GO_AWAY must be the shutdown reason: {outcome:?}"
    );
    assert!(outcome.is_clean(), "{outcome:?}");
    assert_eq!(
        outcome.frames_in, 2,
        "the loop must stop at the draining state, not keep serving: {outcome:?}"
    );
    assert_eq!(
        decode_msg_types(&link.take_outbound()),
        vec![msg::HELLO_ACK],
        "nothing may be written back after GO_AWAY"
    );
    assert_eq!(outcome.exit_code(), 0, "{outcome:?}");
    assert_reaped(&watched, &backend, id, &outcome);
}

/// A refused handshake is not a clean end, but it is still a shutdown path: the NACK reaches
/// the peer first, the process reports the refusal in its exit code, and the tree is reaped.
#[test]
fn a_refused_handshake_is_reported_and_still_reaps() {
    let (mut daemon, backend) = daemon("shutdown-refused");
    let id = SessionId(6);
    let watched = open_watched(&mut daemon, id);
    wait_until_live(&backend, &watched, "the session");

    // A version range this daemon does not speak: no overlap, so the broker answers HELLO_NACK
    // and the connection is over (kernel/07 section 3.3).
    let hello = Hello {
        proto_min: termai_core::PROTO_VERSION + 1,
        proto_max: termai_core::PROTO_VERSION + 2,
        client_kind: ClientKind::Cli,
        caps: vec![CAP_SESSION_READ],
        required: vec![CAP_SESSION_READ],
        session_claim: id,
        feature_bits: 0,
        nonce: [4u8; 16],
    };
    let payload = codec::to_bytes(&codec::hello_to_value(&hello));

    let mut link = MemoryLink::new();
    link.push_inbound(&frame::encode(msg::HELLO, flag::ENC, 1, &payload).expect("hello"));
    let outcome = daemon.serve(&mut link);

    assert!(
        matches!(outcome.shutdown, ShutdownReason::Refused),
        "a version mismatch must be refused, not served: {outcome:?}"
    );
    assert!(!outcome.is_clean(), "a refusal is not a clean end");
    assert_eq!(
        outcome.exit_code(),
        sessiond::daemon::EXIT_REFUSED,
        "the refusal has its own exit code"
    );
    assert_eq!(
        decode_msg_types(&link.take_outbound()),
        vec![msg::HELLO_NACK],
        "the refusal must reach the peer before the daemon stops"
    );
    assert_reaped(&watched, &backend, id, &outcome);
}

/// The connection can also fail mid-frame. The shutdown path must still run - that is what
/// makes it a shutdown path rather than a happy-path cleanup - and the exit code must say so.
#[test]
fn a_frame_the_frame_layer_rejects_still_reaps_and_exits_nonzero() {
    let (mut daemon, backend) = daemon("shutdown-badframe");
    let id = SessionId(7);
    let watched = open_watched(&mut daemon, id);
    wait_until_live(&backend, &watched, "the session");

    let mut wire = frame::encode(msg::PING, 0, 1, &[]).expect("ping");
    let last = wire.len() - 1;
    wire[last] ^= 0xFF; // CRC mismatch: the frame layer refuses this frame.
    let mut link = MemoryLink::new();
    link.push_inbound(&wire);

    let outcome = daemon.serve(&mut link);
    assert!(
        matches!(
            outcome.shutdown,
            ShutdownReason::Frame(termai_ipc::IpcError::CrcMismatch)
        ),
        "a corrupted frame must stop the loop as a frame error: {outcome:?}"
    );
    assert!(!outcome.is_clean(), "a damaged frame is not a clean end");
    assert_eq!(outcome.exit_code(), sessiond::daemon::EXIT_TRANSPORT);
    assert_eq!(outcome.frames_in, 0, "a refused frame is not served");
    assert_reaped(&watched, &backend, id, &outcome);
}

/// The pump path: real bytes from the pty reach the session's engine and its Log. That is the
/// half of "sessiond hosts the pty" which is not about reaping, and on Windows it is only
/// possible because the daemon writes the engine's `ESC[6n` answer back - without it the
/// command would never run and the grid would stay empty.
#[test]
fn the_daemon_pumps_real_pty_output_into_the_session_engine_and_log() {
    let (mut daemon, _backend) = daemon("pump");
    let id = SessionId(8);
    daemon
        .open_session(id, &marker_command(), size())
        .expect("open the marker session");

    let deadline = Instant::now() + SETUP_LIMIT;
    let mut seen = String::new();
    while Instant::now() < deadline {
        let snapshot = daemon
            .broker()
            .registry()
            .snapshot(id)
            .expect("the session's engine answers a snapshot");
        seen.clear();
        for row in 0..snapshot.rows {
            seen.push_str(&snapshot.row_text(row));
            seen.push('\n');
        }
        if seen.contains(MARKER) {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        seen.contains(MARKER),
        "the pumped pty output must reach the session's engine; the grid held {seen:?}"
    );

    let log_path = daemon
        .broker()
        .registry()
        .log_path(id)
        .expect("the session has a Log segment");
    let outcome = daemon.serve(&mut MemoryLink::new());
    assert!(outcome.reaped_all(), "{outcome:?}");
    let read = termai_session::log::read_segment(&log_path).expect("read the session's Log");
    assert!(
        read.records
            .iter()
            .any(|record| matches!(&record.record, termai_session::log::Record::PtyOut { .. })),
        "the pumped bytes must also be recorded, or the screen could not be rebuilt: {} record(s)",
        read.records.len()
    );
    assert_eq!(
        daemon.broker().registry().state(id),
        Some(termai_session::state::SessionState::Dead),
        "the shutdown path must record exited -> dead (reason reaped)"
    );
}
