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
    pub point_cap: Option<u32>,
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

#[gw2_tagged_union]
#[derive(Debug, Clone, Serialize)]
pub enum AchievementBit {
    Text { text: String },
    Item { id: ItemId },
    Minipet { id: MiniId },
    Skin { id: SkinId },
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
