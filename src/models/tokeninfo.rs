// models/tokeninfo.rs
//
// `/v2/tokeninfo` — the first authenticated endpoint, and the one worth having
// first: it is how an app answers "is this key valid, and what may I ask for?"
// without guessing, and it is what a settings page's "test connection" button
// calls.
//
// It is not a `Ghost`/`GhostId` model on purpose. Its answer depends entirely
// on which key asked, so it must never land in the process-wide static caches.

use serde::{Deserialize, Serialize};

/// A scope granted to an API key.
///
/// The `Unknown` catch-all matters: ArenaNet adds permissions over time, and a
/// key carrying one we have not heard of must not fail to deserialize the
/// entire response.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
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
    #[serde(untagged)]
    Unknown(String),
}

impl Permission {
    pub fn as_str(&self) -> &str {
        match self {
            Permission::Account => "account",
            Permission::Builds => "builds",
            Permission::Characters => "characters",
            Permission::Guilds => "guilds",
            Permission::Inventories => "inventories",
            Permission::Progression => "progression",
            Permission::Pvp => "pvp",
            Permission::Tradingpost => "tradingpost",
            Permission::Unlocks => "unlocks",
            Permission::Wallet => "wallet",
            Permission::Unknown(s) => s,
        }
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub id: String,
    pub name: String,
    pub permissions: Vec<Permission>,
    /// Present for subtokens only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_at: Option<String>,
    /// Subtokens may be restricted to a URL allowlist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub urls: Option<Vec<String>>,
}

impl TokenInfo {
    pub fn has(&self, perm: &Permission) -> bool {
        self.permissions.contains(perm)
    }

    /// Turn a missing scope into a typed error, so callers fail with "the key
    /// lacks `wallet`" instead of an opaque 403 from three layers down.
    pub fn require(&self, perm: &Permission) -> Result<(), crate::api::client::ApiError> {
        if self.has(perm) {
            Ok(())
        } else {
            Err(crate::api::client::ApiError::MissingPermission(
                perm.to_string(),
            ))
        }
    }
}
