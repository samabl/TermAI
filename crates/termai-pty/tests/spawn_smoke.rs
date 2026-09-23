mod common;

// Real spawn smoke test. It runs on the pipe fallback, which is a genuine process spawn
// with a byte-exact channel and an immediate EOF; the native ConPTY path has its own
// tests (f0_fidelity.rs, close_semantics.rs). It must fail loudly if spawn fails, never
// skip silently.

use std::time::Duration;

#[test]
fn real_spawn_echoes_hello_and_is_reaped() {
    let backend = common::pipes();
    let handle = match backend.spawn(&common::echo_command(), common::size(), common::opts()) {
        Ok(handle) => handle,
        Err(err) => panic!("spawn failed (must fail loudly, never skip): {err}"),
    };
    let output = common::read_to_eof(&backend, &handle, Duration::from_secs(10));
    let text = String::from_utf8_lossy(&output);
    assert!(
        text.contains("hello"),
        "expected hello in PTY output, got {text:?}"
    );
    let tree = backend.process_tree(&handle).expect("process tree");
    let info = backend
        .wait(&tree, termai_pty::WaitTimeout::Infinite)
        .expect("wait");
    assert!(info.reaped, "spawned process must be reaped");
    backend.close(handle).expect("close");
}
