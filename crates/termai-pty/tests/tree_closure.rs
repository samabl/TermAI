mod common;

use std::time::Duration;

use termai_pty::KillMode;

// PTY-ORPHAN-1 process-tree closure, exercised on the pipe fallback (a byte-exact channel
// with an immediate EOF); ConPTY's Job Object path is covered with the native tests.

#[test]
fn force_kill_leaves_no_live_children_within_two_seconds() {
    let backend = common::pipes();
    let handle = backend
        .spawn(&common::shell_command(), common::size(), common::opts())
        .expect("spawn shell");
    let tree = backend.process_tree(&handle).expect("process tree");
    let before = backend.tree_snapshot(&tree).expect("snapshot");
    assert!(
        !before.is_empty(),
        "a fresh tree should contain the root process"
    );
    let info = backend.kill(&tree, KillMode::Force).expect("kill force");
    assert!(info.reaped, "kill(Force) must reap");
    let entries = common::wait_for_empty(&backend, &tree, Duration::from_secs(2));
    assert!(
        entries.is_empty(),
        "PTY-ORPHAN-1: live processes after Force: {entries:?}"
    );
    assert_eq!(tree.live_children(), 0);
    backend.close(handle).expect("close");
}

// The production path (DC-16: ConPTY + Job Object). The pipe-fallback test above is not a
// substitute: kernel/02 defines PipeFallback as a degraded channel, and AR-30 item 2 makes
// orphan cleanup a section 8.2 acceptance item on the real path, where it must be 100% and
// complete within 2 s of the session closing.
#[cfg(windows)]
#[test]
fn native_force_kill_reaps_the_whole_tree_within_two_seconds() {
    let backend = common::native();
    let handle = backend
        .spawn(&common::shell_command(), common::size(), common::opts())
        .expect("spawn cmd.exe on ConPTY");
    let tree = backend.process_tree(&handle).expect("process tree");
    let before = backend.tree_snapshot(&tree).expect("snapshot");
    assert!(
        !before.is_empty(),
        "a fresh tree should contain the root process"
    );
    let info = backend.kill(&tree, KillMode::Force).expect("kill force");
    assert!(info.reaped, "kill(Force) must reap");
    let entries = common::wait_for_empty(&backend, &tree, Duration::from_secs(2));
    assert!(
        entries.is_empty(),
        "PTY-ORPHAN-1 (native ConPTY): live processes after Force: {entries:?}"
    );
    assert_eq!(tree.live_children(), 0);
    backend.close(handle).expect("close");
}

/// AR-30 item 2 says orphan cleanup is 100%, not "the root exited". A shell that is merely
/// waiting on a child keeps the child in the job; killing the tree must take the descendant
/// with it. This is the case that a root-only check would miss.
#[cfg(windows)]
#[test]
fn native_force_kill_reaps_descendants_not_only_the_root() {
    let backend = common::native();
    let command = termai_pty::Command::with_args(
        "cmd.exe",
        vec!["/C".to_string(), "ping -n 60 127.0.0.1".to_string()],
    );
    let handle = backend
        .spawn(&command, common::size(), common::opts())
        .expect("spawn cmd.exe /C ping on ConPTY");
    // Without an answer to the console's cursor-position request the shell never runs the
    // command and the job only ever holds the root - which is exactly how this test failed
    // before the harness learned to answer CSI 6 n.
    let _reader = common::start_reader_answering_dsr(&backend, &handle);
    let tree = backend.process_tree(&handle).expect("process tree");
    // cmd.exe plus the ping it is waiting on: the job must hold both, otherwise the
    // descendant would already be an orphan outside our control.
    let seen = common::wait_for_count(&backend, &tree, 2, Duration::from_secs(5));
    assert!(
        seen.len() >= 2,
        "expected cmd.exe + ping inside the job, saw {seen:?}"
    );
    assert!(
        seen.iter()
            .any(|entry| entry.name.to_ascii_lowercase().contains("ping")),
        "the descendant must be inside the job (not a root-only listing): {seen:?}"
    );
    let info = backend.kill(&tree, KillMode::Force).expect("kill force");
    assert!(info.reaped, "kill(Force) must reap");
    let entries = common::wait_for_empty(&backend, &tree, Duration::from_secs(2));
    assert!(
        entries.is_empty(),
        "PTY-ORPHAN-1 (native ConPTY): descendants survived Force: {entries:?}"
    );
    backend.close(handle).expect("close");
}
