//! unix forkpty backend (kernel/02 section 3.4).
//!
//! Default path is forkpty (atomic allocation of master + controlling terminal).
//! Resize is the single primitive ioctl(master, TIOCSWINSZ, winsize); the kernel
//! then signals the foreground process group with SIGWINCH. Signals go to the
//! foreground process group with kill(-pgid, sig). Children are reaped with
//! waitpid(WNOHANG) so no zombie accumulates.
//!
//! NOTE: this module is compiled only on cfg(unix); the WS-C development host is
//! Windows, so it is written conservatively and is not compile-verified here.

use std::ffi::CString;
use std::io::{Error as IoError, ErrorKind};
use std::os::unix::ffi::OsStrExt;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::{
    Command, EnvPolicy, ExitInfo, Fidelity, HandleOps, KillMode, ProcEntry, PtyBackend,
    PtyCapabilities, PtyError, PtyHandle, ResizeCap, ResizeEffect, Sig, SignalCap, SignalOutcome,
    SpawnOpts, SpawnStage, TreeOps, WaitTimeout, WinSize,
};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn timeout_duration(to: WaitTimeout) -> Duration {
    match to {
        WaitTimeout::Zero => Duration::ZERO,
        WaitTimeout::Millis(ms) => Duration::from_millis(ms),
        WaitTimeout::Infinite => Duration::MAX,
    }
}

fn signal_number(sig: Sig) -> libc::c_int {
    match sig {
        Sig::Int => libc::SIGINT,
        Sig::Term => libc::SIGTERM,
        Sig::Hup => libc::SIGHUP,
        Sig::Quit => libc::SIGQUIT,
        Sig::Winch => libc::SIGWINCH,
        Sig::Usr1 => libc::SIGUSR1,
        Sig::Usr2 => libc::SIGUSR2,
        Sig::Kill => libc::SIGKILL,
        Sig::Stop => libc::SIGSTOP,
        Sig::Cont => libc::SIGCONT,
    }
}

fn signal_from_number(number: libc::c_int) -> Option<Sig> {
    match number {
        libc::SIGINT => Some(Sig::Int),
        libc::SIGTERM => Some(Sig::Term),
        libc::SIGHUP => Some(Sig::Hup),
        libc::SIGQUIT => Some(Sig::Quit),
        libc::SIGWINCH => Some(Sig::Winch),
        libc::SIGUSR1 => Some(Sig::Usr1),
        libc::SIGUSR2 => Some(Sig::Usr2),
        libc::SIGKILL => Some(Sig::Kill),
        libc::SIGSTOP => Some(Sig::Stop),
        libc::SIGCONT => Some(Sig::Cont),
        _ => None,
    }
}

fn decode_status(status: libc::c_int) -> (Option<i32>, Option<Sig>) {
    if libc::WIFEXITED(status) {
        (Some(libc::WEXITSTATUS(status)), None)
    } else if libc::WIFSIGNALED(status) {
        (None, signal_from_number(libc::WTERMSIG(status)))
    } else {
        (None, None)
    }
}

enum WaitState {
    Running,
    Exited(libc::c_int),
    Gone,
}

fn waitpid_nohang(pid: libc::pid_t) -> Result<WaitState, PtyError> {
    let mut status: libc::c_int = 0;
    // SAFETY: pid is the child owned by this module and status is a valid out-pointer.
    let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    if result == 0 {
        return Ok(WaitState::Running);
    }
    if result == pid {
        return Ok(WaitState::Exited(status));
    }
    let err = IoError::last_os_error();
    if err.raw_os_error() == Some(libc::ECHILD) {
        return Ok(WaitState::Gone);
    }
    Err(PtyError::Io(err))
}

/// Send a signal to the child process group; returns true when it was delivered.
fn kill_group(pid: libc::pid_t, number: libc::c_int) -> bool {
    // SAFETY: negative pid means "process group"; the call only delivers a signal.
    unsafe { libc::kill(-pid, number) == 0 }
}

fn group_alive(pid: libc::pid_t) -> bool {
    // SAFETY: signal 0 only probes existence.
    unsafe { libc::kill(-pid, 0) == 0 }
}

