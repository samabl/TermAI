//! Best-effort pipe fallback backend (kernel/02 section 6).
//!
//! Used when the native pseudo-console path is unavailable, and in tests that need
//! a byte-exact channel. It spawns an ordinary process on pipes, so it has no
//! window size, no signals (only the 0x03 byte fallback) and no Job Object. Every
//! unsupported feature is declared honestly in PtyCapabilities.

use std::io::{Error as IoError, ErrorKind, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command as StdCommand, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::{
    Command, DetachPolicy, EnvPolicy, ExitInfo, Fidelity, HandleOps, KillMode, ProcEntry,
    PtyBackend, PtyCapabilities, PtyError, PtyHandle, ResizeCap, ResizeEffect, Sig, SignalCap,
    SignalOutcome, SpawnOpts, SpawnStage, TreeOps, WaitTimeout, WinSize,
};

/// Pipe fallback backend.
pub(crate) struct PipeBackend {
    caps: PtyCapabilities,
}

impl PipeBackend {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            caps: PtyCapabilities {
                resize: ResizeCap::Unsupported,
                signals: SignalCap::ByteFallback,
                graphics_passthrough: false,
                byte_fidelity: Fidelity::F0,
                job_control: false,
            },
        }
    }
}

impl Default for PipeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyBackend for PipeBackend {
    fn capabilities(&self) -> PtyCapabilities {
        self.caps
    }

