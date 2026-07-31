//! Watches for layer-change events from a ZSA keyboard over raw HID.
//!
//! This speaks the "Oryx protocol" built into stock ZSA firmware — the same
//! channel Keymapp's live training uses. Sending PAIRING_INIT makes the
//! firmware push a 32-byte event packet on every layer change (and on pairing
//! itself, which gives us the initial state). Windows fans input reports out
//! to every open handle, so this coexists fine with Keymapp.

use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};

const ZSA_VID: u16 = 0x3297;
/// QMK raw HID top-level collection (fixed 32-byte reports, no report IDs).
const RAW_USAGE_PAGE: u16 = 0xFF60;
const RAW_USAGE: u16 = 0x61;
const REPORT_SIZE: usize = 32;

const CMD_PAIRING_INIT: u8 = 0x01;
const EVT_PAIRING_SUCCESS: u8 = 0x04;
const EVT_LAYER: u8 = 0x05;
const EVT_KEYDOWN: u8 = 0x06;
const EVT_KEYUP: u8 = 0x07;
const EVT_ERROR: u8 = 0xFF;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidEvent {
    /// Active layer changed; 0 is the base layer.
    Layer(u8),
    /// Physical key pressed, as a QMK matrix position.
    KeyDown { row: u8, col: u8 },
    /// Physical key released.
    KeyUp { row: u8, col: u8 },
    /// No ZSA keyboard found / it was unplugged.
    Disconnected,
}

/// Spawns a background thread that emits [`HidEvent`]s for as long as the
/// process lives, reconnecting and re-pairing whenever the keyboard goes away.
pub fn spawn_watcher(mut on_event: impl FnMut(HidEvent) + Send + 'static) {
    std::thread::Builder::new()
        .name("hid-layer-watcher".into())
        .spawn(move || pump(&mut on_event))
        .expect("failed to spawn HID watcher thread");
}

fn pump(on_event: &mut impl FnMut(HidEvent)) {
    let mut api = loop {
        match HidApi::new() {
            Ok(api) => break api,
            Err(err) => {
                eprintln!("hidapi init failed: {err}");
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    };
    // Belt-and-suspenders with the `macos-shared-device` feature: never seize
    // the device from the OS (exclusive opens freeze the trackball cursor).
    #[cfg(target_os = "macos")]
    api.set_open_exclusive(false);
    loop {
        let _ = api.refresh_devices();
        match open_keyboard(&api) {
            Some(dev) => {
                listen(&dev, on_event);
                on_event(HidEvent::Disconnected);
                std::thread::sleep(Duration::from_millis(500));
            }
            None => {
                on_event(HidEvent::Disconnected);
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    }
}

fn open_keyboard(api: &HidApi) -> Option<HidDevice> {
    let info = api.device_list().find(|d| {
        d.vendor_id() == ZSA_VID && d.usage_page() == RAW_USAGE_PAGE && d.usage() == RAW_USAGE
    })?;
    match info.open_device(api) {
        Ok(dev) => Some(dev),
        Err(err) => {
            log_open_failure_once(&err.to_string());
            None
        }
    }
}

/// The reconnect loop retries every couple of seconds; log the first failure
/// (and run the macOS permission flow) instead of spamming stderr forever.
fn log_open_failure_once(msg: &str) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static LOGGED: AtomicBool = AtomicBool::new(false);
    if !LOGGED.swap(true, Ordering::SeqCst) {
        eprintln!("could not open the ZSA keyboard's raw HID interface: {msg}");
    }
    crate::perms::handle_hid_open_failure(msg);
}

fn pair(dev: &HidDevice) -> bool {
    // Writes are prefixed with a report-ID byte (0x00 — the device uses none).
    let mut pkt = [0u8; REPORT_SIZE + 1];
    pkt[1] = CMD_PAIRING_INIT;
    dev.write(&pkt).is_ok()
}

/// Reads events until the device errors out (unplug, reset, suspend).
fn listen(dev: &HidDevice, on_event: &mut impl FnMut(HidEvent)) {
    let debug = std::env::var_os("STARVIEW_HID_DEBUG").is_some();
    if !pair(dev) {
        return;
    }
    let mut last_pair = Instant::now();
    let mut buf = [0u8; REPORT_SIZE];
    loop {
        match dev.read_timeout(&mut buf, 250) {
            Ok(0) => {
                // Idle. The firmware's paired flag lives in keyboard RAM and is
                // lost on reset or host suspend; re-pairing is idempotent and
                // re-emits the current layer, doubling as a state resync.
                if last_pair.elapsed() > Duration::from_secs(30) {
                    if !pair(dev) {
                        return;
                    }
                    last_pair = Instant::now();
                }
            }
            Ok(_) => match buf[0] {
                EVT_LAYER => {
                    if debug {
                        eprintln!("hid: layer {}", buf[1]);
                    }
                    on_event(HidEvent::Layer(buf[1]));
                }
                // Firmware sends [evt, col, row, ...] for key events.
                EVT_KEYDOWN => on_event(HidEvent::KeyDown { row: buf[2], col: buf[1] }),
                EVT_KEYUP => on_event(HidEvent::KeyUp { row: buf[2], col: buf[1] }),
                // A layer event follows immediately.
                EVT_PAIRING_SUCCESS => eprintln!("hid: paired with ZSA keyboard"),
                EVT_ERROR => eprintln!("oryx error frame: {:02x?}", &buf[..4]),
                _ => {} // RGB/status-LED and other events — ignore
            },
            Err(_) => return, // device gone; caller re-enumerates and re-pairs
        }
    }
}
