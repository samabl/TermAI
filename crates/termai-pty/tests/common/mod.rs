//! Shared helpers for the termai-pty integration tests.
#![allow(dead_code)]

use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use termai_pty::{Command, ProcEntry, ProcessTree, PtyBackend, PtyHandle, SpawnOpts, WinSize};

pub fn native() -> Arc<dyn PtyBackend> {
    Arc::from(termai_pty::native_backend())
}

pub fn probed() -> Arc<dyn PtyBackend> {
    Arc::from(termai_pty::probe_backend())
}

pub fn pipes() -> Arc<dyn PtyBackend> {
    Arc::from(termai_pty::pipe_backend())
}

pub fn opts() -> SpawnOpts {
    SpawnOpts::default()
}

pub fn size() -> WinSize {
    WinSize::new(80, 24)
}

/// Start a reader thread that drains the pty into a channel. It finishes when
/// read returns 0 (natural EOF, or after close aborts an in-flight read).
pub fn start_reader(backend: &Arc<dyn PtyBackend>, handle: &PtyHandle) -> mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    let backend = Arc::clone(backend);
    let handle = handle.clone();
    thread::spawn(move || {
        let mut out = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            match backend.read(&handle, &mut buffer) {
                Ok(0) => break,
                Ok(read) => out.extend_from_slice(&buffer[..read]),
                Err(_) => break,
            }
        }
        let _ = sender.send(out);
    });
    receiver
}

/// Find `needle` inside `hay`.
fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

/// Reader that also answers the terminal status reports a ConPTY program sends while it
/// starts. conhost asks for the cursor position (CSI 6 n) and the shell **blocks until it is
/// answered**: a test that only drains the pty never gets past the init sequence, so it can
/// never observe a real command or its descendants (measured: 4 bytes out - just the query -
/// and the job stayed at one process). The reply here is the minimal 1;1 report
/// (CSI 1 ; 1 R); a grid-backed caller answers from Grid::take_responses instead.
pub fn start_reader_answering_dsr(
    backend: &Arc<dyn PtyBackend>,
    handle: &PtyHandle,
) -> mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    let backend = Arc::clone(backend);
    let handle = handle.clone();
    let writer = Arc::clone(&backend);
    let write_handle = handle.clone();
    thread::spawn(move || {
        let mut out = Vec::new();
        let mut buffer = [0u8; 8192];
        let mut pending: Vec<u8> = Vec::new();
        loop {
            match backend.read(&handle, &mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let chunk = &buffer[..read];
                    out.extend_from_slice(chunk);
                    pending.extend_from_slice(chunk);
                    while let Some(pos) = find_subslice(&pending, b"\x1b[6n") {
                        pending.drain(..pos + 4);
                        write_all(&writer, &write_handle, b"\x1b[1;1R");
                    }
                    // Keep a tail wide enough to catch an escape sequence split across reads.
                    if pending.len() > 8 {
                        let keep = pending.len() - 8;
                        pending.drain(..keep);
                    }
                }
                Err(_) => break,
            }
        }
        let _ = sender.send(out);
    });
    receiver
}
/// Collect a reader thread result with a hard timeout; a hang fails the test.
pub fn collect_reader(receiver: mpsc::Receiver<Vec<u8>>, timeout: Duration) -> Vec<u8> {
    match receiver.recv_timeout(timeout) {
        Ok(bytes) => bytes,
        Err(_) => panic!("hard timeout: PTY reader did not finish within {timeout:?}"),
    }
}

/// Read until EOF with a hard timeout (use only where the backend reaches EOF by
/// itself, e.g. the pipe fallback; ConPTY needs an explicit close).
pub fn read_to_eof(
    backend: &Arc<dyn PtyBackend>,
    handle: &PtyHandle,
    timeout: Duration,
) -> Vec<u8> {
    collect_reader(start_reader(backend, handle), timeout)
}

/// Write the whole slice, looping on partial writes.
pub fn write_all(backend: &Arc<dyn PtyBackend>, handle: &PtyHandle, data: &[u8]) {
    let mut written = 0;
    while written < data.len() {
        match backend.write(handle, &data[written..]) {
            Ok(0) => break,
            Ok(count) => written += count,
            Err(err) => panic!("write failed after {written} bytes: {err}"),
        }
    }
}

/// Best-effort write; returns how many bytes the backend accepted.
pub fn write_best_effort(backend: &Arc<dyn PtyBackend>, handle: &PtyHandle, data: &[u8]) -> usize {
    let mut written = 0;
    while written < data.len() {
        match backend.write(handle, &data[written..]) {
            Ok(0) => break,
            Ok(count) => written += count,
            Err(_) => break,
        }
    }
    written
}

/// A short-lived command that prints hello.
pub fn echo_command() -> Command {
    #[cfg(windows)]
    {
        Command::with_args("cmd.exe", vec!["/C".to_string(), "echo hello".to_string()])
    }
    #[cfg(unix)]
    {
        Command::with_args("/bin/echo", vec!["hello".to_string()])
    }
}

/// A long-lived command for tree / signal tests.
pub fn shell_command() -> Command {
    #[cfg(windows)]
    {
        Command::new("cmd.exe")
    }
    #[cfg(unix)]
    {
        Command::with_args("/bin/sleep", vec!["300".to_string()])
    }
}

/// Poll a tree snapshot until it holds at least `count` processes or the deadline passes.
pub fn wait_for_count(
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

/// Poll a tree snapshot until it is empty or the deadline passes.
pub fn wait_for_empty(
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
