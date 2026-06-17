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
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PostThreadMessageW, TranslateMessage, WM_APP, WM_HOTKEY,
};

use crate::settings::{self, Corner, FADE_STEPS, OPACITY_STEPS, SIZE_STEPS, Settings, Theme};
use crate::updater;

pub enum TrayEvent {
    Settings(Settings),
    ResetStats,
    /// The global show/hide hotkey (Ctrl+Alt+O) was pressed.
    ToggleOverlay,
    Quit,
}

/// Global hotkey id for the overlay show/hide toggle.
const HOTKEY_TOGGLE: i32 = 1;

static TRAY_THREAD: AtomicU32 = AtomicU32::new(0);
static PENDING_UPDATE: Mutex<Option<UpdateState>> = Mutex::new(None);

/// Result of an update check, handed from a checker thread to the tray's
/// message pump (which owns the menu items).
enum UpdateState {
    /// A newer version was downloaded and is ready to install.
    Ready(String),
    /// The check finished and we're already on the latest version.
    UpToDate,
}

/// Tells the tray (from any thread) that an update is downloaded and ready;
/// the menu's update item lights up.
pub fn notify_update(version: &str) {
    set_update_state(UpdateState::Ready(version.to_owned()));
}

/// Tells the tray a manual check came back clean (no newer version).
fn notify_uptodate() {
    set_update_state(UpdateState::UpToDate);
}

