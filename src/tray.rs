//! System tray icon with the settings menu.
//!
//! Runs on its own thread with a Win32 message pump. Menu items are not Send,
//! so instead of muda's global event handler, menu events are drained from the
//! event channel right after each dispatched message — still on this thread,
//! with full access to the items for check-mark updates.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIconBuilder};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PostThreadMessageW, TranslateMessage, WM_APP,
};

use crate::settings::{self, Corner, FADE_STEPS, OPACITY_STEPS, Settings};
use crate::updater;

pub enum TrayEvent {
    Settings(Settings),
    Quit,
}

static TRAY_THREAD: AtomicU32 = AtomicU32::new(0);
static PENDING_UPDATE: Mutex<Option<String>> = Mutex::new(None);

/// Tells the tray (from any thread) that an update is downloaded and ready;
/// the menu's update item lights up.
pub fn notify_update(version: &str) {
    *PENDING_UPDATE.lock().unwrap() = Some(version.to_owned());
    let tid = TRAY_THREAD.load(Ordering::Relaxed);
    if tid != 0 {
        // Wake the message pump so it notices.
        unsafe {
            let _ = PostThreadMessageW(
                tid,
                WM_APP,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            );
        }
    }
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
    let opacity_items: Vec<(u8, CheckMenuItem)> = OPACITY_STEPS
        .into_iter()
        .map(|o| (o, CheckMenuItem::new(format!("{o}%"), true, o == initial.opacity, None)))
        .collect();
    let opacity_menu = Submenu::new("Opacity", true);
    for (_, item) in &opacity_items {
        opacity_menu.append(item)?;
    }
    let fade_items: Vec<(u16, CheckMenuItem)> = FADE_STEPS
        .into_iter()
        .map(|(label, secs)| {
            (secs, CheckMenuItem::new(label, true, secs == initial.fade_secs, None))
        })
        .collect();
    let fade_menu = Submenu::new("Auto-hide after", true);
    for (_, item) in &fade_items {
        fade_menu.append(item)?;
    }
    let rainbow = CheckMenuItem::new("Rainbow key ghosts", true, initial.rainbow, None);
    let heatmap = CheckMenuItem::new("Key heatmap", true, initial.heatmap, None);
    let update = MenuItem::new("Up to date", false, None);
    let quit = MenuItem::new("Quit starview", true, None);

    let menu = Menu::new();
    menu.append(&pin)?;
    menu.append(&corner_menu)?;
    menu.append(&opacity_menu)?;
    menu.append(&fade_menu)?;
    menu.append(&rainbow)?;
    menu.append(&heatmap)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&update)?;
    menu.append(&quit)?;
    // Generous bottom padding: auto-hiding taskbars pop up OVER the bottom of
    // the menu, so inert blank rows take the hit instead of the real items.
    menu.append(&PredefinedMenuItem::separator())?;
    for _ in 0..2 {
        menu.append(&MenuItem::new("", false, None))?;
    }

    // Must stay alive for the icon to remain in the tray.
    let _tray = TrayIconBuilder::new()
        .with_tooltip("starview — keyboard layer overlay")
        .with_icon(make_icon())
        .with_menu(Box::new(menu))
        .build()?;

    TRAY_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            // The update checker wakes us with WM_APP once a new version is
            // downloaded; light up the install item (we're on the tray thread
            // here, so touching the menu items is fine).
            if let Some(version) = PENDING_UPDATE.lock().unwrap().take() {
                update.set_text(format!("Install update v{version}"));
                update.set_enabled(true);
            }
            // Menu clicks were queued by the dispatch above.
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                // Reload from disk before applying: the overlay writes the
                // shift-dragged `position` on its own, and a stale in-memory
                // copy here would silently clobber it.
                let mut state = settings::load();
                if *event.id() == pin.id() {
                    // muda already toggled the check mark.
                    state.pin_base = pin.is_checked();
                } else if *event.id() == rainbow.id() {
                    state.rainbow = rainbow.is_checked();
                } else if *event.id() == heatmap.id() {
                    state.heatmap = heatmap.is_checked();
                } else if *event.id() == quit.id() {
                    on_event(TrayEvent::Quit);
                    continue;
                } else if *event.id() == update.id() {
                    if updater::install_ready_update() {
                        // The installer stops us, swaps the exe, relaunches.
                        on_event(TrayEvent::Quit);
                    }
                    continue;
                } else if let Some((corner, _)) =
                    corner_items.iter().find(|(_, item)| *event.id() == item.id())
                {
                    state.corner = *corner;
                    // Picking a corner re-docks, dropping any dragged spot.
                    state.position = None;
                    for (c, item) in &corner_items {
                        item.set_checked(*c == state.corner);
                    }
                } else if let Some((opacity, _)) =
                    opacity_items.iter().find(|(_, item)| *event.id() == item.id())
                {
                    state.opacity = *opacity;
                    for (o, item) in &opacity_items {
                        item.set_checked(*o == state.opacity);
                    }
                } else if let Some((secs, _)) =
                    fade_items.iter().find(|(_, item)| *event.id() == item.id())
                {
                    state.fade_secs = *secs;
                    for (s, item) in &fade_items {
                        item.set_checked(*s == state.fade_secs);
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
