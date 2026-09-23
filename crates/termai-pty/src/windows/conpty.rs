//! Windows ConPTY backend (DC-16; kernel/02 sections 3.2-3.5).
//!
//! CreatePseudoConsole + CreateProcessW with PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
//! ResizePseudoConsole, ClosePseudoConsole, plus a Job Object
//! (CreateJobObjectW + SetInformationJobObject with KILL_ON_JOB_CLOSE +
//! AssignProcessToJobObject) for process-tree closure (DC-16 / kernel/02 3.3).
//!
//! Signal mapping follows kernel/02 section 3.5 exactly. Int is the byte path
//! (K-03 default) and only tries CTRL_C_EVENT when this process has no console, so
//! we can never signal our own process group. Term tries CTRL_BREAK_EVENT on the
//! child's process group. Hup/Quit/Usr1/Usr2/Winch/Stop/Cont are Unsupported and we
//! never fold CTRL_CLOSE_EVENT into Sig::Int (C-W6).
//!
//! RESOLVED (was: KNOWN DEFECT / HANDOVER, recorded 2026-03 by WS-C). The analysis below is
//! kept verbatim as the historical record, per the project rule that history is not rewritten.
//! ---------------------------------------------------------------------------------------
//! The native ConPTY path now works. There were THREE independent faults, in the order they
//! blocked progress:
//!
//! 1. UpdateProcThreadAttribute passed the ADDRESS of the HPCON variable as lpValue.
//!    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE expects the HPCON VALUE itself (the Microsoft sample
//!    passes hPC; wezterm's portable-pty passes its HPCON directly). Windows therefore read a
//!    stack address as the pseudoconsole handle, the attribute was worthless, the client was
//!    created with NO console, and it died in console initialisation with 0xC0000142 having
//!    written zero bytes. Fix: pass the handle value.
//! 2. A blocking ReadFile plus a ClosePseudoConsole issued from a watchdog thread deadlocked:
//!    closing the console waits behind the pending read, so BOTH threads parked forever and
//!    even a hard timeout could not end the session. Fix: the read runs on a worker thread and
//!    hands chunks over a channel; the pty is closed only when no read is pending.
//! 3. CSI 6 n (DSR) was a no-op, so the client blocked forever waiting for its cursor position
//!    report. Fix: termai-vt answers DSR through Grid::take_responses (CPR / status) and the
//!    caller writes it back to the pty.
//!
//! Evidence after the fixes: cargo test -p termai --test e2e is 6/6, cargo test --workspace is
//! clean and the kernel gates are 8/8. A live run reads the ConPTY init sequence, the program
//! output, and exits 0 with F0 intact (77 bytes read equals 77 bytes fed to the parser).
//!
//! Why the earlier conclusion was wrong (worth remembering): E1-E4 varied parameters that were
//! never at fault, and the standalone probe that mirrored the Microsoft sample shared fault 1
//! with this crate, so it failed identically and appeared to confirm a host limitation. An
//! INDEPENDENT oracle - a throwaway project built on portable-pty, outside this repository
//! because AR-28.3 refuses it as a product dependency - proved ConPTY works on this host, which
//! is what redirected the investigation to our own wiring.
//!
//! HISTORICAL RECORD (unchanged) - the original handover follows.
//! ---------------------------------------------------------------------------------------
//! Symptom: CreatePseudoConsole returns S_OK (hpc != 0) and CreateProcessW succeeds
//! (pid set), but the client process cannot initialise its console host and
//! terminates with 0xC0000142 STATUS_DLL_INIT_FAILED, producing zero bytes on the
//! pseudoconsole output pipe. The failure is intermittent: the same standalone
//! sequence sometimes yields exit code 0x0, still with zero bytes. It is therefore
//! not a logic error in the parameter wiring but a host/console-initialisation
//! failure in this environment.
//!
//! RULED OUT by inspection and by experiment (all against a standalone probe that
//! mirrors the Microsoft "Creating a Pseudoconsole session" sample, i.e. without any
//! of this crate's code):
//! - Conduit direction: CreatePseudoConsole(coord, input_read, output_write); the
//!   kept ends are input_write / output_read and their inherit bit is cleared.
//! - STARTUPINFOEXW: cb = size_of::<STARTUPINFOEXW>(), two-call
//!   InitializeProcThreadAttributeList size probe with a live list buffer,
//!   UpdateProcThreadAttribute(PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, cbSize =
//!   size_of::<HPCON>()), EXTENDED_STARTUPINFO_PRESENT,
//!   DeleteProcThreadAttributeList only after CreateProcessW returns.
//! - EnvPolicy::Inherit passes a null environment pointer; ActiveProcessLimit = 256.
//!
//! EXPERIMENTS (E1-E4) and results:
//! - E1 Job Object: standalone probe assigns no job at all -> same 0xC0000142 and
//!   zero bytes; the job assignment and KILL_ON_JOB_CLOSE / BREAKAWAY_OK flags are
//!   therefore not the cause.
//! - E2 bInheritHandles: FALSE and TRUE both fail (nondeterministically 0x0 vs
//!   0xC0000142); FALSE was adopted because it stops the DLL-init failure most often.
//! - E3 explicit exit probe: WaitForSingleObject(hProcess, 0) signals and
//!   GetExitCodeProcess reports 0xC0000142; the first ReadFile returns 0 bytes
//!   because the child is already dead or console-less.
//! - E4 STARTF_USESTDHANDLES: setting hStdInput/hStdOutput/hStdError to the pty-side
//!   pipe ends did not change the result (0xC0000142, zero bytes) in any of the
//!   {inherit x use_std} combinations tried.
//!
//! Consequences in this crate:
//! - The ConPTY faces are wired and compile, but the end-to-end spawn+read smoke
//!   test cannot pass on this host; the integration tests therefore exercise the
//!   pipe fallback for byte/interface invariants and the ConPTY differential test is
//!   #[ignore]d with this reason.
//! - probe_backend() keeps its documented "fall back when the native path is
//!   unavailable" contract conceptually, but probe_available() cannot yet detect
//!   this failure mode reliably; consumers that require output on Windows should
//!   currently select pipe_backend() explicitly.
//! - kill(Force) does NOT close the pseudo console: it only calls
//!   TerminateJobObject and reaps the root. close() is the single owner of
//!   ClosePseudoConsole, so there is no double close.

