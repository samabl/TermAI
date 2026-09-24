//! sessiond-side PTY lifecycle with real processes: AR-30 item 2 / PTY-ORPHAN-1
//! (orphan cleanup = 100%, and a session close reaps the whole tree within 2 s), plus the
//! two controls that keep that claim honest - an unclosed session is still running at the
//! same moment, and the close path is idempotent.
//!
//! The measurement always polls the process tree; it never sleeps a fixed 2 s and then
//! asserts. Windows/ConPTY is the production path (DC-16: Job Object process trees), so the
//! **named descendant** evidence is asserted there. Unix reports the process-group leader only,
//! so the unix half proves the same acceptance item with the evidence that platform actually
//! has ([`expected_tree_processes`] cites the exact source of the difference) - and where an
//! assertion is impossible on unix the test prints one explicit line instead of skipping
//! silently (`unix_report_limitation`).

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

/// Processes a live two-process tree is expected to list on Windows/ConPTY, which enumerates
/// the whole Job Object: cmd.exe plus the `ping` it waits on.
#[cfg(windows)]
const WINDOWS_TREE_PROCESSES: usize = 2;

/// Processes a live two-process tree is expected to list on unix, where the tree ops report
/// the process group leader only.
#[cfg(unix)]
const UNIX_TREE_PROCESSES: usize = 1;

/// The platform's honest expectation for a live session tree, per platform, instead of one
/// global count that would encode the Windows Job Object view everywhere.
///
/// **Windows/ConPTY** enumerates the whole Job Object, so the shell and the descendant it is
/// waiting on are both listed: `WINDOWS_TREE_PROCESSES`.
///
/// **unix** lists the leader only. `TreeOps::snapshot` for `UnixTree` at
/// `crates/termai-pty/src/unix/pty.rs:417-436` does `waitpid(WNOHANG)` on the single pid
/// `forkpty` returned and then returns exactly **one** `ProcEntry` for that leader, or an empty
/// vector once it is gone:
///
/// ```text
/// 418    fn snapshot(&self) -> Result<Vec<ProcEntry>, PtyError> {
/// 419        if lock(&self.reaped).is_some() {
/// 420            return Ok(Vec::new());
/// 421        }
/// 422        match waitpid_nohang(self.pid)? {
/// 423            WaitState::Running => Ok(vec![ProcEntry {
/// 424                pid: self.pid as u32,
/// 425                ppid: 0,
/// 426                name: self.program.clone(),
/// ...
/// 430            WaitState::Exited(status) => { ... Ok(Vec::new()) }
/// 434            WaitState::Gone => Ok(Vec::new()),
/// ```
///
/// WHY the leader only: `forkpty` puts the child in its own session *and* process group, so
/// `kill(-pgid, sig)` reaches the whole group (`kill_group`, same file lines 102-106) and the
/// `live_children()` probe can ask whether the **group** is still occupied (`kill(-pid, 0)` in
/// `group_alive`, lines 108-111, used by `live_children` at lines 438-447). What the unix
/// backend does not own is any primitive that *enumerates* that group: there is no Job Object
/// equivalent on this side (DC-16 makes the Job Object the Windows path), and walking a group
/// needs `/proc` on Linux and `sysctl(KERN_PROC)` on macOS - two new platform code paths, not
/// something a test may assume. The snapshot face therefore reports what it can prove: the
/// leader.
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

/// The grandchild's name in a Windows job listing.
#[cfg(windows)]
const GRANDCHILD_NAME: &str = "ping";

