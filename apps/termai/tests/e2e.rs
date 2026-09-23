//! M0 end-to-end acceptance: the real binary spawns a real program under a real PTY,
//! parses VT, writes a Session Log, and reports honest numbers.
//!
//! This is the test that must pass for M0 to be called delivered.

use std::path::PathBuf;
use std::process::Command;

fn termai_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_termai"))
}

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("termai-e2e-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// The program to run and its arguments, per platform.
fn echo_command() -> Vec<String> {
    if cfg!(windows) {
        vec![
            "cmd.exe".to_string(),
            "/c".to_string(),
            "echo termai-m0-e2e".to_string(),
        ]
    } else {
        vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "echo termai-m0-e2e".to_string(),
        ]
    }
}

#[test]
fn version_command_works() {
    let out = Command::new(termai_bin())
        .arg("version")
        .output()
        .expect("run termai version");
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.starts_with("termai "), "unexpected version output: {s}");
}

#[test]
fn run_spawns_parses_logs_and_reports_f0() {
    let dir = tmp_dir("run");
    let mut args: Vec<String> = vec![
        "run".to_string(),
        "--cols".to_string(),
        "80".to_string(),
        "--rows".to_string(),
        "24".to_string(),
        "--log-dir".to_string(),
        dir.display().to_string(),
        "--".to_string(),
    ];
    args.extend(echo_command());

    let out = Command::new(termai_bin())
        .args(&args)
        .output()
        .expect("run termai run");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    assert!(
        stdout.contains("termai-m0-e2e"),
        "the program output must reach the grid.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("f0=true"),
        "bytes read from the PTY must equal bytes fed to the VT parser.\n{stdout}"
    );
    assert!(
        stdout.contains("fed_to_vt="),
        "the F0 evidence must be reported even on failure.\n{stdout}"
    );
    assert!(
        stdout.contains("grid digest: "),
        "a grid digest must be reported.\n{stdout}"
    );
    assert!(
        out.status.success(),
        "a successful program must exit 0. status={:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        out.status
    );
    assert!(
        stdout.contains("log: ") && stdout.contains("records)"),
        "the Session Log path and record count must be reported.\n{stdout}"
    );

    // The Session Log really exists and contains the raw bytes.
    let logdump = Command::new(termai_bin())
        .args(["logdump", &dir.display().to_string()])
        .output()
        .expect("run termai logdump");
    let dump = String::from_utf8_lossy(&logdump.stdout).to_string();
    assert!(
        dump.contains("PtyOut"),
        "the Log must contain PtyOut records.\n{dump}"
    );
    assert!(
        dump.contains("CheckpointRef"),
        "the Log must contain a CheckpointRef so the screen can be recovered.\n{dump}"
    );
    assert!(
        dump.contains("StateChange"),
        "the Log must contain StateChange records.\n{dump}"
    );
    assert!(
        !dump.contains("TAIL DAMAGED"),
        "a clean run must not produce a damaged tail.\n{dump}"
    );
    assert!(
        logdump.status.success(),
        "logdump of a clean log must exit 0, got {:?}",
        logdump.status
    );
}

/// The degraded path must still carry the whole chain (spawn -> bytes -> VT -> Log) and
/// must announce itself. A pipe is not a TTY, and the CLI must never pretend otherwise.
#[test]
fn pipe_backend_runs_the_chain_and_announces_the_degradation() {
    let dir = tmp_dir("pipe");
    let mut args: Vec<String> = vec![
        "run".to_string(),
        "--backend".to_string(),
        "pipe".to_string(),
        "--log-dir".to_string(),
        dir.display().to_string(),
        "--".to_string(),
    ];
    args.extend(echo_command());

    let out = Command::new(termai_bin())
        .args(&args)
        .output()
        .expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("WARNING"),
        "the degradation must be announced on stderr.\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("termai-m0-e2e"),
        "the pipe backend must still deliver bytes to the grid.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("f0=true"),
        "F0 (read bytes == fed bytes) must hold on the pipe path too.\n{stdout}"
    );
}

#[test]
fn a_failing_program_reports_its_exit_code_not_zero() {
    let dir = tmp_dir("fail");
    let cmd = if cfg!(windows) {
        vec![
            "cmd.exe".to_string(),
            "/c".to_string(),
            "exit 7".to_string(),
        ]
    } else {
        vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "exit 7".to_string(),
        ]
    };
    let mut args: Vec<String> = vec![
        "run".to_string(),
        "--no-log".to_string(),
        "--log-dir".to_string(),
        dir.display().to_string(),
        "--".to_string(),
    ];
    args.extend(cmd);

    let out = Command::new(termai_bin())
        .args(&args)
        .output()
        .expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(
        out.status.code(),
        Some(7),
        "the child exit code must be propagated, never beautified (AR-16).\n{stdout}"
    );
    assert!(
        stdout.contains("exit: code=7"),
        "the exit code must be reported verbatim.\n{stdout}"
    );
    assert!(
        stdout.contains("log: disabled"),
        "no log means no silent file"
    );
}

#[test]
fn logdump_of_an_unknown_directory_fails_with_a_clear_code() {
    let dir = tmp_dir("missing");
    let _ = std::fs::remove_dir_all(&dir);
    let out = Command::new(termai_bin())
        .args(["logdump", &dir.display().to_string()])
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn run_rejects_bad_options_without_spawning_anything() {
    let out = Command::new(termai_bin())
        .args(["run", "--cols", "abc"])
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("--cols"),
        "the error must name the option: {err}"
    );
}
