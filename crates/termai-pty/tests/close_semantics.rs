//! The close contract that sessiond depends on for long-lived sessions (DC-18,
//! kernel/04 "start a reader task"): once close() returns, a read that was already parked
//! in read() must return promptly instead of parking forever. Before WS-H this held on the
//! pipe fallback only because the child's stdout reached EOF, and it did not hold at all on
//! ConPTY, which is why the CLI carried a "close only when no read is pending" workaround.

mod common;

#[cfg(windows)]
#[test]
fn conpty_close_releases_a_parked_read() {
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::time::Duration;

    let backend = common::native();
    let handle = backend
        .spawn(
            &termai_pty::Command::new("cmd.exe"),
            common::size(),
            common::opts(),
        )
        .expect("spawn cmd.exe on ConPTY");

    let (sender, receiver) = mpsc::channel();
    let reader_backend: Arc<dyn termai_pty::PtyBackend> = Arc::clone(&backend);
    let reader_handle = handle.clone();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        // Drain the init output, then park: ConPTY never reports EOF while the console
        // lives, so the loop can only end through close().
        loop {
            match reader_backend.read(&reader_handle, &mut buffer) {
                Ok(0) => break,
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        let _ = sender.send(());
    });

    // Give the reader time to consume the init sequence and park in ReadFile.
    std::thread::sleep(Duration::from_millis(500));
    let tree = backend.process_tree(&handle).expect("process tree");
    let _ = backend.kill(&tree, termai_pty::KillMode::Force);

    backend
        .close(handle.clone())
        .expect("close releases the parked read");

    let released = receiver.recv_timeout(Duration::from_secs(5)).is_ok();
    backend.close(handle).expect("idempotent close");
    assert!(
        released,
        "close() must release a read parked in read(); the reader is still parked"
    );
}
