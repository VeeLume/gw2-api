//! `/v2/account` endpoints — types, IDs, and endpoint handles.
//!
//! Path → generated handle mapping:
//!   `"account"`              → `AccountEndpoint`          (`client.account()`)
//!   `"account/wallet"`       → `WalletEndpoint`           (`account.wallet()`)         (auth)
//!   `"account/bank"`         → `BankEndpoint`             (`account.bank()`)           (auth)
//!   `"account/materials"`    → `MaterialsEndpoint`        (`account.materials()`)      (auth)
//!   `"account/achievements"` → `AccountAchievementsEndpoint` (`account.achievements()`) (auth)

use gw2_api_macros::{gw2_endpoint, gw2_enum};
use serde::{Deserialize, Serialize};

use crate::endpoints::achievements::AchievementId;
use crate::endpoints::currencies::CurrencyId;
use crate::endpoints::items::ItemId;

// ── Account singleton ─────────────────────────────────────────────────────────

/// Account info from `/v2/account` (auth required).
// path "account" → AccountEndpoint, client.account()
#[gw2_endpoint(path = "account", auth)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub age: u64,
    pub name: String,
    pub world: u32,
    #[serde(default)]
    pub guilds: Vec<String>,
    #[serde(default)]
    pub guild_leader: Vec<String>,
    pub created: String,
    #[serde(default)]
    pub access: Vec<AccountAccess>,
    pub commander: bool,
    pub fractal_level: Option<u32>,
    pub daily_ap: Option<u32>,
    pub monthly_ap: Option<u32>,
    pub wvw_rank: Option<u32>,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AccountAccess {
    None,
    PlayForFree,
    GuildWars2,
    HeartOfThorns,
    PathOfFire,
    EndOfDragons,
    SecretsOfTheObscure,
    JanthirWilds,
}

// ── Account sub-collections ───────────────────────────────────────────────────

/// Wallet currency entry. GET /v2/account/wallet returns Vec<WalletEntry>.
// path "account/wallet" → WalletEndpoint, account.wallet()
#[gw2_endpoint(path = "account/wallet", auth, collection)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletEntry {
    pub id: CurrencyId,
    pub value: i64,
}

/// Bank slot. GET /v2/account/bank returns Vec<Option<BankSlot>>.
// path "account/bank" → BankEndpoint, account.bank()
#[gw2_endpoint(path = "account/bank", auth, collection, nullable)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankSlot {
    pub id: ItemId,
    pub count: u32,
    #[serde(default)]
    pub charges: Option<u32>,
    #[serde(default)]
    pub skin: Option<u32>,
    #[serde(default)]
    pub binding: Option<String>,
}

/// Material storage entry. GET /v2/account/materials returns Vec<MaterialEntry>.
// path "account/materials" → MaterialsEndpoint, account.materials()
#[gw2_endpoint(path = "account/materials", auth, collection)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialEntry {
    pub id: ItemId,
    pub category: u32,
    pub count: u32,
    #[serde(default)]
    pub binding: Option<String>,
}

/// Player's progress on an achievement. GET /v2/account/achievements returns Vec<AccountAchievement>.
// path "account/achievements" → AccountAchievementsEndpoint, account.achievements()
// accessor name "achievements" conflicts with client.achievements() from achievements.rs,
// so we use accessor = "achievements" but the endpoint struct is AccountAchievementsEndpoint.
#[gw2_endpoint(path = "account/achievements", auth, collection)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountAchievement {
    pub id: AchievementId,
    #[serde(default)]
    pub bits: Vec<u32>,
    pub current: Option<u32>,
    pub max: Option<u32>,
    pub done: bool,
    #[serde(default)]
    pub repeated: Option<u32>,
    #[serde(default)]
    pub unlocked: Option<bool>,
}