/// forkpty backend.
pub(crate) struct ForkPtyBackend {
    caps: PtyCapabilities,
}

impl ForkPtyBackend {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            caps: PtyCapabilities {
                resize: ResizeCap::Exact,
                signals: SignalCap::Posix,
                graphics_passthrough: true,
                byte_fidelity: Fidelity::F0,
                job_control: true,
            },
        }
    }
}

impl Default for ForkPtyBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyBackend for ForkPtyBackend {
    fn capabilities(&self) -> PtyCapabilities {
        self.caps
    }

    fn spawn(&self, cmd: &Command, sz: WinSize, o: SpawnOpts) -> Result<PtyHandle, PtyError> {
        spawn_forkpty(cmd, sz, o)
    }
}

fn bad_input(stage: SpawnStage) -> PtyError {
    PtyError::Spawn {
        errno: libc::EINVAL,
        stage,
    }
}

fn cstring(text: &str, stage: SpawnStage) -> Result<CString, PtyError> {
    CString::new(text.as_bytes()).map_err(|_| bad_input(stage))
}

fn spawn_forkpty(cmd: &Command, sz: WinSize, o: SpawnOpts) -> Result<PtyHandle, PtyError> {
    if !sz.is_valid() {
        return Err(bad_input(SpawnStage::CreatePty));
    }

    let program = cstring(&cmd.program, SpawnStage::Resolve)?;
    let mut owned: Vec<CString> = Vec::with_capacity(cmd.args.len() + 1);
    let argv0 = if o.login {
        format!("-{}", cmd.program)
    } else {
        cmd.program.clone()
    };
    owned.push(cstring(&argv0, SpawnStage::Resolve)?);
    for arg in &cmd.args {
        owned.push(cstring(arg, SpawnStage::Resolve)?);
    }
    let mut argv: Vec<*const libc::c_char> = owned.iter().map(|value| value.as_ptr()).collect();
    argv.push(ptr::null());

    let env_owned: Option<Vec<CString>> = match &o.env {
        EnvPolicy::Inherit => None,
        EnvPolicy::Clean => Some(Vec::new()),
        EnvPolicy::Explicit(vars) => {
            let mut list = Vec::with_capacity(vars.len());
            for (key, value) in vars {
                list.push(cstring(&format!("{key}={value}"), SpawnStage::Resolve)?);
            }
            Some(list)
        }
    };
    let mut envp: Vec<*const libc::c_char> = match &env_owned {
        Some(list) => {
            let mut pointers: Vec<*const libc::c_char> =
                list.iter().map(|value| value.as_ptr()).collect();
            pointers.push(ptr::null());
            pointers
        }
        None => Vec::new(),
    };

    let cwd = match &o.cwd {
        Some(path) => Some(
            CString::new(path.as_os_str().as_bytes())
                .map_err(|_| bad_input(SpawnStage::Resolve))?,
        ),
        None => None,
    };

    let mut master: libc::c_int = -1;
    let mut winsize = libc::winsize {
        ws_row: sz.rows,
        ws_col: sz.cols,
        ws_xpixel: sz.px_w,
        ws_ypixel: sz.px_h,
    };
    // SAFETY: forkpty is called with valid out-pointers. In the child only
    // async-signal-safe calls (chdir / execve / execvp / _exit) are made.
    let pid = unsafe { libc::forkpty(&mut master, ptr::null_mut(), ptr::null_mut(), &mut winsize) };
    if pid < 0 {
        return Err(PtyError::Io(IoError::last_os_error()));
    }
    if pid == 0 {
        if let Some(dir) = &cwd {
            // SAFETY: dir is a valid NUL-terminated path.
            unsafe {
                libc::chdir(dir.as_ptr());
            }
        }
        // SAFETY: argv and envp are valid NUL-terminated pointer arrays that outlive
        // the exec call either way.
        unsafe {
            if env_owned.is_some() {
                libc::execve(program.as_ptr(), argv.as_ptr(), envp.as_ptr());
            } else {
                libc::execvp(program.as_ptr(), argv.as_ptr());
            }
            libc::_exit(127);
        }
    }

    let handle = Arc::new(UnixHandle {
        master: AtomicI32::new(master),
        pid,
        closed: AtomicBool::new(false),
        eof: AtomicBool::new(false),
    });
    let tree = Arc::new(UnixTree {
        pid,
        program: cmd.program.clone(),
        started: Instant::now(),
        reaped: Mutex::new(None),
        last_code: Mutex::new((None, None)),
    });
    let ops: Arc<dyn HandleOps> = handle;
    let tree_ops: Arc<dyn TreeOps> = tree;
    Ok(PtyHandle::new(ops, tree_ops))
}

