//! 单调时间。审计与 lease 一律用单调时钟（kernel/04 §3.4、kernel/07 §3.4）。

use core::time::Duration;

/// 单调时间戳（纳秒，由注入的时钟源提供；本 crate 不取时钟）。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct MonoTime {
    nanos: u64,
}

impl MonoTime {
    pub const ZERO: MonoTime = MonoTime { nanos: 0 };

    #[must_use]
    pub const fn from_nanos(nanos: u64) -> Self {
        Self { nanos }
    }

    #[must_use]
    pub const fn from_millis(ms: u64) -> Self {
        Self {
            nanos: ms.saturating_mul(1_000_000),
        }
    }

    #[must_use]
    pub const fn from_secs(s: u64) -> Self {
        Self {
            nanos: s.saturating_mul(1_000_000_000),
        }
    }

    #[must_use]
    pub const fn as_nanos(self) -> u64 {
        self.nanos
    }

    #[must_use]
    pub const fn saturating_add(self, d: Duration) -> Self {
        Self {
            nanos: self.nanos.saturating_add(d.as_nanos() as u64),
        }
    }

    #[must_use]
    pub const fn saturating_sub(self, d: Duration) -> Self {
        Self {
            nanos: self.nanos.saturating_sub(d.as_nanos() as u64),
        }
    }

    /// `self - earlier`，饱和到 0。
    #[must_use]
    pub const fn since(self, earlier: MonoTime) -> Duration {
        Duration::from_nanos(self.nanos.saturating_sub(earlier.nanos))
    }

    #[must_use]
    pub const fn has_passed(self, now: MonoTime) -> bool {
        now.nanos >= self.nanos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturating_and_since() {
        let a = MonoTime::from_secs(2);
        let b = a.saturating_add(Duration::from_millis(500));
        assert_eq!(b.since(a), Duration::from_millis(500));
        assert_eq!(a.saturating_sub(Duration::from_secs(5)), MonoTime::ZERO);
        assert!(MonoTime::from_secs(1).has_passed(b));
        assert!(!b.has_passed(MonoTime::from_secs(1)));
    }

    #[test]
    fn zero_and_display_are_stable() {
        assert_eq!(MonoTime::from_millis(1500).as_nanos(), 1_500_000_000);
        assert_eq!(MonoTime::ZERO.as_nanos(), 0);
    }
}
