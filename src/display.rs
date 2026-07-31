//! Enumerates monitors so the overlay can dock to a chosen display.
//!
//! Rects are in physical desktop pixels (each monitor's `rcMonitor`); the
//! overlay converts to logical points when positioning. The list is sorted
//! primary-first so index 0 is a stable, sensible default.

#[derive(Clone, Copy, Debug)]
pub struct Monitor {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub primary: bool,
}

#[cfg(windows)]
pub fn monitors() -> Vec<Monitor> {
    use windows::Win32::Foundation::{LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
    };
    use windows::core::BOOL;

    // MONITORINFO.dwFlags bit for the primary display.
    const MONITORINFOF_PRIMARY: u32 = 1;

    unsafe extern "system" fn cb(mon: HMONITOR, _hdc: HDC, _rc: *mut RECT, data: LPARAM) -> BOOL {
        let out = unsafe { &mut *(data.0 as *mut Vec<Monitor>) };
        let mut info = MONITORINFO {
            cbSize: core::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(mon, &mut info) }.as_bool() {
            let r = info.rcMonitor;
            out.push(Monitor {
                left: r.left,
                top: r.top,
                right: r.right,
                bottom: r.bottom,
                primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
            });
        }
        BOOL(1)
    }

    let mut out: Vec<Monitor> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(cb),
            LPARAM(&mut out as *mut Vec<Monitor> as isize),
        );
    }
    // Primary first, then by screen position, for a stable index across runs.
    out.sort_by_key(|m| (!m.primary, m.left, m.top));
    out
}

#[cfg(target_os = "macos")]
pub fn monitors() -> Vec<Monitor> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;

    // NSScreen is main-thread-only; on macOS monitors() is only called from
    // the main thread (overlay update + tray poll), but guard anyway.
    let Some(mtm) = MainThreadMarker::new() else {
        return Vec::new();
    };
    let screens = NSScreen::screens(mtm);
    // Cocoa's global space is bottom-left-origin with y up; the rest of
    // starview (and winit) use top-left-origin physical pixels. The primary
    // screen has origin (0,0), so its frame height anchors the flip; each
    // screen's rect scales by its own backing factor, matching winit's
    // logical->physical convention.
    let primary_h = screens
        .iter()
        .map(|s| s.frame())
        .find(|f| f.origin.x == 0.0 && f.origin.y == 0.0)
        .map(|f| f.size.height)
        .unwrap_or(0.0);
    let mut out: Vec<Monitor> = screens
        .iter()
        .map(|s| {
            let f = s.frame();
            let scale = s.backingScaleFactor();
            let top = primary_h - (f.origin.y + f.size.height);
            Monitor {
                left: (f.origin.x * scale) as i32,
                top: (top * scale) as i32,
                right: ((f.origin.x + f.size.width) * scale) as i32,
                bottom: ((top + f.size.height) * scale) as i32,
                primary: f.origin.x == 0.0 && f.origin.y == 0.0,
            }
        })
        .collect();
    out.sort_by_key(|m| (!m.primary, m.left, m.top));
    out
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn monitors() -> Vec<Monitor> {
    Vec::new()
}

/// Today's local date as "YYYY-MM-DD", for the per-day stats buckets.
#[cfg(windows)]
pub fn today() -> String {
    let st = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:04}-{:02}-{:02}", st.wYear, st.wMonth, st.wDay)
}

#[cfg(target_os = "macos")]
pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn today() -> String {
    "1970-01-01".to_owned()
}
