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

#[cfg(not(windows))]
pub fn monitors() -> Vec<Monitor> {
    Vec::new()
}
