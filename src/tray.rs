//! System tray icon with the settings menu.
//!
//! The menu itself (muda via tray-icon) is cross-platform; what differs is
//! where it runs. On Windows a dedicated thread owns the tray and pumps Win32
//! messages; menu items are not Send, so events are drained on that thread
//! right after each dispatched message. On macOS the tray must live on the
//! main thread, so it's built during app creation and the overlay's update
//! loop polls [`TrayState::drain`] there.

use std::sync::Mutex;
#[cfg(windows)]
use std::sync::atomic::{AtomicU32, Ordering};

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
#[cfg(windows)]
use windows::Win32::System::Threading::GetCurrentThreadId;
#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey,
};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PostThreadMessageW, TranslateMessage, WM_APP, WM_HOTKEY,
};

use crate::settings::{
    self, Corner, FADE_STEPS, OPACITY_STEPS, SIZE_STEPS, Settings, Theme, TimeWindow,
};
use crate::updater;

pub enum TrayEvent {
    Settings(Settings),
    ResetStats,
    ExportStats,
    /// The global show/hide hotkey (Ctrl+Alt+O) was pressed.
    ToggleOverlay,
    Quit,
}

/// Global hotkey id for the overlay show/hide toggle.
#[cfg(windows)]
const HOTKEY_TOGGLE: i32 = 1;

#[cfg(windows)]
static TRAY_THREAD: AtomicU32 = AtomicU32::new(0);
static PENDING_UPDATE: Mutex<Option<UpdateState>> = Mutex::new(None);

/// Result of an update check, handed from a checker thread to whichever
/// thread owns the menu items.
enum UpdateState {
    /// A newer version was downloaded (or found) and is ready to install.
    Ready(String),
    /// The check finished and we're already on the latest version.
    UpToDate,
}

/// Tells the tray (from any thread) that an update is ready; the menu's
/// update item lights up.
pub fn notify_update(version: &str) {
    set_update_state(UpdateState::Ready(version.to_owned()));
}

/// Tells the tray a manual check came back clean (no newer version).
fn notify_uptodate() {
    set_update_state(UpdateState::UpToDate);
}

