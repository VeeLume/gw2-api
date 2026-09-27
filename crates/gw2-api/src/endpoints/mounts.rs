//! `/v2/mounts` endpoints — mount types and mount skins.
//!
//! Path → generated handle mapping:
//!   `"mounts"`       → `MountsEndpoint` (`client.mounts()`)
//!   `"mounts/types"` → `TypesEndpoint`  (`mounts.types()`)
//!   `"mounts/skins"` → `SkinsEndpoint`  (`mounts.skins()`)

use gw2_api_macros::{gw2_endpoint, gw2_enum};
use serde::{Deserialize, Serialize};

use crate::endpoints::skills::{SkillId, SkillSlot};

/// Handle for all `/v2/mounts/*` sub-endpoints. Access via `client.mounts()`.
#[gw2_endpoint(path = "mounts", namespace)]
pub struct Mounts;

/// A mount type (Raptor, Springer, …). From `/v2/mounts/types`.
#[gw2_endpoint(path = "mounts/types", id_type = String, paged)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountType {
    /// String id, e.g. `"raptor"`.
    pub id: MountTypeId,
    /// What [`MountSkin::mount_guid`] points at.
    pub guid: String,
    pub name: String,
    pub default_skin: MountSkinId,
    pub skins: Vec<MountSkinId>,
    pub skills: Vec<MountSkill>,
}

/// One skill slot of a mount type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountSkill {
    pub id: SkillId,
    pub slot: SkillSlot,
}

/// A mount skin. From `/v2/mounts/skins`.
#[gw2_endpoint(path = "mounts/skins", id_type = u32, paged)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountSkin {
    pub id: MountSkinId,
    pub name: String,
    pub icon: String,
    /// Links the skin to its [`MountType::guid`]. Schema 2025-08-29 replaced the old
    /// `mount` field (the type's string id) with this.
    pub mount_guid: String,
    pub dye_slots: Vec<MountDyeSlot>,
}

/// Default dye of one mount skin dye channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountDyeSlot {
    // TODO: ColorId once /v2/colors is modelled.
    pub color_id: u32,
    pub material: DyeMaterial,
}

/// Material a dye channel is applied to. Lowercase in JSON; not listed on the wiki.
#[gw2_enum(lowercase)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DyeMaterial {
    Cloth,
    Fur,
    Leather,
    Metal,
}
