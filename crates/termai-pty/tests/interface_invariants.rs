mod common;

use std::time::Duration;

// The interface invariants (kernel/02 section 3.1 / PTY-AC-07) are exercised on the
// pipe fallback backend. The native ConPTY path is blocked on this host
// (0xC0000142, zero readable bytes); see the handover block in
// src/windows/conpty.rs. These invariants are backend-agnostic by construction.

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
