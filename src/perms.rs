//! macOS Input Monitoring handling. Opening a keyboard/pointer HID interface
//! on macOS requires the Input Monitoring permission (TCC); without it,
//! IOHIDDeviceOpen fails with a "not permitted" flavor of error. When that
//! happens, log clear instructions and open the right System Settings pane —
//! once per run, since flipping the toggle is a one-time user action.
//! No-ops on other platforms.

#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
static PANE_OPENED: AtomicBool = AtomicBool::new(false);

/// Call with the error string from a failed HID open.
pub fn handle_hid_open_failure(err: &str) {
    #[cfg(target_os = "macos")]
    {
        let lower = err.to_ascii_lowercase();
        let permission = lower.contains("permission")
            || lower.contains("not permitted")
            || lower.contains("privilege")
            || lower.contains("tcc");
        if permission && !PANE_OPENED.swap(true, Ordering::SeqCst) {
            eprintln!(
                "macOS blocked HID access. Enable starview (or the terminal it runs from) \
                 under System Settings > Privacy & Security > Input Monitoring, then relaunch. \
                 Opening that pane now…"
            );
            let _ = std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
                .spawn();
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = err;
}
