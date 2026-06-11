//! Detects whether the primary monitor (where the overlay lives) is in HDR
//! mode. In HDR, SDR content is pinned to the reference-white level while
//! game content goes far brighter, so the overlay's translucent dark theme
//! washes out — the renderer switches to a high-contrast palette instead.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Dxgi::Common::DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput6};
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint};
use windows::core::Interface;

static HDR_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn active() -> bool {
    HDR_ACTIVE.load(Ordering::Relaxed)
}

/// Polls HDR state every few seconds (it flips when games toggle HDR or the
/// user changes display settings).
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
