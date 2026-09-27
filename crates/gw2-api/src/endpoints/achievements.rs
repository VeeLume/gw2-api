//! `/v2/achievements` endpoints — types, IDs, and endpoint handles.
//!
//! Path → generated handle mapping:
//!   `"achievements"`            → `AchievementsEndpoint`  (`client.achievements()`)
//!   `"achievements/categories"` → `CategoriesEndpoint`    (`achievements.categories()`)
//!   `"achievements/groups"`     → `GroupsEndpoint`        (`achievements.groups()`)

use gw2_api_macros::{gw2_endpoint, gw2_enum, gw2_tagged_union};
use serde::{Deserialize, Serialize};

use crate::coin::Coin;
use crate::endpoints::account::AccountAccess;
use crate::endpoints::items::ItemId;
use crate::endpoints::minis::MiniId;
use crate::endpoints::skins::SkinId;

// ── Achievement resource ──────────────────────────────────────────────────────

/// A GW2 achievement.
// path "achievements" → AchievementsEndpoint, client.achievements()
#[gw2_endpoint(path = "achievements", id_type = u32, paged)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Achievement {
    pub id: AchievementId,
    pub name: String,
    /// Only on achievements with their own icon; otherwise use the category's.
    #[serde(default)]
    pub icon: Option<String>,
    pub description: String,
    pub requirement: String,
    pub locked_text: String,
    #[serde(rename = "type")]
    pub achievement_type: AchievementType,
    #[serde(default)]
    pub flags: Vec<AchievementFlag>,
    #[serde(default)]
    pub tiers: Vec<AchievementTier>,
    #[serde(default)]
    pub prerequisites: Vec<AchievementId>,
    #[serde(default)]
    pub rewards: Vec<AchievementReward>,
    #[serde(default)]
    pub bits: Vec<AchievementBit>,
    /// The maximum AP an achievement flagged `Repeatable` can award.
    ///
    /// Signed because the API sends `-1` on some `Repeatable` achievements (55 as of
    /// 2026-09-27, e.g. 7804). The meaning of `-1` is undocumented; presumably
    /// "no cap". Kept raw rather than guessed.
    #[serde(default)]
    pub point_cap: Option<i32>,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AchievementType {
    Default,
    ItemSet,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AchievementFlag {
    Pvp,
    CategoryDisplay,
    MoveToTop,
    IgnoreNearlyComplete,
    Repeatable,
    Hidden,
    RequiresUnlock,
    RepairOnLogin,
    Daily,
    Weekly,
    Monthly,
    Permanent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchievementTier {
    pub count: u32,
    pub points: u32,
}

/// Region of a mastery point reward.
#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MasteryRegion {
    /// Core Tyria.
    Tyria,
    /// Heart of Thorns.
    Maguuma,
    /// Path of Fire.
    Desert,
    /// Icebrood Saga.
    Tundra,
    /// End of Dragons.
    Jade,
    /// Secrets of the Obscure.
    Sky,
    /// Janthir Wilds.
    Wild,
    /// Visions of Eternity.
    Magic,
}

#[gw2_tagged_union]
#[derive(Debug, Clone, Serialize)]
pub enum AchievementReward {
    Coins { count: Coin },
    Item { id: ItemId, count: u32 },
    // TODO: `id` is a mastery point id, only resolvable via /v2/account/mastery/points.
    Mastery { id: u32, region: MasteryRegion },
    // TODO: TitleId once /v2/titles is modelled.
    Title { id: u32 },
    Unknown { type_: String },
}

/// One step of an achievement. Its position in `bits` is the index that
/// `/v2/account/achievements` reports progress against, so blank bits are kept.
#[gw2_tagged_union]
#[derive(Debug, Clone, Serialize)]
pub enum AchievementBit {
    Text { text: String },
    /// `text` is optional; e.g. fishing achievements put the hole, bait and time of day there.
    Item { id: ItemId, text: Option<String> },
    Minipet { id: MiniId, text: Option<String> },
    Skin { id: SkinId, text: Option<String> },
    /// A blank `{}` object. Documented: achievements that track recipe unlocks
    /// omit the details (e.g. 5585 "Dragon Ice Infuser").
    /// <https://github.com/arenanet/api-cdi/issues/670>,
    /// <https://github.com/gw2-api/issues/issues/15>
    #[no_type]
    Blank,
    Unknown { type_: String },
}

// ── Sub-resources ─────────────────────────────────────────────────────────────

/// Achievement category.
// path "achievements/categories" → CategoriesEndpoint, achievements.categories()
#[gw2_endpoint(path = "achievements/categories", id_type = u32)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchievementCategory {
    pub id: AchievementCategoryId,
    pub name: String,
    pub description: String,
    pub order: u32,
    pub icon: String,
    #[serde(default)]
    pub achievements: Vec<CategoryAchievement>,
    /// The achievements active tomorrow. Only on some daily categories.
    #[serde(default)]
    pub tomorrow: Option<Vec<CategoryAchievement>>,
}

