//! Supervisor restart policy and circuit breaker (kernel/04 section 3.6).
//!
//! Backoff applies to CRASHES only. An intentional exit (exit 0, idle timeout,
//! --no-plugins) resets the backoff. Five crashes inside the window open the circuit.
//! Silent restarts are forbidden: every decision carries an announcement string that
//! the UI/tray must surface (AR-20).

use core::time::Duration;

use termai_core::time::MonoTime;

/// Restart policy. Defaults are the ones ratified in kernel/04 section 3.6.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RestartPolicy {
    pub base_ms: u64,
    pub factor: f64,
    pub max_ms: u64,
    pub jitter_pct: u8,
    pub window_ms: u64,
    pub max_restarts: u32,
    pub circuit_ms: u64,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            base_ms: 250,
            factor: 2.0,
            max_ms: 30_000,
            jitter_pct: 20,
            window_ms: 10 * 60 * 1000,
            max_restarts: 5,
            circuit_ms: 5 * 60 * 1000,
        }
    }
}

/// Why a child process stopped. Only crashes consume the restart budget.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExitKind {
    /// Non-zero exit, signal or heartbeat timeout.
    Crash,
    /// exit 0, idle timeout, or an explicit flag.
    Intentional,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RestartDecision {
    RestartAfter(Duration),
    /// Circuit is open: no new sessions are accepted, diagnostics stay available.
    CircuitOpen {
        until: MonoTime,
    },
    NoRestart,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Component {
    Sessiond,
    Agent,
    PluginHost,
    Tools,
}

impl Component {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Component::Sessiond => "sessiond",
            Component::Agent => "termai-agent",
            Component::PluginHost => "plugin-host",
            Component::Tools => "termai-tools",
        }
    }

    /// Where the component degrades to when the circuit opens.
    #[must_use]
    pub const fn safe_mode(self) -> &'static str {
        match self {
            Component::Sessiond => "diagnostics-only (new sessions refused)",
            Component::Agent => "AI safe mode",
            Component::PluginHost => "plugin safe mode",
            Component::Tools => "tool execution refused",
        }
    }
}

/// Deterministic backoff + circuit breaker.
#[derive(Clone, Debug)]
pub struct Backoff {
    policy: RestartPolicy,
    attempt: u32,
    crashes: Vec<MonoTime>,
    circuit_until: Option<MonoTime>,
    seed: u64,
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new(RestartPolicy::default())
    }
}

impl Backoff {
    #[must_use]
    pub const fn new(policy: RestartPolicy) -> Self {
        Self {
            policy,
            attempt: 0,
            crashes: Vec::new(),
            circuit_until: None,
            seed: 0x2545_F491_4F6C_DD1D,
        }
    }

    #[must_use]
    pub const fn policy(&self) -> RestartPolicy {
        self.policy
    }

    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    #[must_use]
    pub fn crashes_in_window(&self) -> usize {
        self.crashes.len()
    }

    /// Circuit state at a point in time.
    #[must_use]
    pub fn circuit_open(&self, now: MonoTime) -> Option<MonoTime> {
        match self.circuit_until {
            Some(until) if !until.has_passed(now) => Some(until),
            _ => None,
        }
    }

    /// Unjittered delay for the current attempt: base * factor^n, capped at max.
    #[must_use]
    pub fn delay(&self) -> Duration {
        let mut ms = self.policy.base_ms as f64;
        for _ in 0..self.attempt {
            ms *= self.policy.factor;
            if ms >= self.policy.max_ms as f64 {
                return Duration::from_millis(self.policy.max_ms);
            }
        }
        Duration::from_millis(ms as u64)
    }

    /// Deterministic jitter in +/- jitter_pct (no rand dependency, reproducible tests).
    #[must_use]
    pub fn delay_with_jitter(&mut self) -> Duration {
        let base = self.delay().as_millis() as u64;
        if self.policy.jitter_pct == 0 {
            return Duration::from_millis(base);
        }
        self.seed = self
            .seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let span = base * u64::from(self.policy.jitter_pct) / 100;
        if span == 0 {
            return Duration::from_millis(base);
        }
        let offset = self.seed % (span * 2 + 1);
        Duration::from_millis((base + offset).saturating_sub(span))
    }

    fn prune(&mut self, now: MonoTime) {
        let window = Duration::from_millis(self.policy.window_ms);
        let cutoff = now.saturating_sub(window);
        // Keep crashes at or after the window start (strictly older ones fall out).
        self.crashes.retain(|c| *c >= cutoff);
    }

    /// Record a crash and decide.
    pub fn on_exit(&mut self, kind: ExitKind, now: MonoTime) -> RestartDecision {
        if let Some(until) = self.circuit_open(now) {
            return RestartDecision::CircuitOpen { until };
        }
        match kind {
            ExitKind::Intentional => {
                self.attempt = 0;
                self.crashes.clear();
                RestartDecision::NoRestart
            }
            ExitKind::Crash => {
                self.prune(now);
                self.crashes.push(now);
                if self.crashes.len() as u32 >= self.policy.max_restarts {
                    let until = now.saturating_add(Duration::from_millis(self.policy.circuit_ms));
                    self.circuit_until = Some(until);
                    self.attempt = 0;
                    return RestartDecision::CircuitOpen { until };
                }
                let delay = self.delay_with_jitter();
                self.attempt += 1;
                RestartDecision::RestartAfter(delay)
            }
        }
    }

