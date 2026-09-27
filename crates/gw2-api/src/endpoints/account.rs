//! `/v2/account` endpoints — types, IDs, and endpoint handles.
//!
//! Path → generated handle mapping:
//!   `"account"`              → `AccountEndpoint`          (`client.account()`)
//!   `"account/wallet"`       → `WalletEndpoint`           (`account.wallet()`)         (auth)
//!   `"account/bank"`         → `BankEndpoint`             (`account.bank()`)           (auth)
//!   `"account/materials"`    → `MaterialsEndpoint`        (`account.materials()`)      (auth)
//!   `"account/achievements"` → `AccountAchievementsEndpoint` (`account.achievements()`) (auth)

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use gw2_api_macros::{gw2_endpoint, gw2_enum};
use serde::{Deserialize, Serialize};

use crate::endpoints::achievements::AchievementId;
use crate::endpoints::currencies::CurrencyId;
use crate::endpoints::items::{AttributeType, ItemId};
use crate::endpoints::itemstats::ItemStatId;
use crate::endpoints::skins::SkinId;

// ── Account singleton ─────────────────────────────────────────────────────────

/// Account info from `/v2/account` (auth required).
// path "account" → AccountEndpoint, client.account()
#[gw2_endpoint(path = "account", auth)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    /// Seconds played on the account.
    pub age: u64,
    pub name: String,
    // TODO: WorldId once /v2/worlds is modelled.
    pub world: u32,
    #[serde(default)]
    // TODO: GuildId once /v2/guild/:id is modelled (also `guild_leader`).
    pub guilds: Vec<String>,
    /// Requires the `guilds` scope; `None` without it.
    #[serde(default)]
    pub guild_leader: Option<Vec<String>>,
    pub created: DateTime<Utc>,
    #[serde(default)]
    pub access: Vec<AccountAccess>,
    pub commander: bool,
    /// Requires the `progression` scope.
    #[serde(default)]
    pub fractal_level: Option<u32>,
    /// Requires the `progression` scope.
    #[serde(default)]
    pub daily_ap: Option<u32>,
    /// Requires the `progression` scope.
    #[serde(default)]
    pub monthly_ap: Option<u32>,
    /// Schema 2024-07-20 moved `wvw_rank` in here, next to the team id from the
    /// world restructuring.
    #[serde(default)]
    pub wvw: Option<AccountWvw>,
    #[serde(default)]
    pub last_modified: Option<DateTime<Utc>>,
    #[serde(default)]
    pub build_storage_slots: Option<u32>,
}

/// World-vs-World part of [`Account`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountWvw {
    #[serde(default)]
    // TODO: a WvW team id type once the /v2/wvw endpoints are modelled.
    pub team_id: Option<u32>,
    /// Requires the `progression` scope.
    #[serde(default)]
    pub rank: Option<u32>,
}

impl Account {
    /// Whether the account owns the given game or expansion.
    pub fn has_access(&self, access: &AccountAccess) -> bool {
        self.access.contains(access)
    }
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

/// Stats chosen on a selectable-stat item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedStats {
    pub id: ItemStatId,
    /// Attribute bonuses the stats give on this item.
    #[serde(default)]
    pub attributes: HashMap<AttributeType, f64>,
}

/// What an item in the bank or material storage is bound to.
#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Binding {
    Account,
    Character,
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
    pub skin: Option<SkinId>,
    /// Dyes applied to the item.
    // TODO: Vec<ColorId> once /v2/colors is modelled.
    #[serde(default)]
    pub dyes: Option<Vec<u32>>,
    /// Runes and sigils in the item.
    #[serde(default)]
    pub upgrades: Option<Vec<ItemId>>,
    /// The upgrade slot each entry of `upgrades` sits in.
    #[serde(default)]
    pub upgrade_slot_indices: Option<Vec<u32>>,
    #[serde(default)]
    pub infusions: Option<Vec<ItemId>>,
    /// The chosen stats of selectable-stat gear.
    #[serde(default)]
    pub stats: Option<SelectedStats>,
    /// The character the item is soulbound to (with `binding: Character`).
    // TODO: a character name type once /v2/characters is modelled.
    #[serde(default)]
    pub bound_to: Option<String>,
    #[serde(default)]
    pub binding: Option<Binding>,
}

/// Material storage entry. GET /v2/account/materials returns Vec<MaterialEntry>.
// path "account/materials" → MaterialsEndpoint, account.materials()
#[gw2_endpoint(path = "account/materials", auth, collection)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialEntry {
    pub id: ItemId,
    // TODO: a material category id once /v2/materials is modelled.
    pub category: u32,
    pub count: u32,
    #[serde(default)]
    pub binding: Option<Binding>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bank_slot_with_upgrades_and_stats() {
        let json = r#"{"id":30698,"count":1,"skin":4678,"dyes":[1,2],"upgrades":[24615,24599],
            "upgrade_slot_indices":[0,1],"infusions":[49432],"binding":"Character","bound_to":"Hero",
            "stats":{"id":161,"attributes":{"Power":251,"Precision":179,"CritDamage":179}}}"#;
        let slot: BankSlot = serde_json::from_str(json).unwrap();
        assert_eq!(slot.upgrades.unwrap().len(), 2);
        let stats = slot.stats.unwrap();
        assert_eq!(stats.id.0, 161);
        assert_eq!(stats.attributes[&AttributeType::Power], 251.0);
        assert_eq!(slot.binding, Some(Binding::Character));
        assert_eq!(slot.bound_to.as_deref(), Some("Hero"));
    }
}
