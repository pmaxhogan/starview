//! Listens for motion from the ZSA Navigator trackball.
//!
//! The Navigator presents as a HID mouse interface on the Moonlander's USB
//! composite device (VID 0x3297). On Windows, Raw Input reports per-device
//! deltas, which lets us watch the trackball without interfering with it and
//! without reacting to the user's other mice; RIDEV_INPUTSINK delivers events
//! to a hidden message-only window regardless of which window has focus. On
//! macOS, the pointer HID interface is read directly via hidapi (non-seizing,
//! so the cursor keeps working) — that path needs the Input Monitoring
//! permission.

use std::time::{Duration, Instant};

#[cfg(windows)]
use std::collections::HashMap;

#[cfg(windows)]
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
#[cfg(windows)]
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(windows)]
use windows::Win32::UI::Input::{
    GetRawInputData, GetRawInputDeviceInfoW, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE,
    RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK, RIDI_DEVICENAME, RIM_TYPEMOUSE,
    RegisterRawInputDevices,
};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HWND_MESSAGE, MSG,
    RegisterClassW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_INPUT, WNDCLASSW,
};
#[cfg(windows)]
use windows::core::w;

/// Coalescing window: motion arrives at up to 1 kHz; the UI doesn't need
/// more than ~40 updates/s.
const SEND_INTERVAL: Duration = Duration::from_millis(25);

/// Spawns a background thread that calls `on_motion(dx, dy)` with coalesced
/// relative deltas whenever a ZSA pointing device moves.
pub fn spawn_listener(mut on_motion: impl FnMut(i32, i32) + Send + 'static) {
    std::thread::Builder::new()
        .name("trackball-listener".into())
        .spawn(move || {
            #[cfg(windows)]
            if let Err(err) = unsafe { run(&mut on_motion) } {
                eprintln!("trackball listener failed: {err}");
            }
            #[cfg(not(windows))]
            run_hidapi(&mut on_motion);
        })
        .expect("failed to spawn trackball listener thread");
}

#[cfg(windows)]
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

#[cfg(windows)]
unsafe fn run(on_motion: &mut impl FnMut(i32, i32)) -> windows::core::Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = w!("starview-rawinput");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            w!(""),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE), // message-only window: no UI, just a queue
            None,
            Some(instance.into()),
            None,
        )?;

        // Generic Desktop / Mouse, delivered even while unfocused.
        let rid = RAWINPUTDEVICE {
            usUsagePage: 0x01,
            usUsage: 0x02,
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        };
        RegisterRawInputDevices(&[rid], size_of::<RAWINPUTDEVICE>() as u32)?;

        let mut is_zsa_cache: HashMap<isize, bool> = HashMap::new();
        let mut acc = (0i32, 0i32);
        let mut last_send = Instant::now();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_INPUT
                && let Some((dx, dy)) = read_zsa_motion(msg.lParam, &mut is_zsa_cache)
            {
                acc.0 += dx;
                acc.1 += dy;
                if (acc != (0, 0)) && last_send.elapsed() >= SEND_INTERVAL {
                    on_motion(acc.0, acc.1);
                    acc = (0, 0);
                    last_send = Instant::now();
                }
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg); // DefWindowProc frees the raw input buffer
        }
    }
    Ok(())
}

