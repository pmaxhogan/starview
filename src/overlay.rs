//! The overlay window: a frameless, transparent, click-through, always-on-top
//! bubble in the top-right corner naming the active non-base layer.
//!
//! Window behavior relies on raw Win32 extended styles (see `win32` below):
//! winit can't express NOACTIVATE/TOOLWINDOW, and it rewrites GWL_EXSTYLE
//! wholesale on its own flag changes, so the styles are re-asserted every
//! `logic` tick (a no-op compare once stable).

use std::sync::mpsc::Receiver;

use eframe::egui;

use crate::hid::LayerEvent;
use crate::oryx::LayoutInfo;

pub const OVERLAY_W: f32 = 360.0;
pub const OVERLAY_H: f32 = 64.0;
/// Gap between the overlay window and the screen edge, in logical points.
const SCREEN_MARGIN: f32 = 12.0;

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
        self.shown = self.connected && self.layer != 0;
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
        let painter = ui.painter();
        let galley = painter.layout_no_wrap(
            self.label(),
            egui::FontId::proportional(24.0),
            egui::Color32::from_rgb(240, 240, 255),
        );
        let pad = egui::vec2(18.0, 11.0);
        let size = galley.size() + pad * 2.0;
        let max = ui.max_rect();
        let rect = egui::Rect::from_min_size(egui::pos2(max.right() - size.x, max.top()), size);
        painter.rect_filled(
            rect,
            egui::CornerRadius::same(13),
            egui::Color32::from_rgba_unmultiplied(16, 18, 28, 200),
        );
        painter.galley(rect.min + pad, galley, egui::Color32::WHITE);
    }
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