/// One entry of [`AchievementCategory::achievements`] (an object since schema 2022-03-23).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryAchievement {
    pub id: AchievementId,
    #[serde(default)]
    pub flags: Vec<CategoryAchievementFlag>,
    /// Inclusive level range, on level-locked dailies.
    #[serde(default)]
    pub level: Option<[u32; 2]>,
    /// Whether the entry needs (or must lack) an expansion.
    #[serde(default)]
    pub required_access: Option<RequiredAccess>,
}

/// The type of a daily achievement in a category.
#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CategoryAchievementFlag {
    PvE,
    PvP,
    WvW,
    SpecialEvent,
}

/// An expansion requirement on a category entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequiredAccess {
    /// The wiki lists `HeartOfThorns` and `PathOfFire`.
    pub product: AccountAccess,
    pub condition: AccessCondition,
}

/// Whether the account must own [`RequiredAccess::product`] to see the entry.
#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AccessCondition {
    HasAccess,
    NoAccess,
}

/// Achievement group.
// path "achievements/groups" → GroupsEndpoint, achievements.groups()
#[gw2_endpoint(path = "achievements/groups", id_type = String)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchievementGroup {
    pub id: AchievementGroupId,
    pub name: String,
    pub description: String,
    pub order: u32,
    #[serde(default)]
    pub categories: Vec<AchievementCategoryId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn achievement(extra: &str) -> String {
        format!(
            r#"{{"id":1,"name":"a","description":"","requirement":"","locked_text":"","type":"ItemSet","flags":["Repeatable"],"tiers":[]{extra}}}"#
        )
    }

    #[test]
    fn negative_point_cap_parses() {
        let a: Achievement = serde_json::from_str(&achievement(r#","point_cap":-1"#)).unwrap();
        assert_eq!(a.point_cap, Some(-1));
    }

    #[test]
    fn blank_bits_keep_their_positions() {
        let a: Achievement =
            serde_json::from_str(&achievement(r#","bits":[{},{"type":"Item","id":5},{}]"#)).unwrap();
        assert!(matches!(a.bits[0], AchievementBit::Blank));
        assert!(matches!(a.bits[1], AchievementBit::Item { ref id, .. } if id.0 == 5));
        assert!(matches!(a.bits[2], AchievementBit::Blank));
    }

    #[test]
    fn bits_keep_their_text() {
        let a: Achievement = serde_json::from_str(&achievement(
            r#","bits":[{"type":"Item","id":5,"text":"Fishing Hole: Any"},{"type":"Skin","id":6}]"#,
        ))
        .unwrap();
        assert!(matches!(a.bits[0], AchievementBit::Item { ref text, .. } if text.as_deref() == Some("Fishing Hole: Any")));
        assert!(matches!(a.bits[1], AchievementBit::Skin { text: None, .. }));
    }

    #[test]
    fn category_entries_and_tomorrow() {
        let json = r#"{"id":79,"name":"Halloween Daily","description":"","order":1,"icon":"i",
            "achievements":[{"id":1021},{"id":3167,"flags":["SpecialEvent","PvE"],"level":[1,80]}],
            "tomorrow":[{"id":1019,"required_access":{"product":"PathOfFire","condition":"NoAccess"}}]}"#;
        let c: AchievementCategory = serde_json::from_str(json).unwrap();
        assert_eq!(c.achievements[1].flags, [CategoryAchievementFlag::SpecialEvent, CategoryAchievementFlag::PvE]);
        assert_eq!(c.achievements[1].level, Some([1, 80]));
        let tomorrow = c.tomorrow.unwrap();
        let access = tomorrow[0].required_access.as_ref().unwrap();
        assert_eq!(access.product, AccountAccess::PathOfFire);
        assert_eq!(access.condition, AccessCondition::NoAccess);
    }
}