fn set_update_state(state: UpdateState) {
    *PENDING_UPDATE.lock().unwrap() = Some(state);
    // On Windows the tray thread sleeps in GetMessage; wake it. On macOS the
    // overlay's repaint heartbeat polls drain() within a second.
    #[cfg(windows)]
    {
        let tid = TRAY_THREAD.load(Ordering::Relaxed);
        if tid != 0 {
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
}

/// The tray icon, its menu, and every item the event handler needs to read or
/// re-check. Not Send: lives on the tray thread (Windows) or the main thread
/// (macOS).
pub struct TrayState {
    // Must stay alive for the icon to remain in the tray.
    _tray: TrayIcon,
    menu: Menu,
    pin: CheckMenuItem,
    corner_items: Vec<(Corner, CheckMenuItem)>,
    monitor_menu: Submenu,
    monitor_items: Vec<(usize, CheckMenuItem)>,
    monitor_menu_shown: bool,
    monitor_sig: Vec<MonitorSig>,
    fullscreen: CheckMenuItem,
    opacity_items: Vec<(u8, CheckMenuItem)>,
    size_items: Vec<(u8, CheckMenuItem)>,
    fade_items: Vec<(u16, CheckMenuItem)>,
    theme_items: Vec<(Theme, CheckMenuItem)>,
    rainbow: CheckMenuItem,
    heatmap: CheckMenuItem,
    error_heatmap: CheckMenuItem,
    wpm: CheckMenuItem,
    fingers: CheckMenuItem,
    bigrams: CheckMenuItem,
    daily: CheckMenuItem,
    subs: CheckMenuItem,
    range_items: Vec<(TimeWindow, CheckMenuItem)>,
    export_stats: MenuItem,
    reset_stats: MenuItem,
    update: MenuItem,
    quit: MenuItem,
    /// "Start at login" (macOS, only when running from a .app bundle —
    /// SMAppService needs a bundle identity).
    #[cfg(target_os = "macos")]
    login: Option<CheckMenuItem>,
    /// Keeps the Ctrl+Alt+O registration alive (macOS).
    #[cfg(target_os = "macos")]
    hotkey: Option<global_hotkey::GlobalHotKeyManager>,
    /// Counts drain() calls so the periodic monitor rescan (macOS) doesn't
    /// hit the disk-backed settings on every heartbeat.
    #[cfg(target_os = "macos")]
    rescan_tick: u32,
}

impl TrayState {
    pub fn build(initial: &Settings) -> Result<Self, Box<dyn std::error::Error>> {
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
        // Populated by rescan_monitors below and re-scanned periodically, so a
        // plugged/unplugged display is reflected the next time the menu opens.
        let monitor_menu = Submenu::new("Overlay monitor", true);
        // Fullscreen "display" mode: cover the chosen monitor entirely. Pair
        // it with "Overlay monitor" to dedicate a secondary display to
        // starview.
        let fullscreen = CheckMenuItem::new("Fullscreen display", true, initial.fullscreen, None);

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
        let subs = CheckMenuItem::new("Show typo confusions", true, initial.show_subs, None);
        let range_items: Vec<(TimeWindow, CheckMenuItem)> = TimeWindow::ALL
            .into_iter()
            .map(|w| {
                (w, CheckMenuItem::new(w.label(), true, w == initial.heatmap_range, None))
            })
            .collect();
        let range_menu = Submenu::new("Heatmap range", true);
        for (_, item) in &range_items {
            range_menu.append(item)?;
        }
        let export_stats = MenuItem::new("Export stats\u{2026}", true, None);
        let reset_stats = MenuItem::new("Reset key stats", true, None);
        // Disabled label showing the running version. Enabled "Up to date"
        // item below doubles as a manual "check for updates" button.
        let version =
            MenuItem::new(format!("starview v{}", updater::current_version()), false, None);
        let update = MenuItem::new("Up to date", true, None);
        let quit = MenuItem::new("Quit starview", true, None);
        #[cfg(target_os = "macos")]
        let login = running_from_bundle().then(|| {
            CheckMenuItem::new("Start at login", true, login_item_enabled(), None)
        });

        let menu = Menu::new();
        menu.append(&pin)?;
        menu.append(&corner_menu)?;
        menu.append(&fullscreen)?;
        menu.append(&opacity_menu)?;
        menu.append(&size_menu)?;
        menu.append(&fade_menu)?;
        menu.append(&theme_menu)?;
        menu.append(&coloring_menu)?;
        menu.append(&range_menu)?;
        menu.append(&wpm)?;
        menu.append(&fingers)?;
        menu.append(&bigrams)?;
        menu.append(&daily)?;
        menu.append(&subs)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&export_stats)?;
        menu.append(&reset_stats)?;
        menu.append(&PredefinedMenuItem::separator())?;
        #[cfg(target_os = "macos")]
        if let Some(login) = &login {
            menu.append(login)?;
        }
        menu.append(&version)?;
        menu.append(&update)?;
        menu.append(&quit)?;
        // Generous bottom padding: auto-hiding taskbars pop up OVER the bottom
        // of the menu, so inert blank rows take the hit instead of the real
        // items. (Harmless on macOS.)
        menu.append(&PredefinedMenuItem::separator())?;
        for _ in 0..2 {
            menu.append(&MenuItem::new("", false, None))?;
        }

        // Box a clone for the tray (a muda Menu is an Rc handle, so the clone
        // and `menu` drive the same underlying menu); keep `menu` to mutate at
        // runtime.
        let builder = TrayIconBuilder::new()
            .with_tooltip("starview — keyboard layer overlay")
            .with_icon(make_icon())
            .with_menu(Box::new(menu.clone()));
        // Template image: the menu bar tints it to match the other status
        // icons (white on dark menu bars, black on light).
        #[cfg(target_os = "macos")]
        let builder = builder.with_icon_as_template(true);
        let _tray = builder.build()?;

        #[cfg(target_os = "macos")]
        let hotkey = register_hotkey();

        let mut state = Self {
            _tray,
            menu,
            pin,
            corner_items,
            monitor_menu,
            monitor_items: Vec::new(),
            monitor_menu_shown: false,
            monitor_sig: Vec::new(),
            fullscreen,
            opacity_items,
            size_items,
            fade_items,
            theme_items,
            rainbow,
            heatmap,
            error_heatmap,
            wpm,
            fingers,
            bigrams,
            daily,
            subs,
            range_items,
            export_stats,
            reset_stats,
            update,
            quit,
            #[cfg(target_os = "macos")]
            login,
            #[cfg(target_os = "macos")]
            hotkey,
            #[cfg(target_os = "macos")]
            rescan_tick: 0,
        };
        // Populate "Overlay monitor" from the current displays.
        state.rescan_monitors(initial.monitor);
        Ok(state)
    }

    /// Handles everything that has queued up since the last call: update-check
    /// results, tray-icon clicks (monitor rescans), the global hotkey (macOS),
    /// and menu item clicks. Must run on the thread that built the tray.
    pub fn drain(&mut self, on_event: &mut impl FnMut(TrayEvent)) {
        // A checker thread left news in PENDING_UPDATE (we own the menu items
        // here, so touching them is fine).
        if let Some(state) = PENDING_UPDATE.lock().unwrap().take() {
            match state {
                UpdateState::Ready(version) => {
                    // Windows installs in place; macOS opens the release page.
                    self.update.set_text(if cfg!(windows) {
                        format!("Install update v{version}")
                    } else {
                        format!("Update v{version} available\u{2026}")
                    });
                }
                UpdateState::UpToDate => self.update.set_text("Up to date"),
            }
            self.update.set_enabled(true);
        }

        #[cfg(target_os = "macos")]
        {
            use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
            if self.hotkey.is_some() {
                while let Ok(e) = GlobalHotKeyEvent::receiver().try_recv() {
                    if e.state() == HotKeyState::Pressed {
                        on_event(TrayEvent::ToggleOverlay);
                    }
                }
            }
            // No usable "menu is about to open" hook on macOS (the click event
            // arrives after the menu closes), so rescan on a slow timer —
            // drain() runs on the overlay's ~1 s heartbeat.
            self.rescan_tick = self.rescan_tick.wrapping_add(1);
            if self.rescan_tick.is_multiple_of(10) {
                self.rescan_monitors(settings::load().monitor);
            }
        }

        // A tray click is about to open the menu: the menu pops on button-up,
        // but a Click{Down} event fires first (same thread, before
        // TrackPopupMenu). Re-scan monitors in that gap so a plugged/unplugged
        // display shows up — a no-op when unchanged.
        while let Ok(tray_event) = TrayIconEvent::receiver().try_recv() {
            if matches!(
                tray_event,
                TrayIconEvent::Click { button_state: MouseButtonState::Down, .. }
            ) {
                self.rescan_monitors(settings::load().monitor);
            }
        }

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            self.handle_menu_event(&event, on_event);
        }
    }

    fn handle_menu_event(&mut self, event: &MenuEvent, on_event: &mut impl FnMut(TrayEvent)) {
        // Reload from disk before applying: the overlay writes the
        // shift-dragged `position` on its own, and a stale in-memory copy here
        // would silently clobber it.
        let mut state = settings::load();
        if *event.id() == self.pin.id() {
            // muda already toggled the check mark.
            state.pin_base = self.pin.is_checked();
        } else if *event.id() == self.fullscreen.id() {
            state.fullscreen = self.fullscreen.is_checked();
        } else if let Some((theme, _)) =
            self.theme_items.iter().find(|(_, item)| *event.id() == item.id())
        {
            state.theme = *theme;
            for (t, item) in &self.theme_items {
                item.set_checked(*t == state.theme);
            }
        } else if *event.id() == self.rainbow.id()
            || *event.id() == self.heatmap.id()
            || *event.id() == self.error_heatmap.id()
        {
            // The three board-coloring modes are mutually exclusive: turning
            // one on clears the others; clicking the active one again turns it
            // off (back to the plain board). muda has already toggled the
            // clicked item, so its is_checked() is the new state; the
            // unclicked ones are forced off.
            state.rainbow = *event.id() == self.rainbow.id() && self.rainbow.is_checked();
            state.heatmap = *event.id() == self.heatmap.id() && self.heatmap.is_checked();
            state.error_heatmap =
                *event.id() == self.error_heatmap.id() && self.error_heatmap.is_checked();
            self.rainbow.set_checked(state.rainbow);
            self.heatmap.set_checked(state.heatmap);
            self.error_heatmap.set_checked(state.error_heatmap);
        } else if *event.id() == self.wpm.id() {
            state.show_wpm = self.wpm.is_checked();
        } else if *event.id() == self.fingers.id() {
            state.show_fingers = self.fingers.is_checked();
        } else if *event.id() == self.bigrams.id() {
            state.show_bigrams = self.bigrams.is_checked();
        } else if *event.id() == self.daily.id() {
            state.show_daily = self.daily.is_checked();
        } else if *event.id() == self.subs.id() {
            state.show_subs = self.subs.is_checked();
        } else if let Some((range, _)) =
            self.range_items.iter().find(|(_, item)| *event.id() == item.id())
        {
            state.heatmap_range = *range;
            for (w, item) in &self.range_items {
                item.set_checked(*w == state.heatmap_range);
            }
        } else if *event.id() == self.export_stats.id() {
            on_event(TrayEvent::ExportStats);
            return;
        } else if *event.id() == self.reset_stats.id() {
            on_event(TrayEvent::ResetStats);
            return;
        } else if *event.id() == self.quit.id() {
            on_event(TrayEvent::Quit);
            return;
        } else if *event.id() == self.update.id() {
            match updater::install_ready_update() {
                // The installer stops us, swaps the exe, relaunches.
                updater::InstallAction::Quit => on_event(TrayEvent::Quit),
                updater::InstallAction::Handled => {}
                updater::InstallAction::NotReady => {
                    // Nothing staged yet: run a manual check. The network call
                    // blocks, so it runs off-thread and reports back via
                    // notify_update / notify_uptodate.
                    self.update.set_text("Checking for updates\u{2026}");
                    self.update.set_enabled(false);
                    updater::check_now(|found| match found {
                        Some(version) => notify_update(&version),
                        None => notify_uptodate(),
                    });
                }
            }
            return;
        } else if let Some((corner, _)) =
            self.corner_items.iter().find(|(_, item)| *event.id() == item.id())
        {
            state.corner = *corner;
            // Picking a corner re-docks, dropping any dragged spot.
            state.position = None;
            for (c, item) in &self.corner_items {
                item.set_checked(*c == state.corner);
            }
        } else if let Some((idx, _)) =
            self.monitor_items.iter().find(|(_, item)| *event.id() == item.id())
        {
            state.monitor = *idx;
            // Re-dock to the chosen monitor's corner, dropping any dragged
            // spot.
            state.position = None;
            for (i, item) in &self.monitor_items {
                item.set_checked(*i == state.monitor);
            }
        } else if let Some((opacity, _)) =
            self.opacity_items.iter().find(|(_, item)| *event.id() == item.id())
        {
            state.opacity = *opacity;
            for (o, item) in &self.opacity_items {
                item.set_checked(*o == state.opacity);
            }
        } else if let Some((scale, _)) =
            self.size_items.iter().find(|(_, item)| *event.id() == item.id())
        {
            state.scale = *scale;
            for (s, item) in &self.size_items {
                item.set_checked(*s == state.scale);
            }
        } else if let Some((secs, _)) =
            self.fade_items.iter().find(|(_, item)| *event.id() == item.id())
        {
            state.fade_secs = *secs;
            for (s, item) in &self.fade_items {
                item.set_checked(*s == state.fade_secs);
            }
        } else {
            #[cfg(target_os = "macos")]
            if let Some(login) = &self.login
                && *event.id() == login.id()
            {
                // Not a Settings field: registers/unregisters the app as a
                // Login Item, then re-syncs the check mark from the service's
                // actual status (registration can fail or need approval in
                // System Settings > General > Login Items).
                set_login_item(login.is_checked());
                login.set_checked(login_item_enabled());
            }
            return;
        }
        settings::save(&state);
        on_event(TrayEvent::Settings(state));
    }

    /// Rebuild the "Overlay monitor" submenu from a fresh display scan, so
    /// plugging or unplugging a monitor is reflected without a restart.
    /// No-ops when the set is unchanged. The submenu is only present in the
    /// top menu with >1 display, inserted at index 2 (right after "Pin base
    /// layer" and "Overlay corner").
    fn rescan_monitors(&mut self, selected: usize) {
        const INSERT_AT: usize = 2;
        let mons = crate::display::monitors();
        let new_sig: Vec<MonitorSig> = mons
            .iter()
            .map(|m| (m.left, m.top, m.right, m.bottom, m.primary))
            .collect();
        if new_sig == self.monitor_sig {
            return; // Displays unchanged — leave the menu (and check marks) as is.
        }
        self.monitor_sig = new_sig;

        // Drop the old items, then build fresh ones for the current displays.
        for (_, item) in &self.monitor_items {
            let _ = self.monitor_menu.remove(item);
        }
        let (labels, should_show) = monitor_menu_plan(&mons, selected);
        self.monitor_items = labels
            .into_iter()
            .enumerate()
            .map(|(i, (label, checked))| (i, CheckMenuItem::new(label, true, checked, None)))
            .collect();
        for (_, item) in &self.monitor_items {
            let _ = self.monitor_menu.append(item);
        }

        // Show the submenu only when picking a monitor is meaningful (>1
        // display).
        if should_show && !self.monitor_menu_shown {
            let _ = self.menu.insert(&self.monitor_menu, INSERT_AT);
            self.monitor_menu_shown = true;
        } else if !should_show && self.monitor_menu_shown {
            let _ = self.menu.remove(&self.monitor_menu);
            self.monitor_menu_shown = false;
        }
    }
}

