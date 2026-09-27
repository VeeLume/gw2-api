//! Fetch the live GW2 `/v2.json` manifest and compute coverage gaps.

use std::collections::{BTreeMap, HashSet};

use anyhow::Result;
use gw2_api::registry::EndpointEntry;
use serde::Deserialize;

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    schema_versions: Vec<SchemaVersion>,
    routes: Vec<RawRoute>,
}

#[derive(Deserialize)]
struct SchemaVersion {
    v: String,
    desc: String,
}

#[derive(Deserialize)]
struct RawRoute {
    path: String,
    #[serde(default)]
    active: bool,
    #[serde(default)]
    lang: bool,
    #[serde(default)]
    auth: bool,
}

/// One active route of the live API.
#[derive(Debug, Clone)]
pub struct Route {
    /// Without the `/v2/` prefix, e.g. `"items"` or `"characters/:id/backstory"`.
    pub path: String,
    /// Whether the route is localized (`?lang=` matters).
    pub lang: bool,
    pub auth: bool,
}

/// The parts of `/v2.json` the CLI uses.
pub struct LiveManifest {
    pub routes: Vec<Route>,
    /// Schema versions newer than `pinned`, as `(version, description)`.
    pub newer_schemas: Vec<(String, String)>,
}

impl LiveManifest {
    /// Routes keyed by path.
    pub fn by_path(&self) -> BTreeMap<&str, &Route> {
        self.routes.iter().map(|r| (r.path.as_str(), r)).collect()
    }
}

/// Fetch the live manifest. `pinned` is the schema version to compare newer ones against.
pub async fn fetch_manifest(pinned: &str) -> Result<LiveManifest> {
    let url = "https://api.guildwars2.com/v2.json?v=latest";
    let manifest: Manifest = reqwest::get(url).await?.error_for_status()?.json().await?;

    let routes = manifest
        .routes
        .into_iter()
        .filter(|r| r.active)
        .map(|r| Route {
            path: r.path.trim_start_matches('/').trim_start_matches("v2/").to_string(),
            lang: r.lang,
            auth: r.auth,
        })
        .collect();
    // ISO 8601 strings of the same shape compare correctly as strings.
    let newer_schemas = manifest
        .schema_versions
        .into_iter()
        .filter(|s| s.v.as_str() > pinned)
        .map(|s| (s.v, s.desc))
        .collect();
    Ok(LiveManifest { routes, newer_schemas })
}

/// Coverage of the live manifest by the registry.
pub struct CoverageDiff {
    /// Paths in the live manifest but not in the implemented registry.
    pub missing: Vec<Route>,
    /// Paths implemented but not found in the live manifest (stale or extra).
    pub extra: Vec<String>,
    /// Paths in both.
    pub covered: Vec<String>,
    /// Implemented paths whose `auth` flag disagrees with the manifest:
    /// `(path, registry_auth, manifest_auth)`.
    pub auth_mismatch: Vec<(String, bool, bool)>,
}

/// A manifest path in registry form: `"items/:id"` → `"items"`.
fn registry_path(manifest_path: &str) -> &str {
    manifest_path.strip_suffix("/:id").unwrap_or(manifest_path)
}

/// The manifest lists some routes only by their parent: `commerce/transactions`
/// stands for `commerce/transactions/current/buys` and its siblings. A registry
/// path therefore matches a manifest path equal to it or to one of its ancestors.
fn matches(registry: &str, manifest: &str) -> bool {
    registry == manifest
        || registry
            .strip_prefix(manifest)
            .is_some_and(|rest| rest.starts_with('/'))
}

pub fn compute_diff(manifest: &LiveManifest, implemented: &[&EndpointEntry]) -> CoverageDiff {
    let impl_paths: HashSet<&str> = implemented.iter().map(|e| e.path).collect();
    let manifest_paths: HashSet<&str> = manifest.routes.iter().map(|r| registry_path(&r.path)).collect();

    let mut missing: Vec<Route> = manifest
        .routes
        .iter()
        .filter(|r| !impl_paths.iter().any(|p| matches(p, registry_path(&r.path))))
        .cloned()
        .collect();
    missing.sort_by(|a, b| a.path.cmp(&b.path));

    let (mut covered, mut extra): (Vec<String>, Vec<String>) = impl_paths
        .iter()
        .map(|p| p.to_string())
        .partition(|p| manifest_paths.iter().any(|m| matches(p, m)));
    covered.sort();
    extra.sort();

    let by_path = manifest.by_path();
    let mut auth_mismatch: Vec<(String, bool, bool)> = implemented
        .iter()
        .filter_map(|e| {
            let route = by_path.get(e.path)?;
            (route.auth != e.auth).then(|| (e.path.to_string(), e.auth, route.auth))
        })
        .collect();
    auth_mismatch.sort();
    auth_mismatch.dedup();

    CoverageDiff { missing, extra, covered, auth_mismatch }
}
