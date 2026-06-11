//! Persisted user settings (%LOCALAPPDATA%\starview\settings.json).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Corner {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub const ALL: [Corner; 4] = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomLeft,
        Corner::BottomRight,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Corner::TopLeft => "Top left",
            Corner::TopRight => "Top right",
            Corner::BottomLeft => "Bottom left",
            Corner::BottomRight => "Bottom right",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    /// Keep the overlay up on the base layer too.
    pub pin_base: bool,
    pub corner: Corner,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            pin_base: false,
            corner: Corner::TopRight,
        }
    }
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("starview").join("settings.json"))
}

pub fn load() -> Settings {
    path()
        .and_then(|p| fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save(settings: &Settings) {
    let Some(path) = path() else { return };
    let _ = fs::create_dir_all(path.parent().unwrap());
    match serde_json::to_vec_pretty(settings) {
        Ok(bytes) => {
            if let Err(err) = fs::write(&path, bytes) {
                eprintln!("failed to save settings: {err}");
            }
        }
        Err(err) => eprintln!("failed to serialize settings: {err}"),
    }
}
