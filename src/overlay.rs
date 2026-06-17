//! The overlay window: a frameless, transparent, click-through, always-on-top
//! bubble in the top-right corner naming the active non-base layer.
//!
//! Window behavior relies on raw Win32 extended styles (see `win32` below):
//! winit can't express NOACTIVATE/TOOLWINDOW, and it rewrites GWL_EXSTYLE
//! wholesale on its own flag changes, so the styles are re-asserted every
//! `logic` tick (a no-op compare once stable).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::Receiver;

use eframe::egui;
use egui::epaint::TextShape;
use egui::{Align2, Color32, CornerRadius, FontId, Rect, pos2, vec2};

use crate::hid::HidEvent;
use crate::oryx::{self, LayoutInfo};
use crate::settings::{self, Corner, Settings};
use crate::stats::{self, Stats};
use crate::{geometry, keycodes};

/// Everything the background threads feed into the UI.
pub enum AppEvent {
    Hid(HidEvent),
    /// Refreshed layout from the periodic Oryx re-fetch.
    Layout(LayoutInfo),
    /// Coalesced relative motion from the ZSA trackball (Navigator).
    Trackball(i32, i32),
    /// Settings changed from the tray menu.
    Settings(Settings),
    /// Clear all accumulated key stats (tray menu).
    ResetStats,
    /// Quit chosen from the tray menu.
    Quit,
}

pub const OVERLAY_W: f32 = 480.0;
pub const OVERLAY_H: f32 = 272.0;
/// Gap between the overlay window and the screen edge, in logical points.
const SCREEN_MARGIN: f32 = 12.0;
/// Rendered size of one key unit, in logical points.
const BOARD_SCALE: f32 = 26.0;

/// Window-level alpha for a key held down right now.
const HELD_ALPHA: u8 = 150;
/// Alpha a key's highlight starts at the instant it's released — softer than
/// held — then fades linearly to zero over `AFTERGLOW_SECS`.
const RELEASED_ALPHA: u8 = 95;
/// How long a released key keeps glowing before it's fully faded out.
const AFTERGLOW_SECS: f32 = 3.0;
/// How long the recent-layer breadcrumb stays up after a switch before fading.
const BREADCRUMB_SECS: f32 = 2.0;
/// Hue cycles per second for rainbow mode (full spectrum every ~8s), so a
/// press a second later lands on a clearly different color.
const RAINBOW_SPEED: f32 = 0.12;
/// Fraction of the transparency gap (toward fully opaque) to close when the
/// display is in HDR. SDR content is pinned to reference white while HDR
/// content behind it can be many times brighter, so the same window alpha
/// lets far more light show through and the dark panel washes out; raising
/// the effective opacity counters that. 0.0 = no change, 1.0 = force opaque.
const HDR_OPACITY_COMPENSATION: f32 = 0.55;

// rgba(16,18,28), fully opaque: the tray "Opacity" control (window-level
// alpha) fades the whole overlay down from here, so the panel must start
// solid for "100%" to actually read as opaque.
const PANEL_BG: Color32 = Color32::from_rgb(16, 18, 28);
const TEXT_BRIGHT: Color32 = Color32::from_rgb(240, 240, 255);

/// Theme colors; HDR mode swaps to a high-contrast variant because SDR
/// surfaces render at reference white while HDR content goes far brighter,
/// washing out the translucent dark theme.
struct Palette {
    panel_bg: Color32,
    text: Color32,
    text_inherited: Color32,
    hold_text: Color32,
    key_fill: Color32,
    key_blank: Color32,
    /// Accent for key ghosts, the trackball dot, and UI highlights (themable).
    accent: Color32,
}

/// Selected accent theme (`settings::Theme` discriminant), set by the overlay
/// so the free-standing draw helpers can read it without threading it through.
static THEME: AtomicU8 = AtomicU8::new(0);

fn set_theme(theme: settings::Theme) {
    THEME.store(theme as u8, Ordering::Relaxed);
}

fn accent_color() -> Color32 {
    let (r, g, b) = settings::Theme::from_index(THEME.load(Ordering::Relaxed)).accent();
    Color32::from_rgb(r, g, b)
}

fn palette() -> Palette {
    #[cfg(windows)]
    let hdr = crate::hdr::active();
    #[cfg(not(windows))]
    let hdr = false;
    if hdr {
        Palette {
            panel_bg: Color32::from_rgb(16, 18, 28),
            text: Color32::WHITE,
            text_inherited: Color32::from_rgba_unmultiplied(225, 228, 240, 200),
            hold_text: Color32::from_rgba_unmultiplied(225, 228, 250, 255),
            key_fill: Color32::from_rgba_unmultiplied(255, 255, 255, 60),
            key_blank: Color32::from_rgba_unmultiplied(255, 255, 255, 22),
            accent: accent_color(),
        }
    } else {
        Palette {
            panel_bg: PANEL_BG,
            text: TEXT_BRIGHT,
            text_inherited: Color32::from_rgba_unmultiplied(200, 205, 220, 140),
            hold_text: Color32::from_rgba_unmultiplied(200, 205, 235, 200),
            key_fill: Color32::from_rgba_unmultiplied(255, 255, 255, 32),
            key_blank: Color32::from_rgba_unmultiplied(255, 255, 255, 10),
            accent: accent_color(),
        }
    }
}

pub struct OverlayApp {
    events: Receiver<AppEvent>,
    layout: Option<LayoutInfo>,
    layer: u8,
    /// Recently-entered layers (layer, entered_at) for the fading breadcrumb
    /// shown briefly after a switch. Newest last; cleared once faded.
    layer_trail: Vec<(u8, std::time::Instant)>,
    /// Oryx key indices currently held down on the physical board.
    pressed: HashSet<usize>,
    /// Recently released keys -> release time; they afterglow and fade out.
    released: HashMap<usize, std::time::Instant>,
    /// Smoothed trackball motion vector (unit-clamped); decays toward zero.
    ball: egui::Vec2,
    /// Recent smoothed ball vectors (oldest first) for the motion trail.
    ball_trail: Vec<egui::Vec2>,
    /// Whether a ZSA pointing device has ever produced motion.
    ball_seen: bool,
    /// Previous `logic` tick, for time-based (tick-rate-independent) decay.
    last_tick: Option<std::time::Instant>,
    connected: bool,
    positioned: bool,
    shown: bool,
    /// Keep the overlay up on the base layer too (tray toggle / --always).
    always: bool,
    corner: Corner,
    /// Shift-dragged position (logical points); overrides corner docking.
    custom_pos: Option<egui::Pos2>,
    /// Hamburger drag-handle rect from the last drawn frame, window-local
    /// points, already expanded to a comfortable hit target.
    burger: Option<Rect>,
    /// X close-button rect from the last drawn frame, window-local points,
    /// expanded to a comfortable hit target.
    close_btn: Option<Rect>,
    /// Shift is held with the cursor over the hamburger (drag affordance).
    hot: bool,
    /// Shift is held with the cursor over the close button.
    close_hot: bool,
    /// Active drag: cursor-to-window-origin offset, in points.
    drag: Option<egui::Vec2>,
    /// Left-button state last tick, for press edge detection.
    prev_button: bool,
    /// Overlay opacity, percent (tray setting).
    opacity: u8,
    /// Window-level alpha last pushed via SetLayeredWindowAttributes.
    applied_alpha: Option<u8>,
    /// Inactivity before the overlay fades away (None = never).
    fade_after: Option<std::time::Duration>,
    /// Last layer change or key press, for the auto-fade timer.
    last_activity: std::time::Instant,
    /// Rainbow-color the key ghosts (tray toggle).
    rainbow: bool,
    /// Process start, for the rainbow's time phase.
    start: std::time::Instant,
    /// Hue (0..1) captured for each lit key at the moment it was pressed, so
    /// keys pressed together share a color and the hue advances over time.
    key_hue: HashMap<usize, f32>,
    /// Test override (STARVIEW_FORCE_LAYER): pretend this layer is active.
    force_layer: Option<u8>,
    /// Lifetime per-key press counts; drives the heatmap + total counter.
    stats: Stats,
    /// Tint each key by its press count (tray toggle).
    heatmap: bool,
    /// Tint each key by its typo rate + list worst offenders (tray toggle).
    error_heatmap: bool,
    /// Overlay size percent (UI zoom factor); 100 = default.
    scale: u8,
    /// Last scale pushed to the viewport, so a change re-zooms + resizes once.
    applied_scale: Option<u8>,
    /// Recent character-producing presses (Oryx key index + foreground window
    /// at press time), newest last. A backspace pops this to attribute the
    /// deletion; the per-entry window guards against counting edits made after
    /// switching apps. Bounded — typos get corrected within a few keystrokes.
    typed: Vec<(usize, isize)>,
    /// Cursor position at the previous keystroke. A change means the mouse (or
    /// the ZSA trackball, which also drives the OS cursor) moved the caret, so
    /// the typo buffer no longer reflects what a backspace would delete.
    last_cursor: Option<(i32, i32)>,
    /// Press counts changed since the last disk write.
    stats_dirty: bool,
    /// Last time `stats` was flushed to disk (debounces rapid typing).
    stats_saved: std::time::Instant,
}