/// Windows: the tray runs on its own thread with a Win32 message pump.
#[cfg(windows)]
pub fn spawn(initial: Settings, mut on_event: impl FnMut(TrayEvent) + Send + 'static) {
    std::thread::Builder::new()
        .name("tray".into())
        .spawn(move || {
            if let Err(err) = pump(initial, &mut on_event) {
                eprintln!("tray icon failed: {err}");
            }
        })
        .expect("failed to spawn tray thread");
}

#[cfg(windows)]
fn pump(
    initial: Settings,
    on_event: &mut impl FnMut(TrayEvent),
) -> Result<(), Box<dyn std::error::Error>> {
    let mut state = TrayState::build(&initial)?;

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
            state.drain(on_event);
        }
    }
    Ok(())
}

/// macOS: build the tray on the main thread (an AppKit requirement); the
/// overlay's update loop polls [`TrayState::drain`] from then on.
#[cfg(target_os = "macos")]
pub fn init(initial: &Settings) -> Option<TrayState> {
    match TrayState::build(initial) {
        Ok(state) => Some(state),
        Err(err) => {
            eprintln!("tray icon failed: {err}");
            None
        }
    }
}

/// Ctrl+Alt+O via a Carbon event hotkey — no accessibility permission needed.
/// The returned manager must stay alive for the registration to hold.
#[cfg(target_os = "macos")]
fn register_hotkey() -> Option<global_hotkey::GlobalHotKeyManager> {
    use global_hotkey::GlobalHotKeyManager;
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};

    let manager = match GlobalHotKeyManager::new() {
        Ok(m) => m,
        Err(err) => {
            eprintln!("global hotkey manager failed: {err}");
            return None;
        }
    };
    let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyO);
    if let Err(err) = manager.register(hotkey) {
        eprintln!("could not register Ctrl+Alt+O hotkey (already in use?): {err}");
        return None;
    }
    Some(manager)
}