use std::io::{Error as IoError, ErrorKind};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, SetHandleInformation, FALSE, FILETIME, HANDLE, HANDLE_FLAG_INHERIT,
    INVALID_HANDLE_VALUE, TRUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows_sys::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, GenerateConsoleCtrlEvent, GetConsoleCP,
    GetConsoleOutputCP, ResizePseudoConsole, SetConsoleCP, SetConsoleOutputCP, COORD,
    CTRL_BREAK_EVENT, CTRL_C_EVENT, HPCON,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicProcessIdList,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
    TerminateJobObject, JOBOBJECT_BASIC_PROCESS_ID_LIST, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess, GetProcessTimes,
    InitializeProcThreadAttributeList, OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject, CREATE_NEW_PROCESS_GROUP,
    CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, PROCESS_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, STARTF_USESTDHANDLES,
    STARTUPINFOEXW,
};

/// CreatePseudoConsole flag: keep the old resize behaviour working on older Windows builds.
const PSEUDOCONSOLE_RESIZE_QUIRK: u32 = 1;
/// CreatePseudoConsole flag: put the pseudoconsole input side into classic Win32 console
/// input mode, which is what cmd.exe and PowerShell expect (wezterm sets it too).
const PSEUDOCONSOLE_WIN32_INPUT_MODE: u32 = 2;

use crate::{
    Command, DetachPolicy, EnvPolicy, ExitInfo, Fidelity, HandleOps, KillMode, ProcEntry,
    PtyBackend, PtyCapabilities, PtyError, PtyHandle, ResizeCap, ResizeEffect, Sig, SignalCap,
    SignalOutcome, SpawnOpts, SpawnStage, TreeOps, WaitTimeout, WinSize, RULESET_CONPTY,
};

/// Broken pipe / closed handle: read returns EOF.
const ERROR_BROKEN_PIPE: i32 = 109;
/// Read past end of a pipe: EOF.
const ERROR_HANDLE_EOF: i32 = 38;
/// The read was aborted by ClosePseudoConsole.
const ERROR_OPERATION_ABORTED: i32 = 995;
/// QueryInformationJobObject buffer too small.
const ERROR_MORE_DATA: i32 = 234;
/// UTF-8 code page required by C-W5.
const UTF8_CODE_PAGE: u32 = 65001;
/// Active-process ceiling per job, anti fork-bomb (DC-27).
const JOB_ACTIVE_PROCESS_LIMIT: u32 = 256;
/// How long kill() waits for the root process to be reaped.
const JOB_WAIT_MS: u32 = 5_000;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn close_handle(handle: isize) {
    if handle != 0 && handle != INVALID_HANDLE_VALUE as isize {
        // SAFETY: handle is a live Win32 handle owned by this module and is not used
        // again after this call.
        unsafe {
            CloseHandle(handle as HANDLE);
        }
    }
}

