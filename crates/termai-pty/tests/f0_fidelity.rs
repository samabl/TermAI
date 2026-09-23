mod common;

#[cfg(windows)]
use std::sync::Arc;
use std::time::Duration;

// The F0 differential classifier below is exercised only by the ConPTY test, which is
// Windows-only, so on unix it would be dead code and the imports unused. The group is
// cfg-gated rather than silenced with an allow(): the helpers genuinely do not exist on
// the unix path.
#[cfg(windows)]
use termai_pty::{bundled_rules, is_gate_failure, load_rules};

/// The AR-25.2 probe payload: all 256 byte values, in order.
fn probe_payload() -> Vec<u8> {
    (0..=255u8).collect()
}

#[cfg(windows)]
struct DiffReport {
    first_diff: Option<usize>,
    rules: Vec<&'static str>,
    detail: String,
}

#[cfg(windows)]
fn is_subsequence(small: &[u8], large: &[u8]) -> bool {
    let mut index = 0;
    for byte in large {
        if index < small.len() && small[index] == *byte {
            index += 1;
        }
    }
    index == small.len()
}

#[cfg(windows)]
fn count_byte(data: &[u8], needle: u8) -> usize {
    data.iter().filter(|byte| **byte == needle).count()
}

#[cfg(windows)]
fn strip_cr(data: &[u8]) -> Vec<u8> {
    data.iter().copied().filter(|byte| *byte != b'\r').collect()
}

/// Per-byte differential (never sampling, AR-25.2). Each observed phenomenon is
/// mapped to a registered conpty-rules.toml id.
#[cfg(windows)]
fn classify_f0(input: &[u8], output: &[u8]) -> DiffReport {
    let mut rules: Vec<&'static str> = Vec::new();
    if input == output {
        return DiffReport {
            first_diff: None,
            rules,
            detail: "identical".to_string(),
        };
    }
    let first_diff = input
        .iter()
        .zip(output.iter())
        .position(|(left, right)| left != right)
        .or_else(|| Some(input.len().min(output.len())));
    if strip_cr(input) == strip_cr(output) {
        rules.push("C-W1");
    }
    if count_byte(output, 0x1B) > count_byte(input, 0x1B) {
        rules.push("C-W2");
    }
    if output.len() > input.len() {
        rules.push("C-W1");
    }
    if !is_subsequence(input, output) {
        rules.push("C-W8");
    }
    if !is_subsequence(output, input) {
        rules.push("C-W2");
    }
    rules.sort_unstable();
    rules.dedup();
    let detail = format!("input_len={} output_len={}", input.len(), output.len());
    DiffReport {
        first_diff,
        rules,
        detail,
    }
}

#[test]
fn f0_pipe_fallback_is_byte_exact() {
    let backend = common::pipes();
    let helper = env!("CARGO_BIN_EXE_pty_echo");
    let command = termai_pty::Command::with_args(helper, vec!["256".to_string()]);
    let handle = backend
        .spawn(&command, common::size(), common::opts())
        .expect("spawn helper");
    let payload = probe_payload();
    common::write_all(&backend, &handle, &payload);
    let output = common::read_to_eof(&backend, &handle, Duration::from_secs(10));
    assert_eq!(output, payload, "pipe fallback must be byte exact (F0)");
    backend.close(handle).expect("close");
}

#[cfg(windows)]
#[test]
// Measured, not guessed: native ConPTY delivers real data and close() now releases the
// reader thread. termai-pty opens the pseudo-console output as an overlapped named pipe
// and close() issues CancelIoEx, so a read parked in read() returns EOF promptly instead
// of holding the 10s hard timeout (see the close-semantics addendum in
// src/windows/conpty.rs). The registered differences stay registered; a difference
// classified as a gate-failing rule still fails this test.
fn f0_conpty_difference_is_registered() {
    let backend = common::native();
    let handle = backend
        .spawn(
            &termai_pty::Command::new("cmd.exe"),
            common::size(),
            common::opts(),
        )
        .expect("spawn cmd.exe on ConPTY");
    let (sender, receiver) = std::sync::mpsc::channel();
    let reader_backend: Arc<dyn termai_pty::PtyBackend> = Arc::clone(&backend);
    let reader_handle = handle.clone();
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            match reader_backend.read(&reader_handle, &mut buffer) {
                Ok(0) => break,
                Ok(read) => out.extend_from_slice(&buffer[..read]),
                Err(_) => break,
            }
        }
        let _ = sender.send(out);
    });
    let payload = probe_payload();
    let written = common::write_best_effort(&backend, &handle, &payload);
    assert!(written > 0, "ConPTY accepted no input bytes");
    std::thread::sleep(Duration::from_millis(500));
    let tree = backend.process_tree(&handle).expect("process tree");
    let _ = backend.kill(&tree, termai_pty::KillMode::Force);
    backend
        .close(handle.clone())
        .expect("close aborts the in-flight read");
    let output = match receiver.recv_timeout(Duration::from_secs(10)) {
        Ok(bytes) => bytes,
        Err(_) => panic!("hard timeout: ConPTY read did not finish"),
    };
    let report = classify_f0(&payload, &output);
    println!(
        "F0 ConPTY differential: first_diff={:?} rules={:?} written={} {}",
        report.first_diff, report.rules, written, report.detail
    );
    assert!(
        !output.is_empty(),
        "ConPTY produced no output for the F0 probe"
    );
    if output != payload {
        assert!(
            !report.rules.is_empty(),
            "unregistered byte difference at offset {:?}",
            report.first_diff
        );
        let rules = load_rules(bundled_rules()).expect("bundled rules parse");
        for id in &report.rules {
            let rule = rules.iter().find(|rule| rule.id == *id).unwrap_or_else(|| {
                panic!("difference classified as {id}, which is not registered")
            });
            assert!(
                !is_gate_failure(rule),
                "difference classified as gate-failing rule {id}"
            );
        }
    }
    backend.close(handle).expect("idempotent close");
}