/// Returns the relative (dx, dy) if this WM_INPUT is motion from a ZSA device.
#[cfg(windows)]
unsafe fn read_zsa_motion(
    lparam: LPARAM,
    is_zsa_cache: &mut HashMap<isize, bool>,
) -> Option<(i32, i32)> {
    unsafe {
        let hraw = HRAWINPUT(lparam.0 as *mut core::ffi::c_void);
        let header_size = size_of::<RAWINPUTHEADER>() as u32;
        let mut size = 0u32;
        GetRawInputData(hraw, RID_INPUT, None, &mut size, header_size);
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let read = GetRawInputData(
            hraw,
            RID_INPUT,
            Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
            &mut size,
            header_size,
        );
        if read == 0 || read == u32::MAX {
            return None;
        }
        let raw = &*(buf.as_ptr() as *const RAWINPUT);
        if raw.header.dwType != RIM_TYPEMOUSE.0 {
            return None;
        }
        let device = raw.header.hDevice;
        let is_zsa = *is_zsa_cache
            .entry(device.0 as isize)
            .or_insert_with(|| device_is_zsa(device));
        if !is_zsa {
            return None;
        }
        let mouse = raw.data.mouse;
        if mouse.usFlags.0 & 0x01 != 0 {
            return None; // MOUSE_MOVE_ABSOLUTE (tablets etc.) — not the ball
        }
        Some((mouse.lLastX, mouse.lLastY))
    }
}

#[cfg(windows)]
unsafe fn device_is_zsa(device: HANDLE) -> bool {
    unsafe {
        let mut len = 0u32;
        GetRawInputDeviceInfoW(Some(device), RIDI_DEVICENAME, None, &mut len);
        if len == 0 {
            return false;
        }
        let mut name = vec![0u16; len as usize];
        let written = GetRawInputDeviceInfoW(
            Some(device),
            RIDI_DEVICENAME,
            Some(name.as_mut_ptr() as *mut core::ffi::c_void),
            &mut len,
        );
        if written == u32::MAX {
            return false;
        }
        let name = String::from_utf16_lossy(&name[..written as usize]);
        name.to_ascii_uppercase().contains("VID_3297")
    }
}