fn last_error() -> i32 {
    // SAFETY: GetLastError has no preconditions.
    unsafe { GetLastError() as i32 }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn is_eof_error(err: &IoError) -> bool {
    match err.raw_os_error() {
        Some(code) => {
            code == ERROR_BROKEN_PIPE || code == ERROR_HANDLE_EOF || code == ERROR_OPERATION_ABORTED
        }
        None => false,
    }
}

fn timeout_duration(to: WaitTimeout) -> Duration {
    match to {
        WaitTimeout::Zero => Duration::ZERO,
        WaitTimeout::Millis(ms) => Duration::from_millis(ms),
        WaitTimeout::Infinite => Duration::MAX,
    }
}

/// Quote one argv element for CreateProcessW (MSVCRT rules). Command is argv only
/// (AR-06 rule 2); this is the mandatory explicit escaping, never shell -c.
fn append_quoted(out: &mut String, arg: &str) {
    if !arg.is_empty() && !arg.chars().any(|c| c == ' ' || c == '\t' || c == '"') {
        out.push_str(arg);
        return;
    }
    out.push('"');
    let mut backslashes: usize = 0;
    for ch in arg.chars() {
        if ch == '\\' {
            backslashes += 1;
            continue;
        }
        if ch == '"' {
            for _ in 0..(backslashes * 2 + 1) {
                out.push('\\');
            }
            out.push('"');
            backslashes = 0;
            continue;
        }
        for _ in 0..backslashes {
            out.push('\\');
        }
        backslashes = 0;
        out.push(ch);
    }
    for _ in 0..(backslashes * 2) {
        out.push('\\');
    }
    out.push('"');
}

fn build_command_line(cmd: &Command) -> String {
    let mut line = String::new();
    append_quoted(&mut line, &cmd.program);
    for arg in &cmd.args {
        line.push(' ');
        append_quoted(&mut line, arg);
    }
    line
}

/// UTF-16 environment block; None means inherit the parent environment.
fn build_env_block(policy: &EnvPolicy) -> Option<Vec<u16>> {
    match policy {
        EnvPolicy::Inherit => None,
        EnvPolicy::Clean => Some(vec![0, 0]),
        EnvPolicy::Explicit(vars) => {
            let mut block: Vec<u16> = Vec::new();
            for (key, value) in vars {
                block.extend(format!("{key}={value}").encode_utf16());
                block.push(0);
            }
            if block.is_empty() {
                block.push(0);
            }
            block.push(0);
            Some(block)
        }
    }
}

fn create_pipe() -> Result<(isize, isize), PtyError> {
    let mut read: HANDLE = ptr::null_mut();
    let mut write: HANDLE = ptr::null_mut();
    // SAFETY: SECURITY_ATTRIBUTES is a plain C struct; an all-zero value is a valid
    // initial state and every field is written below.
    let mut attrs: SECURITY_ATTRIBUTES = unsafe { std::mem::zeroed() };
    attrs.nLength = std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32;
    attrs.bInheritHandle = TRUE;
    attrs.lpSecurityDescriptor = ptr::null_mut();
    // SAFETY: read and write are valid out-pointers; attrs is fully initialised and
    // outlives the call.
    let ok = unsafe { CreatePipe(&mut read, &mut write, &attrs, 0) };
    if ok == 0 {
        return Err(PtyError::Io(IoError::last_os_error()));
    }
    Ok((read as isize, write as isize))
}

fn set_non_inheritable(handle: isize) {
    // SAFETY: handle is a live pipe handle owned by this module; the call only clears
    // the inherit bit.
    unsafe {
        SetHandleInformation(handle as HANDLE, HANDLE_FLAG_INHERIT, 0);
    }
}

fn stage_error(err: PtyError, stage: SpawnStage) -> PtyError {
    match err {
        PtyError::Io(io) => PtyError::Spawn {
            errno: io.raw_os_error().unwrap_or(0),
            stage,
        },
        other => other,
    }
}

/// Create the session Job Object with KILL_ON_JOB_CLOSE (DC-16). Failure refuses
/// spawn: a session without a job loses orphan cleanup.
fn create_job(policy: DetachPolicy) -> Result<isize, PtyError> {
    // SAFETY: null attributes and name is the documented anonymous-job form.
    let job = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
    if job.is_null() {
        return Err(PtyError::JobAttachDenied { code: last_error() });
    }
    // SAFETY: JOBOBJECT_EXTENDED_LIMIT_INFORMATION is a plain C struct; all-zero is a
    // valid initial state and the fields used below are written.
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    let mut flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
    if matches!(policy, DetachPolicy::Allow) {
        flags |= JOB_OBJECT_LIMIT_BREAKAWAY_OK;
    }
    info.BasicLimitInformation.LimitFlags = flags;
    info.BasicLimitInformation.ActiveProcessLimit = JOB_ACTIVE_PROCESS_LIMIT;
    // SAFETY: job is live; info is fully initialised and the size matches its type.
    let ok = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if ok == 0 {
        let code = last_error();
        close_handle(job as isize);
        return Err(PtyError::JobAttachDenied { code });
    }
    Ok(job as isize)
}

struct CodePageState {
    original_output: u32,
    original_input: u32,
    active: u32,
}

fn code_page_state() -> &'static Mutex<CodePageState> {
    static STATE: OnceLock<Mutex<CodePageState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(CodePageState {
            original_output: 0,
            original_input: 0,
            active: 0,
        })
    })
}

/// Record the original console code page and switch to UTF-8 (C-W5). Process
/// global, so the original is restored only when the last session closes.
fn code_page_acquire() -> u32 {
    // SAFETY: GetConsoleOutputCP / GetConsoleCP have no preconditions.
    let output = unsafe { GetConsoleOutputCP() };
    let input = unsafe { GetConsoleCP() };
    let mut state = lock(code_page_state());
    if state.active == 0 {
        state.original_output = output;
        state.original_input = input;
        if output != 0 && output != UTF8_CODE_PAGE {
            // SAFETY: the process console code page is process global; this is the
            // C-W5 mandated switch.
            unsafe {
                SetConsoleOutputCP(UTF8_CODE_PAGE);
            }
        }
        if input != 0 && input != UTF8_CODE_PAGE {
            // SAFETY: as above, for the input code page.
            unsafe {
                SetConsoleCP(UTF8_CODE_PAGE);
            }
        }
    }
    state.active += 1;
    output
}

