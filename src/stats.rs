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
    /// Total presses per local calendar day, keyed "YYYY-MM-DD".
    #[serde(default)]
    pub daily: HashMap<String, u64>,
    /// Per-key presses per day, for time-windowed heatmaps. Keyed day -> (key
    /// index -> count). Only accrues going forward; `presses` keeps the
    /// complete lifetime history for the all-time view.
    #[serde(default)]
    pub daily_keys: HashMap<String, HashMap<usize, u64>>,
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

    /// One more press recorded for the given local day ("YYYY-MM-DD").
    pub fn record_day(&mut self, day: &str) {
        *self.daily.entry(day.to_owned()).or_insert(0) += 1;
    }

    /// One more press for a key on a given day (for time-windowed heatmaps).
    pub fn record_key_day(&mut self, day: &str, key: usize) {
        *self.daily_keys.entry(day.to_owned()).or_default().entry(key).or_insert(0) += 1;
    }

    /// Aggregate per-key presses over the given days (for a windowed heatmap).
    pub fn key_counts_over(&self, days: &[String]) -> HashMap<usize, u64> {
        let mut out: HashMap<usize, u64> = HashMap::new();
        for d in days {
            if let Some(m) = self.daily_keys.get(d) {
                for (k, c) in m {
                    *out.entry(*k).or_insert(0) += c;
                }
            }
        }
        out
    }

    /// Presses on a given day.
    pub fn day_count(&self, day: &str) -> u64 {
        self.daily.get(day).copied().unwrap_or(0)
    }

    /// Consecutive days with activity ending at `today` (or yesterday, if today
    /// has none yet — the streak isn't broken until a full empty day passes).
    pub fn streak(&self, today: &str) -> u32 {
        let Some(mut cur) = parse_day(today) else { return 0 };
        if self.day_count(&fmt_day(cur)) == 0 {
            cur = prev_day(cur);
        }
        let mut count = 0;
        while self.day_count(&fmt_day(cur)) > 0 {
            count += 1;
            cur = prev_day(cur);
        }
        count
    }

    /// Fraction of this key's presses that were immediately deleted, or None
    /// when there aren't enough presses for the rate to mean anything.
    pub fn error_rate(&self, key: usize) -> Option<f32> {
        let presses = self.count(key);
        (presses >= MIN_ERROR_SAMPLES)
            .then(|| self.delete_count(key) as f32 / presses as f32)
    }
}

/// The last `n` local day-keys ending at (and including) `today`, newest first.
pub fn recent_days(today: &str, n: usize) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(mut cur) = parse_day(today) {
        for _ in 0..n {
            out.push(fmt_day(cur));
            cur = prev_day(cur);
        }
    }
    out
}

/// Parse "YYYY-MM-DD" into (year, month, day).
fn parse_day(s: &str) -> Option<(i32, i32, i32)> {
    let mut it = s.split('-');
    let y = it.next()?.parse().ok()?;
    let m = it.next()?.parse().ok()?;
    let d = it.next()?.parse().ok()?;
    Some((y, m, d))
}

fn fmt_day((y, m, d): (i32, i32, i32)) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

fn days_in_month(y: i32, m: i32) -> i32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        _ => 30,
    }
}

