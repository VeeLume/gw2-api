// models/mounts.rs
use serde::{Deserialize, Serialize};

/// This is a placeholder type for mount skin dye information.
/// normally this would be in a separate file, but for brevity it's included here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dye {
    #[serde(rename = "color_id")]
    pub color: i32,
    pub material: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountSkin {
    pub id: i32,
    pub name: String,
    pub icon: String,
    /// Links the skin to its `MountType::guid`. Schema 2025-08-29 removed the
    /// old `mount` field (the type's string id) in favour of this.
    pub mount_guid: String,
    pub dye_slots: Vec<Dye>,
}

crate::ghost_id!(MountSkin, "mounts/skins", i32);
crate::cached_resource!(MountSkin, 128);
crate::id_type!(MountSkin => MountSkinId);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountSkill {
    pub id: i32,
    pub slot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountType {
    pub id: String,
    /// What `MountSkin::mount_guid` points at.
    pub guid: String,
    pub name: String,
    pub default_skin: MountSkinId,
    pub skins: Vec<MountSkinId>,
    pub skills: Vec<MountSkill>,
}

crate::ghost_id!(MountType, "mounts/types", String);
crate::cached_resource!(MountType, 128);
crate::id_type!(MountType => MountTypeId);
