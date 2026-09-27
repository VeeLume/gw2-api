use gw2_api_macros::{gw2_endpoint, gw2_enum};
use serde::{Deserialize, Serialize};

#[gw2_endpoint(path="skills", id_type = u32, paged)]
#[derive(Debug, Clone, Serialize, Deserialize)]
// TODO: stub with only `id` (not even pub). The full shape (facts, traited_facts, slots,
//       chains, …) is in the 2026-09-27 wiki audit in Firefly.
pub struct Skill {
    id: SkillId,
}

/// The slot a skill occupies on the skill bar. Also used by mount skills.
///
/// `Elite`, `Heal` and `Toolbelt` occur in live data but are not on the wiki.
#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SkillSlot {
    #[serde(rename = "Downed_1")]
    Downed1,
    #[serde(rename = "Downed_2")]
    Downed2,
    #[serde(rename = "Downed_3")]
    Downed3,
    #[serde(rename = "Downed_4")]
    Downed4,
    Elite,
    Heal,
    Pet,
    #[serde(rename = "Profession_1")]
    Profession1,
    #[serde(rename = "Profession_2")]
    Profession2,
    #[serde(rename = "Profession_3")]
    Profession3,
    #[serde(rename = "Profession_4")]
    Profession4,
    #[serde(rename = "Profession_5")]
    Profession5,
    Toolbelt,
    Utility,
    #[serde(rename = "Weapon_1")]
    Weapon1,
    #[serde(rename = "Weapon_2")]
    Weapon2,
    #[serde(rename = "Weapon_3")]
    Weapon3,
    #[serde(rename = "Weapon_4")]
    Weapon4,
    #[serde(rename = "Weapon_5")]
    Weapon5,
}