/// The one explicit line every unix test prints where it cannot make a *named descendant*
/// assertion. A silent skip is forbidden: this says which assertion is missing, why, and which
/// evidence replaces it.
///
/// Unix cannot name the `sleep 300` behind `/bin/sh -c "sleep 300 & wait"` because
/// `TreeOps::snapshot` for `UnixTree` returns the process-group leader only
/// (`crates/termai-pty/src/unix/pty.rs:417-436`). The group itself is still killed and probed:
/// `kill(-pgid, ...)` at `crates/termai-pty/src/unix/pty.rs:102-111` and 449-471, and the
/// `kill(-pid, 0)` group probe behind `live_children()` at lines 438-447 - so
/// `live_children() == 0` after the close is evidence about the whole group, grandchild
/// included, not about the shell alone.
#[cfg(unix)]
fn unix_report_limitation(context: &str) {
    println!(
        "unix: {context}: no named-grandchild assertion is possible on this platform - \
         TreeOps::snapshot for UnixTree returns the process-group leader only \
         (crates/termai-pty/src/unix/pty.rs:417-436, waitpid(WNOHANG) on the single forkpty \
         pid), so a descendant can never be listed by name here. The whole group is still \
         signalled and probed via kill(-pgid, ...) \
         (crates/termai-pty/src/unix/pty.rs:102-111, 438-447)."
    );
}

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

