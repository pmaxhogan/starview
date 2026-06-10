//! Fetches layer metadata for an Oryx layout from ZSA's public GraphQL endpoint.
//!
//! The endpoint is unofficial and has changed shape before, so only the fields
//! we actually need are selected, and the last good response is cached on disk
//! so the overlay still has layer names when offline or if the API drifts.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

const ENDPOINT: &str = "https://oryx.zsa.io/graphql";

const META_QUERY: &str = r#"
query Meta($hashId: String!, $revisionId: String!, $geometry: String) {
  layout(hashId: $hashId, revisionId: $revisionId, geometry: $geometry) {
    title
    revision { hashId layers { title position keys } }
  }
}
"#;

/// One action slot on a key (tap, hold, ...). Oryx stores keys as loose JSON
/// that has drifted over the years, so every field is optional.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KeyAction {
    pub code: Option<String>,
    /// Target layer for layer-switch codes ("MO", "TG", "TO", "TT", ...).
    pub layer: Option<i64>,
    /// Wrapping modifiers for keys like LGUI(KC_1).
    pub modifiers: Option<Modifiers>,
    /// One-shot modifier target (OSM keys) — singular, unlike `modifiers`.
    pub modifier: Option<String>,
    /// Non-null for user macros (shape varies; presence is what matters).
    #[serde(rename = "macro")]
    pub macro_: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Modifiers {
    pub left_ctrl: bool,
    pub left_shift: bool,
    pub left_alt: bool,
    pub left_gui: bool,
    pub right_ctrl: bool,
    pub right_shift: bool,
    pub right_alt: bool,
    pub right_gui: bool,
}

impl Modifiers {
    /// Single-letter prefix in QMK order, e.g. "G+" for LGUI(x), "CS+" for
    /// ctrl+shift. Empty when no flag is set.
    pub fn prefix(&self) -> String {
        let mut s = String::new();
        if self.left_ctrl || self.right_ctrl {
            s.push('C');
        }
        if self.left_shift || self.right_shift {
            s.push('S');
        }
        if self.left_alt || self.right_alt {
            s.push('A');
        }
        if self.left_gui || self.right_gui {
            s.push('G');
        }
        if !s.is_empty() {
            s.push('+');
        }
        s
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Key {
    pub tap: Option<KeyAction>,
    pub hold: Option<KeyAction>,
    pub custom_label: Option<String>,
    pub glow_color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub title: String,
    pub position: usize,
    /// Indexed by the board's Oryx key order (see `geometry`).
    #[serde(default, deserialize_with = "lenient_keys")]
    pub keys: Vec<Key>,
}

/// Oryx keys are heterogeneous stored JSON; a single odd element shouldn't
/// cost us the whole layout, so unparseable keys become empty ones.
fn lenient_keys<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Key>, D::Error> {
    let raw: Vec<serde_json::Value> = Vec::deserialize(d)?;
    Ok(raw
        .into_iter()
        .map(|v| serde_json::from_value(v).unwrap_or_default())
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutInfo {
    pub title: String,
    pub revision: String,
    pub layers: Vec<Layer>,
}

impl LayoutInfo {
    pub fn layer_name(&self, index: usize) -> Option<&str> {
        self.layers
            .iter()
            .find(|l| l.position == index)
            .map(|l| l.title.as_str())
    }
}

#[derive(Deserialize)]
struct GqlResponse {
    data: Option<GqlData>,
    #[serde(default)]
    errors: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct GqlData {
    layout: Option<GqlLayout>,
}

#[derive(Deserialize)]
struct GqlLayout {
    title: String,
    revision: GqlRevision,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlRevision {
    hash_id: String,
    layers: Vec<Layer>,
}

fn cache_path(hash_id: &str) -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    // v2 suffix: invalidates pre-keys caches.
    Some(
        PathBuf::from(base)
            .join("starview")
            .join(format!("layout-{hash_id}-v2.json")),
    )
}

fn fetch_remote(hash_id: &str, geometry: &str) -> Result<LayoutInfo> {
    let body = serde_json::json!({
        "query": META_QUERY,
        "variables": { "hashId": hash_id, "revisionId": "latest", "geometry": geometry },
    });
    let resp: GqlResponse = reqwest::blocking::Client::builder()
        .user_agent(concat!("starview/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(10))
        .build()?
        .post(ENDPOINT)
        .json(&body)
        .send()
        .context("request to oryx.zsa.io failed")?
        .error_for_status()?
        .json()
        .context("malformed GraphQL response")?;

    if !resp.errors.is_empty() {
        bail!("GraphQL errors: {:?}", resp.errors);
    }
    let layout = resp
        .data
        .and_then(|d| d.layout)
        .with_context(|| format!("layout '{hash_id}' not found (is it public?)"))?;

    Ok(LayoutInfo {
        title: layout.title,
        revision: layout.revision.hash_id,
        layers: layout.revision.layers,
    })
}

/// Fetch layer names and key data for a layout, falling back to the on-disk
/// cache when the network or the (unofficial) API is unavailable.
pub fn load_layout(hash_id: &str, geometry: &str) -> Result<LayoutInfo> {
    let cache = cache_path(hash_id);
    match fetch_remote(hash_id, geometry) {
        Ok(info) => {
            if let Some(path) = &cache {
                let _ = fs::create_dir_all(path.parent().unwrap());
                let _ = fs::write(path, serde_json::to_vec_pretty(&info)?);
            }
            Ok(info)
        }
        Err(err) => {
            if let Some(path) = &cache
                && let Ok(bytes) = fs::read(path)
                && let Ok(info) = serde_json::from_slice::<LayoutInfo>(&bytes)
            {
                eprintln!("oryx fetch failed ({err:#}); using cached layout");
                return Ok(info);
            }
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    /// Hits the live Oryx API; needs network.
    #[test]
    fn fetches_layers_and_keys() {
        let info = super::fetch_remote("jmvGw", "moonlander").unwrap();
        assert!(info.layers.len() >= 5);
        assert_eq!(info.layer_name(1), Some("nav."));
        for layer in &info.layers {
            assert_eq!(layer.keys.len(), 72, "layer '{}' key count", layer.title);
        }
        let main = &info.layers[0];
        assert!(
            main.keys
                .iter()
                .any(|k| k.tap.as_ref().is_some_and(|t| t.code.as_deref() == Some("KC_W"))),
            "expected a KC_W somewhere on the base layer"
        );
    }
}
