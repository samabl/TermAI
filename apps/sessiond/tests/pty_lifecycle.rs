//! sessiond-side PTY lifecycle with real processes: AR-30 item 2 / PTY-ORPHAN-1
//! (orphan cleanup = 100%, and a session close reaps the whole tree within 2 s), plus the
//! two controls that keep that claim honest - an unclosed session is still running at the
//! same moment, and the close path is idempotent.
//!
//! The measurement always polls the process tree; it never sleeps a fixed 2 s and then
//! asserts. Windows/ConPTY is the production path (DC-16: Job Object process trees), so the
//! grandchild evidence is asserted there; the unix tree ops report the process group leader
//! only, which is why the expected process count differs per platform.

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use sessiond::host::{PtySession, SessionHost};
use termai_core::SessionId;
use termai_pty::{Command, ProcEntry, ProcessTree, PtyBackend, SpawnOpts, WinSize};

/// The acceptance bound from AR-30 item 2 and the HARNESS section 8.2 reliability row.
const ORPHAN_BUDGET: Duration = Duration::from_secs(2);

/// How long a freshly spawned session gets to show its descendant. This is setup, not the
/// measurement, so it is deliberately generous.
const SETUP_LIMIT: Duration = Duration::from_secs(10);

/// Processes a live two-process tree is expected to list. Windows/ConPTY enumerates the
/// whole Job (cmd.exe + the ping it waits on); the unix backend lists the group leader only.
#[cfg(windows)]
const TREE_PROCESSES: usize = 2;
#[cfg(unix)]
const TREE_PROCESSES: usize = 1;

/// The grandchild's name in a Windows job listing.
#[cfg(windows)]
const GRANDCHILD_NAME: &str = "ping";

fn native() -> Arc<dyn PtyBackend> {
    Arc::from(termai_pty::native_backend())
}

fn size() -> WinSize {
    WinSize::new(80, 24)
}

fn opts() -> SpawnOpts {
    SpawnOpts::default()
}

/// A shell that waits on a long-lived descendant: the session tree really holds a child and
/// a grandchild, which is the case a root-only reaper would miss (kernel/02 section 3.3).
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

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

fn write_all(session: &PtySession, data: &[u8]) {
    let mut written = 0;
    while written < data.len() {
        match session.write(&data[written..]) {
            Ok(0) => break,
            Ok(count) => written += count,
            Err(_) => break,
        }
    }
}

/// Drain the pty and answer the cursor-position report ConPTY programs block on (CSI 6 n).
/// Without the answer the shell never gets to run the command and the job only ever holds
/// the root, which would make the descendant assertions vacuous.
fn start_dsr_reader(session: Arc<PtySession>) {
    thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        let mut pending: Vec<u8> = Vec::new();
        loop {
            match session.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    pending.extend_from_slice(&buffer[..read]);
                    while let Some(pos) = find_subslice(&pending, b"\x1b[6n") {
                        pending.drain(..pos + 4);
                        write_all(&session, b"\x1b[1;1R");
                    }
                    // Keep a tail wide enough to catch a sequence split across reads.
                    if pending.len() > 8 {
                        let keep = pending.len() - 8;
                        pending.drain(..keep);
                    }
                }
                Err(_) => break,
            }
        }
    });
}

/// Read the tree, failing the test on an error: an unreadable tree must never be mistaken
/// for an empty one.
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

/// Open a session, keep a clone of its tree for post-close polling and start its pump.
fn open_watched(host: &mut SessionHost, id: SessionId) -> ProcessTree {
    host.open(id, &shell_waiting_on_a_grandchild(), size(), opts())
        .expect("open session");
    let session = host.get(id).expect("session");
    let tree = session.tree().clone();
    start_dsr_reader(session);
    tree
}

