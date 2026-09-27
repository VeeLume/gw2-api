//! `/v2/achievements` endpoints — types, IDs, and endpoint handles.
//!
//! Path → generated handle mapping:
//!   `"achievements"`            → `AchievementsEndpoint`  (`client.achievements()`)
//!   `"achievements/categories"` → `CategoriesEndpoint`    (`achievements.categories()`)
//!   `"achievements/groups"`     → `GroupsEndpoint`        (`achievements.groups()`)

use gw2_api_macros::{gw2_endpoint, gw2_enum, gw2_tagged_union};
use serde::{Deserialize, Serialize};

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

#[gw2_tagged_union]
#[derive(Debug, Clone, Serialize)]
pub enum AchievementReward {
    Coins { count: u32 },
    Item { id: ItemId, count: u32 },
    Mastery { id: u32, region: String },
    Title { id: u32 },
    Unknown { type_: String },
}

/// One step of an achievement. Its position in `bits` is the index that
/// `/v2/account/achievements` reports progress against, so blank bits are kept.
#[gw2_tagged_union]
#[derive(Debug, Clone, Serialize)]
pub enum AchievementBit {
    Text { text: String },
    Item { id: ItemId },
    Minipet { id: MiniId },
    Skin { id: SkinId },
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
    pub achievements: Vec<AchievementId>,
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
        assert!(matches!(a.bits[1], AchievementBit::Item { ref id } if id.0 == 5));
        assert!(matches!(a.bits[2], AchievementBit::Blank));
    }
}
