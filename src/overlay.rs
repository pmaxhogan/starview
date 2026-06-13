//! The overlay window: a frameless, transparent, click-through, always-on-top
//! bubble in the top-right corner naming the active non-base layer.
//!
//! Window behavior relies on raw Win32 extended styles (see `win32` below):
//! winit can't express NOACTIVATE/TOOLWINDOW, and it rewrites GWL_EXSTYLE
//! wholesale on its own flag changes, so the styles are re-asserted every
//! `logic` tick (a no-op compare once stable).

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;

use eframe::egui;
use egui::epaint::TextShape;
use egui::{Align2, Color32, CornerRadius, FontId, Rect, pos2, vec2};

use crate::hid::HidEvent;
use crate::oryx::{self, LayoutInfo};
use crate::settings::{self, Corner, Settings};
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
const AFTERGLOW_SECS: f32 = 1.8;

// rgba(16,18,28) at ~78% opacity, stored premultiplied.
const PANEL_BG: Color32 = Color32::from_rgba_premultiplied(13, 14, 22, 200);
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
}

fn palette() -> Palette {
    #[cfg(windows)]
    let hdr = crate::hdr::active();
    #[cfg(not(windows))]
    let hdr = false;
    if hdr {
        Palette {
            panel_bg: Color32::from_rgba_unmultiplied(16, 18, 28, 246),
            text: Color32::WHITE,
            text_inherited: Color32::from_rgba_unmultiplied(225, 228, 240, 200),
            hold_text: Color32::from_rgba_unmultiplied(225, 228, 250, 255),
            key_fill: Color32::from_rgba_unmultiplied(255, 255, 255, 60),
            key_blank: Color32::from_rgba_unmultiplied(255, 255, 255, 22),
        }
    } else {
        Palette {
            panel_bg: PANEL_BG,
            text: TEXT_BRIGHT,
            text_inherited: Color32::from_rgba_unmultiplied(200, 205, 220, 140),
            hold_text: Color32::from_rgba_unmultiplied(200, 205, 235, 200),
            key_fill: Color32::from_rgba_unmultiplied(255, 255, 255, 32),
            key_blank: Color32::from_rgba_unmultiplied(255, 255, 255, 10),
        }
    }
}

pub struct OverlayApp {
    events: Receiver<AppEvent>,
    layout: Option<LayoutInfo>,
    layer: u8,
    /// Oryx key indices currently held down on the physical board.
    pressed: HashSet<usize>,
    /// Recently released keys -> release time; they afterglow and fade out.
    released: HashMap<usize, std::time::Instant>,
    /// Smoothed trackball motion vector (unit-clamped); decays toward zero.
    ball: egui::Vec2,
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
    /// Shift is held with the cursor over the hamburger (drag affordance).
    hot: bool,
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
    /// Test override (STARVIEW_FORCE_LAYER): pretend this layer is active.
    force_layer: Option<u8>,
}