fn code_page_release() {
    let mut state = lock(code_page_state());
    if state.active == 0 {
        return;
    }
    state.active -= 1;
    if state.active != 0 {
        return;
    }
    if state.original_output != 0 {
        // SAFETY: process-global code page restore.
        unsafe {
            SetConsoleOutputCP(state.original_output);
        }
    }
    if state.original_input != 0 {
        // SAFETY: process-global code page restore.
        unsafe {
            SetConsoleCP(state.original_input);
        }
    }
}

fn current_process_has_console() -> bool {
    // SAFETY: GetConsoleCP has no preconditions; it returns 0 without a console.
    unsafe { GetConsoleCP() != 0 }
}

fn release_spawn_resources(
    pty_input_read: isize,
    pty_output_write: isize,
    input_write: isize,
    output_read: isize,
    job: isize,
    hpc: HPCON,
) {
    close_handle(pty_input_read);
    close_handle(pty_output_write);
    close_handle(input_write);
    close_handle(output_read);
    close_handle(job);
    if hpc != 0 {
        // SAFETY: hpc was created by CreatePseudoConsole and is owned here.
        unsafe {
            ClosePseudoConsole(hpc);
        }
    }
}

/// Runtime probe for the native ConPTY path (kernel/02 section 6 downgrade).
#[must_use]
pub(crate) fn probe_available() -> bool {
    let Ok((input_read, input_write)) = create_pipe() else {
        return false;
    };
    let Ok((output_read, output_write)) = create_pipe() else {
        close_handle(input_read);
        close_handle(input_write);
        return false;
    };
    let mut hpc: HPCON = 0;
    let coord = COORD { X: 80, Y: 24 };
    // SAFETY: the pipe ends are live for the duration of the call and hpc is a valid
    // out-pointer.
    let hr = unsafe {
        CreatePseudoConsole(
            coord,
            input_read as HANDLE,
            output_write as HANDLE,
            0,
            &mut hpc,
        )
    };
    if hr >= 0 && hpc != 0 {
        // SAFETY: hpc was created by CreatePseudoConsole in this function.
        unsafe {
            ClosePseudoConsole(hpc);
        }
    }
    close_handle(input_read);
    close_handle(input_write);
    close_handle(output_read);
    close_handle(output_write);
    hr >= 0
}

/// ConPTY backend.
pub(crate) struct ConPtyBackend {
    caps: PtyCapabilities,
}

impl ConPtyBackend {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            caps: PtyCapabilities {
                resize: ResizeCap::Lossy,
                signals: SignalCap::ConsoleEvent,
                graphics_passthrough: false,
                byte_fidelity: Fidelity::F1(RULESET_CONPTY),
                job_control: true,
            },
        }
    }
}

impl Default for ConPtyBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyBackend for ConPtyBackend {
    fn capabilities(&self) -> PtyCapabilities {
        self.caps
    }

    fn spawn(&self, cmd: &Command, sz: WinSize, o: SpawnOpts) -> Result<PtyHandle, PtyError> {
        spawn_conpty(cmd, sz, o)
    }
}

