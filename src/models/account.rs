// models/account.rs
//
// `/v2/account` and `/v2/account/wallet` — account data. Like `tokeninfo`,
// these are not `Ghost`/`GhostId` models: the answer depends on which key asked,
// so they must never land in the process-wide static caches.

use serde::{Deserialize, Serialize};

use crate::models::currencies::CurrencyId;

/// A game or expansion the account owns.
///
/// Carries the same `Unknown` catch-all as `Permission`: every expansion adds a
/// value, and an account that owns one we have not listed must still
/// deserialize.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Access {
    /// Named to avoid shadowing `Option::None` at use sites.
    #[serde(rename = "None")]
    NoAccess,
    PlayForFree,
    GuildWars2,
    HeartOfThorns,
    PathOfFire,
    EndOfDragons,
    SecretsOfTheObscure,
    JanthirWilds,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    /// Seconds played on the account.
    pub age: u64,
    pub world: i32,
    pub guilds: Vec<String>,
    /// Requires the `guilds` scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guild_leader: Option<Vec<String>>,
    pub created: String,
    pub access: Vec<Access>,
    pub commander: bool,
    /// Requires the `progression` scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fractal_level: Option<i32>,
    /// Requires the `progression` scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_ap: Option<i32>,
    /// Requires the `progression` scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monthly_ap: Option<i32>,
    /// Schema 2024-07-20 moved `wvw_rank` in here, next to the team id from
    /// the world restructuring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wvw: Option<AccountWvw>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_storage_slots: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountWvw {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_id: Option<i32>,
    /// Requires the `progression` scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<i32>,
}

impl Account {
    pub fn has_access(&self, access: &Access) -> bool {
        self.access.contains(access)
    }
}

/// One line of `/v2/account/wallet`. Resolve the name with
/// `entry.id.get(false)`, or `Currency::get_many` for the whole wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletEntry {
    pub id: CurrencyId,
    pub value: u64,
}
