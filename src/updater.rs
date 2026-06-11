//! Auto-update from GitHub releases.
//!
//! A background thread checks `releases/latest` shortly after startup and
//! daily after that. When a newer version exists, its installer is downloaded
//! to a temp file and the tray menu's update item lights up; installing runs
//! the Inno installer silently (it stops the app, swaps the exe, relaunches).

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result};

const REPO: &str = "pmaxhogan/starview";
const CURRENT: &str = env!("CARGO_PKG_VERSION");
const INSTALLER_ASSET: &str = "starview-setup.exe";

/// Downloaded-and-ready update, waiting for the user to click install.
static READY: Mutex<Option<(String, PathBuf)>> = Mutex::new(None);

/// Spawns the periodic checker; `on_ready(version)` fires (from a background
/// thread) once an update has been downloaded and is ready to install.
pub fn spawn_checker(on_ready: impl Fn(&str) + Send + 'static) {
    std::thread::Builder::new()
        .name("update-checker".into())
        .spawn(move || {
            // Don't compete with startup (layout fetch, HID pairing).
            std::thread::sleep(Duration::from_secs(15));
            loop {
                match check_and_download() {
                    Ok(Some(version)) => {
                        on_ready(&version);
                        return; // one pending update at a time; install restarts us
                    }
                    Ok(None) => {}
                    Err(err) => eprintln!("update check failed: {err:#}"),
                }
                std::thread::sleep(Duration::from_secs(24 * 60 * 60));
            }
        })
        .expect("failed to spawn update checker thread");
}

/// Launches the downloaded installer (silent, relaunches the app) and returns
/// true if it started; the caller should quit the app right after.
pub fn install_ready_update() -> bool {
    let Some((_, path)) = READY.lock().unwrap().clone() else {
        return false;
    };
    match std::process::Command::new(&path)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/RELAUNCH=1"])
        .spawn()
    {
        Ok(_) => true,
        Err(err) => {
            eprintln!("failed to launch installer {}: {err}", path.display());
            false
        }
    }
}

fn check_and_download() -> Result<Option<String>> {
    let current = std::env::var("STARVIEW_FAKE_VERSION").unwrap_or_else(|_| CURRENT.to_owned());
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("starview/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(60))
        .build()?;

    let release: serde_json::Value = client
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()?
        .error_for_status()
        .context("releases/latest request failed")?
        .json()?;

    let tag = release["tag_name"].as_str().context("release has no tag")?;
    let latest = tag.trim_start_matches('v');
    if !is_newer(latest, &current) {
        return Ok(None);
    }
    let url = release["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| a["name"].as_str() == Some(INSTALLER_ASSET))
        .and_then(|a| a["browser_download_url"].as_str())
        .with_context(|| format!("release {tag} has no {INSTALLER_ASSET} asset"))?;

    let path = std::env::temp_dir().join(format!("starview-setup-{latest}.exe"));
    let bytes = client
        .get(url)
        .send()?
        .error_for_status()
        .context("installer download failed")?
        .bytes()?;
    anyhow::ensure!(bytes.len() > 100_000, "installer download suspiciously small");
    std::fs::write(&path, &bytes).context("failed writing installer to temp")?;

    eprintln!("update v{latest} downloaded to {}", path.display());
    *READY.lock().unwrap() = Some((latest.to_owned(), path));
    Ok(Some(latest.to_owned()))
}

/// Numeric semver-ish comparison: is `a` newer than `b`?
fn is_newer(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.')
            .map(|p| {
                p.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0)
            })
            .collect()
    };
    parse(a) > parse(b)
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.2.1", "0.2.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
    }
}