impl OverlayApp {
    pub fn new(
        events: Receiver<AppEvent>,
        layout: Option<LayoutInfo>,
        settings: Settings,
        stats: Stats,
    ) -> Self {
        set_theme(settings.theme);
        Self {
            events,
            layout,
            layer: 0,
            layer_trail: Vec::new(),
            pressed: HashSet::new(),
            released: HashMap::new(),
            ball: egui::Vec2::ZERO,
            ball_trail: Vec::new(),
            ball_seen: false,
            last_tick: None,
            connected: false,
            positioned: false,
            shown: false,
            always: settings.pin_base,
            corner: settings.corner,
            custom_pos: settings.position.map(|(x, y)| pos2(x, y)),
            burger: None,
            close_btn: None,
            hot: false,
            close_hot: false,
            drag: None,
            prev_button: false,
            opacity: settings.opacity,
            applied_alpha: None,
            fade_after: (settings.fade_secs > 0)
                .then(|| std::time::Duration::from_secs(settings.fade_secs as u64)),
            last_activity: std::time::Instant::now(),
            rainbow: settings.rainbow,
            start: std::time::Instant::now(),
            key_hue: HashMap::new(),
            force_layer: std::env::var("STARVIEW_FORCE_LAYER")
                .ok()
                .and_then(|v| v.parse().ok()),
            stats,
            heatmap: settings.heatmap,
            error_heatmap: settings.error_heatmap,
            scale: settings.scale,
            applied_scale: None,
            typed: Vec::new(),
            last_cursor: None,
            stats_dirty: false,
            stats_saved: std::time::Instant::now(),
        }
    }

    /// The effective tap keycode for a physical key index on the active layer,
    /// following KC_TRANSPARENT fall-through to the base layer. None if there's
    /// no layout, no such key, or the slot is a macro/empty.
    fn effective_code(&self, i: usize) -> Option<String> {
        let layout = self.layout.as_ref()?;
        let on = |pos: usize| {
            layout
                .layers
                .iter()
                .find(|l| l.position == pos)
                .and_then(|l| l.keys.get(i))
                .and_then(|k| k.tap.as_ref())
                .filter(|a| a.macro_.is_none())
                .and_then(|a| a.code.clone())
        };
        match on(self.layer as usize).as_deref() {
            // Transparent / unmapped falls through to the base layer in QMK.
            None | Some("KC_TRANSPARENT" | "KC_TRNS") => on(0),
            _ => on(self.layer as usize),
        }
    }

    /// Feed a keypress to the typo tracker. Character keys go on a small
    /// recent-typing buffer; a backspace pops the buffer and blames that key —
    /// but only when the foreground window hasn't changed since it was typed,
    /// so editing in another app doesn't masquerade as correcting a typo here.
    /// Caret-moving keys clear the buffer; everything else leaves it intact.
    ///
    /// A held Ctrl/Alt/Win makes the press a shortcut, not text: Ctrl+C and
    /// friends clear the buffer, and Ctrl+Backspace (word-delete) is dropped
    /// rather than blamed on the single key before it.
    fn track_typo(&mut self, i: usize) {
        const TYPED_CAP: usize = 64;
        // Mouse use (external mouse or the ZSA trackball — both drive the OS
        // cursor — plus any held mouse button) repositions the caret, so we
        // can't tell what a backspace removes. Drop the buffer when the cursor
        // moved since the last keystroke or a button is down. (Arrow keys are
        // handled separately: they classify as Break, which also clears it.)
        let (cursor, button) = pointer_state();
        if pointer_invalidates(self.last_cursor, cursor, button) {
            self.typed.clear();
        }
        self.last_cursor = cursor;
        let kind = self
            .effective_code(i)
            .as_deref()
            .map(keycodes::classify)
            .unwrap_or(keycodes::KeyKind::Other);
        let shortcut = shortcut_mods_down();
        let fg = foreground_window();
        match kind {
            // A modified character key (Ctrl+C, Ctrl+V, …) isn't typed text and
            // may have changed the buffer behind our back, so reset.
            keycodes::KeyKind::Text if shortcut => self.typed.clear(),
            keycodes::KeyKind::Text => {
                self.typed.push((i, fg));
                let overflow = self.typed.len().saturating_sub(TYPED_CAP);
                if overflow > 0 {
                    self.typed.drain(0..overflow);
                }
            }
            // Ctrl/Alt+Backspace deletes a whole word, not a single mistyped
            // char — don't pin that on the last key; just invalidate the buffer.
            keycodes::KeyKind::Backspace if shortcut => self.typed.clear(),
            keycodes::KeyKind::Backspace => {
                if let Some((prev_i, prev_fg)) = self.typed.pop()
                    && prev_fg == fg
                {
                    self.stats.record_delete(prev_i);
                    self.stats_dirty = true;
                }
            }
            keycodes::KeyKind::Break => self.typed.clear(),
            keycodes::KeyKind::Other => {}
        }
    }

    /// Header text: layer number + name, like the Oryx configurator.
    fn label(&self) -> String {
        match self
            .layout
            .as_ref()
            .and_then(|l| l.layer_name(self.layer as usize))
        {
            Some(name) => format!("{}: {}", self.layer, name),
            None => format!("Layer {}", self.layer),
        }
    }

