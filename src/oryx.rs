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
    revision { hashId layers { title position } }
  }
}
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub title: String,
    pub position: usize,
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
    Some(
        PathBuf::from(base)
            .join("starview")
            .join(format!("layout-{hash_id}.json")),
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

/// Fetch layer names for a layout, falling back to the on-disk cache when the
/// network or the (unofficial) API is unavailable.
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
