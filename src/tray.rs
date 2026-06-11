//! System tray icon with the settings menu.
//!
//! Runs on its own thread with a Win32 message pump. Menu items are not Send,
//! so instead of muda's global event handler, menu events are drained from the
//! event channel right after each dispatched message — still on this thread,
//! with full access to the items for check-mark updates.

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIconBuilder};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, TranslateMessage,
};

use crate::settings::{self, Corner, Settings};

pub enum TrayEvent {
    Settings(Settings),
    Quit,
}

pub fn spawn(initial: Settings, mut on_event: impl FnMut(TrayEvent) + Send + 'static) {
    std::thread::Builder::new()
        .name("tray".into())
        .spawn(move || {
            if let Err(err) = run(initial, &mut on_event) {
                eprintln!("tray icon failed: {err}");
            }
        })
        .expect("failed to spawn tray thread");
}

fn run(
    initial: Settings,
    on_event: &mut impl FnMut(TrayEvent),
) -> Result<(), Box<dyn std::error::Error>> {
    let pin = CheckMenuItem::new("Pin base layer", true, initial.pin_base, None);
    let corner_items: Vec<(Corner, CheckMenuItem)> = Corner::ALL
        .into_iter()
        .map(|c| (c, CheckMenuItem::new(c.label(), true, c == initial.corner, None)))
        .collect();
    let corner_menu = Submenu::new("Overlay corner", true);
    for (_, item) in &corner_items {
        corner_menu.append(item)?;
    }
    let quit = MenuItem::new("Quit starview", true, None);

    let menu = Menu::new();
    menu.append(&pin)?;
    menu.append(&corner_menu)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit)?;
    // Generous bottom padding: auto-hiding taskbars pop up OVER the bottom of
    // the menu, so inert blank rows take the hit instead of the real items.
    menu.append(&PredefinedMenuItem::separator())?;
    for _ in 0..3 {
        menu.append(&MenuItem::new("", false, None))?;
    }

    // Must stay alive for the icon to remain in the tray.
    let _tray = TrayIconBuilder::new()
        .with_tooltip("starview — keyboard layer overlay")
        .with_icon(make_icon())
        .with_menu(Box::new(menu))
        .build()?;

    let mut state = initial;
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            // Menu clicks were queued by the dispatch above.
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if *event.id() == pin.id() {
                    // muda already toggled the check mark.
                    state.pin_base = pin.is_checked();
                } else if *event.id() == quit.id() {
                    on_event(TrayEvent::Quit);
                    continue;
                } else if let Some((corner, _)) =
                    corner_items.iter().find(|(_, item)| *event.id() == item.id())
                {
                    state.corner = *corner;
                    for (c, item) in &corner_items {
                        item.set_checked(*c == state.corner);
                    }
                } else {
                    continue;
                }
                settings::save(&state);
                on_event(TrayEvent::Settings(state));
            }
        }
    }
    Ok(())
}

/// Dark disc with the trackball-blue dot — drawn in code, no asset file.
fn make_icon() -> Icon {
    const S: usize = 32;
    let mut rgba = vec![0u8; S * S * 4];
    for y in 0..S {
        for x in 0..S {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            let r = (dx * dx + dy * dy).sqrt();
            let px = (y * S + x) * 4;
            if r < 15.0 {
                rgba[px..px + 4].copy_from_slice(&[26, 30, 46, 235]);
            }
            if r < 6.0 {
                rgba[px..px + 4].copy_from_slice(&[110, 165, 255, 255]);
            }
        }
    }
    Icon::from_rgba(rgba, S as u32, S as u32).expect("static icon dimensions are valid")
}