    fn spawn(&self, cmd: &Command, _sz: WinSize, o: SpawnOpts) -> Result<PtyHandle, PtyError> {
        let mut child_cmd = StdCommand::new(&cmd.program);
        child_cmd.args(&cmd.args);
        match &o.env {
            EnvPolicy::Inherit => {}
            EnvPolicy::Clean => {
                child_cmd.env_clear();
            }
            EnvPolicy::Explicit(vars) => {
                child_cmd.env_clear();
                for (key, value) in vars {
                    child_cmd.env(key, value);
                }
            }
        }
        if let Some(dir) = &o.cwd {
            child_cmd.current_dir(dir);
        }
        // A pipe pair has no process group: DetachPolicy is accepted but inert, and
        // login is meaningless. Both are documented fallback limitations.
        match o.detach_policy {
            DetachPolicy::Allow | DetachPolicy::Deny => {}
        }
        child_cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = child_cmd.spawn().map_err(|err| PtyError::Spawn {
            errno: err.raw_os_error().unwrap_or(0),
            stage: SpawnStage::Spawn,
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or(PtyError::Unsupported("pipe fallback: missing child stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(PtyError::Unsupported("pipe fallback: missing child stdout"))?;
        let pid = child.id();
        let shared = Arc::new(PipeShared {
            pid,
            program: cmd.program.clone(),
            started: Instant::now(),
            child: Mutex::new(child),
            stdin: Mutex::new(Some(stdin)),
            stdout: Mutex::new(stdout),
            closed: AtomicBool::new(false),
            eof: AtomicBool::new(false),
            reaped: Mutex::new(None),
        });
        let ops: Arc<dyn HandleOps> = shared.clone();
        let tree: Arc<dyn TreeOps> = shared;
        Ok(PtyHandle::new(ops, tree))
    }
}

struct PipeShared {
    pid: u32,
    program: String,
    started: Instant,
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    stdout: Mutex<ChildStdout>,
    closed: AtomicBool,
    eof: AtomicBool,
    reaped: Mutex<Option<ExitInfo>>,
}

impl PipeShared {
    fn record_exit(&self, status: ExitStatus) -> ExitInfo {
        if let Some(info) = *lock(&self.reaped) {
            return info;
        }
        let info = ExitInfo {
            code: status.code(),
            signal: None,
            reaped: true,
            live_children: 0,
            wall: self.started.elapsed(),
        };
        *lock(&self.reaped) = Some(info);
        info
    }

    fn exited(&self) -> bool {
        if lock(&self.reaped).is_some() {
            return true;
        }
        let status = {
            let mut child = lock(&self.child);
            child.try_wait()
        };
        match status {
            Ok(Some(status)) => {
                let _ = self.record_exit(status);
                true
            }
            Ok(None) | Err(_) => false,
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn pipe_closed() -> PtyError {
    PtyError::Io(IoError::new(
        ErrorKind::BrokenPipe,
        "pipe fallback: stdin is closed",
    ))
}

fn timeout_duration(to: WaitTimeout) -> Duration {
    match to {
        WaitTimeout::Zero => Duration::ZERO,
        WaitTimeout::Millis(ms) => Duration::from_millis(ms),
        WaitTimeout::Infinite => Duration::MAX,
    }
}

impl HandleOps for PipeShared {
    fn write(&self, data: &[u8]) -> Result<usize, PtyError> {
        if data.is_empty() {
            return Ok(0);
        }
        if self.closed.load(Ordering::SeqCst) {
            return Err(pipe_closed());
        }
        let mut guard = lock(&self.stdin);
        let stdin = guard.as_mut().ok_or_else(pipe_closed)?;
        loop {
            match stdin.write(data) {
                Ok(written) => return Ok(written),
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => return Err(PtyError::Io(err)),
            }
        }
    }

    fn read(&self, buf: &mut [u8]) -> Result<usize, PtyError> {
        if buf.is_empty() || self.closed.load(Ordering::SeqCst) || self.eof.load(Ordering::SeqCst) {
            return Ok(0);
        }
        let mut guard = lock(&self.stdout);
        loop {
            match guard.read(buf) {
                Ok(0) => {
                    self.eof.store(true, Ordering::SeqCst);
                    return Ok(0);
                }
                Ok(read) => return Ok(read),
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => return Err(PtyError::Io(err)),
            }
        }
    }

    fn resize(&self, _sz: WinSize) -> Result<ResizeEffect, PtyError> {
        Err(PtyError::Unsupported("pipe fallback has no window size"))
    }

    fn signal(&self, sig: Sig) -> Result<SignalOutcome, PtyError> {
        match sig {
            Sig::Int | Sig::Term => {
                self.write(&[0x03])?;
                Ok(SignalOutcome::ByteFallback(0x03))
            }
            _ => Ok(SignalOutcome::Unsupported),
        }
    }

    fn close(&self) -> Result<(), PtyError> {
        if self.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        // Dropping child stdin signals EOF to the child. We deliberately do not kill
        // here: close() releases the pty and leaves process cleanup to the tree.
        *lock(&self.stdin) = None;
        Ok(())
    }
}

impl TreeOps for PipeShared {
    fn snapshot(&self) -> Result<Vec<ProcEntry>, PtyError> {
        if self.exited() {
            return Ok(Vec::new());
        }
        Ok(vec![ProcEntry {
            pid: self.pid,
            ppid: 0,
            name: self.program.clone(),
            start_time: 0,
            cpu_ms: 0,
        }])
    }

    fn live_children(&self) -> u32 {
        if self.exited() {
            0
        } else {
            1
        }
    }

    fn kill(&self, mode: KillMode) -> Result<ExitInfo, PtyError> {
        if let Some(info) = *lock(&self.reaped) {
            return Ok(info);
        }
        if let KillMode::Graceful(grace) = mode {
            let _ = self.write(&[0x03]);
            let deadline = Instant::now() + grace;
            loop {
                let status = {
                    let mut child = lock(&self.child);
                    child.try_wait()
                };
                match status {
                    Ok(Some(status)) => return Ok(self.record_exit(status)),
                    Ok(None) => {}
                    Err(err) => return Err(PtyError::Io(err)),
                }
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        {
            let mut child = lock(&self.child);
            let _ = child.kill();
            let _ = child.wait();
        }
        let info = ExitInfo {
            code: None,
            signal: Some(Sig::Kill),
            reaped: true,
            live_children: 0,
            wall: self.started.elapsed(),
        };
        *lock(&self.reaped) = Some(info);
        Ok(info)
    }

    fn wait(&self, to: WaitTimeout) -> Result<ExitInfo, PtyError> {
        if let Some(info) = *lock(&self.reaped) {
            return Ok(info);
        }
        let deadline = match to {
            WaitTimeout::Zero => Some(Instant::now()),
            WaitTimeout::Millis(ms) => Some(Instant::now() + Duration::from_millis(ms)),
            WaitTimeout::Infinite => None,
        };
        loop {
            let status = {
                let mut child = lock(&self.child);
                child.try_wait()
            };
            match status {
                Ok(Some(status)) => return Ok(self.record_exit(status)),
                Ok(None) => {}
                Err(err) => return Err(PtyError::Io(err)),
            }
            match deadline {
                Some(dl) if Instant::now() >= dl => {
                    return Err(PtyError::Timeout {
                        pid: self.pid,
                        after: timeout_duration(to),
                    });
                }
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }
}