    /// A small fading "2 › 5 › 1" trail of recently-visited layers, centered at
    /// the top of the panel for a couple seconds after a switch.
    fn draw_breadcrumb(&self, ui: &egui::Ui, panel: Rect) {
        let Some(&(_, last)) = self.layer_trail.last() else { return };
        let age = last.elapsed().as_secs_f32();
        if self.layer_trail.len() < 2 || age >= BREADCRUMB_SECS {
            return;
        }
        let fade = (1.0 - age / BREADCRUMB_SECS).clamp(0.0, 1.0);
        let text = self
            .layer_trail
            .iter()
            .map(|(l, _)| l.to_string())
            .collect::<Vec<_>>()
            .join("  \u{203A}  ");
        let p = palette();
        let painter = ui.painter();
        let tcol = Color32::from_rgba_unmultiplied(
            p.text.r(),
            p.text.g(),
            p.text.b(),
            (fade * 255.0) as u8,
        );
        let galley = painter.layout_no_wrap(text, FontId::proportional(12.0), tcol);
        let pad = vec2(8.0, 3.0);
        let rect = Rect::from_center_size(
            pos2(panel.center().x, panel.min.y + 13.0),
            galley.size() + pad * 2.0,
        );
        painter.rect_filled(
            rect,
            CornerRadius::same(7),
            Color32::from_rgba_unmultiplied(40, 44, 60, (fade * 170.0) as u8),
        );
        painter.galley(rect.min + pad, galley, tcol);
    }
}

