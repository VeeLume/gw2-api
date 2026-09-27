//! Fetch the live GW2 `/v2.json` manifest and compute coverage gaps.

use anyhow::Result;
use serde::Deserialize;

#[derive(Deserialize)]
struct Manifest {
    routes: Vec<Route>,
}

#[derive(Deserialize)]
struct Route {
    path: String,
    #[serde(default)]
    active: bool,
}

/// Fetches the live GW2 API manifest and returns the list of active path segments.
///
/// Each entry in the returned `Vec` is a path like `"items"` or `"account/wallet"`.
pub async fn fetch_manifest() -> Result<Vec<String>> {
    let url = "https://api.guildwars2.com/v2.json?v=latest";
    let resp = reqwest::get(url).await?.error_for_status()?;
    let manifest: Manifest = resp.json().await?;

    // Keep only active routes and strip the leading "/v2/" prefix.
    Ok(manifest
        .routes
        .into_iter()
        .filter(|r| r.active)
        .map(|r| {
            r.path
                .trim_start_matches('/')
                .trim_start_matches("v2/")
                .to_string()
        })
        .collect())
}

/// Computes which manifest paths are not in the implemented set, and vice versa.
pub struct CoverageDiff {
    /// Paths in the live manifest but not in the implemented registry.
    pub missing: Vec<String>,
    /// Paths implemented but not found in the live manifest (stale or extra).
    pub extra: Vec<String>,
    /// Paths in both.
    pub covered: Vec<String>,
}

pub fn compute_diff(manifest: &[String], implemented: &[&str]) -> CoverageDiff {
    let impl_set: std::collections::HashSet<&str> =
        implemented.iter().copied().collect();
    let manifest_set: std::collections::HashSet<&str> =
        manifest.iter().map(|s| s.as_str()).collect();

    let missing = manifest
        .iter()
        .filter(|p| !impl_set.contains(p.as_str()))
        .cloned()
        .collect();

    let mut extra: Vec<String> = impl_set
        .iter()
        .filter(|&&p| !manifest_set.contains(p))
        .map(|&p| p.to_string())
        .collect();
    extra.sort();

    let mut covered: Vec<String> = impl_set
        .iter()
        .filter(|&&p| manifest_set.contains(p))
        .map(|&p| p.to_string())
        .collect();
    covered.sort();

    CoverageDiff { missing, extra, covered }
}