struct UnixHandle {
    master: AtomicI32,
    pid: libc::pid_t,
    closed: AtomicBool,
    eof: AtomicBool,
}

impl HandleOps for UnixHandle {
    fn write(&self, data: &[u8]) -> Result<usize, PtyError> {
        if data.is_empty() {
            return Ok(0);
        }
        let fd = self.master.load(Ordering::SeqCst);
        if fd < 0 || self.closed.load(Ordering::SeqCst) {
            return Err(PtyError::Io(IoError::new(
                ErrorKind::BrokenPipe,
                "pty master closed",
            )));
        }
        loop {
            // SAFETY: fd is a live master fd and data is a valid readable slice.
            let result =
                unsafe { libc::write(fd, data.as_ptr().cast::<libc::c_void>(), data.len()) };
            if result >= 0 {
                return Ok(result as usize);
            }
            let err = IoError::last_os_error();
            if err.kind() == ErrorKind::Interrupted {
                continue;
            }
            return Err(PtyError::Io(err));
        }
    }

    fn read(&self, buf: &mut [u8]) -> Result<usize, PtyError> {
        if buf.is_empty() || self.closed.load(Ordering::SeqCst) || self.eof.load(Ordering::SeqCst) {
            return Ok(0);
        }
        let fd = self.master.load(Ordering::SeqCst);
        if fd < 0 {
            return Ok(0);
        }
        loop {
            // SAFETY: fd is a live master fd and buf is a valid writable slice.
            let result =
                unsafe { libc::read(fd, buf.as_mut_ptr().cast::<libc::c_void>(), buf.len()) };
            if result > 0 {
                return Ok(result as usize);
            }
            if result == 0 {
                self.eof.store(true, Ordering::SeqCst);
                return Ok(0);
            }
            let err = IoError::last_os_error();
            if err.kind() == ErrorKind::Interrupted {
                continue;
            }
            return Err(PtyError::Io(err));
        }
    }

    fn resize(&self, sz: WinSize) -> Result<ResizeEffect, PtyError> {
        if !sz.is_valid() {
            return Err(PtyError::Unsupported("resize cols/rows must be >= 1"));
        }
        let fd = self.master.load(Ordering::SeqCst);
        if fd < 0 {
            return Err(PtyError::NoSuchPty);
        }
        let winsize = libc::winsize {
            ws_row: sz.rows,
            ws_col: sz.cols,
            ws_xpixel: sz.px_w,
            ws_ypixel: sz.px_h,
        };
        // SAFETY: fd is a live terminal master and winsize is a valid winsize.
        let result = unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &winsize) };
        if result != 0 {
            return Err(PtyError::Io(IoError::last_os_error()));
        }
        // The kernel raises SIGWINCH for the foreground process group (kernel/02 3.4).
        Ok(ResizeEffect::Applied)
    }

    fn signal(&self, sig: Sig) -> Result<SignalOutcome, PtyError> {
        if self.pid <= 0 {
            return Ok(SignalOutcome::Unsupported);
        }
        if kill_group(self.pid, signal_number(sig)) {
            Ok(SignalOutcome::Delivered)
        } else {
            Ok(SignalOutcome::Unsupported)
        }
    }

    fn close(&self) -> Result<(), PtyError> {
        if self.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let fd = self.master.swap(-1, Ordering::SeqCst);
        if fd >= 0 {
            // SAFETY: fd is the master fd owned by this handle and is closed once.
            unsafe {
                libc::close(fd);
            }
        }
        Ok(())
    }
}

impl Drop for UnixHandle {
    fn drop(&mut self) {
        let _ = HandleOps::close(self);
    }
}