/// The calendar day before the given one.
fn prev_day((y, m, d): (i32, i32, i32)) -> (i32, i32, i32) {
    if d > 1 {
        (y, m, d - 1)
    } else if m > 1 {
        (y, m - 1, days_in_month(y, m - 1))
    } else {
        (y - 1, 12, 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prev_day_handles_boundaries() {
        assert_eq!(prev_day((2026, 6, 16)), (2026, 6, 15));
        assert_eq!(prev_day((2026, 7, 1)), (2026, 6, 30));
        assert_eq!(prev_day((2026, 1, 1)), (2025, 12, 31));
        assert_eq!(prev_day((2024, 3, 1)), (2024, 2, 29)); // leap year
        assert_eq!(prev_day((2026, 3, 1)), (2026, 2, 28));
    }

    #[test]
    fn streak_counts_consecutive_days() {
        let mut s = Stats::default();
        for d in ["2026-06-14", "2026-06-15", "2026-06-16"] {
            s.record_day(d);
        }
        assert_eq!(s.streak("2026-06-16"), 3);
        // Today empty but yesterday active: streak still alive.
        assert_eq!(s.streak("2026-06-17"), 3);
        // A two-day gap breaks it.
        assert_eq!(s.streak("2026-06-18"), 0);
        // A gap in the middle stops the count.
        let mut s2 = Stats::default();
        s2.record_day("2026-06-16");
        s2.record_day("2026-06-14");
        assert_eq!(s2.streak("2026-06-16"), 1);
    }

    #[test]
    fn report_summarizes() {
        let mut s = Stats::default();
        for _ in 0..100 {
            s.record(0);
        }
        s.record_delete(0);
        s.record_bigram("T", "H");
        s.record_day("2026-06-16");
        let r = s.report("2026-06-16", |i| format!("k{i}"));
        assert!(r.contains("Total presses:"), "has totals");
        assert!(r.contains("k0"), "labels the top key");
        assert!(r.contains("TH"), "lists bigrams");
        assert!(r.contains("Finger load"), "has finger section");
        assert!(r.contains("Recent days"), "has daily section");
    }

    #[test]
    fn records_and_counts() {
        let mut s = Stats::default();
        for _ in 0..3 {
            s.record(5);
        }
        s.record(9);
        assert_eq!(s.count(5), 3);
        assert_eq!(s.count(9), 1);
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

impl Stats {
    /// A plain-text summary of all accumulated stats. Pure (no I/O): `label`
    /// resolves an Oryx key index to a display label, `today` is the local date.
    pub fn report(&self, today: &str, label: impl Fn(usize) -> String) -> String {
        use std::fmt::Write as _;
        let mut s = String::new();
        let total: u64 = self.presses.values().sum();
        let corrections: u64 = self.deletes.values().sum();
        let rate = if total > 0 { corrections as f64 / total as f64 * 100.0 } else { 0.0 };

        let _ = writeln!(s, "starview key stats - {today}\n");
        let _ = writeln!(s, "Lifetime");
        let _ = writeln!(s, "  Total presses:    {}", commas(total));
        let _ = writeln!(s, "  Corrections:      {} (tracked backspaces)", commas(corrections));
        let _ = writeln!(s, "  Correction rate:  {rate:.1}%");
        let _ = writeln!(s, "  Daily streak:     {} days\n", self.streak(today));

        let mut keys: Vec<(usize, u64)> = self.presses.iter().map(|(k, c)| (*k, *c)).collect();
        keys.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let _ = writeln!(s, "Most-pressed keys");
        for (rank, (k, c)) in keys.iter().take(15).enumerate() {
            let _ = writeln!(s, "  {:>2}. {:<6} {}", rank + 1, label(*k), commas(*c));
        }
        let _ = writeln!(s);

        let mut loads = [0u64; 10];
        for i in 0..crate::geometry::MOONLANDER_KEYS.len() {
            loads[crate::geometry::finger_slot(i)] += self.count(i);
        }
        let fsum: u64 = loads.iter().sum();
        let pct = |v: u64| if fsum > 0 { v as f64 / fsum as f64 * 100.0 } else { 0.0 };
        let names = [
            "L pinky", "L ring", "L middle", "L index", "L thumb", "R thumb", "R index",
            "R middle", "R ring", "R pinky",
        ];
        let _ = writeln!(s, "Finger load");
        for (n, l) in names.iter().zip(loads) {
            let _ = writeln!(s, "  {:<9} {:>12}  {:>5.1}%", n, commas(l), pct(l));
        }
        let left: u64 = loads[..5].iter().sum();
        let _ = writeln!(
            s,
            "  Hand balance: left {:.0}%  right {:.0}%\n",
            pct(left),
            pct(fsum - left)
        );

        let _ = writeln!(s, "Top bigrams");
        for (b, c) in self.top_bigrams(15) {
            let _ = writeln!(s, "  {:<4} {}", b, commas(c));
        }
        let _ = writeln!(s);

        let mut typos: Vec<(usize, f32)> = (0..crate::geometry::MOONLANDER_KEYS.len())
            .filter_map(|i| self.error_rate(i).map(|r| (i, r)))
            .filter(|t| t.1 > 0.0)
            .collect();
        typos.sort_by(|a, b| b.1.total_cmp(&a.1));
        let _ = writeln!(s, "Most-corrected keys (>= {MIN_ERROR_SAMPLES} presses)");
        for (k, r) in typos.iter().take(10) {
            let _ = writeln!(
                s,
                "  {:<6} {:>5.1}%  ({} of {})",
                label(*k),
                r * 100.0,
                self.delete_count(*k),
                self.count(*k)
            );
        }
        let _ = writeln!(s);

        let _ = writeln!(s, "Recent days");
        for d in recent_days(today, 7) {
            let _ = writeln!(s, "  {}  {}", d, commas(self.day_count(&d)));
        }
        s
    }
}

/// Group an integer with thousands separators.
fn commas(n: u64) -> String {
    let s = n.to_string();
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, &c) in b.iter().enumerate() {
        if i > 0 && (b.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c as char);
    }
    out
}

/// The starview data directory (%LOCALAPPDATA%\starview).
pub fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("starview"))
}

fn path() -> Option<PathBuf> {
    Some(dir()?.join("stats.json"))
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
