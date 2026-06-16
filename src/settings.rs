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
    /// Custom overlay position (logical points), set by shift-dragging the
    /// panel's hamburger icon. Overrides `corner` until a corner is re-picked.
    pub position: Option<(f32, f32)>,
    /// Overlay opacity, percent.
    pub opacity: u8,
    /// Seconds of inactivity before the overlay fades away (0 = never).
    pub fade_secs: u16,
    /// Color pressed/afterglow key highlights as a moving rainbow.
    pub rainbow: bool,
    /// Tint each key by its lifetime press count (a usage heatmap).
    pub heatmap: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            pin_base: false,
            corner: Corner::TopRight,
            position: None,
            opacity: 100,
            fade_secs: 0,
            rainbow: false,
            heatmap: false,
        }
    }
}

/// Opacity choices offered in the tray menu.
pub const OPACITY_STEPS: [u8; 5] = [100, 85, 70, 55, 40];

/// Auto-fade choices offered in the tray menu: (label, seconds); 0 = never.
pub const FADE_STEPS: [(&str, u16); 7] = [
    ("Never", 0),
    ("5 seconds", 5),
    ("15 seconds", 15),
    ("30 seconds", 30),
    ("1 minute", 60),
    ("2 minutes", 120),
    ("5 minutes", 300),
];

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("starview").join("settings.json"))
}

pub fn load() -> Settings {
    let mut s: Settings = path()
        .and_then(|p| fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    // A hand-edited 0 would make the overlay invisible with no way back.
    s.opacity = s.opacity.clamp(20, 100);
    s
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
