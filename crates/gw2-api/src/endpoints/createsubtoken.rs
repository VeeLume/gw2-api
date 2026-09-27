//! `/v2/createsubtoken` endpoint — JWT subtoken generation.
//!
//! Call chain:
//!   `client.create_subtoken(expire, permissions, urls)` → `Result<Subtoken>` (auth)

use std::fmt;

use gw2_api_macros::gw2_endpoint;
use serde::{Deserialize, Serialize};

use crate::endpoints::tokeninfo::SubtokenPermission;
use crate::error::Gw2ApiError;

// ── Response type ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtoken {
    /// The generated JWT, usable as an API key with the requested limitations.
    pub subtoken: String,
}

// ── Method ────────────────────────────────────────────────────────────────────

/// Create a subtoken with the given expiry, permissions, and (optionally) URL allowlist.
///
/// - `expire`      — ISO-8601 datetime for expiry (max one year from now; clamped if exceeded).
/// - `permissions` — Permissions to inherit; unknown or ungranted permissions are silently ignored.
/// - `urls`        — Optional endpoint allowlist; if empty, all endpoints permitted by `permissions` are accessible.
#[gw2_endpoint(path = "createsubtoken", auth, test_params(expire = "2100-12-31T23:59:59Z", permissions = [SubtokenPermission::Inventories], urls = ["account", "characters"],))]
pub async fn createsubtoken(
    &self,
    // TODO: take DateTime<Utc>; omit `permissions`/`urls` when empty.
    expire: &str,
    permissions: impl IntoIterator<Item = SubtokenPermission>,
    urls: impl IntoIterator<Item = impl fmt::Display>,
) -> Result<Subtoken, Gw2ApiError> {
    let permissions_str = permissions
        .into_iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let urls_str = urls
        .into_iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");

    self.request("/createsubtoken")
        .param("expire", expire)
        .param("permissions", permissions_str)
        .param("urls", urls_str)
        .send()
        .await
}
