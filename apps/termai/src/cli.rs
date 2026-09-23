//! CLI argument handling, the headless terminal runner, and log inspection.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use termai_pty::{
    native_backend, Command, DetachPolicy, EnvPolicy, KillMode, PtyBackend, SpawnOpts, WaitTimeout,
    WinSize,
};
use termai_session::log::{
    list_segments, read_segment, FlushMode, Record, SegmentWriter, Source, SEG_FLAG_RAW_RING,
};
use termai_vt::Terminal;

pub const USAGE: &str = "\
termai - terminal-first AI workspace (headless M0)

USAGE:
  termai run [OPTIONS] [-- PROGRAM [ARGS...]]
      --cols N          initial columns (default 80)
      --rows N          initial rows (default 24)
      --log-dir DIR     Session Log directory (default .termai/logs)
      --no-log          do not write a Session Log
      --shell PATH      explicit shell/program to run
      --backend KIND    native (default) or pipe. pipe is the degraded fallback:
                        no TTY semantics, no resize, no signals, declared as such
      --timeout SECS    kill the session after SECS (default 30 for a command,
                        0 = no limit; an interactive shell defaults to no limit)
  termai logdump <log-dir>     print the Session Log as structured records
  termai version
  termai help

Exit codes: 0 ok, 1 partial (damaged log tail or live children), 2 usage/IO error,
            3 permission denied, 4 version incompatible (kernel/07 section 3.3).
";

/// Command dispatch. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let Some(cmd) = args.first().map(String::as_str) else {
        print!("{USAGE}");
        return 0;
    };
    match cmd {
        "version" | "--version" | "-V" => {
            println!("termai {}", env!("CARGO_PKG_VERSION"));
            0
        }
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            0
        }
        "run" => match run_session(&args[1..]) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("run: {e}");
                2
            }
        },
        "logdump" => match args.get(1) {
            Some(dir) => logdump(Path::new(dir)),
            None => {
                eprintln!("logdump: missing <log-dir>");
                2
            }
        },
        other => {
            eprintln!("unknown command: {other}");
            print!("{USAGE}");
            2
        }
    }
}

// ---------------------------------------------------------------------------
// termai run
// ---------------------------------------------------------------------------

/// Which PTY backend to use. The pipe backend is a declared degradation, never a
/// silent substitution: the caller has to ask for it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BackendKind {
    /// Native pty: ConPTY on Windows, forkpty elsewhere.
    #[default]
    Native,
    /// Pipe fallback: no TTY semantics, no resize, no signals.
    Pipe,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RunOpts {
    pub cols: u16,
    pub rows: u16,
    pub log_dir: PathBuf,
    pub write_log: bool,
    pub shell: Option<String>,
    pub argv: Vec<String>,
    pub timeout: Duration,
    /// True when the user passed --timeout explicitly (0 means "no limit").
    pub timeout_explicit: bool,
    pub backend: BackendKind,
}

impl Default for RunOpts {
    fn default() -> Self {
        Self {
            cols: 80,
            rows: 24,
            log_dir: PathBuf::from(".termai/logs"),
            write_log: true,
            shell: None,
            argv: Vec::new(),
            timeout: Duration::from_secs(30),
            timeout_explicit: false,
            backend: BackendKind::Native,
        }
    }
}

/// Parse run arguments. Errors are user-facing strings with an actionable hint.
pub fn parse_run(args: &[String]) -> Result<RunOpts, String> {
    let mut o = RunOpts::default();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--" => {
                o.argv = args[i + 1..].to_vec();
                break;
            }
            "--cols" => {
                let v = args.get(i + 1).ok_or("--cols needs a value")?;
                o.cols = v
                    .parse()
                    .map_err(|_| format!("--cols: not a number: {v}"))?;
                i += 2;
            }
            "--rows" => {
                let v = args.get(i + 1).ok_or("--rows needs a value")?;
                o.rows = v
                    .parse()
                    .map_err(|_| format!("--rows: not a number: {v}"))?;
                i += 2;
            }
            "--log-dir" => {
                let v = args.get(i + 1).ok_or("--log-dir needs a value")?;
                o.log_dir = PathBuf::from(v);
                i += 2;
            }
            "--no-log" => {
                o.write_log = false;
                i += 1;
            }
            "--shell" => {
                let v = args.get(i + 1).ok_or("--shell needs a value")?;
                o.shell = Some(v.clone());
                i += 2;
            }
            "--backend" => {
                let v = args.get(i + 1).ok_or("--backend needs a value")?;
                o.backend = match v.as_str() {
                    "native" => BackendKind::Native,
                    "pipe" => BackendKind::Pipe,
                    other => {
                        return Err(format!("--backend: expected native or pipe, got {other}"));
                    }
                };
                i += 2;
            }
            "--timeout" => {
                let v = args.get(i + 1).ok_or("--timeout needs a value")?;
                let secs: u64 = v
                    .parse()
                    .map_err(|_| format!("--timeout: not a number of seconds: {v}"))?;
                o.timeout = Duration::from_secs(secs);
                o.timeout_explicit = true;
                i += 2;
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option: {other}"));
            }
            _other => {
                // A bare program implies the caller's argv, e.g. termai run -- ls.
                o.argv = args[i..].to_vec();
                break;
            }
        }
    }
    if o.cols == 0 || o.rows == 0 {
        return Err("--cols/--rows must be greater than zero".to_string());
    }
    Ok(o)
}