struct UnixTree {
    pid: libc::pid_t,
    program: String,
    started: Instant,
    reaped: Mutex<Option<ExitInfo>>,
    last_code: Mutex<(Option<i32>, Option<Sig>)>,
}

impl UnixTree {
    fn record(&self, status: libc::c_int) -> ExitInfo {
        let (code, signal) = decode_status(status);
        *lock(&self.last_code) = (code, signal);
        let info = ExitInfo {
            code,
            signal,
            reaped: true,
            live_children: 0,
            wall: self.started.elapsed(),
        };
        *lock(&self.reaped) = Some(info);
        info
    }
}

impl Drop for UnixTree {
    fn drop(&mut self) {
        // Reap without blocking so a finished child never stays a zombie.
        let _ = waitpid_nohang(self.pid);
    }
}

impl TreeOps for UnixTree {
    fn snapshot(&self) -> Result<Vec<ProcEntry>, PtyError> {
        if lock(&self.reaped).is_some() {
            return Ok(Vec::new());
        }
        match waitpid_nohang(self.pid)? {
            WaitState::Running => Ok(vec![ProcEntry {
                pid: self.pid as u32,
                ppid: 0,
                name: self.program.clone(),
                start_time: 0,
                cpu_ms: 0,
            }]),
            WaitState::Exited(status) => {
                let _ = self.record(status);
                Ok(Vec::new())
            }
            WaitState::Gone => Ok(Vec::new()),
        }
    }

    fn live_children(&self) -> u32 {
        if lock(&self.reaped).is_some() && !group_alive(self.pid) {
            return 0;
        }
        if group_alive(self.pid) {
            1
        } else {
            0
        }
    }

    fn kill(&self, mode: KillMode) -> Result<ExitInfo, PtyError> {
        if let Some(info) = *lock(&self.reaped) {
            return Ok(info);
        }
        match mode {
            KillMode::Graceful(grace) => {
                kill_group(self.pid, libc::SIGTERM);
                let deadline = Instant::now() + grace;
                loop {
                    if let WaitState::Exited(status) = waitpid_nohang(self.pid)? {
                        return Ok(self.record(status));
                    }
                    if Instant::now() >= deadline {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                kill_group(self.pid, libc::SIGKILL);
            }
            KillMode::Force => {
                kill_group(self.pid, libc::SIGKILL);
            }
        }
        // Reap the root; the process group was signalled above.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match waitpid_nohang(self.pid)? {
                WaitState::Exited(status) => {
                    let mut info = self.record(status);
                    info.signal = Some(Sig::Kill);
                    *lock(&self.reaped) = Some(info);
                    return Ok(info);
                }
                WaitState::Gone => {
                    let (code, signal) = *lock(&self.last_code);
                    let info = ExitInfo {
                        code,
                        signal: Some(Sig::Kill),
                        reaped: true,
                        live_children: 0,
                        wall: self.started.elapsed(),
                    };
                    *lock(&self.reaped) = Some(info);
                    return Ok(info);
                }
                WaitState::Running => {}
            }
            if Instant::now() >= deadline {
                let info = ExitInfo {
                    code: None,
                    signal: Some(Sig::Kill),
                    reaped: false,
                    live_children: self.live_children(),
                    wall: self.started.elapsed(),
                };
                // PTY-ORPHAN-1: report honestly instead of pretending success.
                if info.live_children > 0 {
                    return Err(PtyError::TreeNotEmpty {
                        live: info.live_children,
                    });
                }
                *lock(&self.reaped) = Some(info);
                return Ok(info);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
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
            match waitpid_nohang(self.pid)? {
                WaitState::Exited(status) => return Ok(self.record(status)),
                WaitState::Gone => {
                    let (code, signal) = *lock(&self.last_code);
                    let info = ExitInfo {
                        code,
                        signal,
                        reaped: true,
                        live_children: 0,
                        wall: self.started.elapsed(),
                    };
                    *lock(&self.reaped) = Some(info);
                    return Ok(info);
                }
                WaitState::Running => {}
            }
            match deadline {
                Some(limit) if Instant::now() >= limit => {
                    return Err(PtyError::Timeout {
                        pid: self.pid as u32,
                        after: timeout_duration(to),
                    });
                }
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }
}
