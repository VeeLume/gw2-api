//! Global endpoint registry populated at link time by `#[gw2_endpoint]` macros.
//!
//! Each macro expansion contributes one [`EndpointEntry`] to [`ENDPOINTS`] via
//! `linkme::distributed_slice`. The CLI iterates this slice to:
//!   1. Show which endpoints are implemented vs the live GW2 API manifest.
//!   2. Run a single representative call per endpoint (`single`) through the real client.
//!   3. Fetch and deserialize all data for an endpoint (`full`) using the same code path as the lib.
//!   4. Parse every raw item against the model and report dropped fields (`check`).

use std::future::Future;
use std::pin::Pin;

use serde::de::DeserializeOwned;

use crate::client::Gw2Client;
use crate::client::auth::{Authenticated, Unauthenticated};

/// Result of a full endpoint fetch.
pub struct FullFetchResult {
    /// Number of records successfully deserialized.
    pub ok_count: usize,
    /// Errors encountered, as `(id_or_index, error_message)`.
    pub errors: Vec<(String, String)>,
}

/// `single` test: one representative call. Receives the public client and, if a key
/// was given, the authenticated one.
pub type SingleFn = fn(
    Gw2Client<Unauthenticated>,
    Option<Gw2Client<Authenticated>>,
) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

/// `full` fetch: all data through the real client.
pub type FullFn = fn(
    Gw2Client<Unauthenticated>,
    Option<Gw2Client<Authenticated>>,
) -> Pin<Box<dyn Future<Output = FullFetchResult> + Send>>;

/// How the raw data of an endpoint is laid out, for the `check` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// `GET /path` lists IDs; items come from `?ids=` in chunks.
    Bulk,
    /// `GET /path` returns one object.
    One,
    /// `GET /path` returns an array of items (`null` entries are skipped).
    Array,
}

/// What the `check` command needs to validate an endpoint against raw API data.
pub struct CheckSpec {
    pub shape: Shape,
    /// Parse one raw item into the model; `Ok` holds the fields the model ignored.
    pub parse: fn(serde_json::Value) -> Result<Vec<String>, String>,
}

/// Deserialize one raw item as `T`, collecting the fields `T` silently drops.
///
/// Paths are generalized so drift is counted per field, not per array position:
/// `dye_slots.3.material` → `dye_slots[].material`.
pub fn parse_value<T: DeserializeOwned>(v: serde_json::Value) -> Result<Vec<String>, String> {
    let mut ignored = Vec::new();
    serde_ignored::deserialize::<_, _, T>(v, |p| ignored.push(generalize(&p.to_string())))
        .map(|_| ignored)
        .map_err(|e| e.to_string())
}

fn generalize(path: &str) -> String {
    let mut out = String::new();
    for seg in path.split('.') {
        if seg.chars().all(|c| c.is_ascii_digit()) {
            out.push_str("[]");
        } else {
            if !out.is_empty() {
                out.push('.');
            }
            out.push_str(seg);
        }
    }
    out
}

/// Metadata for one registered GW2 API endpoint.
pub struct EndpointEntry {
    /// API path without leading slash, e.g. `"items"` or `"account/wallet"`.
    pub path: &'static str,
    /// Whether this endpoint requires an API key.
    pub auth: bool,
    /// Human-readable Rust return type, e.g. `"Item"`, `"Vec<WalletEntry>"`, `"ExchangeRate"`.
    pub type_name: &'static str,
    /// Human-readable call signature, e.g. `"get(id)"`, `"get()"`, `"coins(quantity)"`.
    pub call: &'static str,
    /// How to validate raw data for this endpoint; `None` for namespaces and
    /// parameterised functions.
    pub check: Option<CheckSpec>,
    /// Single test: one representative call through the real [`Gw2Client`].
    ///
    /// Uses the same code path as library consumers — rate limiter, retry,
    /// schema version, lang, user-agent, bearer auth, and `Patchable::patch()`.
    ///
    /// Callers skip auth-required endpoints when they have no key.
    pub single: SingleFn,
    /// Full fetch: retrieve all data through the real client and report counts.
    ///
    /// For bulk resources, uses `client.pages()` which bulk-deserializes each
    /// page via `response.json::<Vec<T>>()` — identical to the library's own path.
    ///
    /// Callers skip auth-required endpoints when they have no key.
    pub full: FullFn,
}

/// All registered endpoints, populated at link time by the resource macros.
#[linkme::distributed_slice]
pub static ENDPOINTS: [EndpointEntry];

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    #[allow(dead_code)]
    struct Slot {
        color_id: u32,
    }

    #[derive(serde::Deserialize)]
    #[allow(dead_code)]
    struct Skin {
        id: u32,
        dye_slots: Vec<Slot>,
    }

    #[test]
    fn parse_value_reports_generalized_ignored_fields() {
        let v = serde_json::json!({
            "id": 1, "extra": true,
            "dye_slots": [{ "color_id": 1, "material": "cloth" }, { "color_id": 2, "material": "metal" }]
        });
        let mut ignored = parse_value::<Skin>(v).unwrap();
        ignored.sort();
        assert_eq!(ignored, ["dye_slots[].material", "dye_slots[].material", "extra"]);
        assert!(parse_value::<Skin>(serde_json::json!({ "id": "x" })).is_err());
    }
}