fn default_shell() -> String {
    if cfg!(windows) {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}

fn hex32(b: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for byte in b {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

/// Run a real program under a real PTY, feed every byte straight into the VT parser,
/// and write the Session Log. Returns the process exit code.
pub fn run_session(args: &[String]) -> Result<i32, String> {
    let opts = parse_run(args)?;
    let backend: Arc<dyn PtyBackend> = match opts.backend {
        BackendKind::Native => Arc::from(native_backend()),
        BackendKind::Pipe => {
            // Declared degradation, printed so the user is never misled (AR-20).
            eprintln!(
                "run: WARNING: --backend pipe: no TTY semantics, no resize, no signals; \
                 fidelity is declared by the backend capability set"
            );
            Arc::from(termai_pty::pipe_backend())
        }
    };
    let caps = backend.capabilities();

    let (program, argv) = match (&opts.shell, opts.argv.is_empty()) {
        (Some(s), _) => (s.clone(), opts.argv.clone()),
        (None, false) => (opts.argv[0].clone(), opts.argv[1..].to_vec()),
        (None, true) => (default_shell(), Vec::new()),
    };

    // An interactive shell with no command must never be killed by a default timer;
    // a one-shot command gets a hard deadline so a blocking PTY read cannot hang.
    let interactive = opts.argv.is_empty() && opts.shell.is_none();
    let timeout = if opts.timeout_explicit {
        opts.timeout
    } else if interactive {
        Duration::ZERO
    } else {
        Duration::from_secs(30)
    };

    let cmd = Command::with_args(program.clone(), argv);
    let sz = WinSize::new(opts.cols, opts.rows);
    let spawn_opts = SpawnOpts {
        env: EnvPolicy::Inherit,
        cwd: None,
        login: false,
        latency_budget: Duration::from_millis(5_000),
        detach_policy: DetachPolicy::Allow,
    };

    let started = now_ns();
    let mut writer = if opts.write_log {
        Some(
            SegmentWriter::create(&opts.log_dir, 0, 0, [0u8; 8], started, SEG_FLAG_RAW_RING)
                .map_err(|e| format!("cannot create the Session Log: {e}"))?,
        )
    } else {
        None
    };

    let pty = backend
        .spawn(&cmd, sz, spawn_opts)
        .map_err(|e| format!("spawn failed: {e:?}"))?;
    let tree = backend
        .process_tree(&pty)
        .map_err(|e| format!("process tree unavailable: {e:?}"))?;

    if let Some(w) = writer.as_mut() {
        let _ = w.append(
            &Record::StateChange {
                from: 0,
                to: 1,
                reason: 0,
                exit_code: None,
            },
            now_ns(),
        );
        let _ = w.flush(FlushMode::FsyncData);
    }

    // Watchdog: force-kill the process tree after the deadline, which closes the pty
    // and guarantees the read loop terminates. Without it a blocking read can hang.
    let stop = Arc::new(AtomicBool::new(false));
    let timed_out = Arc::new(AtomicBool::new(false));
    // ConPTY does not signal EOF when the child exits: the pseudo console has to be
    // closed before a blocked read returns. The watchdog therefore watches the process
    // tree, not just a deadline:
    //   - children gone  -> close the pty, which turns the pending read into EOF
    //   - deadline hit   -> force-kill the tree, then close the pty
    // This keeps a one-shot command from hanging and still lets an interactive shell
    // own the terminal until it exits (timeout 0 means no deadline, not no watchdog).
    let watchdog = {
        let backend_wd = Arc::clone(&backend);
        let tree_wd = tree.clone();
        let handle_wd = pty.clone();
        let stop_wd = Arc::clone(&stop);
        let fired_wd = Arc::clone(&timed_out);
        std::thread::spawn(move || {
            let step = Duration::from_millis(50);
            let grace = Duration::from_millis(300);
            let mut waited = Duration::ZERO;
            loop {
                if stop_wd.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(step);
                waited += step;
                if waited >= grace && tree_wd.live_children() == 0 {
                    let _ = backend_wd.close(handle_wd);
                    return;
                }
                if !timeout.is_zero() && waited >= timeout {
                    fired_wd.store(true, Ordering::Relaxed);
                    let _ = backend_wd.kill(&tree_wd, KillMode::Force);
                    let _ = backend_wd.close(handle_wd);
                    return;
                }
            }
        })
    };

    let mut term = Terminal::new(opts.cols, opts.rows);
    let mut buf = [0u8; 16 * 1024];
    let mut read_bytes = 0usize;
    let mut fed_bytes = 0usize;

    loop {
        match backend.read(&pty, &mut buf) {
            Ok(0) => break,
            Ok(n) => {
                read_bytes += n;
                // F0: the bytes read are handed to the parser unmodified, and the Log
                // stores exactly the same bytes (kernel/02 section 3.6).
                term.feed(&buf[..n]);
                fed_bytes += n;
                if let Some(w) = writer.as_mut() {
                    let _ = w.append(
                        &Record::PtyOut {
                            pane: 0,
                            bytes: buf[..n].to_vec(),
                        },
                        now_ns(),
                    );
                }
            }
            Err(e) => {
                eprintln!("run: pty read stopped: {e:?}");
                break;
            }
        }
    }

    stop.store(true, Ordering::Relaxed);
    let _ = watchdog.join();
    let timed_out = timed_out.load(Ordering::Relaxed);

    let exit = backend
        .wait(&tree, WaitTimeout::Millis(10_000))
        .map_err(|e| format!("wait failed: {e:?}"))?;

    let snapshot = term.snapshot();
    let digest = term.grid().digest();

    let mut records = 0usize;
    let mut log_path: Option<PathBuf> = None;
    if let Some(mut w) = writer {
        let _ = w.append(
            &Record::StateChange {
                from: 1,
                to: 3,
                reason: 0,
                exit_code: exit.code,
            },
            now_ns(),
        );
        let _ = w.append(
            &Record::CheckpointRef {
                ckpt_id: 1,
                segment_id: w.segment_id(),
                offset: u32::try_from(w.bytes_written()).unwrap_or(u32::MAX),
                grid_digest: digest,
            },
            now_ns(),
        );
        let _ = w.flush(FlushMode::FsyncData);
        log_path = Some(w.path().to_path_buf());
        drop(w);
        if let Some(p) = &log_path {
            if let Ok(read) = read_segment(p) {
                records = read.records.len();
            }
        }
    }

    // Report exactly what happened; no invented numbers.
    println!(
        "termai run: program={} size={}x{} fidelity={:?} backend={}",
        program,
        opts.cols,
        opts.rows,
        caps.byte_fidelity,
        term.backend_label()
    );
    println!("--- grid ---");
    let last_non_empty = (0..snapshot.rows)
        .rev()
        .find(|r| !snapshot.row_text(*r).is_empty())
        .map_or(0, |r| r + 1);
    for row in 0..last_non_empty {
        println!("{}", snapshot.row_text(row));
    }
    println!("--- end grid ---");
    match exit.code {
        Some(c) => println!("exit: code={c} reaped={}", exit.reaped),
        None => println!("exit: signal={:?} reaped={}", exit.signal, exit.reaped),
    }
    println!(
        "bytes: read={read_bytes} fed_to_vt={fed_bytes} f0={}",
        read_bytes == fed_bytes
    );
    println!("live_children: {}", exit.live_children);
    if timed_out {
        println!("timeout: killed after {}s", timeout.as_secs());
    }
    println!("grid digest: {}", hex32(&digest));
    match log_path {
        Some(p) => println!("log: {} ({records} records)", p.display()),
        None => println!("log: disabled (--no-log)"),
    }

    // A native session that produced zero bytes is a backend failure, not a silent
    // empty screen. Say so, and point at the declared degraded path.
    if read_bytes == 0 && opts.backend == BackendKind::Native {
        eprintln!(
            "run: the native pty produced no output (exit={:?}). This is a backend \
             failure, not an empty program. Re-run with --backend pipe for the declared \
             degraded path, and report the session as failed.",
            exit.code
        );
        return Ok(exit.code.unwrap_or(1).max(1));
    }

    // Orphan check (kernel/02 PTY-ORPHAN-1): report, never hide.
    if exit.live_children > 0 {
        eprintln!(
            "run: warning: {} child process(es) still alive after the session ended",
            exit.live_children
        );
        return Ok(1);
    }
    Ok(exit.code.unwrap_or(0))
}

// ---------------------------------------------------------------------------
// termai logdump
// ---------------------------------------------------------------------------

/// Dump the Session Log in a stable, greppable text form. A damaged tail is reported.
pub fn logdump(dir: &Path) -> i32 {
    let segments = match list_segments(dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("logdump: cannot list segments: {e}");
            return 2;
        }
    };
    if segments.is_empty() {
        eprintln!("logdump: no segments in {}", dir.display());
        return 2;
    }
    let mut total = 0usize;
    let mut damaged_any = false;
    for (id, path) in &segments {
        match read_segment(path) {
            Ok(read) => {
                println!(
                    "segment {} flags={:#06x} records={} first_seq={} last_valid_seq={:?}",
                    id,
                    read.header.flags,
                    read.records.len(),
                    read.header.first_seq,
                    read.last_valid_seq
                );
                for r in &read.records {
                    println!("  {:>6} {} {}", r.id.seq, r.ts_ns, describe(&r.record));
                    total += 1;
                }
                if read.tail_truncated {
                    damaged_any = true;
                    match read.crc_mismatch_at {
                        Some(at) => println!("  !! TAIL DAMAGED: crc mismatch at offset {at}"),
                        None => println!("  !! TAIL DAMAGED: truncated record"),
                    }
                }
            }
            Err(e) => {
                eprintln!("logdump: segment {id} unreadable: {e}");
                return 2;
            }
        }
    }
    println!("total records: {total}");
    if damaged_any {
        println!("warning: a segment ended with a damaged tail; a gap is present (never hidden)");
        return 1;
    }
    0
}

fn describe(r: &Record) -> String {
    match r {
        Record::PtyOut { pane, bytes } => format!("PtyOut pane={pane} len={}", bytes.len()),
        Record::PtyIn {
            pane,
            len,
            source,
            lease_id,
            ..
        } => format!(
            "PtyIn pane={pane} len={len} source={} lease={lease_id} (digest only)",
            match source {
                Source::Human => "human",
                Source::Agent => "agent",
                Source::Plugin => "plugin",
            }
        ),
        Record::Resize {
            pane,
            cols,
            rows,
            px_w,
            px_h,
        } => format!("Resize pane={pane} {cols}x{rows} px={px_w}x{px_h}"),
        Record::CmdStart {
            pane,
            cmd_id,
            prompt_marker,
            ..
        } => format!(
            "CmdStart pane={pane} cmd={cmd_id} marker={}",
            *prompt_marker as char
        ),
        Record::CmdEnd {
            pane,
            cmd_id,
            exit_code,
            duration_ms,
            confidence,
            ..
        } => format!(
            "CmdEnd pane={pane} cmd={cmd_id} exit={exit_code} dur={duration_ms}ms confidence={confidence}"
        ),
        Record::CwdChange { pane, cwd } => format!("CwdChange pane={pane} cwd={cwd}"),
        Record::TitleChange { pane, title } => format!("TitleChange pane={pane} title={title}"),
        Record::ContextEvent { pane, kind, payload } => {
            format!("ContextEvent pane={pane} kind={kind} len={}", payload.len())
        }
        Record::StateChange {
            from,
            to,
            reason,
            exit_code,
        } => format!("StateChange {from} -> {to} reason={reason} exit={exit_code:?}"),
        Record::CheckpointRef { ckpt_id, .. } => format!("CheckpointRef ckpt={ckpt_id}"),
        Record::LeaseEvent {
            from,
            to,
            action,
            approver,
            ..
        } => format!("LeaseEvent action={action} from={from} to={to} approver={approver}"),
        Record::SubscriptionDrop {
            sub_id,
            class,
            dropped_bytes,
            ..
        } => format!("SubscriptionDrop sub={sub_id} class={class} bytes={dropped_bytes}"),
        Record::AuditRef { audit_seq, kind, .. } => {
            format!("AuditRef seq={audit_seq} kind={kind}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_and_help_exit_zero() {
        assert_eq!(run(&["version".to_string()]), 0);
        assert_eq!(run(&["help".to_string()]), 0);
        assert_eq!(run(&[]), 0);
    }

    #[test]
    fn unknown_command_fails() {
        assert_eq!(run(&["frobnicate".to_string()]), 2);
    }

    #[test]
    fn run_options_parse_including_the_double_dash_form() {
        let o = parse_run(&[
            "--cols".into(),
            "120".into(),
            "--rows".into(),
            "40".into(),
            "--no-log".into(),
            "--".into(),
            "echo".into(),
            "hi".into(),
        ])
        .unwrap();
        assert_eq!((o.cols, o.rows), (120, 40));
        assert!(!o.write_log);
        assert_eq!(o.argv, vec!["echo".to_string(), "hi".to_string()]);
    }

    #[test]
    fn bare_program_form_is_accepted() {
        let o = parse_run(&["ls".into(), "-la".into()]).unwrap();
        assert_eq!(o.argv, vec!["ls".to_string(), "-la".to_string()]);
        assert!(o.shell.is_none());
    }

    #[test]
    fn bad_options_are_rejected_with_a_hint() {
        assert!(parse_run(&["--cols".into()]).is_err());
        assert!(parse_run(&["--cols".into(), "abc".into()]).is_err());
        assert!(parse_run(&["--wat".into()]).is_err());
        assert!(parse_run(&["--cols".into(), "0".into()]).is_err());
    }

    #[test]
    fn timeout_option_parses_and_defaults_are_sane() {
        let o = parse_run(&["--timeout".into(), "5".into(), "--".into(), "echo".into()]).unwrap();
        assert_eq!(o.timeout, Duration::from_secs(5));
        assert!(o.timeout_explicit);
        assert!(parse_run(&["--timeout".into(), "abc".into()]).is_err());
        assert!(parse_run(&["--timeout".into()]).is_err());
        let d = parse_run(&[]).unwrap();
        assert!(!d.timeout_explicit);
        assert_eq!(d.timeout, Duration::from_secs(30));
    }

    #[test]
    fn backend_selection_is_explicit_and_rejects_unknown_values() {
        assert_eq!(parse_run(&[]).unwrap().backend, BackendKind::Native);
        assert_eq!(
            parse_run(&["--backend".into(), "pipe".into()])
                .unwrap()
                .backend,
            BackendKind::Pipe
        );
        assert!(parse_run(&["--backend".into(), "quantum".into()]).is_err());
        assert!(parse_run(&["--backend".into()]).is_err());
    }

    #[test]
    fn logdump_on_missing_dir_fails_loudly() {
        assert_eq!(run(&["logdump".to_string()]), 2);
        let mut p = std::env::temp_dir();
        p.push("termai-cli-does-not-exist-xyz");
        let _ = std::fs::remove_dir_all(&p);
        assert_eq!(logdump(&p), 2);
    }

    #[test]
    fn logdump_reports_a_damaged_tail_with_a_nonzero_code() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("termai-cli-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, 0).unwrap();
        w.append(
            &Record::TitleChange {
                pane: 0,
                title: "t".into(),
            },
            1,
        )
        .unwrap();
        w.append(
            &Record::PtyOut {
                pane: 0,
                bytes: b"ab".to_vec(),
            },
            2,
        )
        .unwrap();
        w.flush(FlushMode::FsyncFull).unwrap();
        let path = w.path().to_path_buf();
        drop(w);
        let full = std::fs::read(&path).unwrap();
        std::fs::write(&path, &full[..full.len() - 2]).unwrap();
        assert_eq!(
            logdump(&dir),
            1,
            "a damaged tail must be reported, not hidden"
        );
    }

    #[test]
    fn logdump_of_a_clean_log_is_zero() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("termai-cli-clean-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, 0).unwrap();
        w.append(
            &Record::PtyOut {
                pane: 0,
                bytes: b"hi".to_vec(),
            },
            1,
        )
        .unwrap();
        w.flush(FlushMode::FsyncFull).unwrap();
        drop(w);
        assert_eq!(logdump(&dir), 0);
    }

    #[test]
    fn describe_never_prints_input_bytes() {
        let r = Record::PtyIn {
            pane: 0,
            sha256: [1u8; 32],
            len: 3,
            source: Source::Human,
            lease_id: 1,
        };
        assert!(describe(&r).contains("digest only"));
    }

    #[test]
    fn hex32_is_lowercase_fixed_width() {
        assert_eq!(hex32(&[0u8; 32]), "0".repeat(64));
        assert_eq!(hex32(&[0xABu8; 32]).len(), 64);
        assert!(hex32(&[0xABu8; 32]).starts_with("ab"));
    }
}