impl eframe::App for OverlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    /// Runs even while the window is hidden (whenever a repaint is requested —
    /// the HID watcher requests one per event), so show/hide decisions live here.
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Drain into a Vec first so the loop body can call &mut self methods
        // (e.g. track_typo) without holding a borrow on self.events.
        let events: Vec<AppEvent> = self.events.try_iter().collect();
        for event in events {
            match event {
                AppEvent::Hid(HidEvent::Layer(idx)) => {
                    // Only a genuine change is activity — the watcher re-emits
                    // the current layer on each idle re-pair, which must not
                    // keep resetting the auto-fade timer.
                    if idx != self.layer {
                        let t = std::time::Instant::now();
                        self.last_activity = t;
                        // Seed the trail with where we came from on the first
                        // hop, so the breadcrumb always shows at least a pair.
                        if self.layer_trail.is_empty() {
                            self.layer_trail.push((self.layer, t));
                        }
                        self.layer_trail.push((idx, t));
                        const TRAIL_MAX: usize = 5;
                        let overflow = self.layer_trail.len().saturating_sub(TRAIL_MAX);
                        if overflow > 0 {
                            self.layer_trail.drain(0..overflow);
                        }
                    }
                    self.layer = idx;
                    self.connected = true;
                }
                AppEvent::Hid(HidEvent::KeyDown { row, col }) => {
                    self.last_activity = std::time::Instant::now();
                    if let Some(i) = geometry::key_index_for_matrix(row, col) {
                        self.pressed.insert(i);
                        self.released.remove(&i);
                        // Tally the lifetime press count (heatmap + counter).
                        self.stats.record(i);
                        self.stats_dirty = true;
                        self.track_typo(i);
                        // Capture the current rainbow hue so this key (and its
                        // afterglow) keeps the color of the moment it was hit.
                        let phase =
                            (self.start.elapsed().as_secs_f32() * RAINBOW_SPEED).rem_euclid(1.0);
                        self.key_hue.insert(i, phase);
                    }
                }
                AppEvent::Hid(HidEvent::KeyUp { row, col }) => {
                    if let Some(i) = geometry::key_index_for_matrix(row, col) {
                        self.pressed.remove(&i);
                        self.released.insert(i, std::time::Instant::now());
                    }
                }
                AppEvent::Hid(HidEvent::Disconnected) => {
                    self.connected = false;
                    self.pressed.clear();
                    self.released.clear();
                    self.key_hue.clear();
                    self.typed.clear();
                }
                AppEvent::Layout(info) => self.layout = Some(info),
                AppEvent::Settings(s) => {
                    self.always = s.pin_base;
                    self.opacity = s.opacity;
                    self.fade_after = (s.fade_secs > 0)
                        .then(|| std::time::Duration::from_secs(s.fade_secs as u64));
                    self.rainbow = s.rainbow;
                    self.heatmap = s.heatmap;
                    self.error_heatmap = s.error_heatmap;
                    self.scale = s.scale;
                    set_theme(s.theme);
                    // Re-show at full opacity and restart the timer on change.
                    self.last_activity = std::time::Instant::now();
                    let pos = s.position.map(|(x, y)| pos2(x, y));
                    if self.corner != s.corner || self.custom_pos != pos {
                        self.corner = s.corner;
                        self.custom_pos = pos;
                        self.positioned = false; // re-anchor on next tick
                    }
                }
                AppEvent::ResetStats => {
                    self.stats = Stats::default();
                    self.typed.clear();
                    stats::save(&self.stats);
                    self.stats_dirty = false;
                }
                AppEvent::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                AppEvent::Trackball(dx, dy) => {
                    self.ball_seen = true;
                    // Pointing the trackball moves the caret target — abandon
                    // the typo buffer so a later backspace isn't misattributed.
                    self.typed.clear();
                    // Ease toward this motion window's direction instead of
                    // adding raw deltas — individual ~25ms windows are noisy
                    // and made the dot jitter during momentum spins.
                    let mut target = vec2(dx as f32, dy as f32) * 0.03;
                    let len = target.length();
                    if len > 1.0 {
                        target /= len;
                    }
                    self.ball += (target - self.ball) * 0.4;
                }
            }
        }

        // Glide the trackball dot back to center: time-based half-life so the
        // decay is identical whether the UI ticks at 1 Hz (idle) or ~40 Hz
        // (during motion) — per-tick decay fought the event stream and
        // jittered.
        let now = std::time::Instant::now();
        let dt = self.last_tick.map_or(0.0, |t| (now - t).as_secs_f32());
        self.last_tick = Some(now);
        if self.ball != egui::Vec2::ZERO {
            self.ball *= 0.5f32.powf(dt / 0.12);
            if self.ball.length() > 0.02 {
                ctx.request_repaint_after(std::time::Duration::from_millis(30));
            } else {
                self.ball = egui::Vec2::ZERO;
            }
        }

        // Motion trail: record the dot's recent path (capped), so the renderer
        // can draw a fading comet streak. Cleared once the ball settles back
        // to center, where every sample would sit anyway.
        const TRAIL_LEN: usize = 16;
        if self.ball != egui::Vec2::ZERO {
            self.ball_trail.push(self.ball);
            let overflow = self.ball_trail.len().saturating_sub(TRAIL_LEN);
            if overflow > 0 {
                self.ball_trail.drain(0..overflow);
            }
        } else {
            self.ball_trail.clear();
        }

        // Drop fully-faded afterglows; keep animating while any remain.
        self.released
            .retain(|_, t| now.duration_since(*t).as_secs_f32() < AFTERGLOW_SECS);
        if !self.released.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        // Drop captured hues for keys that are no longer lit.
        self.key_hue
            .retain(|i, _| self.pressed.contains(i) || self.released.contains_key(i));

        // Layer breadcrumb: animate the fade, then clear once fully faded.
        if let Some(&(_, last)) = self.layer_trail.last() {
            if last.elapsed().as_secs_f32() >= BREADCRUMB_SECS {
                self.layer_trail.clear();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(30));
            }
        }

        // Flush press counts to disk, debounced so a burst of typing writes
        // once rather than per keystroke. on_exit catches anything still dirty.
        const STATS_SAVE_AFTER: std::time::Duration = std::time::Duration::from_secs(5);
        if self.stats_dirty {
            let since = now.duration_since(self.stats_saved);
            if since >= STATS_SAVE_AFTER {
                stats::save(&self.stats);
                self.stats_dirty = false;
                self.stats_saved = now;
            } else {
                // Make sure the flush actually fires even if typing stops.
                ctx.request_repaint_after(STATS_SAVE_AFTER - since);
            }
        }

        // Apply the overlay-size zoom and resize the window to match, once per
        // change. The zoom factor scales all UI content; the window grows by
        // the same factor so the panel still fits, then re-anchors.
        if self.applied_scale != Some(self.scale) {
            let z = self.scale as f32 / 100.0;
            ctx.set_zoom_factor(z);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                vec2(OVERLAY_W, OVERLAY_H) * z,
            ));
            self.applied_scale = Some(self.scale);
            self.positioned = false; // re-anchor at the new size
        }

        // Pin to the shift-dragged spot if there is one, else to the
        // configured corner once the monitor size is known.
        if !self.positioned {
            if let Some(pos) = self.custom_pos {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
                self.positioned = true;
            } else if let Some(size) = ctx.input(|i| i.viewport().monitor_size) {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(corner_pos(
                    self.corner,
                    size,
                )));
                self.positioned = true;
            }
        }

        if let Some(forced) = self.force_layer {
            self.layer = forced;
            self.connected = true;
        }
        // The window stays permanently visible (it's transparent and
        // click-through); "hiding" means drawing nothing. A raw-hidden window
        // stops presenting, so its swapchain kept the previous layer's frame
        // and flashed it on re-show — content-level hiding swaps in a single
        // frame instead. Forced mode also shows layer 0 for screenshots.
        self.shown = self.force_layer.is_some()
            || self.always
            || (self.connected && self.layer != 0);

        // Shift-drag via the panel's hamburger icon. The window is normally
        // click-through (WS_EX_TRANSPARENT) and never receives real mouse
        // input, so global cursor/key state is polled instead; click-through
        // is dropped only while Shift is over the icon (or a drag is live) so
        // that specific click can't fall through to the window underneath.
        #[cfg(windows)]
        let interactive = {
            let shift = win32::shift_down();
            let button = win32::lbutton_down();
            let ppp = ctx.pixels_per_point();
            let cursor = win32::cursor_pos().map(|(x, y)| pos2(x as f32 / ppp, y as f32 / ppp));
            let window = ctx.input(|i| i.viewport().inner_rect);
            // Is the cursor over a window-local control rect? (controls are
            // stored in window-local points; the cursor is desktop-logical.)
            let over = |r: Option<Rect>| match (cursor, window, r) {
                (Some(c), Some(win), Some(rr)) => rr.translate(win.min.to_vec2()).contains(c),
                _ => false,
            };
            self.hot = self.drag.is_none() && shift && over(self.burger);
            self.close_hot = self.drag.is_none() && shift && over(self.close_btn);
            if let Some(offset) = self.drag {
                if let (true, Some(c)) = (button, cursor) {
                    let mut target = c - offset;
                    // Keep the whole window within the monitor under the
                    // cursor, so it can't be dragged off into empty space
                    // (dragging onto an adjacent monitor still works — it
                    // clamps to whichever monitor the cursor is over).
                    if let Some((l, t, r, b)) =
                        win32::monitor_rect_for_point((c.x * ppp) as i32, (c.y * ppp) as i32)
                    {
                        let (l, t, r, b) =
                            (l as f32 / ppp, t as f32 / ppp, r as f32 / ppp, b as f32 / ppp);
                        target.x = target.x.clamp(l, (r - OVERLAY_W).max(l));
                        target.y = target.y.clamp(t, (b - OVERLAY_H).max(t));
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(target));
                } else {
                    // Drag finished: persist the spot (overrides corner
                    // docking until a corner is re-picked in the tray).
                    self.drag = None;
                    if let Some(win) = window {
                        self.custom_pos = Some(win.min);
                        let mut s = settings::load();
                        s.position = Some((win.min.x, win.min.y));
                        settings::save(&s);
                    }
                }
            } else if button && !self.prev_button {
                // Fresh left-press while a control is armed (Shift held over
                // it): the X closes, the hamburger starts a drag.
                if self.close_hot {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else if self.hot
                    && let (Some(c), Some(win)) = (cursor, window)
                {
                    self.drag = Some(c - win.min);
                }
            }
            self.prev_button = button;
            if shift || self.drag.is_some() {
                // Track the cursor smoothly while a drag is possible/live.
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
            self.hot || self.close_hot || self.drag.is_some()
        };

        #[cfg(windows)]
        {
            // Auto-fade: once idle past the timeout, ramp alpha to zero over a
            // short fade. Activity (layer change / keypress / drag) resets the
            // timer and snaps back to full. Never fades in forced mode.
            const FADE_SECS: f32 = 0.5;
            if interactive {
                self.last_activity = now;
            }
            let fade = match self.fade_after {
                Some(timeout) if self.force_layer.is_none() => {
                    let idle = now.duration_since(self.last_activity);
                    if idle < timeout {
                        // Wake right when the timeout elapses to start fading.
                        ctx.request_repaint_after(timeout - idle);
                        1.0
                    } else {
                        let into = (idle - timeout).as_secs_f32();
                        let f = (1.0 - into / FADE_SECS).clamp(0.0, 1.0);
                        if f > 0.0 {
                            ctx.request_repaint_after(std::time::Duration::from_millis(30));
                        }
                        f
                    }
                }
                _ => 1.0,
            };
            // Under HDR, raise the effective opacity to offset the washout
            // (see HDR_OPACITY_COMPENSATION) so a given setting reads the same
            // as on an SDR display.
            let opacity = if crate::hdr::active() {
                let gap = 100.0 - self.opacity as f32;
                self.opacity as f32 + gap * HDR_OPACITY_COMPENSATION
            } else {
                self.opacity as f32
            };
            let alpha = (opacity / 100.0 * 255.0 * fade).round() as u8;
            win32::assert_overlay_styles(
                frame,
                !interactive,
                alpha,
                self.applied_alpha != Some(alpha),
            );
            self.applied_alpha = Some(alpha);
        }

        if self.shown {
            // Poll often enough that holding Shift over the hamburger turns
            // interactive before the click lands.
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
        // Low-rate heartbeat so the style assert self-heals even when no HID
        // events arrive.
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.shown {
            self.burger = None;
            self.close_btn = None;
            return;
        }
        let title = self.label();
        let keys = self
            .layout
            .as_ref()
            .and_then(|l| l.layers.iter().find(|ly| ly.position == self.layer as usize))
            .map(|ly| ly.keys.as_slice())
            .unwrap_or(&[]);

        // Only render the board when the layout's key list lines up with the
        // geometry table; otherwise fall back to the name-only bubble.
        // Base-layer keys, for KC_TRANSPARENT fall-through labels.
        let base = (self.layer != 0)
            .then(|| {
                self.layout
                    .as_ref()
                    .and_then(|l| l.layers.iter().find(|ly| ly.position == 0))
                    .map(|ly| ly.keys.as_slice())
            })
            .flatten()
            .filter(|b| b.len() == geometry::MOONLANDER_KEYS.len());

        // Show the trackball widget once a ZSA pointing device has moved
        // (forced mode always shows it, for screenshots).
        let ball = (self.ball_seen || self.force_layer.is_some()).then_some(self.ball);

        // Hug the same corner of the window that the window hugs on screen.
        let align = match self.corner {
            Corner::TopLeft => Align2::LEFT_TOP,
            Corner::TopRight => Align2::RIGHT_TOP,
            Corner::BottomLeft => Align2::LEFT_BOTTOM,
            Corner::BottomRight => Align2::RIGHT_BOTTOM,
        };

        // When rainbow mode is on, pass the per-key captured hues.
        let key_hue = self.rainbow.then_some(&self.key_hue);
        // When a stats heatmap is on, pass the counts. The typo heatmap takes
        // precedence over the press heatmap for the board coloring + header.
        let error = self.error_heatmap.then_some(&self.stats);
        let heatmap = self.heatmap.then_some(&self.stats);
        let panel = if !keys.is_empty() && keys.len() == geometry::MOONLANDER_KEYS.len() {
            draw_board(
                ui,
                &title,
                keys,
                base,
                &self.pressed,
                &self.released,
                ball,
                &self.ball_trail,
                key_hue,
                heatmap,
                error,
                align,
            )
        } else {
            draw_name_bubble(ui, &title, align)
        };
        // Top-right controls: hamburger (Shift-drag to move) and, to its left,
        // an X (Shift+click to quit). Both sit in the header's right margin.
        let icon = vec2(15.0, 15.0);
        let burger_rect =
            Rect::from_min_size(pos2(panel.max.x - 8.0 - icon.x, panel.min.y + 8.0), icon);
        let close_rect =
            Rect::from_min_size(pos2(burger_rect.min.x - 10.0 - icon.x, panel.min.y + 8.0), icon);
        draw_burger(ui.painter(), burger_rect, self.hot || self.drag.is_some());
        draw_close(ui.painter(), close_rect, self.close_hot);
        self.burger = Some(burger_rect.expand(5.0));
        self.close_btn = Some(close_rect.expand(5.0));

        self.draw_breadcrumb(ui, panel);
    }

    /// Flush any unsaved press counts when the app closes cleanly (tray Quit /
    /// updater relaunch both go through a viewport Close).
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.stats_dirty {
            stats::save(&self.stats);
            self.stats_dirty = false;
        }
    }
}

