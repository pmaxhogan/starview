#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod display;
mod geometry;
#[cfg(windows)]
mod hdr;
mod hid;
mod keycodes;
mod oryx;
mod overlay;
mod settings;
mod stats;
#[cfg(windows)]
mod trackball;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod updater;

use eframe::egui;

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
    // Usage: starview [--always] [layout-hash-id] [geometry]
    let (flags, positional): (Vec<String>, Vec<String>) =
        std::env::args().skip(1).partition(|a| a.starts_with("--"));
    let always = flags.iter().any(|f| f == "--always");
    for flag in flags.iter().filter(|f| *f != "--always") {
        eprintln!("unknown flag {flag} (known: --always)");
    }
    let mut positional = positional.into_iter();
    let arg_layout = positional.next();
    let arg_geometry = positional.next();

    let mut cfg = settings::load();
    cfg.pin_base |= always;

    // Resolve the layout: a command-line argument wins and is remembered;
    // otherwise use the last-saved layout (which defaults to jmvGw/moonlander).
    let layout_id = arg_layout.clone().unwrap_or_else(|| cfg.layout_id.clone());
    let geometry = arg_geometry.clone().unwrap_or_else(|| cfg.geometry.clone());
    if (arg_layout.is_some() && layout_id != cfg.layout_id)
        || (arg_geometry.is_some() && geometry != cfg.geometry)
    {
        cfg.layout_id = layout_id.clone();
        cfg.geometry = geometry.clone();
        settings::save(&cfg);
        eprintln!("remembered layout {layout_id} ({geometry})");
    }

    let stats = stats::load();

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

            #[cfg(windows)]
            {
                let ctx = cc.egui_ctx.clone();
                let ball_tx = tx.clone();
                trackball::spawn_listener(move |dx, dy| {
                    let _ = ball_tx.send(overlay::AppEvent::Trackball(dx, dy));
                    ctx.request_repaint();
                });

                let ctx = cc.egui_ctx.clone();
                let tray_tx = tx.clone();
                tray::spawn(cfg.clone(), move |event| {
                    let _ = tray_tx.send(match event {
                        tray::TrayEvent::Settings(s) => overlay::AppEvent::Settings(s),
                        tray::TrayEvent::ResetStats => overlay::AppEvent::ResetStats,
                        tray::TrayEvent::ExportStats => overlay::AppEvent::ExportStats,
                        tray::TrayEvent::ToggleOverlay => overlay::AppEvent::ToggleOverlay,
                        tray::TrayEvent::Quit => overlay::AppEvent::Quit,
                    });
                    ctx.request_repaint();
                });

                updater::spawn_checker(tray::notify_update);
                hdr::spawn_monitor();
            }

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

            Ok(Box::new(overlay::OverlayApp::new(rx, layout, cfg, stats)))
        }),
    )
}