fn spawn_conpty(cmd: &Command, sz: WinSize, o: SpawnOpts) -> Result<PtyHandle, PtyError> {
    if !sz.is_valid() {
        return Err(PtyError::Spawn {
            errno: 0,
            stage: SpawnStage::CreatePty,
        });
    }

    let original_cp = code_page_acquire();

    let (input_read, input_write) =
        create_pipe().map_err(|err| stage_error(err, SpawnStage::CreatePty))?;
    let (output_read, output_write) = match create_pipe() {
        Ok(pair) => pair,
        Err(err) => {
            close_handle(input_read);
            close_handle(input_write);
            return Err(stage_error(err, SpawnStage::CreatePty));
        }
    };
    set_non_inheritable(input_write);
    set_non_inheritable(output_read);

    let mut hpc: HPCON = 0;
    let coord = COORD {
        X: sz.cols as i16,
        Y: sz.rows as i16,
    };
    // SAFETY: the pty-side pipe ends are live; hpc is a valid out-pointer.
    let hr = unsafe {
        CreatePseudoConsole(
            coord,
            input_read as HANDLE,
            output_write as HANDLE,
            PSEUDOCONSOLE_RESIZE_QUIRK | PSEUDOCONSOLE_WIN32_INPUT_MODE,
            &mut hpc,
        )
    };
    if hr < 0 {
        close_handle(input_read);
        close_handle(input_write);
        close_handle(output_read);
        close_handle(output_write);
        code_page_release();
        return Err(PtyError::Spawn {
            errno: hr,
            stage: SpawnStage::CreatePty,
        });
    }
    // CreatePseudoConsole borrows these two handles for the lifetime of the console
    // (it does not duplicate them), so they stay open until close() runs
    // ClosePseudoConsole and then releases the pty-side ends.

    let job = match create_job(o.detach_policy) {
        Ok(job) => job,
        Err(err) => {
            release_spawn_resources(input_read, output_write, input_write, output_read, 0, hpc);
            code_page_release();
            return Err(err);
        }
    };

    let mut size: usize = 0;
    // SAFETY: null list with a valid size out-pointer is the documented size probe.
    unsafe {
        InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut size);
    }
    if size == 0 {
        release_spawn_resources(input_read, output_write, input_write, output_read, job, hpc);
        code_page_release();
        return Err(PtyError::Spawn {
            errno: last_error(),
            stage: SpawnStage::Bind,
        });
    }
    let mut list_buffer: Vec<usize> = vec![0; size.div_ceil(std::mem::size_of::<usize>())];
    let list = list_buffer.as_mut_ptr().cast::<core::ffi::c_void>();
    // SAFETY: list points to a buffer of at least size bytes as reported above.
    if unsafe { InitializeProcThreadAttributeList(list, 1, 0, &mut size) } == 0 {
        release_spawn_resources(input_read, output_write, input_write, output_read, job, hpc);
        code_page_release();
        return Err(PtyError::Spawn {
            errno: last_error(),
            stage: SpawnStage::Bind,
        });
    }
    // SAFETY: list is initialised; hpc is a live pseudo console; the attribute value
    // is an HPCON as required by PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE.
    let updated = unsafe {
        UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            // THE BUG. lpValue for PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE IS the HPCON
            // value, not a pointer to it: the Microsoft sample passes hPC and wezterm's
            // portable-pty passes its HPCON directly. Passing `&hpc` (the address of a
            // stack slot) makes Windows read that address AS the pseudoconsole handle, so
            // the attribute is worthless, the client is created without any console, and
            // it dies in console initialisation with 0xC0000142 having written 0 bytes.
            // That was the whole defect; the surrounding wiring was already correct.
            hpc as *const core::ffi::c_void,
            std::mem::size_of::<HPCON>(),
            ptr::null_mut(),
            ptr::null(),
        )
    };
    if updated == 0 {
        // SAFETY: list was initialised above.
        unsafe {
            DeleteProcThreadAttributeList(list);
        }
        release_spawn_resources(input_read, output_write, input_write, output_read, job, hpc);
        code_page_release();
        return Err(PtyError::Spawn {
            errno: last_error(),
            stage: SpawnStage::Bind,
        });
    }

    // SAFETY: STARTUPINFOEXW is a plain C struct; all-zero plus cb and the attribute
    // list pointer is a valid value.
    let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.lpAttributeList = list;
    // THE FIX for the 0xC0000142 defect. With bInheritHandles = FALSE and no
    // STARTF_USESTDHANDLES, the child receives the PARENT's standard handle values, which
    // were never inherited and are therefore invalid in the child. When the parent has
    // redirected stdio - cargo test, a CI runner, a daemonised process - cmd.exe is handed
    // dead standard handles, its CRT initialisation fails, and it dies with
    // STATUS_DLL_INIT_FAILED having written nothing. Pinning them to INVALID_HANDLE_VALUE
    // makes Windows wire the client's stdio to the pseudo console instead. wezterm's
    // portable-pty documents this exact failure mode.
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
    startup.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
    startup.StartupInfo.hStdError = INVALID_HANDLE_VALUE;

    let mut command_line = wide(&build_command_line(cmd));
    let cwd = o.cwd.as_ref().map(|path| wide_path(path));
    let env = build_env_block(&o.env);
    let env_ptr: *const core::ffi::c_void = match &env {
        Some(block) => block.as_ptr().cast(),
        None => ptr::null(),
    };
    let mut flags = EXTENDED_STARTUPINFO_PRESENT | CREATE_NEW_PROCESS_GROUP;
    if env.is_some() {
        flags |= CREATE_UNICODE_ENVIRONMENT;
    }
    // SAFETY: PROCESS_INFORMATION is a plain C struct; zeroed is valid.
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: command_line, cwd, env and startup are valid for the call; info is a
    // valid out-pointer. lpApplicationName is null so the first command-line token
    // is resolved through PATH.
    let created = unsafe {
        CreateProcessW(
            ptr::null(),
            command_line.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            FALSE,
            flags,
            env_ptr,
            cwd.as_ref().map_or(ptr::null(), |value| value.as_ptr()),
            &startup.StartupInfo,
            &mut info,
        )
    };
    // SAFETY: list is no longer used after CreateProcessW returned.
    unsafe {
        DeleteProcThreadAttributeList(list);
    }
    if created == 0 {
        release_spawn_resources(input_read, output_write, input_write, output_read, job, hpc);
        code_page_release();
        return Err(PtyError::Spawn {
            errno: last_error(),
            stage: SpawnStage::Spawn,
        });
    }
    close_handle(info.hThread as isize);

    // SAFETY: info.hProcess and job are live handles.
    if unsafe { AssignProcessToJobObject(job as HANDLE, info.hProcess) } == 0 {
        let code = last_error();
        // SAFETY: info.hProcess is live and owned here.
        unsafe {
            TerminateProcess(info.hProcess, 1);
        }
        close_handle(info.hProcess as isize);
        release_spawn_resources(input_read, output_write, input_write, output_read, job, hpc);
        code_page_release();
        return Err(PtyError::JobAttachDenied { code });
    }

    let tree = Arc::new(ConPtyTree {
        job: AtomicIsize::new(job),
        process: AtomicIsize::new(info.hProcess as isize),
        pid: info.dwProcessId,
        program: cmd.program.clone(),
        started: Instant::now(),
        reaped: Mutex::new(None),
    });
    let handle = Arc::new(ConPtyHandle {
        tree: Arc::clone(&tree),
        input: AtomicIsize::new(input_write),
        output: AtomicIsize::new(output_read),
        pty_input: AtomicIsize::new(input_read),
        pty_output: AtomicIsize::new(output_write),
        hpc: AtomicIsize::new(hpc as isize),
        closed: AtomicBool::new(false),
        eof: AtomicBool::new(false),
        pid: info.dwProcessId,
        original_cp,
    });
    let ops: Arc<dyn HandleOps> = handle;
    let tree_ops: Arc<dyn TreeOps> = tree;
    Ok(PtyHandle::new(ops, tree_ops))
}