/// Tint of a header control glyph: highlighted when armed (Shift over it).
fn control_color(hot: bool) -> Color32 {
    if hot {
        palette().text
    } else {
        Color32::from_rgba_unmultiplied(200, 205, 220, 110)
    }
}

/// Soft accent backing drawn behind a header control while it's armed.
fn control_bg(painter: &egui::Painter, rect: Rect, hot: bool) {
    if hot {
        let a = accent_color();
        painter.rect_filled(
            rect.expand(4.0),
            CornerRadius::same(4),
            Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), 70),
        );
    }
}

/// Hamburger drag handle — hold Shift and drag it with the left button.
fn draw_burger(painter: &egui::Painter, rect: Rect, hot: bool) {
    control_bg(painter, rect, hot);
    let color = control_color(hot);
    let lines = rect.shrink2(vec2(1.0, 3.5));
    for t in [0.0, 0.5, 1.0] {
        let y = lines.min.y + lines.height() * t;
        painter.line_segment(
            [pos2(lines.min.x, y), pos2(lines.max.x, y)],
            egui::Stroke::new(1.5, color),
        );
    }
}

/// X close button — Shift+click quits starview.
fn draw_close(painter: &egui::Painter, rect: Rect, hot: bool) {
    control_bg(painter, rect, hot);
    let color = control_color(hot);
    let x = rect.shrink(2.0);
    painter.line_segment([x.min, x.max], egui::Stroke::new(1.5, color));
    painter.line_segment(
        [pos2(x.min.x, x.max.y), pos2(x.max.x, x.min.y)],
        egui::Stroke::new(1.5, color),
    );
}

/// Single characters get a larger font than multi-char text labels: letters
/// and digits a bit larger, symbol glyphs (⏎, ␣, ⇧, arrows, punctuation) —
/// which render visually smaller — larger still.
fn label_font_size(label: &str, text: f32, letter: f32, symbol: f32) -> f32 {
    let mut chars = label.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphanumeric() => letter,
        (Some(_), None) => symbol,
        _ => text,
    }
}

/// A few modifier/whitespace labels render small relative to how important
/// the key is — the size tiers above don't make them legible (the ⇧ glyph
/// sits tiny in its em box; "Ctl"/"Esc" only qualify for the text tier). Give
/// those specific labels a fixed, larger font. `scale` matches the tier sizes
/// (1.0 for primary labels, smaller for hold hints).
fn emphasized_font_size(label: &str, scale: f32) -> Option<f32> {
    Some(scale * match label {
        "\u{21E7}" => 26.0,     // ⇧ shift
        "Ctl" | "RCtl" => 14.5, // control
        "\u{2423}" => 16.5,     // ␣ space
        "Esc" => 12.0,          // escape
        "\u{23CE}" => 16.5,     // ⏎ enter
        _ => return None,
    })
}

/// Color for a pressed/afterglow key ghost. Normally the fixed blue accent;
/// in rainbow mode the key's hue captured at press time (HSL with a fixed
/// lightness so every hue reads at the same brightness — HSV would make
/// yellow/cyan glow far brighter), at the given alpha.
fn ghost_color(key_hue: Option<&HashMap<usize, f32>>, i: usize, alpha: u8, accent: Color32) -> Color32 {
    match key_hue.and_then(|m| m.get(&i)) {
        Some(&hue) => {
            let (r, g, b) = hsl_rgb(hue, 0.85, 0.62);
            Color32::from_rgba_unmultiplied(r, g, b, alpha)
        }
        None => Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha),
    }
}

/// Heatmap fill for a key: cold blue when rarely pressed, hot red for the
/// most-pressed key, log-scaled so a handful of very high keys (space, e…)
/// don't flatten everything else to the same color. Never-pressed keys keep
/// the normal dim blank fill so the board still reads as a board.
fn heat_fill(count: u64, max: u64, p: &Palette) -> Color32 {
    if count == 0 || max == 0 {
        return p.key_blank;
    }
    let t = (count as f32 + 1.0).ln() / (max as f32 + 1.0).ln();
    let (r, g, b) = hsl_rgb(0.66 * (1.0 - t), 0.85, 0.55);
    // More opaque the hotter the key, so frequency reads at a glance.
    let alpha = (70.0 + 150.0 * t).round() as u8;
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

/// Heatmap fill for a key's typo rate: hot red for the most error-prone key,
/// scaled against the worst key so the gradient uses its full range (typo
/// rates are all small). Keys with enough presses and no deletions show a
/// faint green ("clean"); keys without enough data to judge stay blank.
fn error_fill(rate: Option<f32>, max: f32, p: &Palette) -> Color32 {
    match rate {
        Some(r) if r > 0.0 && max > 0.0 => {
            let t = (r / max).clamp(0.0, 1.0);
            let (rr, gg, bb) = hsl_rgb(0.66 * (1.0 - t), 0.85, 0.55);
            let alpha = (70.0 + 150.0 * t).round() as u8;
            Color32::from_rgba_unmultiplied(rr, gg, bb, alpha)
        }
        Some(_) => Color32::from_rgba_unmultiplied(80, 200, 120, 30),
        None => p.key_blank,
    }
}

/// Group an integer with thousands separators ("12345" -> "12,345").
fn group_thousands(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, &c) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c as char);
    }
    out
}

/// HSL (each 0..1, hue wraps) to sRGB bytes.
fn hsl_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h = h.rem_euclid(1.0) * 6.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as i32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

/// Parses an Oryx "#rrggbb" color.
fn parse_hex_color(s: &str) -> Option<Color32> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn corner_pos(corner: Corner, monitor: egui::Vec2) -> egui::Pos2 {
    let m = SCREEN_MARGIN;
    // Extra clearance for the taskbar on bottom corners.
    let bottom = monitor.y - OVERLAY_H - 52.0;
    let right = monitor.x - OVERLAY_W - m;
    match corner {
        Corner::TopLeft => pos2(m, m),
        Corner::TopRight => pos2(right, m),
        Corner::BottomLeft => pos2(m, bottom),
        Corner::BottomRight => pos2(right, bottom),
    }
}

