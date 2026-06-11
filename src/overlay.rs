//! The overlay window: a frameless, transparent, click-through, always-on-top
//! bubble in the top-right corner naming the active non-base layer.
//!
//! Window behavior relies on raw Win32 extended styles (see `win32` below):
//! winit can't express NOACTIVATE/TOOLWINDOW, and it rewrites GWL_EXSTYLE
//! wholesale on its own flag changes, so the styles are re-asserted every
//! `logic` tick (a no-op compare once stable).

use std::sync::mpsc::Receiver;

use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Rect, pos2, vec2};

use crate::hid::LayerEvent;
use crate::oryx::{self, LayoutInfo};
use crate::{geometry, keycodes};

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
    events: Receiver<LayerEvent>,
    layout: Option<LayoutInfo>,
    layer: u8,
    connected: bool,
    positioned: bool,
    shown: bool,
    /// Test override (STARVIEW_FORCE_LAYER): pretend this layer is active.
    force_layer: Option<u8>,
}

impl OverlayApp {
    pub fn new(events: Receiver<LayerEvent>, layout: Option<LayoutInfo>) -> Self {
        Self {
            events,
            layout,
            layer: 0,
            connected: false,
            positioned: false,
            shown: false,
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
                LayerEvent::Layer(idx) => {
                    self.layer = idx;
                    self.connected = true;
                }
                LayerEvent::Disconnected => self.connected = false,
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
        // Forced mode always shows (lets layer 0 be previewed/screenshotted).
        self.shown = self.force_layer.is_some() || (self.connected && self.layer != 0);
        // Re-asserted every tick rather than on transitions: eframe/winit
        // re-show the window on their own (e.g. the deferred first-frame show),
        // so a one-shot hide can be silently undone.
        #[cfg(windows)]
        win32::sync_overlay_visible(frame, self.shown);

        // Low-rate heartbeat so the asserts above self-heal even when no
        // layer events arrive (e.g. right after the startup show race).
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
        if !keys.is_empty() && keys.len() == geometry::MOONLANDER_KEYS.len() {
            draw_board(ui, &title, keys);
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

fn draw_board(ui: &mut egui::Ui, title: &str, keys: &[oryx::Key]) {
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
    for (geom, key) in geometry::MOONLANDER_KEYS.iter().zip(keys) {
        // Axis-aligned keycaps drawn at their rotated centers — close enough
        // for the thumb clusters at this size.
        let (cx, cy) = rotated_center(geom);
        let center = origin + vec2(cx, cy) * BOARD_SCALE;
        let key_size = vec2(geom.w, geom.h) * BOARD_SCALE - vec2(2.0, 2.0);
        let kr = Rect::from_center_size(center, key_size);

        let label = key_text(key);
        let fill = if label.is_empty() {
            Color32::from_rgba_unmultiplied(255, 255, 255, 10)
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, 32)
        };
        painter.rect_filled(kr, CornerRadius::same(4), fill);
        let key_painter = painter.with_clip_rect(kr);

        if !label.is_empty() {
            let max_w = kr.width() - 3.0;
            let mut galley =
                key_painter.layout_no_wrap(label.clone(), FontId::proportional(9.5), TEXT_BRIGHT);
            if galley.size().x > max_w {
                if label.contains(' ') {
                    // Multi-word labels (custom labels mostly) wrap instead.
                    galley = key_painter.layout(label, FontId::proportional(7.0), TEXT_BRIGHT, max_w);
                } else {
                    let font = 9.5 * (max_w / galley.size().x).max(0.6);
                    galley =
                        key_painter.layout_no_wrap(label, FontId::proportional(font), TEXT_BRIGHT);
                }
            }
            key_painter.galley(kr.center() - galley.size() / 2.0, galley, Color32::WHITE);
        }
        if let Some(hold) = hold_text(key) {
            key_painter.text(
                pos2(kr.center().x, kr.bottom() - 1.0),
                Align2::CENTER_BOTTOM,
                hold,
                FontId::proportional(7.0),
                Color32::from_rgba_unmultiplied(200, 205, 235, 200),
            );
        }
    }
}

fn rotated_center(g: &geometry::KeyGeom) -> (f32, f32) {
    let cx = g.x + g.w / 2.0;
    let cy = g.y + g.h / 2.0;
    if g.rot_deg == 0.0 {
        return (cx, cy);
    }
    let (s, c) = g.rot_deg.to_radians().sin_cos();
    let (dx, dy) = (cx - g.rot_x, cy - g.rot_y);
    (g.rot_x + dx * c - dy * s, g.rot_y + dx * s + dy * c)
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
        GWL_EXSTYLE, GetWindowLongPtrW, IsWindowVisible, LWA_ALPHA, SW_HIDE, SW_SHOWNA,
        SetLayeredWindowAttributes, SetWindowLongPtrW, ShowWindow, WS_EX_APPWINDOW,
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
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

    /// SW_SHOWNA shows without activating. Raw ShowWindow instead of
    /// ViewportCommand::Visible because winit's set_visible can use SW_SHOW
    /// (which activates) and triggers its ex-style rewrite. Compares against
    /// the real window state so it converges even when winit re-shows us.
    pub fn sync_overlay_visible(frame: &eframe::Frame, visible: bool) {
        let Some(hwnd) = hwnd(frame) else { return };
        unsafe {
            if IsWindowVisible(hwnd).as_bool() != visible {
                let _ = ShowWindow(hwnd, if visible { SW_SHOWNA } else { SW_HIDE });
            }
        }
    }
}