    /// Manual restart after the circuit opened (operator action).
    pub fn manual_reset(&mut self) {
        self.attempt = 0;
        self.crashes.clear();
        self.circuit_until = None;
    }

    /// Human-readable announcement. Silent restart is forbidden (AR-20).
    #[must_use]
    pub fn announce(component: Component, decision: RestartDecision) -> String {
        match decision {
            RestartDecision::RestartAfter(d) => format!(
                "{} crashed; restarting after {} ms (safe mode: {} stays available)",
                component.as_str(),
                d.as_millis(),
                component.safe_mode()
            ),
            RestartDecision::CircuitOpen { until } => format!(
                "{} crashed {} times; circuit OPEN until mono {} - {} (manual restart available)",
                component.as_str(),
                "max_restarts",
                until.as_nanos(),
                component.safe_mode()
            ),
            RestartDecision::NoRestart => format!(
                "{} exited intentionally; no restart (silent restart is forbidden)",
                component.as_str()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(ms: u64) -> MonoTime {
        MonoTime::from_millis(ms)
    }

    #[test]
    fn backoff_is_exponential_and_capped() {
        // Keep the circuit out of the way: this test is about the delay curve.
        let mut b = Backoff::new(RestartPolicy {
            jitter_pct: 0,
            max_restarts: 100,
            ..Default::default()
        });
        assert_eq!(b.delay(), Duration::from_millis(250));
        b.on_exit(ExitKind::Crash, t(0));
        assert_eq!(b.attempt(), 1);
        assert_eq!(b.delay(), Duration::from_millis(500));
        b.on_exit(ExitKind::Crash, t(1));
        assert_eq!(b.delay(), Duration::from_millis(1000));
        for i in 2..12u64 {
            b.on_exit(ExitKind::Crash, t(i));
        }
        assert_eq!(b.attempt(), 12);
        assert_eq!(
            b.delay(),
            Duration::from_millis(30_000),
            "delay must cap at max"
        );
    }

    #[test]
    fn five_crashes_in_window_open_the_circuit() {
        let mut b = Backoff::default();
        let mut last = RestartDecision::NoRestart;
        for i in 0..5u64 {
            last = b.on_exit(ExitKind::Crash, t(i * 1000));
        }
        match last {
            RestartDecision::CircuitOpen { until } => {
                assert_eq!(
                    until,
                    t(4000).saturating_add(Duration::from_millis(300_000))
                );
            }
            other => panic!("expected circuit open, got {other:?}"),
        }
        // Once open, further crashes keep reporting the open circuit.
        assert!(matches!(
            b.on_exit(ExitKind::Crash, t(10_000)),
            RestartDecision::CircuitOpen { .. }
        ));
    }

    #[test]
    fn intentional_exit_does_not_consume_the_budget() {
        let mut b = Backoff::default();
        for i in 0..10u64 {
            assert_eq!(
                b.on_exit(ExitKind::Intentional, t(i * 1000)),
                RestartDecision::NoRestart
            );
        }
        assert_eq!(b.crashes_in_window(), 0);
        assert!(b.circuit_open(t(20_000)).is_none());
        assert!(matches!(
            b.on_exit(ExitKind::Crash, t(20_000)),
            RestartDecision::RestartAfter(_)
        ));
    }

    #[test]
    fn old_crashes_fall_out_of_the_window() {
        let mut b = Backoff::default();
        for i in 0..4u64 {
            b.on_exit(ExitKind::Crash, t(i * 1000));
        }
        // 20 minutes later the window is empty again.
        assert!(matches!(
            b.on_exit(ExitKind::Crash, t(20 * 60 * 1000)),
            RestartDecision::RestartAfter(_)
        ));
        assert_eq!(b.crashes_in_window(), 1);
    }

    #[test]
    fn manual_reset_closes_the_circuit() {
        let mut b = Backoff::default();
        for i in 0..5u64 {
            b.on_exit(ExitKind::Crash, t(i));
        }
        assert!(b.circuit_open(t(100)).is_some());
        b.manual_reset();
        assert!(b.circuit_open(t(100)).is_none());
        assert_eq!(b.attempt(), 0);
    }

    #[test]
    fn jitter_is_deterministic_and_bounded() {
        let policy = RestartPolicy::default();
        let mut a = Backoff::new(policy);
        let mut c = Backoff::new(policy);
        for _ in 0..8 {
            let da = a.delay_with_jitter();
            let dc = c.delay_with_jitter();
            assert_eq!(da, dc, "jitter must be reproducible for the same seed");
            assert!(da <= Duration::from_millis(250 + 50));
            assert!(da >= Duration::from_millis(200));
        }
    }

    #[test]
    fn announcements_are_never_silent() {
        let s = Backoff::announce(
            Component::Sessiond,
            RestartDecision::RestartAfter(Duration::from_millis(250)),
        );
        assert!(s.contains("sessiond"));
        assert!(s.contains("restarting"));
        let s = Backoff::announce(
            Component::PluginHost,
            RestartDecision::CircuitOpen { until: t(1) },
        );
        assert!(s.contains("circuit OPEN"));
        assert!(s.contains("plugin safe mode"));
        let s = Backoff::announce(Component::Agent, RestartDecision::NoRestart);
        assert!(s.contains("silent restart is forbidden"));
    }
}