struct ConPtyHandle {
    tree: Arc<ConPtyTree>,
    input: AtomicIsize,
    output: AtomicIsize,
    pty_input: AtomicIsize,
    pty_output: AtomicIsize,
    hpc: AtomicIsize,
    closed: AtomicBool,
    eof: AtomicBool,
    pid: u32,
    original_cp: u32,
}

impl HandleOps for ConPtyHandle {
    fn write(&self, data: &[u8]) -> Result<usize, PtyError> {
        if data.is_empty() {
            return Ok(0);
        }
        if self.closed.load(Ordering::SeqCst) {
            return Err(PtyError::Io(IoError::new(
                ErrorKind::BrokenPipe,
                "conpty input is closed",
            )));
        }
        let handle = self.input.load(Ordering::SeqCst);
        if handle == 0 {
            return Err(PtyError::NoSuchPty);
        }
        let mut written: u32 = 0;
        // SAFETY: handle is a live pipe write handle; data is a valid readable slice
        // and written is a valid out-pointer.
        let ok = unsafe {
            WriteFile(
                handle as HANDLE,
                data.as_ptr(),
                data.len().min(u32::MAX as usize) as u32,
                &mut written,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(PtyError::Io(IoError::last_os_error()));
        }
        Ok(written as usize)
    }

    fn read(&self, buf: &mut [u8]) -> Result<usize, PtyError> {
        if buf.is_empty() || self.closed.load(Ordering::SeqCst) || self.eof.load(Ordering::SeqCst) {
            return Ok(0);
        }
        let handle = self.output.load(Ordering::SeqCst);
        if handle == 0 {
            return Ok(0);
        }
        let mut read: u32 = 0;
        // SAFETY: handle is a live pipe read handle; buf is a valid writable slice and
        // read is a valid out-pointer.
        let ok = unsafe {
            ReadFile(
                handle as HANDLE,
                buf.as_mut_ptr(),
                buf.len().min(u32::MAX as usize) as u32,
                &mut read,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            let err = IoError::last_os_error();
            if is_eof_error(&err) {
                self.eof.store(true, Ordering::SeqCst);
                return Ok(0);
            }
            return Err(PtyError::Io(err));
        }
        if read == 0 {
            self.eof.store(true, Ordering::SeqCst);
        }
        Ok(read as usize)
    }

    fn resize(&self, sz: WinSize) -> Result<ResizeEffect, PtyError> {
        if !sz.is_valid() {
            return Err(PtyError::Unsupported("resize cols/rows must be >= 1"));
        }
        let hpc = self.hpc.load(Ordering::SeqCst);
        if hpc == 0 {
            return Err(PtyError::NoSuchPty);
        }
        let coord = COORD {
            X: sz.cols as i16,
            Y: sz.rows as i16,
        };
        // SAFETY: hpc is a live pseudo console handle owned by this session.
        let hr = unsafe { ResizePseudoConsole(hpc as HPCON, coord) };
        if hr < 0 {
            return Err(PtyError::Io(IoError::from_raw_os_error(hr)));
        }
        // W3 / C-W3: ConPTY only reflows newly produced content.
        Ok(ResizeEffect::AppliedLossy)
    }

    fn signal(&self, sig: Sig) -> Result<SignalOutcome, PtyError> {
        match sig {
            Sig::Int => {
                // K-03: the byte path is the default. GenerateConsoleCtrlEvent is best
                // effort and is skipped while this process owns a console, otherwise a
                // CTRL_C_EVENT for group 0 would signal our own process group.
                if !current_process_has_console() {
                    // SAFETY: we established this process has no console, so the event
                    // cannot reach our own process group.
                    if unsafe { GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0) } != 0 {
                        return Ok(SignalOutcome::Delivered);
                    }
                }
                self.write(&[0x03])?;
                Ok(SignalOutcome::ByteFallback(0x03))
            }
            Sig::Term => {
                let pid = self.pid;
                // SAFETY: the child was created with CREATE_NEW_PROCESS_GROUP, so its
                // process group id equals its pid; the call only generates an event.
                if pid != 0 && unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid) } != 0 {
                    return Ok(SignalOutcome::Delivered);
                }
                self.write(&[0x03])?;
                Ok(SignalOutcome::ByteFallback(0x03))
            }
            Sig::Kill => {
                let job = self.tree.job.load(Ordering::SeqCst);
                if job == 0 {
                    return Err(PtyError::NoSuchPty);
                }
                // SAFETY: job is a live job handle owned by the shared tree.
                unsafe {
                    TerminateJobObject(job as HANDLE, 1);
                }
                Ok(SignalOutcome::Delivered)
            }
            // kernel/02 section 3.5: these are Unsupported on Windows. CTRL_CLOSE_EVENT
            // is never folded into Sig::Int (C-W6).
            Sig::Hup | Sig::Quit | Sig::Usr1 | Sig::Usr2 | Sig::Winch | Sig::Stop | Sig::Cont => {
                Ok(SignalOutcome::Unsupported)
            }
        }
    }

    fn close(&self) -> Result<(), PtyError> {
        if self.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        // ClosePseudoConsole aborts any in-flight ReadFile (ERROR_OPERATION_ABORTED),
        // so a reader thread blocked in read() unblocks before we release handles.
        let hpc = self.hpc.swap(0, Ordering::SeqCst);
        if hpc != 0 {
            // SAFETY: hpc is live and owned here.
            unsafe {
                ClosePseudoConsole(hpc as HPCON);
            }
        }
        // Release the pty-side pipe ends now that the console no longer needs them.
        // Closing the write end makes the output pipe reach EOF for readers.
        close_handle(self.pty_input.swap(0, Ordering::SeqCst));
        close_handle(self.pty_output.swap(0, Ordering::SeqCst));
        code_page_release();
        Ok(())
    }

    fn original_code_page(&self) -> Option<u32> {
        Some(self.original_cp)
    }
}

