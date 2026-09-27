//! `/v2/createsubtoken` endpoint — JWT subtoken generation.
//!
//! Call chain:
//!   `client.createsubtoken(expire, permissions, urls)` → `Result<Subtoken>` (auth)

use std::fmt;

use chrono::{DateTime, SecondsFormat, Utc};
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
#[gw2_endpoint(path = "createsubtoken", auth, test_params(expire = ::chrono::Utc::now() + ::chrono::Duration::hours(1), permissions = [SubtokenPermission::Account], urls = ["/v2/account"],))]
pub async fn createsubtoken(
    &self,
    expire: DateTime<Utc>,
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

    let mut req = self
        .request("/createsubtoken")
        .param("expire", expire.to_rfc3339_opts(SecondsFormat::Secs, true));
    if !permissions_str.is_empty() {
        req = req.param("permissions", permissions_str);
    }
    if !urls_str.is_empty() {
        req = req.param("urls", urls_str);
    }
    req.send().await
}