/// SMAppService only works from a real .app bundle; when run as a bare
/// binary (cargo run) the Login Item toggle is hidden entirely.
#[cfg(target_os = "macos")]
fn running_from_bundle() -> bool {
    std::env::current_exe()
        .is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS"))
}

#[cfg(target_os = "macos")]
fn login_item_enabled() -> bool {
    use objc2_service_management::{SMAppService, SMAppServiceStatus};
    unsafe { SMAppService::mainAppService().status() == SMAppServiceStatus::Enabled }
}

#[cfg(target_os = "macos")]
fn set_login_item(enable: bool) {
    use objc2_service_management::SMAppService;
    let service = unsafe { SMAppService::mainAppService() };
    let result = if enable {
        unsafe { service.registerAndReturnError() }
    } else {
        unsafe { service.unregisterAndReturnError() }
    };
    if let Err(err) = result {
        eprintln!("login item {}: {err}", if enable { "register" } else { "unregister" });
    }
}

/// Identity of a display for change detection: its physical rect + primary
/// flag. The whole list is compared so a resolution change, a moved monitor,
/// or an add/remove all count as "changed".
type MonitorSig = (i32, i32, i32, i32, bool);

/// Decide the monitor submenu from a display scan: a `(label, checked)` per
/// monitor (the selected one checked), and whether the submenu should appear
/// at all — only meaningful with more than one display. Pure, so it's unit
/// tested; rescan_monitors turns it into actual menu items.
fn monitor_menu_plan(
    mons: &[crate::display::Monitor],
    selected: usize,
) -> (Vec<(String, bool)>, bool) {
    let labels = mons
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let label = if m.primary {
                format!("Monitor {} (primary)", i + 1)
            } else {
                format!("Monitor {}", i + 1)
            };
            (label, i == selected)
        })
        .collect();
    (labels, mons.len() > 1)
}

