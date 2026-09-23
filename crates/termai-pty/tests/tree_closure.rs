mod common;

use std::time::Duration;

use termai_pty::KillMode;

// PTY-ORPHAN-1 process-tree closure, exercised on the pipe fallback (the native
// ConPTY path is blocked on this host; see src/windows/conpty.rs handover).

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
