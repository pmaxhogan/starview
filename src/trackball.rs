//! Listens for motion from the ZSA Navigator trackball via Raw Input.
//!
//! The Navigator presents as a HID mouse interface on the Moonlander's USB
//! composite device (VID 0x3297). Raw Input reports per-device deltas, which
//! lets us watch the trackball without interfering with it and without
//! reacting to the user's other mice. RIDEV_INPUTSINK delivers events to a
//! hidden message-only window regardless of which window has focus.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::{
    GetRawInputData, GetRawInputDeviceInfoW, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE,
    RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK, RIDI_DEVICENAME, RIM_TYPEMOUSE,
    RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HWND_MESSAGE, MSG,
    RegisterClassW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_INPUT, WNDCLASSW,
};
use windows::core::w;

/// Coalescing window: motion arrives at up to 1 kHz; the UI doesn't need
/// more than ~40 updates/s.
const SEND_INTERVAL: Duration = Duration::from_millis(25);

/// Spawns a background thread that calls `on_motion(dx, dy)` with coalesced
/// relative deltas whenever a ZSA pointing device moves.
pub fn spawn_listener(mut on_motion: impl FnMut(i32, i32) + Send + 'static) {
    std::thread::Builder::new()
        .name("trackball-rawinput".into())
        .spawn(move || {
            if let Err(err) = unsafe { run(&mut on_motion) } {
                eprintln!("trackball listener failed: {err}");
            }
        })
        .expect("failed to spawn trackball listener thread");
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

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
