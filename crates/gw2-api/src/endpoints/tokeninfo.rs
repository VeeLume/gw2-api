//! `/v2/tokeninfo` endpoint — token introspection.
//!
//! Also defines shared types used by `/v2/createsubtoken`:
//!   `SubtokenPermission`, `TokenType`
//!
//! Call chain:
//!   `client.tokeninfo().get()` → `Result<TokenInfo>` (auth)

use chrono::{DateTime, Utc};
use gw2_api_macros::{gw2_endpoint, gw2_enum};
use serde::{Deserialize, Serialize};

// ── Shared types (also used by createsubtoken) ────────────────────────────────

/// A permission scope that an API key or subtoken may carry.
#[gw2_enum(lowercase)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubtokenPermission {
    Account,
    Builds,
    Characters,
    Guilds,
    Inventories,
    Progression,
    Pvp,
    Tradingpost,
    Unlocks,
    Wallet,
    WVW,
}

/// Discriminates between a full API key and a JWT subtoken.
#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenType {
    #[serde(rename = "APIKey")]
    ApiKey,
    Subtoken,
}

// ── TokenInfo singleton ───────────────────────────────────────────────────────

/// Info about the API key used in the request. From `/v2/tokeninfo` (auth required).
#[gw2_endpoint(path = "tokeninfo", auth)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    /// First half of the API key.
    pub id: String,
    /// Human-readable name set by the account owner.
    ///
    /// **Warning**: Not HTML-escaped — handle with care.
    pub name: String,
    /// Permission scopes granted to this key.
    pub permissions: Vec<SubtokenPermission>,
    /// Token type: `"APIKey"` or `"Subtoken"` (schema 2019-05-22+).
    #[serde(rename = "type")]
    pub token_type: TokenType,
    /// Subtoken expiry timestamp (schema 2019-05-22+, subtokens only).
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    /// Subtoken creation timestamp (schema 2019-05-22+, subtokens only).
    #[serde(default)]
    pub issued_at: Option<DateTime<Utc>>,
    /// Restricted endpoint list (schema 2019-05-22+, subtokens only).
    #[serde(default)]
    pub urls: Option<Vec<String>>,
}
