//! Global endpoint registry populated at link time by `#[gw2_endpoint]` macros.
//!
//! Each macro expansion contributes one [`EndpointEntry`] to [`ENDPOINTS`] via
//! `linkme::distributed_slice`. The CLI iterates this slice to:
//!   1. Show which endpoints are implemented vs the live GW2 API manifest.
//!   2. Run a single representative call per endpoint (`single`) through the real client.
//!   3. Fetch and deserialize all data for an endpoint (`full`) using the same code path as the lib.

use std::future::Future;
use std::pin::Pin;

use crate::client::Gw2Client;
use crate::client::auth::{Authenticated, Unauthenticated};

/// Result of a full endpoint fetch.
pub struct FullFetchResult {
    /// Number of records successfully deserialized.
    pub ok_count: usize,
    /// Errors encountered, as `(id_or_index, error_message)`.
    pub errors: Vec<(String, String)>,
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
    /// Single test: one representative call through the real [`Gw2Client`].
    ///
    /// Uses the same code path as library consumers — rate limiter, retry,
    /// schema version, lang, user-agent, bearer auth, and `Patchable::patch()`.
    ///
    /// Auth-required endpoints return `Ok(())` immediately when `auth` is `None`.
    pub single: fn(
        Gw2Client<Unauthenticated>,
        Option<Gw2Client<Authenticated>>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>,
    /// Full fetch: retrieve all data through the real client and report counts.
    ///
    /// For bulk resources, uses `client.pages()` which bulk-deserializes each
    /// page via `response.json::<Vec<T>>()` — identical to the library's own path.
    ///
    /// Auth-required endpoints return an empty result when `auth` is `None`.
    pub full: fn(
        Gw2Client<Unauthenticated>,
        Option<Gw2Client<Authenticated>>,
    ) -> Pin<Box<dyn Future<Output = FullFetchResult> + Send>>,
}

/// All registered endpoints, populated at link time by the resource macros.
#[linkme::distributed_slice]
pub static ENDPOINTS: [EndpointEntry];
