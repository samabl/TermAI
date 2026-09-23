//! VT counter block (kernel/01 section 3.4).
//!
//! One u64 per counter key, plus a per-number breakdown for osc_unknown{num}. The
//! breakdown is only ever exposed locally (termai diag vt --unknown); it is never
//! serialized into logs or events.

use std::collections::BTreeMap;

use crate::backend::ParseErrorKind;

/// Counters for the 14 keys of kernel/01 section 3.4.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VtCounters {
    counts: [u64; 14],
    osc_unknown_by_num: BTreeMap<u32, u64>,
}

impl VtCounters {
    /// Create an empty counter block.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment one counter key. For OscUnknown this increments the running total
    /// only; use bump_osc_unknown to also record the OSC number.
    pub fn bump(&mut self, kind: ParseErrorKind) {
        if let Some(slot) = self.counts.get_mut(kind.index()) {
            *slot = slot.saturating_add(1);
        }
    }

    /// Record an unrecognised OSC number.
    pub fn bump_osc_unknown(&mut self, osc_num: u32) {
        self.bump(ParseErrorKind::OscUnknown);
        let entry = self.osc_unknown_by_num.entry(osc_num).or_insert(0);
        *entry = entry.saturating_add(1);
    }

    /// Read one counter key.
    #[must_use]
    pub fn get(&self, kind: ParseErrorKind) -> u64 {
        self.counts.get(kind.index()).copied().unwrap_or(0)
    }

    /// Sum of all counter keys.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.counts.iter().copied().sum()
    }

    /// Add another counter block into this one.
    pub fn merge(&mut self, other: &VtCounters) {
        for (slot, add) in self.counts.iter_mut().zip(other.counts.iter()) {
            *slot = slot.saturating_add(*add);
        }
        for (num, add) in &other.osc_unknown_by_num {
            let entry = self.osc_unknown_by_num.entry(*num).or_insert(0);
            *entry = entry.saturating_add(*add);
        }
    }

    /// How often a specific unknown OSC number was seen.
    #[must_use]
    pub fn osc_unknown_count(&self, osc_num: u32) -> u64 {
        self.osc_unknown_by_num.get(&osc_num).copied().unwrap_or(0)
    }

    /// Deterministic view sorted by counter key.
    #[must_use]
    pub fn as_pairs(&self) -> Vec<(&'static str, u64)> {
        let mut pairs: Vec<(&'static str, u64)> = ParseErrorKind::ALL
            .iter()
            .map(|kind| (kind.key(), self.get(*kind)))
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(b.0));
        pairs
    }

    /// Look a counter up by the string key used in .trec scripts. Supports the
    /// osc_unknown[N] form in addition to the 14 static keys.
    #[must_use]
    pub fn get_key(&self, key: &str) -> Option<u64> {
        if let Some(rest) = key.strip_prefix("osc_unknown[") {
            let num = rest.strip_suffix(']')?.parse::<u32>().ok()?;
            return Some(self.osc_unknown_count(num));
        }
        ParseErrorKind::ALL
            .iter()
            .find(|kind| kind.key() == key)
            .map(|kind| self.get(*kind))
    }
}