/// Drawn in code, no asset file. Windows: dark disc with the trackball-blue
/// dot. macOS: the same shape as a monochrome template — a ring with a center
/// dot in black+alpha only (the color channel is ignored for templates, the
/// menu bar tints it).
fn make_icon() -> Icon {
    const S: usize = 32;
    let template = cfg!(target_os = "macos");
    let mut rgba = vec![0u8; S * S * 4];
    for y in 0..S {
        for x in 0..S {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            let r = (dx * dx + dy * dy).sqrt();
            let px = (y * S + x) * 4;
            if template {
                // Antialiased coverage: 1 inside an edge, fading over ~1px.
                let edge = |d: f32| (1.0 - d).clamp(0.0, 1.0);
                // Ring between radii 11.5 and 14.5, plus a dot of radius 5.5.
                let ring = edge(r - 14.5).min(edge(11.5 - r));
                let dot = edge(r - 5.5);
                let a = (ring.max(dot) * 255.0) as u8;
                rgba[px..px + 4].copy_from_slice(&[0, 0, 0, a]);
            } else {
                if r < 15.0 {
                    rgba[px..px + 4].copy_from_slice(&[26, 30, 46, 235]);
                }
                if r < 6.0 {
                    rgba[px..px + 4].copy_from_slice(&[110, 165, 255, 255]);
                }
            }
        }
    }
    Icon::from_rgba(rgba, S as u32, S as u32).expect("static icon dimensions are valid")
}

