//! Shell integration state machine (kernel/01 section 3.6): OSC 133 + OSC 633 + OSC 7.
//!
//! L2 privacy rule: the command text from OSC 633 E is stored locally as metadata and
//! is never logged and never placed into an event payload. The type only exposes it
//! through an explicit accessor so callers must opt in.

/// One command boundary produced by OSC 133 / 633.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CommandBlock {
    /// Monotonic command id inside this session.
    pub cmd_id: u64,
    /// Optional prompt marker from OSC 133 A.
    pub prompt_marker: Option<String>,
    /// Logical start time (ns; deterministic counter, see ShellIntegration::now_ns).
    pub started_ns: u64,
    /// Logical end time, None while open.
    pub ended_ns: Option<u64>,
    /// Exit code. Missing stays None and is never guessed.
    pub exit_code: Option<i32>,
    /// Working directory at command end.
    pub cwd: Option<String>,
    /// Confidence 0-100. OSC 133/633 = 100; a heuristic fallback would be lower.
    pub confidence: u8,
}

/// Confidence for OSC 133 / 633 sourced blocks.
pub const CONFIDENCE_OSC: u8 = 100;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ShellState {
    Idle,
    PromptOpen,
    PromptClosed,
    Executing,
}

/// Shell integration tracker driven by OSC 7 / 133 / 633.
pub struct ShellIntegration {
    cwd: Option<String>,
    cwd_remote: bool,
    last_command: Option<String>,
    is_windows: Option<bool>,
    blocks: Vec<CommandBlock>,
    state: ShellState,
    pending: Option<CommandBlock>,
    next_id: u64,
    now_ns: u64,
    unpaired: u64,
    overlap: u64,
}

impl Default for ShellIntegration {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellIntegration {
    /// Create an empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cwd: None,
            cwd_remote: false,
            last_command: None,
            is_windows: None,
            blocks: Vec::new(),
            state: ShellState::Idle,
            pending: None,
            next_id: 1,
            now_ns: 0,
            unpaired: 0,
            overlap: 0,
        }
    }

    /// Closed command blocks in arrival order.
    #[must_use]
    pub fn blocks(&self) -> &[CommandBlock] {
        &self.blocks
    }

    /// Current working directory.
    #[must_use]
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    /// Whether the cwd belongs to a remote host.
    #[must_use]
    pub fn cwd_remote(&self) -> bool {
        self.cwd_remote
    }

    /// Last command text captured from OSC 633 E. L2 sensitive: never log or emit.
    #[must_use]
    pub fn last_command(&self) -> Option<&str> {
        self.last_command.as_deref()
    }

    /// OSC 633 P;IsWindows= value.
    #[must_use]
    pub fn is_windows(&self) -> Option<bool> {
        self.is_windows
    }

    /// Number of OSC 133 D markers dropped because no command was executing.
    #[must_use]
    pub fn unpaired_count(&self) -> u64 {
        self.unpaired
    }

    /// Number of consecutive A markers that overwrote a pending block.
    #[must_use]
    pub fn overlap_count(&self) -> u64 {
        self.overlap
    }

    /// Dispatch one OSC payload. num is the OSC number, params includes it as [0].
    pub fn on_osc(&mut self, num: u32, params: &[&[u8]]) {
        self.now_ns = self.now_ns.saturating_add(1);
        match num {
            7 => self.on_osc7(params),
            133 | 633 => self.on_shell(num, params),
            _ => {}
        }
    }

    fn on_osc7(&mut self, params: &[&[u8]]) {
        let payload = join_from(params, 1);
        if payload.is_empty() {
            return;
        }
        if let Some((path, remote)) = crate::grid::parse_file_uri(&payload) {
            self.cwd = Some(path);
            self.cwd_remote = remote;
        }
    }

    fn on_shell(&mut self, num: u32, params: &[&[u8]]) {
        let sub = params.get(1).and_then(|raw| raw.first()).copied();
        let sub = match sub {
            Some(byte) => byte,
            None => return,
        };
        match sub {
            b'A' => self.on_prompt_open(params),
            b'B' => {
                if self.state == ShellState::PromptOpen {
                    self.state = ShellState::PromptClosed;
                }
            }
            b'C' => self.on_command_start(),
            b'D' => self.on_command_end(params),
            b'E' => {
                if num == 633 {
                    let text = join_from(params, 2);
                    if !text.is_empty() {
                        self.last_command = Some(text);
                    }
                }
            }
            b'P' => {
                if num == 633 {
                    self.on_property(params);
                }
            }
            _ => {}
        }
    }

    fn on_prompt_open(&mut self, params: &[&[u8]]) {
        if self.state != ShellState::Idle {
            self.overlap = self.overlap.saturating_add(1);
        }
        let marker = {
            let text = join_from(params, 2);
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        };
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.pending = Some(CommandBlock {
            cmd_id: id,
            prompt_marker: marker,
            started_ns: self.now_ns,
            ended_ns: None,
            exit_code: None,
            cwd: self.cwd.clone(),
            confidence: CONFIDENCE_OSC,
        });
        self.state = ShellState::PromptOpen;
    }

    fn on_command_start(&mut self) {
        match self.state {
            ShellState::PromptOpen | ShellState::PromptClosed | ShellState::Executing => {
                if let Some(block) = self.pending.as_mut() {
                    block.started_ns = self.now_ns;
                }
                self.state = ShellState::Executing;
            }
            ShellState::Idle => {}
        }
    }

    fn on_command_end(&mut self, params: &[&[u8]]) {
        if self.state != ShellState::Executing {
            self.unpaired = self.unpaired.saturating_add(1);
            return;
        }
        let exit_code = parse_exit(params.get(2).copied());
        if let Some(mut block) = self.pending.take() {
            block.ended_ns = Some(self.now_ns);
            block.exit_code = exit_code;
            block.cwd = self.cwd.clone();
            self.blocks.push(block);
        }
        self.state = ShellState::Idle;
    }

    fn on_property(&mut self, params: &[&[u8]]) {
        let text = join_from(params, 2);
        if let Some(value) = text.strip_prefix("Cwd=") {
            if !value.is_empty() {
                self.cwd = Some(value.to_string());
                self.cwd_remote = false;
            }
        } else if let Some(value) = text.strip_prefix("IsWindows=") {
            self.is_windows = Some(value == "1" || value.eq_ignore_ascii_case("true"));
        }
    }
}

fn join_from(params: &[&[u8]], start: usize) -> String {
    if start >= params.len() {
        return String::new();
    }
    let mut out = String::new();
    for (i, raw) in params[start..].iter().enumerate() {
        if i > 0 {
            out.push(';');
        }
        out.push_str(&String::from_utf8_lossy(raw));
    }
    out
}

fn parse_exit(raw: Option<&[u8]>) -> Option<i32> {
    let raw = raw?;
    if raw.is_empty() {
        return None;
    }
    let text = std::str::from_utf8(raw).ok()?;
    let value = text.parse::<i64>().ok()?;
    i32::try_from(value).ok()
}