fn draw_name_bubble(ui: &mut egui::Ui, title: &str, align: Align2) -> Rect {
    let p = palette();
    let painter = ui.painter();
    let galley = painter.layout_no_wrap(title.to_owned(), FontId::proportional(24.0), p.text);
    let pad = vec2(18.0, 11.0);
    // Extra width on the right so the close + hamburger controls have room.
    let size = galley.size() + pad * 2.0 + vec2(56.0, 0.0);
    let rect = align.align_size_within_rect(size, ui.max_rect());
    painter.rect_filled(rect, CornerRadius::same(13), p.panel_bg);
    painter.galley(rect.min + pad, galley, Color32::WHITE);
    rect
}

fn draw_board(
    ui: &mut egui::Ui,
    title: &str,
    keys: &[oryx::Key],
    base: Option<&[oryx::Key]>,
    pressed: &HashSet<usize>,
    released: &HashMap<usize, std::time::Instant>,
    ball: Option<egui::Vec2>,
    trail: &[egui::Vec2],
    key_hue: Option<&HashMap<usize, f32>>,
    heatmap: Option<&Stats>,
    error: Option<&Stats>,
    align: Align2,
) -> Rect {
    let now = std::time::Instant::now();
    // Hottest key, for normalizing the press heatmap's color scale.
    let heat_max = heatmap.map(Stats::max).unwrap_or(0);
    // Worst typo rate, for normalizing the error heatmap's scale.
    let error_max = error
        .map(|s| {
            (0..keys.len())
                .filter_map(|i| s.error_rate(i))
                .fold(0.0_f32, f32::max)
        })
        .unwrap_or(0.0);
    let pad = 12.0;
    let header_h = 24.0;
    let board_size = vec2(geometry::BOARD_WIDTH_U, geometry::BOARD_HEIGHT_U) * BOARD_SCALE;
    // A little extra height: the enlarged thumb clusters extend past the
    // nominal board box at the bottom.
    let size = board_size + vec2(pad * 2.0, header_h + pad * 2.0 + 5.0);
    let rect = align.align_size_within_rect(size, ui.max_rect());
    let p = palette();
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(13), p.panel_bg);
    painter.text(
        rect.min + vec2(pad + 2.0, pad - 2.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(15.0),
        p.text,
    );
    // Stats readout in the header, right-aligned so it clears the controls
    // (close + hamburger) in the top-right corner. The typo heatmap shows the
    // worst offenders; otherwise the press heatmap shows a running total.
    let readout_at = pos2(rect.max.x - 56.0, rect.min.y + pad - 1.0);
    if let Some(stats) = error {
        // Resolve a key's character label, following transparency to base.
        let label_for = |i: usize| -> String {
            let l = keys.get(i).map(key_text).unwrap_or_default();
            if l.is_empty() {
                base.and_then(|b| b.get(i)).map(key_text).unwrap_or_default()
            } else {
                l
            }
        };
        let mut top: Vec<(String, f32)> = (0..keys.len())
            .filter_map(|i| stats.error_rate(i).map(|r| (i, r)))
            .filter(|&(_, r)| r > 0.0)
            // Skip blank/unlabeled keys (KC_NO etc.) — you can't mistype those.
            .filter_map(|(i, r)| {
                let l = label_for(i);
                (!l.is_empty()).then_some((l, r))
            })
            .collect();
        top.sort_by(|a, b| b.1.total_cmp(&a.1));
        top.truncate(3);
        let text = if top.is_empty() {
            "\u{232B} no typos yet".to_owned()
        } else {
            let parts: Vec<String> = top
                .iter()
                .map(|(label, r)| format!("{label} {:.0}%", r * 100.0))
                .collect();
            format!("\u{232B} {}", parts.join("  "))
        };
        painter.text(readout_at, Align2::RIGHT_TOP, text, FontId::proportional(11.0), p.text_inherited);
    } else if let Some(stats) = heatmap {
        painter.text(
            readout_at,
            Align2::RIGHT_TOP,
            format!("{} presses", group_thousands(stats.total())),
            FontId::proportional(11.0),
            p.text_inherited,
        );
    }

    let origin = rect.min + vec2(pad, pad + header_h);
    // Thumb keys (the rotated ones) draw slightly enlarged; scaling around the
    // cluster's rotation origin grows and spreads them together, so they don't
    // collide with each other.
    const THUMB_SCALE: f32 = 1.08;
    for (i, (geom, key)) in geometry::MOONLANDER_KEYS.iter().zip(keys).enumerate() {
        let angle = geom.rot_deg.to_radians();
        let (sin, cos) = angle.sin_cos();
        let (kx, ky, kw, kh) = if geom.rot_deg == 0.0 {
            (geom.x, geom.y, geom.w, geom.h)
        } else {
            (
                geom.rot_x + (geom.x - geom.rot_x) * THUMB_SCALE,
                geom.rot_y + (geom.y - geom.rot_y) * THUMB_SCALE,
                geom.w * THUMB_SCALE,
                geom.h * THUMB_SCALE,
            )
        };
        // Unit-space point -> screen, applying the key's rotation.
        let to_screen = |ux: f32, uy: f32| {
            let (px, py) = if geom.rot_deg == 0.0 {
                (ux, uy)
            } else {
                let (dx, dy) = (ux - geom.rot_x, uy - geom.rot_y);
                (geom.rot_x + dx * cos - dy * sin, geom.rot_y + dx * sin + dy * cos)
            };
            origin + vec2(px, py) * BOARD_SCALE
        };
        let rotate = |v: egui::Vec2| vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos);

        let gap = 1.0 / BOARD_SCALE; // keycap gap, in key units
        let corners = [
            to_screen(kx + gap, ky + gap),
            to_screen(kx + kw - gap, ky + gap),
            to_screen(kx + kw - gap, ky + kh - gap),
            to_screen(kx + gap, ky + kh - gap),
        ];
        let center = to_screen(kx + kw / 2.0, ky + kh / 2.0);

        let mut label = key_text(key);
        // KC_TRANSPARENT falls through to the base layer in QMK (KC_NO does
        // not) — show the inherited label dimmed, on the unmapped background.
        let mut inherited = false;
        if label.is_empty()
            && tap_is_transparent(key)
            && let Some(base_key) = base.and_then(|b| b.get(i))
        {
            label = key_text(base_key);
            inherited = !label.is_empty();
        }
        let base_fill = if let Some(stats) = error {
            error_fill(stats.error_rate(i), error_max, &p)
        } else if let Some(stats) = heatmap {
            heat_fill(stats.count(i), heat_max, &p)
        } else if label.is_empty() || inherited {
            p.key_blank
        } else {
            p.key_fill
        };
        // Accent highlight: full alpha while physically held, then fading
        // linearly to nothing over AFTERGLOW_SECS after release. Painted as an
        // overlay on the base fill so it reveals the normal key as it fades.
        let accent_alpha = if pressed.contains(&i) {
            HELD_ALPHA
        } else if let Some(t) = released.get(&i) {
            let frac = 1.0 - now.duration_since(*t).as_secs_f32() / AFTERGLOW_SECS;
            (frac.clamp(0.0, 1.0) * RELEASED_ALPHA as f32).round() as u8
        } else {
            0
        };
        let accent = (accent_alpha > 0).then(|| ghost_color(key_hue, i, accent_alpha, p.accent));
        // In a heatmap, the colored press accent can blend into a same-hue
        // cell, hiding which key is live. Add a bright ring (brightest while
        // held, fading with the afterglow) — it reads against any fill because
        // its outer edge meets the dark keycap gap.
        let press_ring = ((heatmap.is_some() || error.is_some()) && accent_alpha > 0).then(|| {
            let a = ((accent_alpha as f32 / HELD_ALPHA as f32) * 235.0).round() as u8;
            egui::Stroke::new(2.0, Color32::from_rgba_unmultiplied(255, 255, 255, a))
        });
        let text_color = if inherited { p.text_inherited } else { p.text };
        // Keys with an Oryx LED color get a border in that color
        // (#000000 means the LED is off).
        let border = key
            .glow_color
            .as_deref()
            .and_then(parse_hex_color)
            .filter(|c| *c != Color32::BLACK)
            .map(|c| egui::Stroke::new(1.5, c));
        if geom.rot_deg == 0.0 {
            let kr = Rect::from_min_max(corners[0], corners[2]);
            painter.rect_filled(kr, CornerRadius::same(4), base_fill);
            if let Some(accent) = accent {
                painter.rect_filled(kr, CornerRadius::same(4), accent);
            }
            if let Some(stroke) = border {
                painter.rect_stroke(kr, CornerRadius::same(4), stroke, egui::StrokeKind::Inside);
            }
            if let Some(ring) = press_ring {
                painter.rect_stroke(kr, CornerRadius::same(4), ring, egui::StrokeKind::Inside);
            }
        } else {
            painter.add(egui::Shape::convex_polygon(
                corners.to_vec(),
                base_fill,
                border.unwrap_or(egui::Stroke::NONE),
            ));
            if let Some(accent) = accent {
                painter.add(egui::Shape::convex_polygon(
                    corners.to_vec(),
                    accent,
                    egui::Stroke::NONE,
                ));
            }
            if let Some(ring) = press_ring {
                painter.add(egui::Shape::convex_polygon(
                    corners.to_vec(),
                    Color32::TRANSPARENT,
                    ring,
                ));
            }
        }
        let key_painter = painter.with_clip_rect(Rect::from_points(&corners));

        if !label.is_empty() {
            let max_w = kw * BOARD_SCALE - 5.0;
            let size = emphasized_font_size(&label, 1.0)
                .unwrap_or_else(|| label_font_size(&label, 9.5, 11.5, 14.5));
            let mut galley =
                key_painter.layout_no_wrap(label.clone(), FontId::proportional(size), text_color);
            if galley.size().x > max_w {
                if label.contains(' ') {
                    // Multi-word labels (custom labels mostly) wrap instead.
                    galley = key_painter.layout(label, FontId::proportional(7.0), text_color, max_w);
                } else {
                    let font = size * (max_w / galley.size().x).max(0.6);
                    galley =
                        key_painter.layout_no_wrap(label, FontId::proportional(font), text_color);
                }
            }
            // Center the galley on the key, rotated with it.
            let pos = center - rotate(galley.size() / 2.0);
            key_painter.add(TextShape::new(pos, galley, Color32::WHITE).with_angle(angle));
        }
        if let Some(hold) = hold_text(key) {
            let galley = key_painter.layout_no_wrap(
                hold.clone(),
                FontId::proportional(
                    emphasized_font_size(&hold, 0.65)
                        .unwrap_or_else(|| label_font_size(&hold, 7.0, 8.0, 9.5)),
                ),
                p.hold_text,
            );
            // Anchor at the key's bottom-center, rotated with it.
            let anchor = to_screen(kx + kw / 2.0, ky + kh - 2.0 * gap);
            let pos = anchor - rotate(vec2(galley.size().x / 2.0, galley.size().y));
            key_painter.add(TextShape::new(pos, galley, Color32::WHITE).with_angle(angle));
        }
    }

    // Trackball indicator: a ring in the center gap between the thumb
    // clusters; the dot deflects in the direction of motion and brightens
    // while the ball is moving, then glides back to center.
    if let Some(ball) = ball {
        let center = origin + vec2(8.5, 5.2) * BOARD_SCALE;
        let radius = 0.65 * BOARD_SCALE;
        painter.circle_stroke(
            center,
            radius,
            egui::Stroke::new(1.2, Color32::from_rgba_unmultiplied(255, 255, 255, 60)),
        );
        // Map a smoothed ball vector to its dot position in the ring. Soft
        // response curve: moderate rolls already swing well out, full speed
        // brings the dot's edge to the ring.
        let dot_pos = |v: egui::Vec2| {
            let activity = v.length().min(1.0);
            let deflection = activity.powf(0.6) * radius * 0.68;
            center + if v.length() > 0.0 { v / v.length() } else { v } * deflection
        };
        let dot = dot_pos(ball);

        // Comet trail: a fading streak through the dot's recent path, drawn
        // oldest (faint, thin) to newest, ending at the live dot.
        let path: Vec<egui::Pos2> = trail.iter().map(|v| dot_pos(*v)).chain([dot]).collect();
        let acc = p.accent;
        if path.len() >= 2 {
            let segs = (path.len() - 1) as f32;
            for (i, w) in path.windows(2).enumerate() {
                let f = (i + 1) as f32 / segs; // 0..1 toward the head
                painter.line_segment(
                    [w[0], w[1]],
                    egui::Stroke::new(
                        0.8 + 2.4 * f,
                        Color32::from_rgba_unmultiplied(acc.r(), acc.g(), acc.b(), (120.0 * f) as u8),
                    ),
                );
            }
        }

        // Head dot: brighter the faster the ball is moving.
        let activity = ball.length().min(1.0);
        let alpha = (70.0 + 185.0 * activity) as u8;
        painter.circle_filled(
            dot,
            radius * 0.30,
            Color32::from_rgba_unmultiplied(acc.r(), acc.g(), acc.b(), alpha),
        );
    }
    rect
}