fn set_update_state(state: UpdateState) {
    *PENDING_UPDATE.lock().unwrap() = Some(state);
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
    // Monitor picker — only meaningful with more than one display.
    let monitors = crate::display::monitors();
    let monitor_items: Vec<(usize, CheckMenuItem)> = monitors
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let label = if m.primary {
                format!("Monitor {} (primary)", i + 1)
            } else {
                format!("Monitor {}", i + 1)
            };
            (i, CheckMenuItem::new(label, true, i == initial.monitor, None))
        })
        .collect();
    let monitor_menu = Submenu::new("Overlay monitor", true);
    for (_, item) in &monitor_items {
        monitor_menu.append(item)?;
    }

    let opacity_items: Vec<(u8, CheckMenuItem)> = OPACITY_STEPS
        .into_iter()
        .map(|o| (o, CheckMenuItem::new(format!("{o}%"), true, o == initial.opacity, None)))
        .collect();
    let opacity_menu = Submenu::new("Opacity", true);
    for (_, item) in &opacity_items {
        opacity_menu.append(item)?;
    }
    let size_items: Vec<(u8, CheckMenuItem)> = SIZE_STEPS
        .into_iter()
        .map(|s| (s, CheckMenuItem::new(format!("{s}%"), true, s == initial.scale, None)))
        .collect();
    let size_menu = Submenu::new("Overlay size", true);
    for (_, item) in &size_items {
        size_menu.append(item)?;
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
    let theme_items: Vec<(Theme, CheckMenuItem)> = Theme::ALL
        .into_iter()
        .map(|t| (t, CheckMenuItem::new(t.label(), true, t == initial.theme, None)))
        .collect();
    let theme_menu = Submenu::new("Accent color", true);
    for (_, item) in &theme_items {
        theme_menu.append(item)?;
    }
    let rainbow = CheckMenuItem::new("Rainbow key ghosts", true, initial.rainbow, None);
    let heatmap = CheckMenuItem::new("Key heatmap", true, initial.heatmap, None);
    let error_heatmap = CheckMenuItem::new("Typo heatmap", true, initial.error_heatmap, None);
    // Mutually-exclusive board-coloring modes, grouped in their own submenu.
    let coloring_menu = Submenu::new("Key coloring", true);
    coloring_menu.append(&rainbow)?;
    coloring_menu.append(&heatmap)?;
    coloring_menu.append(&error_heatmap)?;
    let wpm = CheckMenuItem::new("Show typing speed", true, initial.show_wpm, None);
    let fingers = CheckMenuItem::new("Finger load chart", true, initial.show_fingers, None);
    let bigrams = CheckMenuItem::new("Show top bigrams", true, initial.show_bigrams, None);
    let daily = CheckMenuItem::new("Show daily count & streak", true, initial.show_daily, None);
    let reset_stats = MenuItem::new("Reset key stats", true, None);
    // Disabled label showing the running version. Enabled "Up to date" item
    // below doubles as a manual "check for updates" button.
    let version = MenuItem::new(format!("starview v{}", updater::current_version()), false, None);
    let update = MenuItem::new("Up to date", true, None);
    let quit = MenuItem::new("Quit starview", true, None);

    let menu = Menu::new();
    menu.append(&pin)?;
    menu.append(&corner_menu)?;
    if monitor_items.len() > 1 {
        menu.append(&monitor_menu)?;
    }
    menu.append(&opacity_menu)?;
    menu.append(&size_menu)?;
    menu.append(&fade_menu)?;
    menu.append(&theme_menu)?;
    menu.append(&coloring_menu)?;
    menu.append(&wpm)?;
    menu.append(&fingers)?;
    menu.append(&bigrams)?;
    menu.append(&daily)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&reset_stats)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&version)?;
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

    // Global show/hide hotkey: Ctrl+Alt+O. WM_HOTKEY lands in this thread's
    // message queue (hwnd = null), so the pump below picks it up. NOREPEAT so
    // holding the keys fires once.
    unsafe {
        if RegisterHotKey(None, HOTKEY_TOGGLE, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, b'O' as u32)
            .is_err()
        {
            eprintln!("could not register Ctrl+Alt+O hotkey (already in use?)");
        }
    }

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if msg.message == WM_HOTKEY && msg.wParam.0 as i32 == HOTKEY_TOGGLE {
                on_event(TrayEvent::ToggleOverlay);
            }
            // A checker thread wakes us with WM_APP when it has news (we're on
            // the tray thread here, so touching the menu items is fine).
            if let Some(state) = PENDING_UPDATE.lock().unwrap().take() {
                match state {
                    UpdateState::Ready(version) => {
                        update.set_text(format!("Install update v{version}"));
                    }
                    UpdateState::UpToDate => update.set_text("Up to date"),
                }
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
                } else if let Some((theme, _)) =
                    theme_items.iter().find(|(_, item)| *event.id() == item.id())
                {
                    state.theme = *theme;
                    for (t, item) in &theme_items {
                        item.set_checked(*t == state.theme);
                    }
                } else if *event.id() == rainbow.id()
                    || *event.id() == heatmap.id()
                    || *event.id() == error_heatmap.id()
                {
                    // The three board-coloring modes are mutually exclusive:
                    // turning one on clears the others; clicking the active one
                    // again turns it off (back to the plain board). muda has
                    // already toggled the clicked item, so its is_checked() is
                    // the new state; the unclicked ones are forced off.
                    state.rainbow = *event.id() == rainbow.id() && rainbow.is_checked();
                    state.heatmap = *event.id() == heatmap.id() && heatmap.is_checked();
                    state.error_heatmap =
                        *event.id() == error_heatmap.id() && error_heatmap.is_checked();
                    rainbow.set_checked(state.rainbow);
                    heatmap.set_checked(state.heatmap);
                    error_heatmap.set_checked(state.error_heatmap);
                } else if *event.id() == wpm.id() {
                    state.show_wpm = wpm.is_checked();
                } else if *event.id() == fingers.id() {
                    state.show_fingers = fingers.is_checked();
                } else if *event.id() == bigrams.id() {
                    state.show_bigrams = bigrams.is_checked();
                } else if *event.id() == daily.id() {
                    state.show_daily = daily.is_checked();
                } else if *event.id() == reset_stats.id() {
                    on_event(TrayEvent::ResetStats);
                    continue;
                } else if *event.id() == quit.id() {
                    on_event(TrayEvent::Quit);
                    continue;
                } else if *event.id() == update.id() {
                    if updater::install_ready_update() {
                        // The installer stops us, swaps the exe, relaunches.
                        on_event(TrayEvent::Quit);
                    } else {
                        // Nothing downloaded yet: run a manual check. The
                        // network call blocks, so it runs off-thread and reports
                        // back via notify_update / notify_uptodate.
                        update.set_text("Checking for updates\u{2026}");
                        update.set_enabled(false);
                        updater::check_now(|found| match found {
                            Some(version) => notify_update(&version),
                            None => notify_uptodate(),
                        });
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
                } else if let Some((idx, _)) =
                    monitor_items.iter().find(|(_, item)| *event.id() == item.id())
                {
                    state.monitor = *idx;
                    // Re-dock to the chosen monitor's corner, dropping any
                    // dragged spot.
                    state.position = None;
                    for (i, item) in &monitor_items {
                        item.set_checked(*i == state.monitor);
                    }
                } else if let Some((opacity, _)) =
                    opacity_items.iter().find(|(_, item)| *event.id() == item.id())
                {
                    state.opacity = *opacity;
                    for (o, item) in &opacity_items {
                        item.set_checked(*o == state.opacity);
                    }
                } else if let Some((scale, _)) =
                    size_items.iter().find(|(_, item)| *event.id() == item.id())
                {
                    state.scale = *scale;
                    for (s, item) in &size_items {
                        item.set_checked(*s == state.scale);
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