/// Primary: after closing the session, the direct child and the grandchild it spawned are
/// both gone within 2 s - polled, never a fixed sleep.
#[cfg(windows)]
#[test]
fn closing_a_session_reaps_its_child_and_grandchild_within_two_seconds() {
    let backend = native();
    let mut host = SessionHost::new(Arc::clone(&backend));
    let (a, b) = (SessionId(1), SessionId(2));

    let tree_a = open_watched(&mut host, a);
    let tree_b = open_watched(&mut host, b);
    let session_a = host.get(a).expect("session A");

    // The tree must really hold a descendant before the measurement means anything.
    let before = wait_for_count(&backend, &tree_a, TREE_PROCESSES, SETUP_LIMIT);
    assert!(
        before.len() >= TREE_PROCESSES,
        "expected cmd.exe + ping inside the job, saw {before:?}"
    );
    assert!(
        before
            .iter()
            .any(|entry| entry.name.to_ascii_lowercase().contains(GRANDCHILD_NAME)),
        "the grandchild must be inside the tree, not a root-only listing: {before:?}"
    );
    let pids: Vec<u32> = before.iter().map(|entry| entry.pid).collect();

    // Control 1, before: the session that stays open owns a live tree right now.
    let control_before = wait_for_count(&backend, &tree_b, TREE_PROCESSES, SETUP_LIMIT);
    assert!(
        control_before.len() >= TREE_PROCESSES,
        "the control session must be live before the close: {control_before:?}"
    );

    let started = Instant::now();
    let outcome = host.close(a).expect("close session A");
    assert!(
        !outcome.already_closed,
        "the first close must really close: {outcome:?}"
    );
    assert!(
        outcome.reaped,
        "kill(Force) plus the bounded wait must report the tree reaped: {outcome:?}"
    );
    assert_eq!(
        outcome.live_children, 0,
        "PTY-ORPHAN-1: the closed session must have no live process left"
    );
    assert!(
        outcome.wall <= ORPHAN_BUDGET,
        "the close path took {:?}, AR-30 item 2 allows 2 s",
        outcome.wall
    );

    // Control 1, at the same moment: closing A must not have touched B.
    let control_after = snapshot(&backend, &tree_b);
    assert!(
        control_after.len() >= TREE_PROCESSES,
        "the reaper must kill one tree, not every tree: {control_after:?}"
    );

    // Primary: poll with the deadline measured from the moment the close began.
    let left = wait_for_empty(
        &backend,
        &tree_a,
        ORPHAN_BUDGET.saturating_sub(started.elapsed()),
    );
    assert!(
        left.is_empty(),
        "PTY-ORPHAN-1 / AR-30 item 2: processes still live {left:?}"
    );
    // A failed snapshot must make this test fail, never look like an empty tree.
    let final_a = snapshot(&backend, &tree_a);
    assert!(
        final_a.is_empty(),
        "PTY-ORPHAN-1 / AR-30 item 2: the closed session still holds {final_a:?}"
    );
    for pid in &pids {
        assert!(
            !final_a.iter().any(|entry| entry.pid == *pid),
            "child or grandchild pid {pid} survived the session close: {final_a:?}"
        );
    }
    assert_eq!(session_a.live_children(), 0);

    host.close(b).expect("close session B");
}

/// Control 1: a second session that is NOT closed still has its child alive at the same
/// moment - this is what proves the reaper kills a tree on close rather than everything.
#[test]
fn closing_one_session_leaves_an_open_session_running() {
    let backend = native();
    let mut host = SessionHost::new(Arc::clone(&backend));
    let (a, b) = (SessionId(11), SessionId(12));

    let tree_a = open_watched(&mut host, a);
    let tree_b = open_watched(&mut host, b);

    let live_before = wait_for_count(&backend, &tree_b, TREE_PROCESSES, SETUP_LIMIT);
    assert!(
        live_before.len() >= TREE_PROCESSES,
        "the control session must be live before the close: {live_before:?}"
    );

    let outcome = host.close(a).expect("close session A");
    assert!(!outcome.already_closed);
    assert_eq!(outcome.live_children, 0);

    let live_after = snapshot(&backend, &tree_b);
    assert!(
        live_after.len() >= TREE_PROCESSES,
        "closing A left the open session B without its child: {live_after:?}"
    );

    let gone = wait_for_empty(&backend, &tree_a, ORPHAN_BUDGET);
    assert!(
        gone.is_empty(),
        "the closed session must leave nothing behind: {gone:?}"
    );
    assert!(
        snapshot(&backend, &tree_a).is_empty(),
        "the closed session's tree must be readable and empty"
    );

    host.close(b).expect("close session B");
}

/// Control 2: the close path is idempotent - a second call neither errors nor panics.
#[test]
fn closing_a_session_twice_does_not_error_or_panic() {
    let backend = native();
    let mut host = SessionHost::new(Arc::clone(&backend));
    let id = SessionId(21);

    let tree = open_watched(&mut host, id);
    let before = wait_for_count(&backend, &tree, TREE_PROCESSES, SETUP_LIMIT);
    assert!(
        before.len() >= TREE_PROCESSES,
        "the session must own a real tree before it is closed: {before:?}"
    );

    let first = host.close(id).expect("the first close must succeed");
    assert!(!first.already_closed, "{first:?}");
    assert!(first.reaped, "{first:?}");

    let second = host.close(id).expect("a second close must not error");
    assert!(
        second.already_closed,
        "close must be idempotent: {second:?}"
    );
    assert_eq!(second.live_children, 0);

    // Same contract for an id that was never opened.
    let never = host
        .close(SessionId(22))
        .expect("closing an unknown session must not error");
    assert!(never.already_closed, "{never:?}");
    assert_eq!(never.live_children, 0);
    assert!(host.is_empty());
}