/// hidapi path (macOS): open every ZSA pointer interface (usage page 0x01,
/// usage 0x02 — only ZSA devices, so other mice are never touched) and read
/// mouse reports directly, re-enumerating when devices come and go.
#[cfg(not(windows))]
fn run_hidapi(on_motion: &mut impl FnMut(i32, i32)) {
    use hidapi::HidApi;

    const ZSA_VID: u16 = 0x3297;
    let debug = std::env::var_os("STARVIEW_HID_DEBUG").is_some();

    let mut api = loop {
        match HidApi::new() {
            Ok(api) => break api,
            Err(err) => {
                eprintln!("hidapi init failed (trackball): {err}");
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    };
    // Never seize the pointer device from the OS: an exclusive open (the
    // macOS default without the `macos-shared-device` feature) makes the
    // trackball stop moving the system cursor entirely.
    #[cfg(target_os = "macos")]
    api.set_open_exclusive(false);
    let mut logged_open_failure = false;
    loop {
        let _ = api.refresh_devices();
        let devices: Vec<hidapi::HidDevice> = api
            .device_list()
            .filter(|d| d.vendor_id() == ZSA_VID && d.usage_page() == 0x01 && d.usage() == 0x02)
            .filter_map(|info| match info.open_device(&api) {
                Ok(dev) => {
                    let _ = dev.set_blocking_mode(false);
                    Some(dev)
                }
                Err(err) => {
                    let msg = err.to_string();
                    if !logged_open_failure {
                        logged_open_failure = true;
                        eprintln!("could not open ZSA pointer interface: {msg}");
                    }
                    crate::perms::handle_hid_open_failure(&msg);
                    None
                }
            })
            .collect();
        if devices.is_empty() {
            std::thread::sleep(Duration::from_secs(2));
            continue;
        }

        let mut acc = (0i32, 0i32);
        let mut last_send = Instant::now();
        let mut buf = [0u8; 64];
        'devices: loop {
            let mut idle = true;
            for dev in &devices {
                loop {
                    match dev.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            idle = false;
                            if debug {
                                eprintln!("trackball report: {:02x?}", &buf[..n]);
                            }
                            if let Some((dx, dy)) = parse_mouse_report(&buf[..n]) {
                                acc.0 += dx;
                                acc.1 += dy;
                            }
                        }
                        Err(_) => break 'devices, // device gone; re-enumerate
                    }
                }
            }
            if acc != (0, 0) && last_send.elapsed() >= SEND_INTERVAL {
                on_motion(acc.0, acc.1);
                acc = (0, 0);
                last_send = Instant::now();
            }
            if idle {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Decode the relative (dx, dy) from a QMK mouse report. Firmware builds
/// differ: reports may carry a leading report-ID byte (shared USB endpoint)
/// and coordinates may be i8 or i16 little-endian (MOUSE_EXTENDED_REPORT).
/// Sizes: 5 = [buttons, x, y, v, h], 6 = id + that,
/// 7 = [buttons, x lo, x hi, y lo, y hi, v, h], 8 = id + that,
/// 11 = [buttons, boot_x, boot_y, x i16, y i16, v i16, h i16] — the
/// Moonlander's actual layout (verified against its report descriptor: one
/// buttons byte, two boot-protocol i8 deltas declared as constant padding,
/// then 16-bit X/Y/wheel/pan).
#[cfg(not(windows))]
fn parse_mouse_report(buf: &[u8]) -> Option<(i32, i32)> {
    let (dx, dy) = match buf.len() {
        5 => (buf[1] as i8 as i32, buf[2] as i8 as i32),
        6 => (buf[2] as i8 as i32, buf[3] as i8 as i32),
        7 => (
            i16::from_le_bytes([buf[1], buf[2]]) as i32,
            i16::from_le_bytes([buf[3], buf[4]]) as i32,
        ),
        8 => (
            i16::from_le_bytes([buf[2], buf[3]]) as i32,
            i16::from_le_bytes([buf[4], buf[5]]) as i32,
        ),
        11 => (
            i16::from_le_bytes([buf[3], buf[4]]) as i32,
            i16::from_le_bytes([buf[5], buf[6]]) as i32,
        ),
        _ => return None,
    };
    if dx == 0 && dy == 0 {
        return None;
    }
    Some((dx, dy))
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::parse_mouse_report;

    #[test]
    fn compact_report_decodes_signed_deltas() {
        // [buttons, x, y, v, h] with x = -2, y = 3.
        assert_eq!(parse_mouse_report(&[0, 0xFE, 0x03, 0, 0]), Some((-2, 3)));
        // Same with a leading report id.
        assert_eq!(parse_mouse_report(&[2, 0, 0xFE, 0x03, 0, 0]), Some((-2, 3)));
    }

    #[test]
    fn extended_report_decodes_i16_deltas() {
        // [buttons, x lo, x hi, y lo, y hi, v, h] with x = -300, y = 300.
        let x = (-300i16).to_le_bytes();
        let y = 300i16.to_le_bytes();
        assert_eq!(
            parse_mouse_report(&[0, x[0], x[1], y[0], y[1], 0, 0]),
            Some((-300, 300))
        );
        assert_eq!(
            parse_mouse_report(&[2, 0, x[0], x[1], y[0], y[1], 0, 0]),
            Some((-300, 300))
        );
    }

    #[test]
    fn moonlander_report_decodes_i16_deltas_after_boot_bytes() {
        // Captured from a real Moonlander + Navigator on macOS: buttons,
        // boot_x, boot_y, x i16, y i16, v i16, h i16 — a dy = +2 nudge.
        assert_eq!(
            parse_mouse_report(&[0, 0, 2, 0, 0, 2, 0, 0, 0, 0, 0]),
            Some((0, 2))
        );
        let x = (-300i16).to_le_bytes();
        assert_eq!(
            parse_mouse_report(&[0, 0, 0, x[0], x[1], 9, 0, 0, 0, 0, 0]),
            Some((-300, 9))
        );
    }

    #[test]
    fn zero_motion_and_odd_sizes_are_ignored() {
        assert_eq!(parse_mouse_report(&[0, 0, 0, 0, 0]), None);
        assert_eq!(parse_mouse_report(&[0, 1]), None);
        assert_eq!(parse_mouse_report(&[0; 32]), None);
    }
}
