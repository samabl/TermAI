mod common;

use std::time::Duration;

// The interface invariants (kernel/02 section 3.1 / PTY-AC-07) are exercised on the
// pipe fallback backend because it reaches EOF without help. These invariants are
// backend-agnostic by construction; the native ConPTY path is covered by
// f0_fidelity.rs and close_semantics.rs, and the unix forkpty spawn invariant
// (PTY-READY-1) by `spawn_returns_only_when_the_child_owns_its_process_group` below.

#[test]
fn read_after_eof_keeps_returning_zero() {
    let backend = common::pipes();
    let handle = backend
        .spawn(&common::echo_command(), common::size(), common::opts())
        .expect("spawn echo command");
    let _ = common::read_to_eof(&backend, &handle, Duration::from_secs(10));
    let mut buffer = [0u8; 32];
    assert_eq!(
        backend.read(&handle, &mut buffer).expect("read after EOF"),
        0
    );
    assert_eq!(
        backend.read(&handle, &mut buffer).expect("read after EOF"),
        0
    );
    let tree = backend.process_tree(&handle).expect("process tree");
    let info = backend
        .wait(&tree, termai_pty::WaitTimeout::Infinite)
        .expect("wait");
    assert!(info.reaped);
    backend.close(handle).expect("close");
}

#[test]
fn wait_always_returns_reaped() {
    let backend = common::pipes();
    let handle = backend
        .spawn(&common::echo_command(), common::size(), common::opts())
        .expect("spawn echo command");
    let _ = common::read_to_eof(&backend, &handle, Duration::from_secs(10));
    let tree = backend.process_tree(&handle).expect("process tree");
    let info = backend
        .wait(&tree, termai_pty::WaitTimeout::Infinite)
        .expect("wait");
    assert!(info.reaped, "wait must always return reaped == true");
    assert_eq!(info.live_children, 0);
    backend.close(handle).expect("close");
}

#[test]
fn kill_force_then_tree_snapshot_is_empty() {
    let backend = common::pipes();
    let handle = backend
        .spawn(&common::shell_command(), common::size(), common::opts())
        .expect("spawn shell");
    let tree = backend.process_tree(&handle).expect("process tree");
    let info = backend
        .kill(&tree, termai_pty::KillMode::Force)
        .expect("kill force");
    assert!(info.reaped, "kill(Force) must reap");
    let entries = common::wait_for_empty(&backend, &tree, Duration::from_secs(2));
    assert!(
        entries.is_empty(),
        "tree must be empty after Force: {entries:?}"
    );
    assert_eq!(tree.live_children(), 0);
    backend.close(handle).expect("close");
}

#[test]
fn capabilities_are_constant_across_calls() {
    let backend = common::native();
    let first = backend.capabilities();
    for _ in 0..16 {
        assert_eq!(backend.capabilities(), first);
    }
    let probed = common::probed();
    assert_eq!(probed.capabilities(), probed.capabilities());
    let pipes = common::pipes();
    assert_eq!(pipes.capabilities(), pipes.capabilities());
}

#[test]
fn close_is_idempotent() {
    let backend = common::pipes();
    let handle = backend
        .spawn(&common::shell_command(), common::size(), common::opts())
        .expect("spawn shell");
    let second = handle.clone();
    backend.close(handle).expect("first close");
    backend.close(second).expect("second close");
}

/// PTY-READY-1, unix forkpty backend: `spawn` must not return until the child has established
/// its own session and process group, because every tree operation addresses that group -
/// `kill(-pgid, sig)` and the `kill(-pgid, 0)` behind `live_children()`. `forkpty` returns in
/// the parent as soon as the fork is done, so without the readiness handshake in
/// `crates/termai-pty/src/unix/pty.rs` (`await_child_ready`) this is a race the parent can lose,
/// and losing it leaked `/bin/sleep 30` in the CI failure at commit 6911c9e
/// (`host::tests::the_probed_backend_can_open_and_close_a_session` on macOS arm64 and Linux x64).
///
/// The probe is the platform's own and is the first thing done after `spawn` returned: no
/// polling and no sleep, exactly like the failing unit test.
#[cfg(unix)]
#[test]
fn spawn_returns_only_when_the_child_owns_its_process_group() {
    let backend = common::native();
    let handle = backend
        .spawn(&common::shell_command(), common::size(), common::opts())
        .expect("spawn a long-lived child on the native unix backend");
    let tree = backend.process_tree(&handle).expect("process tree");
    let entries = backend
        .tree_snapshot(&tree)
        .expect("snapshot of a live tree");
    let pid = entries.first().expect("the live group leader").pid;
    // SAFETY: signal 0 only probes existence, and -pid addresses the child's process group.
    let probe = unsafe { libc::kill(-(pid as libc::pid_t), 0) };
    assert_eq!(
        probe,
        0,
        "spawn returned before the child owned its process group: kill(-{pid}, 0) failed with {:?}",
        std::io::Error::last_os_error()
    );
    // Clean up through the public contract, so a failure here cannot leave a stray sleep 300.
    let info = backend
        .kill(&tree, termai_pty::KillMode::Force)
        .expect("kill force");
    assert!(info.reaped, "kill(Force) must reap: {info:?}");
    assert_eq!(tree.live_children(), 0);
    backend.close(handle).expect("close");
}
