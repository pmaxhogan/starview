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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Stats {
    /// Lifetime press count per Oryx key index.
    pub presses: HashMap<usize, u64>,
}

impl Stats {
    /// One more press recorded for this key.
    pub fn record(&mut self, key: usize) {
        *self.presses.entry(key).or_insert(0) += 1;
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