/// True when the key's tap slot falls through to lower layers (KC_TRANSPARENT
/// or simply unset). KC_NO is NOT transparent — it blocks fall-through.
fn tap_is_transparent(key: &oryx::Key) -> bool {
    key.custom_label
        .as_ref()
        .is_none_or(|l| l.trim().is_empty())
        && key.tap.as_ref().is_none_or(|a| {
            a.macro_.is_none()
                && matches!(a.code.as_deref(), None | Some("KC_TRANSPARENT" | "KC_TRNS"))
        })
}

/// Primary label for a keycap: the Oryx custom label if set, else the tap action.
fn key_text(key: &oryx::Key) -> String {
    if let Some(label) = &key.custom_label {
        // Strip invisible emoji plumbing — variation selectors (how "⬆️"
        // differs from "⬆"), zero-width joiners, and the keycap combiner.
        // Fonts have no glyphs for them, so they render as tofu boxes.
        let label: String = label
            .chars()
            .filter(|c| !matches!(c, '\u{FE00}'..='\u{FE0F}' | '\u{200D}' | '\u{20E3}'))
            .collect();
        let label = label.trim();
        if !label.is_empty() {
            return label.to_owned();
        }
    }
    key.tap.as_ref().map(action_text).unwrap_or_default()
}

/// Small secondary label for keys with a hold action (e.g. layer-taps).
fn hold_text(key: &oryx::Key) -> Option<String> {
    let text = action_text(key.hold.as_ref()?);
    (!text.is_empty()).then_some(text)
}

