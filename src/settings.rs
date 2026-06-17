//! Persisted user settings (%LOCALAPPDATA%\starview\settings.json).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Accent color theme for key ghosts, the trackball dot, and UI highlights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    Blue,
    Green,
    Purple,
    Amber,
    Pink,
}

impl Theme {
    pub const ALL: [Theme; 5] = [
        Theme::Blue,
        Theme::Green,
        Theme::Purple,
        Theme::Amber,
        Theme::Pink,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Theme::Blue => "Blue",
            Theme::Green => "Green",
            Theme::Purple => "Purple",
            Theme::Amber => "Amber",
            Theme::Pink => "Pink",
        }
    }

    /// The accent's sRGB bytes.
    pub fn accent(self) -> (u8, u8, u8) {
        match self {
            Theme::Blue => (110, 165, 255),
            Theme::Green => (110, 220, 140),
            Theme::Purple => (180, 140, 255),
            Theme::Amber => (255, 190, 90),
            Theme::Pink => (255, 130, 200),
        }
    }

    pub fn from_index(i: u8) -> Theme {
        Theme::ALL.get(i as usize).copied().unwrap_or(Theme::Blue)
    }
}

/// Time range for the press heatmap and its counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TimeWindow {
    #[default]
    AllTime,
    Today,
    Week,
}

impl TimeWindow {
    pub const ALL: [TimeWindow; 3] = [TimeWindow::AllTime, TimeWindow::Today, TimeWindow::Week];

    pub fn label(self) -> &'static str {
        match self {
            TimeWindow::AllTime => "All-time",
            TimeWindow::Today => "Today",
            TimeWindow::Week => "This week",
        }
    }
}

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
    /// Tint each key by its typo rate (presses immediately backspaced away),
    /// and list the worst offenders in the header.
    pub error_heatmap: bool,
    /// Overlay size, percent (scales the whole panel via the UI zoom factor).
    pub scale: u8,
    /// Accent color theme.
    pub theme: Theme,
    /// Which monitor to dock to (index into the primary-first monitor list).
    pub monitor: usize,
    /// Show a live words-per-minute readout in the header.
    pub show_wpm: bool,
    /// Show the per-finger load chart in the center gap.
    pub show_fingers: bool,
    /// Show the most frequent bigrams in the header.
    pub show_bigrams: bool,
    /// Show today's press count and the daily streak in the header.
    pub show_daily: bool,
    /// Time range for the press heatmap + its total counter.
    pub heatmap_range: TimeWindow,
    /// Show the most common substitution confusions in the header.
    pub show_subs: bool,
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
            error_heatmap: false,
            scale: 100,
            theme: Theme::Blue,
            monitor: 0,
            show_wpm: false,
            show_fingers: false,
            show_bigrams: false,
            show_daily: false,
            heatmap_range: TimeWindow::AllTime,
            show_subs: false,
        }
    }
}

/// Opacity choices offered in the tray menu.
pub const OPACITY_STEPS: [u8; 5] = [100, 85, 70, 55, 40];

/// Overlay-size choices offered in the tray menu (percent).
pub const SIZE_STEPS: [u8; 5] = [75, 100, 125, 150, 200];

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
    s.scale = s.scale.clamp(50, 200);
    // The three board-coloring modes are mutually exclusive; if a legacy or
    // hand-edited config set more than one, keep a single one by priority.
    if s.error_heatmap {
        s.heatmap = false;
        s.rainbow = false;
    } else if s.heatmap {
        s.rainbow = false;
    }
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
