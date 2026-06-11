//! The overlay window: a frameless, transparent, click-through, always-on-top
//! bubble in the top-right corner naming the active non-base layer.
//!
//! Window behavior relies on raw Win32 extended styles (see `win32` below):
//! winit can't express NOACTIVATE/TOOLWINDOW, and it rewrites GWL_EXSTYLE
//! wholesale on its own flag changes, so the styles are re-asserted every
//! `logic` tick (a no-op compare once stable).

use std::collections::HashSet;
use std::sync::mpsc::Receiver;

use eframe::egui;
use egui::epaint::TextShape;
use egui::{Align2, Color32, CornerRadius, FontId, Rect, pos2, vec2};

use crate::hid::HidEvent;
use crate::oryx::{self, LayoutInfo};
use crate::{geometry, keycodes};

/// Everything the background threads feed into the UI.
pub enum AppEvent {
    Hid(HidEvent),
    /// Refreshed layout from the periodic Oryx re-fetch.
    Layout(LayoutInfo),
    /// Coalesced relative motion from the ZSA trackball (Navigator).
    Trackball(i32, i32),
}

pub const OVERLAY_W: f32 = 480.0;
pub const OVERLAY_H: f32 = 272.0;
/// Gap between the overlay window and the screen edge, in logical points.
const SCREEN_MARGIN: f32 = 12.0;
/// Rendered size of one key unit, in logical points.
const BOARD_SCALE: f32 = 26.0;

// rgba(16,18,28) at ~78% opacity, stored premultiplied.
const PANEL_BG: Color32 = Color32::from_rgba_premultiplied(13, 14, 22, 200);
const TEXT_BRIGHT: Color32 = Color32::from_rgb(240, 240, 255);

pub struct OverlayApp {
    events: Receiver<AppEvent>,
    layout: Option<LayoutInfo>,
    layer: u8,
    /// Oryx key indices currently held down on the physical board.
    pressed: HashSet<usize>,
    /// Smoothed trackball motion vector (unit-clamped); decays toward zero.
    ball: egui::Vec2,
    /// Whether a ZSA pointing device has ever produced motion.
    ball_seen: bool,
    /// Previous `logic` tick, for time-based (tick-rate-independent) decay.
    last_tick: Option<std::time::Instant>,
    connected: bool,
    positioned: bool,
    shown: bool,
    /// --always: keep the overlay up on the base layer too.
    always: bool,
    /// Test override (STARVIEW_FORCE_LAYER): pretend this layer is active.
    force_layer: Option<u8>,
}

impl OverlayApp {
    pub fn new(events: Receiver<AppEvent>, layout: Option<LayoutInfo>, always: bool) -> Self {
        Self {
            events,
            layout,
            layer: 0,
            pressed: HashSet::new(),
            ball: egui::Vec2::ZERO,
            ball_seen: false,
            last_tick: None,
            connected: false,
            positioned: false,
            shown: false,
            always,
            force_layer: std::env::var("STARVIEW_FORCE_LAYER")
                .ok()
                .and_then(|v| v.parse().ok()),
        }
    }

