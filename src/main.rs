#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hid;
mod oryx;
mod overlay;

use eframe::egui;

const DEFAULT_LAYOUT: &str = "jmvGw";
const DEFAULT_GEOMETRY: &str = "moonlander";

fn main() -> eframe::Result {
    // Usage: starview [layout-hash-id] [geometry]
    let mut args = std::env::args().skip(1);
    let layout_id = args.next().unwrap_or_else(|| DEFAULT_LAYOUT.to_owned());
    let geometry = args.next().unwrap_or_else(|| DEFAULT_GEOMETRY.to_owned());

    let layout = match oryx::load_layout(&layout_id, &geometry) {
        Ok(info) => {
            eprintln!("layout: {} ({} layers)", info.title, info.layers.len());
            Some(info)
        }
        Err(err) => {
            // Not fatal: the overlay falls back to layer numbers.
            eprintln!("could not load layer names from Oryx: {err:#}");
            None
        }
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("starview")
            .with_inner_size([overlay::OVERLAY_W, overlay::OVERLAY_H])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_mouse_passthrough(true)
            .with_taskbar(false)
            .with_active(false)
            .with_drag_and_drop(false)
            .with_resizable(false),
        // wgpu (the default) can't do transparent windows on Windows; glow can.
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "starview",
        options,
        Box::new(|cc| {
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = cc.egui_ctx.clone();
            hid::spawn_watcher(move |event| {
                let _ = tx.send(event);
                ctx.request_repaint();
            });
            Ok(Box::new(overlay::OverlayApp::new(rx, layout)))
        }),
    )
}
