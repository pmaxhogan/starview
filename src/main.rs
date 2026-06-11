#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod geometry;
mod hid;
mod keycodes;
mod oryx;
mod overlay;

use eframe::egui;

const DEFAULT_LAYOUT: &str = "jmvGw";
const DEFAULT_GEOMETRY: &str = "moonlander";

/// egui's default fonts have no plain-arrow glyphs (←↑→↓) and miss most of
/// the symbols people put in Oryx custom labels. Append Windows' Segoe UI
/// Symbol as a fallback so those render instead of tofu.
fn install_symbol_font(ctx: &egui::Context) {
    let path = r"C:\Windows\Fonts\seguisym.ttf";
    let Ok(bytes) = std::fs::read(path) else {
        eprintln!("symbol font not found at {path}; arrows may not render");
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "segoe-ui-symbol".to_owned(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("segoe-ui-symbol".to_owned());
    }
    ctx.set_fonts(fonts);
}

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
        Box::new(move |cc| {
            install_symbol_font(&cc.egui_ctx);
            let (tx, rx) = std::sync::mpsc::channel();

            let ctx = cc.egui_ctx.clone();
            let hid_tx = tx.clone();
            hid::spawn_watcher(move |event| {
                let _ = hid_tx.send(overlay::AppEvent::Hid(event));
                ctx.request_repaint();
            });

            // Periodic Oryx re-fetch so layout edits show up without a restart.
            let ctx = cc.egui_ctx.clone();
            std::thread::Builder::new()
                .name("oryx-refresh".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(300));
                        match oryx::load_layout(&layout_id, &geometry) {
                            Ok(info) => {
                                let _ = tx.send(overlay::AppEvent::Layout(info));
                                ctx.request_repaint();
                            }
                            Err(err) => eprintln!("layout refresh failed: {err:#}"),
                        }
                    }
                })
                .expect("failed to spawn oryx refresh thread");

            Ok(Box::new(overlay::OverlayApp::new(rx, layout)))
        }),
    )
}