fn action_text(a: &oryx::KeyAction) -> String {
    if a.macro_.is_some() {
        return "Macro".to_owned();
    }
    let code = a.code.as_deref().unwrap_or("");
    let base = if let Some(layer) = a.layer {
        // Layer-switch actions: bare code ("MO") + target layer index.
        format!("{} {layer}", keycodes::key_label(code).unwrap_or(code))
    } else if code == "OSM" {
        // One-shot modifier: the target arrives in the singular `modifier` field.
        let m = a.modifier.as_deref().unwrap_or("");
        format!("OSM {}", keycodes::key_label(m).unwrap_or(m)).trim_end().to_owned()
    } else {
        keycodes::key_label(code)
            .map(str::to_owned)
            // Unknown code: trim the QMK prefix so it's at least recognizable.
            .unwrap_or_else(|| code.trim_start_matches("KC_").to_owned())
    };
    let mods = a.modifiers.as_ref().map(oryx::Modifiers::prefix).unwrap_or_default();
    if base.is_empty() {
        return base;
    }
    format!("{mods}{base}")
}

/// Opaque id of the window currently receiving keystrokes, for the typo
/// tracker's "did the user switch apps?" guard. 0 on non-Windows.
#[cfg(windows)]
fn foreground_window() -> isize {
    win32::foreground_window()
}
#[cfg(not(windows))]
fn foreground_window() -> isize {
    0
}

/// True when a Ctrl/Alt/Win modifier is held (so the keypress is a shortcut,
/// not text). Always false on non-Windows.
#[cfg(windows)]
fn shortcut_mods_down() -> bool {
    win32::ctrl_alt_gui_down()
}
#[cfg(not(windows))]
fn shortcut_mods_down() -> bool {
    false
}

/// (cursor position, any mouse button down) for the typo tracker's "did the
/// mouse move or click?" check. (None, false) off-Windows.
#[cfg(windows)]
fn pointer_state() -> (Option<(i32, i32)>, bool) {
    (win32::cursor_pos(), win32::mouse_buttons_down())
}
#[cfg(not(windows))]
fn pointer_state() -> (Option<(i32, i32)>, bool) {
    (None, false)
}

/// Whether pointer activity since the last keystroke should void the typo
/// buffer: any mouse button held, or the cursor moved from where it was. A
/// missing reading (first keystroke / off-Windows) is treated as no movement.
fn pointer_invalidates(last: Option<(i32, i32)>, now: Option<(i32, i32)>, button: bool) -> bool {
    button || matches!((last, now), (Some(a), Some(b)) if a != b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_invalidation() {
        // Cursor sat still and no button: keep the buffer.
        assert!(!pointer_invalidates(Some((5, 5)), Some((5, 5)), false));
        // Cursor moved: void it.
        assert!(pointer_invalidates(Some((5, 5)), Some((6, 5)), false));
        assert!(pointer_invalidates(Some((5, 5)), Some((5, 6)), false));
        // A held mouse button voids it even without movement.
        assert!(pointer_invalidates(Some((5, 5)), Some((5, 5)), true));
        // First reading (or no cursor) isn't movement on its own.
        assert!(!pointer_invalidates(None, Some((5, 5)), false));
        assert!(pointer_invalidates(None, Some((5, 5)), true));
    }
}

#[cfg(windows)]
mod win32 {
    use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
    use windows::Win32::Foundation::{COLORREF, HWND, POINT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LBUTTON, VK_LWIN, VK_MBUTTON, VK_MENU, VK_RBUTTON, VK_RWIN,
        VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, LWA_ALPHA,
        SetLayeredWindowAttributes, SetWindowLongPtrW, WS_EX_APPWINDOW, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    };

    fn hwnd(frame: &eframe::Frame) -> Option<HWND> {
        match frame.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut core::ffi::c_void)),
            _ => None,
        }
    }

    pub fn shift_down() -> bool {
        unsafe { (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 }
    }

    /// Handle of the foreground window as a plain integer, for equality checks.
    /// The overlay itself is WS_EX_NOACTIVATE, so this is the app the user is
    /// actually typing into, never us.
    pub fn foreground_window() -> isize {
        unsafe { GetForegroundWindow().0 as isize }
    }

    pub fn lbutton_down() -> bool {
        unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 }
    }

    /// True when any mouse button (left/right/middle) is currently held — a
    /// click repositions the caret, invalidating the typo buffer.
    pub fn mouse_buttons_down() -> bool {
        unsafe {
            let down = |vk: i32| (GetAsyncKeyState(vk) as u16 & 0x8000) != 0;
            down(VK_LBUTTON.0 as i32) || down(VK_RBUTTON.0 as i32) || down(VK_MBUTTON.0 as i32)
        }
    }

    /// True when any of Ctrl/Alt/Win is held — i.e. the keypress is a shortcut
    /// (Ctrl+C, Ctrl+Backspace word-delete, …), not typed text. Shift is
    /// excluded: Shift+letter is still text. Reads OS-level modifier state, so
    /// it's correct regardless of which physical key produced the modifier.
    pub fn ctrl_alt_gui_down() -> bool {
        unsafe {
            let down = |vk: i32| (GetAsyncKeyState(vk) as u16 & 0x8000) != 0;
            down(VK_CONTROL.0 as i32)
                || down(VK_MENU.0 as i32)
                || down(VK_LWIN.0 as i32)
                || down(VK_RWIN.0 as i32)
        }
    }

    /// Cursor position in physical (per-pixel) desktop coordinates.
    pub fn cursor_pos() -> Option<(i32, i32)> {
        let mut p = POINT::default();
        unsafe { GetCursorPos(&mut p).ok()? };
        Some((p.x, p.y))
    }

    /// Full bounds (left, top, right, bottom, physical pixels) of the monitor
    /// containing the given desktop point — nearest monitor if it's in a gap.
    pub fn monitor_rect_for_point(x: i32, y: i32) -> Option<(i32, i32, i32, i32)> {
        unsafe {
            let mon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: core::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(mon, &mut info).as_bool() {
                let r = info.rcMonitor;
                Some((r.left, r.top, r.right, r.bottom))
            } else {
                None
            }
        }
    }

    /// NOACTIVATE: never steals focus. TOOLWINDOW minus APPWINDOW: never in
    /// Alt-Tab, no taskbar button. LAYERED|TRANSPARENT: clicks pass through
    /// (winit sets these for mouse passthrough, but clobbers them on flag
    /// changes, so they're asserted here too). `click_through` is false only
    /// while the hamburger drag handle is armed (Shift held over it), so that
    /// click lands on us instead of the window underneath. `alpha` is the
    /// window-level opacity (composes with per-pixel alpha); `force_alpha`
    /// pushes it even when the styles are already right.
    pub fn assert_overlay_styles(
        frame: &eframe::Frame,
        click_through: bool,
        alpha: u8,
        force_alpha: bool,
    ) {
        let Some(hwnd) = hwnd(frame) else { return };
        let mut want =
            (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0 | WS_EX_LAYERED.0) as isize;
        let mut unwant = WS_EX_APPWINDOW.0 as isize;
        if click_through {
            want |= WS_EX_TRANSPARENT.0 as isize;
        } else {
            unwant |= WS_EX_TRANSPARENT.0 as isize;
        }
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let desired = (ex | want) & !unwant;
            if desired != ex {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired);
            }
            // A layered window needs one SLWA call before it renders at all,
            // so re-push the alpha whenever the styles were rewritten.
            if desired != ex || force_alpha {
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
            }
        }
    }
}