impl OverlayApp {
    pub fn new(events: Receiver<AppEvent>, layout: Option<LayoutInfo>, settings: Settings) -> Self {
        Self {
            events,
            layout,
            layer: 0,
            pressed: HashSet::new(),
            released: HashMap::new(),
            ball: egui::Vec2::ZERO,
            ball_seen: false,
            last_tick: None,
            connected: false,
            positioned: false,
            shown: false,
            always: settings.pin_base,
            corner: settings.corner,
            custom_pos: settings.position.map(|(x, y)| pos2(x, y)),
            burger: None,
            hot: false,
            drag: None,
            prev_button: false,
            opacity: settings.opacity,
            applied_alpha: None,
            fade_after: (settings.fade_secs > 0)
                .then(|| std::time::Duration::from_secs(settings.fade_secs as u64)),
            last_activity: std::time::Instant::now(),
            force_layer: std::env::var("STARVIEW_FORCE_LAYER")
                .ok()
                .and_then(|v| v.parse().ok()),
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
}

impl eframe::App for OverlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    /// Runs even while the window is hidden (whenever a repaint is requested —
    /// the HID watcher requests one per event), so show/hide decisions live here.
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        for event in self.events.try_iter() {
            match event {
                AppEvent::Hid(HidEvent::Layer(idx)) => {
                    // Only a genuine change is activity — the watcher re-emits
                    // the current layer on each idle re-pair, which must not
                    // keep resetting the auto-fade timer.
                    if idx != self.layer {
                        self.last_activity = std::time::Instant::now();
                    }
                    self.layer = idx;
                    self.connected = true;
                }
                AppEvent::Hid(HidEvent::KeyDown { row, col }) => {
                    self.last_activity = std::time::Instant::now();
                    if let Some(i) = geometry::key_index_for_matrix(row, col) {
                        self.pressed.insert(i);
                        self.released.remove(&i);
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
                }
                AppEvent::Layout(info) => self.layout = Some(info),
                AppEvent::Settings(s) => {
                    self.always = s.pin_base;
                    self.opacity = s.opacity;
                    self.fade_after = (s.fade_secs > 0)
                        .then(|| std::time::Duration::from_secs(s.fade_secs as u64));
                    // Re-show at full opacity and restart the timer on change.
                    self.last_activity = std::time::Instant::now();
                    let pos = s.position.map(|(x, y)| pos2(x, y));
                    if self.corner != s.corner || self.custom_pos != pos {
                        self.corner = s.corner;
                        self.custom_pos = pos;
                        self.positioned = false; // re-anchor on next tick
                    }
                }
                AppEvent::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                AppEvent::Trackball(dx, dy) => {
                    self.ball_seen = true;
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

        // Drop fully-faded afterglows; keep animating while any remain.
        self.released
            .retain(|_, t| now.duration_since(*t).as_secs_f32() < AFTERGLOW_SECS);
        if !self.released.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
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
            self.hot = self.drag.is_none()
                && shift
                && match (cursor, window, self.burger) {
                    (Some(c), Some(win), Some(b)) => b.translate(win.min.to_vec2()).contains(c),
                    _ => false,
                };
            if let Some(offset) = self.drag {
                if let (true, Some(c)) = (button, cursor) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(c - offset));
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
            } else if self.hot
                && button
                && !self.prev_button
                && let (Some(c), Some(win)) = (cursor, window)
            {
                self.drag = Some(c - win.min);
            }
            self.prev_button = button;
            if shift || self.drag.is_some() {
                // Track the cursor smoothly while a drag is possible/live.
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
            self.hot || self.drag.is_some()
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
            let alpha = (self.opacity as f32 / 100.0 * 255.0 * fade).round() as u8;
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

        let panel = if !keys.is_empty() && keys.len() == geometry::MOONLANDER_KEYS.len() {
            draw_board(
                ui,
                &title,
                keys,
                base,
                &self.pressed,
                &self.released,
                ball,
                align,
            )
        } else {
            draw_name_bubble(ui, &title, align)
        };
        self.burger = Some(draw_burger(
            ui.painter(),
            panel,
            self.hot || self.drag.is_some(),
        ));
    }
}

/// Hamburger drag handle at the panel's top-right. Hold Shift and drag it
/// with the left button to move the overlay. Returns the (expanded) hit rect.
fn draw_burger(painter: &egui::Painter, panel: Rect, hot: bool) -> Rect {
    let p = palette();
    let rect = Rect::from_min_size(
        pos2(panel.max.x - 15.0 - 8.0, panel.min.y + 8.0),
        vec2(15.0, 15.0),
    );
    if hot {
        painter.rect_filled(
            rect.expand(4.0),
            CornerRadius::same(4),
            Color32::from_rgba_unmultiplied(110, 165, 255, 70),
        );
    }
    let color = if hot {
        p.text
    } else {
        Color32::from_rgba_unmultiplied(200, 205, 220, 110)
    };
    let lines = rect.shrink2(vec2(1.0, 3.5));
    for t in [0.0, 0.5, 1.0] {
        let y = lines.min.y + lines.height() * t;
        painter.line_segment(
            [pos2(lines.min.x, y), pos2(lines.max.x, y)],
            egui::Stroke::new(1.5, color),
        );
    }
    rect.expand(5.0)
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
    // Extra width on the right so the hamburger handle has room.
    let size = galley.size() + pad * 2.0 + vec2(24.0, 0.0);
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
    align: Align2,
) -> Rect {
    let now = std::time::Instant::now();
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
        let base_fill = if label.is_empty() || inherited {
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
        let accent = (accent_alpha > 0)
            .then(|| Color32::from_rgba_unmultiplied(110, 165, 255, accent_alpha));
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
        let activity = ball.length().min(1.0);
        // Soft response curve: moderate rolls already swing well out, full
        // speed brings the dot's edge to the ring.
        let deflection = activity.powf(0.6) * radius * 0.68;
        let dot = center + if activity > 0.0 { ball / ball.length() } else { ball } * deflection;
        let alpha = (70.0 + 185.0 * activity) as u8;
        painter.circle_filled(
            dot,
            radius * 0.30,
            Color32::from_rgba_unmultiplied(110, 165, 255, alpha),
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

#[cfg(windows)]
mod win32 {
    use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
    use windows::Win32::Foundation::{COLORREF, HWND, POINT};
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_SHIFT};
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetCursorPos, GetWindowLongPtrW, LWA_ALPHA, SetLayeredWindowAttributes,
        SetWindowLongPtrW, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_TRANSPARENT,
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

    pub fn lbutton_down() -> bool {
        unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 }
    }

    /// Cursor position in physical (per-pixel) desktop coordinates.
    pub fn cursor_pos() -> Option<(i32, i32)> {
        let mut p = POINT::default();
        unsafe { GetCursorPos(&mut p).ok()? };
        Some((p.x, p.y))
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