#[cfg(test)]
mod tests {
    use super::monitor_menu_plan;
    use crate::display::Monitor;

    fn mon(left: i32, primary: bool) -> Monitor {
        Monitor { left, top: 0, right: left + 1920, bottom: 1080, primary }
    }

    #[test]
    fn single_monitor_submenu_is_hidden() {
        let (labels, show) = monitor_menu_plan(&[mon(0, true)], 0);
        assert!(!show, "picking a monitor is pointless with one display");
        assert_eq!(labels, vec![("Monitor 1 (primary)".to_owned(), true)]);
    }

    #[test]
    fn multiple_monitors_label_and_mark_selection() {
        let mons = [mon(0, true), mon(1920, false), mon(3840, false)];
        let (labels, show) = monitor_menu_plan(&mons, 1);
        assert!(show, "submenu shown once there's more than one display");
        assert_eq!(
            labels,
            vec![
                ("Monitor 1 (primary)".to_owned(), false),
                ("Monitor 2".to_owned(), true),
                ("Monitor 3".to_owned(), false),
            ]
        );
    }

    #[test]
    fn stale_selection_checks_nothing() {
        // The saved monitor index can outlive the display that had it.
        let mons = [mon(0, true), mon(1920, false)];
        let (labels, _) = monitor_menu_plan(&mons, 5);
        assert!(labels.iter().all(|(_, checked)| !checked));
    }

    #[test]
    fn no_displays_is_hidden_and_empty() {
        let (labels, show) = monitor_menu_plan(&[], 0);
        assert!(!show);
        assert!(labels.is_empty());
    }
}