    fn label(&self) -> String {
        match self
            .layout
            .as_ref()
            .and_then(|l| l.layer_name(self.layer as usize))
        {
            Some(name) => name.to_owned(),
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
        #[cfg(windows)]
        win32::assert_overlay_styles(frame);

        for event in self.events.try_iter() {
            match event {
                AppEvent::Hid(HidEvent::Layer(idx)) => {
                    self.layer = idx;
                    self.connected = true;
                }
                AppEvent::Hid(HidEvent::KeyDown { row, col }) => {
                    if let Some(i) = geometry::key_index_for_matrix(row, col) {
                        self.pressed.insert(i);
                    }
                }
                AppEvent::Hid(HidEvent::KeyUp { row, col }) => {
                    if let Some(i) = geometry::key_index_for_matrix(row, col) {
                        self.pressed.remove(&i);
                    }
                }
                AppEvent::Hid(HidEvent::Disconnected) => {
                    self.connected = false;
                    self.pressed.clear();
                }
                AppEvent::Layout(info) => self.layout = Some(info),
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

        // Pin to the top-right corner once the monitor size is known.
        if !self.positioned
            && let Some(size) = ctx.input(|i| i.viewport().monitor_size)
        {
            let pos = egui::pos2(size.x - OVERLAY_W - SCREEN_MARGIN, SCREEN_MARGIN);
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
            self.positioned = true;
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

        // Low-rate heartbeat so the style assert above self-heals even when
        // no HID events arrive.
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.shown {
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

        if !keys.is_empty() && keys.len() == geometry::MOONLANDER_KEYS.len() {
            draw_board(ui, &title, keys, base, &self.pressed, ball);
        } else {
            draw_name_bubble(ui, &title);
        }
    }
}

fn draw_name_bubble(ui: &mut egui::Ui, title: &str) {
    let painter = ui.painter();
    let galley = painter.layout_no_wrap(title.to_owned(), FontId::proportional(24.0), TEXT_BRIGHT);
    let pad = vec2(18.0, 11.0);
    let size = galley.size() + pad * 2.0;
    let max = ui.max_rect();
    let rect = Rect::from_min_size(pos2(max.right() - size.x, max.top()), size);
    painter.rect_filled(rect, CornerRadius::same(13), PANEL_BG);
    painter.galley(rect.min + pad, galley, Color32::WHITE);
}

fn draw_board(
    ui: &mut egui::Ui,
    title: &str,
    keys: &[oryx::Key],
    base: Option<&[oryx::Key]>,
    pressed: &HashSet<usize>,
    ball: Option<egui::Vec2>,
) {
    let pad = 12.0;
    let header_h = 24.0;
    let board_size = vec2(geometry::BOARD_WIDTH_U, geometry::BOARD_HEIGHT_U) * BOARD_SCALE;
    let size = board_size + vec2(pad * 2.0, header_h + pad * 2.0);
    let max = ui.max_rect();
    let rect = Rect::from_min_size(pos2(max.right() - size.x, max.top()), size);
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(13), PANEL_BG);
    painter.text(
        rect.min + vec2(pad + 2.0, pad - 2.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(15.0),
        TEXT_BRIGHT,
    );

    let origin = rect.min + vec2(pad, pad + header_h);
    for (i, (geom, key)) in geometry::MOONLANDER_KEYS.iter().zip(keys).enumerate() {
        let angle = geom.rot_deg.to_radians();
        let (sin, cos) = angle.sin_cos();
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
            to_screen(geom.x + gap, geom.y + gap),
            to_screen(geom.x + geom.w - gap, geom.y + gap),
            to_screen(geom.x + geom.w - gap, geom.y + geom.h - gap),
            to_screen(geom.x + gap, geom.y + geom.h - gap),
        ];
        let center = to_screen(geom.x + geom.w / 2.0, geom.y + geom.h / 2.0);

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
        let fill = if pressed.contains(&i) {
            // Physically held right now — accent highlight.
            Color32::from_rgba_unmultiplied(110, 165, 255, 150)
        } else if label.is_empty() || inherited {
            Color32::from_rgba_unmultiplied(255, 255, 255, 10)
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, 32)
        };
        let text_color = if inherited {
            Color32::from_rgba_unmultiplied(200, 205, 220, 140)
        } else {
            TEXT_BRIGHT
        };
        if geom.rot_deg == 0.0 {
            let kr = Rect::from_min_max(corners[0], corners[2]);
            painter.rect_filled(kr, CornerRadius::same(4), fill);
        } else {
            painter.add(egui::Shape::convex_polygon(
                corners.to_vec(),
                fill,
                egui::Stroke::NONE,
            ));
        }
        let key_painter = painter.with_clip_rect(Rect::from_points(&corners));

        if !label.is_empty() {
            let max_w = geom.w * BOARD_SCALE - 5.0;
            let mut galley =
                key_painter.layout_no_wrap(label.clone(), FontId::proportional(9.5), text_color);
            if galley.size().x > max_w {
                if label.contains(' ') {
                    // Multi-word labels (custom labels mostly) wrap instead.
                    galley = key_painter.layout(label, FontId::proportional(7.0), text_color, max_w);
                } else {
                    let font = 9.5 * (max_w / galley.size().x).max(0.6);
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
                hold,
                FontId::proportional(7.0),
                Color32::from_rgba_unmultiplied(200, 205, 235, 200),
            );
            // Anchor at the key's bottom-center, rotated with it.
            let anchor = to_screen(geom.x + geom.w / 2.0, geom.y + geom.h - 2.0 * gap);
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
    use windows::Win32::Foundation::{COLORREF, HWND};
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, LWA_ALPHA, SetLayeredWindowAttributes,
        SetWindowLongPtrW, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_TRANSPARENT,
    };

    fn hwnd(frame: &eframe::Frame) -> Option<HWND> {
        match frame.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut core::ffi::c_void)),
            _ => None,
        }
    }

    /// NOACTIVATE: never steals focus. TOOLWINDOW minus APPWINDOW: never in
    /// Alt-Tab, no taskbar button. LAYERED|TRANSPARENT: clicks pass through
    /// (winit sets these for mouse passthrough, but clobbers them on flag
    /// changes, so they're asserted here too).
    pub fn assert_overlay_styles(frame: &eframe::Frame) {
        let Some(hwnd) = hwnd(frame) else { return };
        let want = (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0 | WS_EX_LAYERED.0
            | WS_EX_TRANSPARENT.0) as isize;
        let unwant = WS_EX_APPWINDOW.0 as isize;
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let desired = (ex | want) & !unwant;
            if desired != ex {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired);
                // A layered window needs one SLWA call before it renders;
                // 255 = opaque at the window level, per-pixel alpha still applies.
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
            }
        }
    }

}
