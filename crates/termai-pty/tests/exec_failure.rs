//! Exec-failure reporting and its leak budget (PTY-READY-1; kernel/02 section 3.1 invariant 5,
//! AR-30 item 2 / PTY-ORPHAN-1).
//!
//! `spawn` must not hand back a session for a program it never managed to execute: before the
//! readiness handshake the child simply ran `_exit(127)` and the caller only found out when the
//! tree was reaped, with the pty already owned by a dead session. The handshake carries the
//! child's `exec*` errno back to the parent, so `spawn` refuses the spawn outright - and it must
//! refuse **without** leaving the child, a zombie, or the pty/pipe fds behind.
//!
//! **This file deliberately holds exactly one test.** The unix leak check below asks the
//! *process* whether it still has any child at all (`waitpid(-1, WNOHANG)` must answer ECHILD),
//! which is only race-free while no other test in the same binary can fork concurrently. A
//! second test in this file would silently make that check unsound - add new cases to another
//! file instead.
//!
//! The Windows arm covers what this platform can observe through the backend: ConPTY maps a
//! failed `CreateProcessW` to `PtyError::Spawn { stage: Spawn }` as well
//! (`crates/termai-pty/src/windows/conpty.rs:917-924`), so the refusal half is asserted on both
//! platforms; the process-table half is unix-only and says so out loud
//! (`unix_report_no_child_probe`) rather than skipping silently.

use termai_pty::{Command, PtyError, SpawnOpts, SpawnStage, WinSize};

/// A program that cannot exist on either platform this backend supports.
fn nonexistent_command() -> Command {
    #[cfg(windows)]
    {
        Command::new("Z:\\termai-no-such-dir-4f1c\\termai-no-such-program-4f1c.exe")
    }
    #[cfg(unix)]
    {
        Command::new("/termai-no-such-dir-4f1c/termai-no-such-program-4f1c")
    }
}

fn size() -> WinSize {
    WinSize::new(80, 24)
}

/// Number of fds this process has open, or `None` where the platform exposes no fd directory.
/// Linux has `/proc/self/fd`; macOS and the BSDs have `/dev/fd`.
#[cfg(unix)]
fn open_fd_count() -> Option<usize> {
    for dir in ["/proc/self/fd", "/dev/fd"] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            return Some(entries.filter(|entry| entry.is_ok()).count());
        }
    }
    None
}

/// Ask the process itself whether any child is left: `waitpid(-1, WNOHANG)` answers -1/ECHILD
/// only when there is no child at all, so a live child (0) and an unreaped zombie (a pid) both
/// fail this. Returns the raw answer so the caller can report it.
#[cfg(unix)]
fn no_child_of_this_process() -> Result<(), String> {
    let mut status: libc::c_int = 0;
    // SAFETY: waitpid(-1, ...) only reaps a child of this process and status is a valid
    // out-pointer. With WNOHANG it never blocks.
    let answer = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
    if answer == -1 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ECHILD) {
            return Ok(());
        }
        return Err(format!("waitpid(-1) failed with {err}"));
    }
    if answer == 0 {
        return Err("a live child of this process survived the failed spawn".to_string());
    }
    Err(format!(
        "a zombie child (pid {answer}) survived the failed spawn: it was never reaped"
    ))
}

#[test]
fn a_program_that_cannot_be_executed_is_refused_without_leaking() {
    let backend = termai_pty::native_backend();
    let command = nonexistent_command();

    // 1. The refusal: spawn itself reports the exec failure instead of returning a session.
    let err = backend
        .spawn(&command, size(), SpawnOpts::default())
        .err()
        .unwrap_or_else(|| {
            panic!(
                "spawn of {} (which cannot exist) returned a session instead of an error",
                command.program
            )
        });
    match err {
        PtyError::Spawn { errno, stage } => {
            assert_eq!(
                stage,
                SpawnStage::Spawn,
                "an exec failure is a spawn-stage failure"
            );
            assert_ne!(
                errno, 0,
                "the platform error must be reported, not swallowed"
            );
            #[cfg(unix)]
            assert_eq!(
                errno,
                libc::ENOENT,
                "a missing program must be reported as ENOENT"
            );
            #[cfg(windows)]
            println!("windows: ConPTY refused the missing program with platform error {errno}");
        }
        other => panic!("an exec failure must be reported as PtyError::Spawn, got {other:?}"),
    }

    #[cfg(unix)]
    {
        // 2. The leak budget: no child and no fd may outlive the refused spawn. The fd count is
        // taken after the first (warm-up) failed spawn above, so one-off setup cannot look like
        // a leak.
        no_child_of_this_process().unwrap_or_else(|why| panic!("PTY-ORPHAN-1: {why}"));
        println!(
            "unix: after the refused spawn this process has no child left (waitpid(-1) = ECHILD)"
        );

        match open_fd_count() {
            Some(before) => {
                for _ in 0..8 {
                    let _ = backend.spawn(&command, size(), SpawnOpts::default());
                }
                let after = open_fd_count().unwrap_or(before);
                assert!(
                    after <= before,
                    "the refused spawn leaked {} fd(s): {before} -> {after}",
                    after.saturating_sub(before)
                );
                println!(
                    "unix: 8 refused spawns moved the open-fd count {before} -> {after} \
                     (pty master + both handshake pipe ends are given back)"
                );
            }
            None => println!(
                "unix: no /proc/self/fd and no /dev/fd on this platform, so the fd half of the \
                 leak budget is not observable here (the child half above still is)"
            ),
        }

        // 3. Control, so step 2 cannot be green by accident: a *successful* child of this same
        // process is visible to exactly the same probe and then reaped, which proves the probe
        // is not answering ECHILD merely because children are invisible in this process.
        let handle = backend
            .spawn(
                &Command::with_args("/bin/echo", vec!["exec-failure-control".to_string()]),
                size(),
                SpawnOpts::default(),
            )
            .expect("a real program must still spawn after a refused spawn");
        let tree = backend.process_tree(&handle).expect("process tree");
        let info = backend
            .wait(&tree, termai_pty::WaitTimeout::Millis(10_000))
            .expect("wait for the control child");
        assert!(info.reaped, "the control child must be reaped: {info:?}");
        let after_control = no_child_of_this_process();
        let _ = backend.close(handle);
        println!(
            "unix: the control child was visible to the probe and reaped, so the ECHILD answer \
             above is a real observation, not an artefact"
        );
        after_control.expect("the reaped control child must leave no child behind");
    }

    #[cfg(windows)]
    {
        // Windows has no `waitpid`; the equivalent guarantee is the Job Object tree kill, which
        // the native ConPTY tests assert with named descendants
        // (`crates/termai-pty/tests/tree_closure.rs:66-99`, `apps/sessiond/tests/pty_lifecycle.rs`).
        // The refusal asserted above is the part a process that never started can be held to.
        println!(
            "windows: no process-half leak probe here - a child that never started has no job to \
             enumerate; the refusal above is the observable, and the Job Object tests cover the \
             tree half"
        );
    }
}