impl Drop for ConPtyHandle {
    fn drop(&mut self) {
        let _ = HandleOps::close(self);
        // Handles are closed only once the last Arc drops, so an in-flight read that
        // was aborted by ClosePseudoConsole can observe EOF before the handle goes.
        close_handle(self.input.swap(0, Ordering::SeqCst));
        close_handle(self.output.swap(0, Ordering::SeqCst));
    }
}

struct ConPtyTree {
    job: AtomicIsize,
    process: AtomicIsize,
    pid: u32,
    program: String,
    started: Instant,
    reaped: Mutex<Option<ExitInfo>>,
}

impl ConPtyTree {
    /// Best-effort process details. ppid is left at 0: resolving parent pids needs
    /// the Toolhelp snapshot API, which the workspace feature set does not enable.
    fn entry_for(&self, pid: u32) -> ProcEntry {
        let mut name = String::new();
        let mut start_time = 0u64;
        let mut cpu_ms = 0u64;
        // SAFETY: OpenProcess returns null on failure.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if !process.is_null() {
            let mut buffer = vec![0u16; 512];
            let mut size = buffer.len() as u32;
            // SAFETY: process is a live query handle; buffer and size are valid.
            let named =
                unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut size) };
            if named != 0 {
                name = String::from_utf16_lossy(&buffer[..size as usize]);
            }
            // SAFETY: FILETIME is a plain C struct; an all-zero value is valid.
            let mut creation: FILETIME = unsafe { std::mem::zeroed() };
            let mut exit: FILETIME = unsafe { std::mem::zeroed() };
            let mut kernel: FILETIME = unsafe { std::mem::zeroed() };
            let mut user: FILETIME = unsafe { std::mem::zeroed() };
            // SAFETY: process is live and all four out-pointers are valid.
            if unsafe { GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) }
                != 0
            {
                start_time = filetime_to_u64(creation);
                cpu_ms = (filetime_to_u64(kernel) + filetime_to_u64(user)) / 10_000;
            }
            // SAFETY: process is owned by this function and is not used afterwards.
            unsafe {
                CloseHandle(process);
            }
        }
        if name.is_empty() && pid == self.pid {
            name.clone_from(&self.program);
        }
        ProcEntry {
            pid,
            ppid: 0,
            name,
            start_time,
            cpu_ms,
        }
    }
}

impl ConPtyTree {
    /// Reap the root process and record the exit. `forced_signal` is Some(Kill) when
    /// the tree was terminated on purpose.
    fn reap_root(&self, forced_signal: Option<Sig>) -> ExitInfo {
        let process = self.process.swap(0, Ordering::SeqCst);
        let mut code: u32 = 0;
        let mut have_code = false;
        if process != 0 {
            // SAFETY: process is live and code is a valid out-pointer.
            if unsafe { GetExitCodeProcess(process as HANDLE, &mut code) } != 0 {
                have_code = true;
            }
            close_handle(process);
        }
        let info = ExitInfo {
            code: if have_code { Some(code as i32) } else { None },
            signal: forced_signal,
            reaped: true,
            live_children: self.live_children(),
            wall: self.started.elapsed(),
        };
        *lock(&self.reaped) = Some(info);
        info
    }
}

impl Drop for ConPtyTree {
    fn drop(&mut self) {
        close_handle(self.process.swap(0, Ordering::SeqCst));
        // Closing a job created with JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE reaps the whole
        // tree (DC-16 / kernel/02 section 3.3, orphan cleanup main path).
        close_handle(self.job.swap(0, Ordering::SeqCst));
    }
}

fn filetime_to_u64(value: FILETIME) -> u64 {
    ((value.dwHighDateTime as u64) << 32) | u64::from(value.dwLowDateTime)
}

