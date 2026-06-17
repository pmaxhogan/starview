//! Persisted lifetime key-press counts (%LOCALAPPDATA%\starview\stats.json).
//!
//! Counts are keyed by Oryx key index (the same order as
//! `geometry::MOONLANDER_KEYS`), so they line up with the physical board the
//! overlay draws. The heatmap reads `max()` to normalize, and the header reads
//! `total()` for the running press counter.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A key needs at least this many presses before its error rate is shown — a
/// key pressed twice that got deleted once isn't a "50% typo key", just noise.
pub const MIN_ERROR_SAMPLES: u64 = 20;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Stats {
    /// Lifetime press count per Oryx key index.
    pub presses: HashMap<usize, u64>,
    /// How many times a key's character was deleted right after typing it
    /// (the typo proxy — see the overlay's backspace tracker). Counted only
    /// when no window switch happened between the keypress and the backspace.
    pub deletes: HashMap<usize, u64>,
    /// Counts of consecutive character pairs ("th", "he", …), keyed by the two
    /// characters concatenated.
    #[serde(default)]
    pub bigrams: HashMap<String, u64>,
}

impl Stats {
    /// One more press recorded for this key.
    pub fn record(&mut self, key: usize) {
        *self.presses.entry(key).or_insert(0) += 1;
    }

    /// One more "typed then immediately deleted" event for this key.
    pub fn record_delete(&mut self, key: usize) {
        *self.deletes.entry(key).or_insert(0) += 1;
    }

    /// Sum of all key presses ever.
    pub fn total(&self) -> u64 {
        self.presses.values().sum()
    }

    /// The single most-pressed key's count (the heatmap's hot end).
    pub fn max(&self) -> u64 {
        self.presses.values().copied().max().unwrap_or(0)
    }

    /// Presses recorded for one key.
    pub fn count(&self, key: usize) -> u64 {
        self.presses.get(&key).copied().unwrap_or(0)
    }

    /// Deletions recorded for one key.
    pub fn delete_count(&self, key: usize) -> u64 {
        self.deletes.get(&key).copied().unwrap_or(0)
    }

    /// One more occurrence of the character pair `a` then `b`.
    pub fn record_bigram(&mut self, a: &str, b: &str) {
        *self.bigrams.entry(format!("{a}{b}")).or_insert(0) += 1;
    }

    /// The `n` most frequent bigrams, highest first (ties broken alphabetically).
    pub fn top_bigrams(&self, n: usize) -> Vec<(String, u64)> {
        let mut v: Vec<(String, u64)> =
            self.bigrams.iter().map(|(k, c)| (k.clone(), *c)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v.truncate(n);
        v
    }

    /// Fraction of this key's presses that were immediately deleted, or None
    /// when there aren't enough presses for the rate to mean anything.
    pub fn error_rate(&self, key: usize) -> Option<f32> {
        let presses = self.count(key);
        (presses >= MIN_ERROR_SAMPLES)
            .then(|| self.delete_count(key) as f32 / presses as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_and_max() {
        let mut s = Stats::default();
        for _ in 0..3 {
            s.record(5);
        }
        s.record(9);
        assert_eq!(s.total(), 4);
        assert_eq!(s.max(), 3);
        assert_eq!(s.count(5), 3);
        assert_eq!(s.count(42), 0);
    }

    #[test]
    fn error_rate_needs_min_samples() {
        let mut s = Stats::default();
        // One press, one delete: a 100% rate, but too little data to report.
        s.record(1);
        s.record_delete(1);
        assert_eq!(s.error_rate(1), None);

        // Enough presses: the rate is real.
        let mut s = Stats::default();
        for _ in 0..MIN_ERROR_SAMPLES {
            s.record(2);
        }
        for _ in 0..(MIN_ERROR_SAMPLES / 4) {
            s.record_delete(2);
        }
        assert_eq!(s.error_rate(2), Some(0.25));
        // A key never pressed has no rate.
        assert_eq!(s.error_rate(3), None);
    }
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("starview").join("stats.json"))
}

pub fn load() -> Stats {
    path()
        .and_then(|p| fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save(stats: &Stats) {
    let Some(path) = path() else { return };
    let _ = fs::create_dir_all(path.parent().unwrap());
    match serde_json::to_vec(stats) {
        Ok(bytes) => {
            if let Err(err) = fs::write(&path, bytes) {
                eprintln!("failed to save stats: {err}");
            }
        }
        Err(err) => eprintln!("failed to serialize stats: {err}"),
    }
}
