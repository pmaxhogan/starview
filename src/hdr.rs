//! Detects whether the primary monitor (where the overlay lives) is in HDR
//! mode. In HDR, SDR content is pinned to the reference-white level while
//! game content goes far brighter, so the overlay's translucent dark theme
//! washes out — the renderer switches to a high-contrast palette instead.

//! Windows-only: macOS pins SDR content to reference white itself, so the
//! overlay's normal palette reads fine there and `active()` is always false.

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
use windows::Win32::Foundation::POINT;
#[cfg(windows)]
use windows::Win32::Graphics::Dxgi::Common::DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020;
#[cfg(windows)]
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput6};
#[cfg(windows)]
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint};
#[cfg(windows)]
use windows::core::Interface;

#[cfg(windows)]
static HDR_ACTIVE: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
pub fn active() -> bool {
    HDR_ACTIVE.load(Ordering::Relaxed)
}

#[cfg(not(windows))]
pub fn active() -> bool {
    false
}

/// Polls HDR state every few seconds (it flips when games toggle HDR or the
/// user changes display settings).
#[cfg(windows)]
pub fn spawn_monitor() {
    std::thread::Builder::new()
        .name("hdr-monitor".into())
        .spawn(|| {
            loop {
                HDR_ACTIVE.store(detect().unwrap_or(false), Ordering::Relaxed);
                std::thread::sleep(Duration::from_secs(5));
            }
        })
        .expect("failed to spawn HDR monitor thread");
}

#[cfg(windows)]
fn detect() -> windows::core::Result<bool> {
    unsafe {
        let primary = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut adapter_idx = 0;
        while let Ok(adapter) = factory.EnumAdapters1(adapter_idx) {
            adapter_idx += 1;
            let mut output_idx = 0;
            while let Ok(output) = adapter.EnumOutputs(output_idx) {
                output_idx += 1;
                let Ok(output6) = output.cast::<IDXGIOutput6>() else {
                    continue;
                };
                let Ok(desc) = output6.GetDesc1() else { continue };
                if desc.Monitor == primary {
                    return Ok(desc.ColorSpace == DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020);
                }
            }
        }
        Ok(false)
    }
}