/// Poll the platform's own liveness probe until it reports no live process, or the limit
/// passes. On unix that probe is `kill(-pgid, 0)`, i.e. the whole process group
/// (`crates/termai-pty/src/unix/pty.rs:438-447`), so a zero here is evidence about the group
/// and not only about the leader. AR-30 item 2 gives 2 s, so this is a bounded liveness
/// question and not a single instantaneous sample.
fn wait_for_zero_live_children(session: &PtySession, limit: Duration) -> u32 {
    let deadline = Instant::now() + limit;
    loop {
        let live = session.live_children();
        if live == 0 || Instant::now() >= deadline {
            return live;
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

/// Primary, Windows/ConPTY (DC-16: the production path): after closing the session, the direct
/// child and the grandchild it spawned are both gone within 2 s - polled, never a fixed sleep.
/// The grandchild assertions here are the strongest form of AR-30 item 2 and are deliberately
/// Windows-only: they are the reason the unix tests below cannot simply be deleted.
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
    let before = wait_for_count(&backend, &tree_a, expected_tree_processes(), SETUP_LIMIT);
    assert!(
        before.len() >= expected_tree_processes(),
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
    let control_before = wait_for_count(&backend, &tree_b, expected_tree_processes(), SETUP_LIMIT);
    assert!(
        control_before.len() >= expected_tree_processes(),
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
        control_after.len() >= expected_tree_processes(),
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

/// Primary, unix. Same AR-30 item 2 acceptance, with the evidence this platform layer can
/// actually produce (see [`expected_tree_processes`]): the leader is alive and its group is
/// occupied before the close, `close()` reports `reaped` with `live_children == 0`, the tree is
/// empty after the close, and the closed leader's pid is gone. Control 1 (the unclosed session
/// is live at that same moment) and Control 2 (idempotent close) are the two shared tests
/// below, which run on both platforms.
///
/// The `live_children == 0` assertion is not a weaker substitute for the Windows grandchild
/// assertion: on unix that number is the answer to `kill(-pgid, 0)`
/// (`crates/termai-pty/src/unix/pty.rs:438-447`), i.e. "is any process still in this process
/// group?" - and `sleep 300` inherits the group of the `sh` that `forkpty` made the leader. A
/// zero therefore covers the grandchild, but it cannot *name* it, which is why
/// [`unix_report_limitation`] is printed here rather than the test pretending to know more.
#[cfg(unix)]
#[test]
fn closing_a_session_reaps_its_process_group_within_two_seconds() {
    let backend = native();
    let mut host = SessionHost::new(Arc::clone(&backend));
    let (a, b) = (SessionId(1), SessionId(2));

    let tree_a = open_watched(&mut host, a);
    let tree_b = open_watched(&mut host, b);
    let session_a = host.get(a).expect("session A");

    // The leader must be there before the measurement means anything. Unix lists the leader
    // only, so the honest expectation is exactly one entry, and it must be the forked program.
    let before = wait_for_count(&backend, &tree_a, expected_tree_processes(), SETUP_LIMIT);
    assert!(
        !before.is_empty(),
        "the unix backend must list the live group leader before the close: {before:?}"
    );
    assert_eq!(
        before.len(),
        UNIX_TREE_PROCESSES,
        "unix lists the process-group leader only, never a descendant: {before:?}"
    );
    let leader = before.first().expect("the leader entry");
    // Assert against the program this test itself spawned, never a hardcoded path: the name
    // the tree reports is `Command::program` verbatim (UnixTree.program,
    // crates/termai-pty/src/unix/pty.rs:258-260), so the expected value is the one the test
    // chose above and not a platform literal a rename or a different shell would break.
    assert_eq!(
        leader.name,
        shell_waiting_on_a_grandchild().program,
        "the single unix entry must be the program the test spawned: {before:?}"
    );
    let pids: Vec<u32> = before.iter().map(|entry| entry.pid).collect();

    // The group is occupied: on unix `live_children()` is `kill(-pgid, 0)`, so >= 1 here means
    // the group still holds the `sh` *and* the `sleep 300` it is waiting on.
    assert!(
        session_a.live_children() >= 1,
        "the unix process group must be occupied before the close"
    );

    // Control 1, before: the session that stays open owns a live group right now.
    let control_before = wait_for_count(&backend, &tree_b, expected_tree_processes(), SETUP_LIMIT);
    assert!(
        !control_before.is_empty(),
        "the control session must be live before the close: {control_before:?}"
    );

    // The assertion this platform cannot make, stated out loud (never a silent skip).
    unix_report_limitation("closing_a_session_reaps_its_process_group_within_two_seconds");

    let started = Instant::now();
    let outcome = host.close(a).expect("close session A");
    assert!(
        !outcome.already_closed,
        "the first close must really close: {outcome:?}"
    );
    assert!(
        outcome.reaped,
        "kill(Force) plus the bounded wait must report the group reaped: {outcome:?}"
    );
    assert_eq!(
        outcome.live_children, 0,
        "PTY-ORPHAN-1: on unix this is the whole process group (kill(-pgid, 0), \
         crates/termai-pty/src/unix/pty.rs:438-447), so 0 covers the grandchild too"
    );
    assert!(
        outcome.wall <= ORPHAN_BUDGET,
        "the close path took {:?}, AR-30 item 2 allows 2 s",
        outcome.wall
    );

    // Control 1, at the same moment: closing A must not have touched B's group.
    let control_after = snapshot(&backend, &tree_b);
    assert!(
        !control_after.is_empty(),
        "the reaper must kill one process group, not every group: {control_after:?}"
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
            "the group leader pid {pid} survived the session close: {final_a:?}"
        );
    }
    // The platform's own group probe, polled inside the same 2 s budget.
    assert_eq!(
        wait_for_zero_live_children(&session_a, ORPHAN_BUDGET.saturating_sub(started.elapsed())),
        0,
        "PTY-ORPHAN-1: the closed process group still has a live process"
    );
    assert_eq!(session_a.live_children(), 0);
    assert_eq!(
        session_a.root_pid(),
        None,
        "a closed session must not report a root process any more"
    );

    host.close(b).expect("close session B");
}

/// A long-lived command with **no** descendants - the exact shape the reported CI failure used
/// (`/bin/sleep 30` in `host::tests::idle_command`, `cmd.exe` on Windows).
fn long_lived_single_process() -> Command {
    #[cfg(windows)]
    {
        Command::new("cmd.exe")
    }
    #[cfg(unix)]
    {
        Command::with_args("/bin/sleep", vec!["30".to_string()])
    }
}

/// The reported CI failure, as an acceptance test:
/// `test host::tests::the_probed_backend_can_open_and_close_a_session FAILED` on macOS arm64 and
/// Linux x64 at commit 6911c9e. That unit test opens `/bin/sleep 30` on `SessionHost::probed()`
/// (forkpty on unix) and closes it in the very next statement, and the close reported
/// `NotReaped`.
///
/// Every other test in this file polls the tree before closing (`wait_for_count` with a 10 s
/// `SETUP_LIMIT`), which hands the child all the time in the world to finish its session setup
/// and therefore hides the window this test exists for: on unix `spawn` used to return as soon
/// as `forkpty` had forked, i.e. possibly *before* the child had run `setsid` inside
/// `login_tty`. In that window the child is still in the parent's process group, so
/// `kill(-pid, SIGKILL)` fails with ESRCH, the child survives and the close reports
/// `TreeNotEmpty { live: 1 }` -> `HostError::NotReaped`. `spawn` now waits for the child's
/// readiness report before it returns (`crates/termai-pty/src/unix/pty.rs`, `await_child_ready`,
/// PTY-READY-1), and the kill path falls back to the pid (PTY-KILL-1), so the window is closed
/// at the source.
///
/// This test must never grow a sleep, a `wait_for_count` or a retry: any of those would make it
/// pass without the fix and it would stop being evidence.
#[test]
fn closing_a_session_immediately_after_open_reaps_its_tree() {
    let backend = native();
    let mut host = SessionHost::new(Arc::clone(&backend));

    // The exact command from the failing unit test (one process, no descendants) and the
    // shell-plus-grandchild command the rest of this file uses, so the acceptance covers both
    // the reported case and the tree case. Three independent rounds, because the defect being
    // guarded against is a race: one cycle could pass on the pre-fix code by luck. The
    // assertion inside a cycle stays exactly "open, then close" - no poll, no sleep, no retry.
    let commands = [long_lived_single_process(), shell_waiting_on_a_grandchild()];
    for round in 0..3u128 {
        for (index, command) in commands.iter().enumerate() {
            let id = SessionId(40 + round * 10 + index as u128);
            host.open(id, command, size(), opts())
                .unwrap_or_else(|err| panic!("open of {} failed: {err}", command.program));
            let session = host.get(id).expect("session");
            let tree = session.tree().clone();

            // The measurement: close in the very next statement, no poll and no sleep between.
            let started = Instant::now();
            let outcome = host.close(id).unwrap_or_else(|err| {
                panic!(
                    "PTY-ORPHAN-1: {} opened and closed back to back was not reaped: {err}",
                    command.program
                )
            });
            assert!(
                !outcome.already_closed,
                "the first close must really close: {outcome:?}"
            );
            assert!(
                outcome.reaped,
                "the immediate close must report the reap: {outcome:?}"
            );
            assert_eq!(
                outcome.live_children, 0,
                "PTY-ORPHAN-1: an immediate close of {} left a live process behind (on unix this \
                 count is kill(-pgid, 0), i.e. the whole process group)",
                command.program
            );
            assert!(
                outcome.wall <= ORPHAN_BUDGET,
                "the immediate close took {:?}, AR-30 item 2 allows 2 s",
                outcome.wall
            );

            let left = wait_for_empty(
                &backend,
                &tree,
                ORPHAN_BUDGET.saturating_sub(started.elapsed()),
            );
            assert!(
                left.is_empty(),
                "PTY-ORPHAN-1 / AR-30 item 2: {} still holds {left:?}",
                command.program
            );
            assert!(
                snapshot(&backend, &tree).is_empty(),
                "the closed session must leave a readable and empty tree"
            );
            assert_eq!(
                session.live_children(),
                0,
                "PTY-ORPHAN-1: {} still reports a live child after the immediate close",
                command.program
            );
            assert!(session.is_closed(), "the pty must be released");
        }
    }
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
    let session_a = host.get(a).expect("session A");

    let live_before = wait_for_count(&backend, &tree_b, expected_tree_processes(), SETUP_LIMIT);
    assert!(
        live_before.len() >= expected_tree_processes(),
        "the control session must be live before the close: {live_before:?}"
    );

    let started = Instant::now();
    let outcome = host.close(a).expect("close session A");
    assert!(!outcome.already_closed);
    assert_eq!(outcome.live_children, 0);

    let live_after = snapshot(&backend, &tree_b);
    assert!(
        live_after.len() >= expected_tree_processes(),
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
    // The platform's own liveness probe, not only the listing: on unix this is the whole
    // process group, so it must not need more than the remaining AR-30 budget to read zero.
    assert_eq!(
        wait_for_zero_live_children(&session_a, ORPHAN_BUDGET.saturating_sub(started.elapsed())),
        0,
        "PTY-ORPHAN-1: the closed session still reports a live child"
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
    let before = wait_for_count(&backend, &tree, expected_tree_processes(), SETUP_LIMIT);
    assert!(
        before.len() >= expected_tree_processes(),
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