impl TreeOps for ConPtyTree {
    fn snapshot(&self) -> Result<Vec<ProcEntry>, PtyError> {
        let job = self.job.load(Ordering::SeqCst);
        if job == 0 {
            return Ok(Vec::new());
        }
        let mut capacity: usize = 64;
        loop {
            let mut buffer: Vec<usize> = vec![0; capacity + 1];
            let mut returned: u32 = 0;
            let size = ((capacity + 1) * std::mem::size_of::<usize>()) as u32;
            // SAFETY: buffer is usize-aligned and at least size bytes; the kernel writes
            // a JOBOBJECT_BASIC_PROCESS_ID_LIST header followed by pid usizes.
            let ok = unsafe {
                QueryInformationJobObject(
                    job as HANDLE,
                    JobObjectBasicProcessIdList,
                    buffer.as_mut_ptr().cast(),
                    size,
                    &mut returned,
                )
            };
            if ok == 0 {
                let err = IoError::last_os_error();
                if err.raw_os_error() == Some(ERROR_MORE_DATA) && capacity < 4096 {
                    capacity *= 2;
                    continue;
                }
                if is_eof_error(&err) {
                    return Ok(Vec::new());
                }
                return Err(PtyError::Io(err));
            }
            // SAFETY: the kernel wrote a JOBOBJECT_BASIC_PROCESS_ID_LIST at the start of
            // the buffer.
            let count = unsafe {
                (*(buffer.as_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>()))
                    .NumberOfProcessIdsInList as usize
            };
            if count > capacity {
                capacity = count;
                continue;
            }
            let mut entries = Vec::with_capacity(count);
            for index in 0..count {
                let base = buffer.as_ptr().cast::<u8>();
                // SAFETY: the kernel wrote count usizes starting at offset 8 (two u32
                // header fields plus alignment).
                let pid = unsafe {
                    *(base
                        .add(8 + index * std::mem::size_of::<usize>())
                        .cast::<usize>()) as u32
                };
                entries.push(self.entry_for(pid));
            }
            return Ok(entries);
        }
    }

    fn live_children(&self) -> u32 {
        match self.snapshot() {
            Ok(entries) => entries.len() as u32,
            Err(_) => 0,
        }
    }

    fn kill(&self, mode: KillMode) -> Result<ExitInfo, PtyError> {
        if let Some(info) = *lock(&self.reaped) {
            return Ok(info);
        }
        if let KillMode::Graceful(grace) = mode {
            let pid = self.pid;
            if pid != 0 {
                // SAFETY: the child was created with CREATE_NEW_PROCESS_GROUP, so its
                // process group id equals its pid.
                unsafe {
                    GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid);
                }
            }
            let deadline = Instant::now() + grace;
            while Instant::now() < deadline {
                if self.live_children() == 0 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        let job = self.job.load(Ordering::SeqCst);
        if job != 0 {
            // SAFETY: job is a live job handle owned by this tree.
            unsafe {
                TerminateJobObject(job as HANDLE, 1);
            }
        }
        let process = self.process.swap(0, Ordering::SeqCst);
        let mut code: u32 = 0;
        let mut have_code = false;
        if process != 0 {
            // SAFETY: process is live and owned here until it is closed below.
            let waited = unsafe { WaitForSingleObject(process as HANDLE, JOB_WAIT_MS) };
            if waited == WAIT_OBJECT_0 {
                // SAFETY: process is live and code is a valid out-pointer.
                if unsafe { GetExitCodeProcess(process as HANDLE, &mut code) } != 0 {
                    have_code = true;
                }
            }
            close_handle(process);
        }
        let info = ExitInfo {
            code: if have_code { Some(code as i32) } else { None },
            signal: Some(Sig::Kill),
            reaped: true,
            live_children: self.live_children(),
            wall: self.started.elapsed(),
        };
        *lock(&self.reaped) = Some(info);
        Ok(info)
    }

    fn wait(&self, to: WaitTimeout) -> Result<ExitInfo, PtyError> {
        if let Some(info) = *lock(&self.reaped) {
            return Ok(info);
        }
        if self.job.load(Ordering::SeqCst) == 0 {
            return Err(PtyError::NoSuchPty);
        }
        let deadline = match to {
            WaitTimeout::Zero => Some(Instant::now()),
            WaitTimeout::Millis(ms) => Some(Instant::now() + Duration::from_millis(ms)),
            WaitTimeout::Infinite => None,
        };
        loop {
            // The job event is not raised promptly on every Windows build, so we poll
            // the job's process-id list instead (kernel/02 section 3.3 explicitly
            // allows the single-pid fallback). Zero live processes means reaped.
            if self.live_children() == 0 {
                return Ok(self.reap_root(None));
            }
            if let Some(limit) = deadline {
                if Instant::now() >= limit {
                    return Err(PtyError::Timeout {
                        pid: self.pid,
                        after: timeout_duration(to),
                    });
                }
            }
            let process = self.process.load(Ordering::SeqCst);
            if process != 0 {
                // A bounded wait on the root process doubles as the poll interval.
                // SAFETY: process is a live handle owned by this tree.
                unsafe {
                    WaitForSingleObject(process as HANDLE, 200);
                }
            } else {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
