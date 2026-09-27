//! `/v2/skins` endpoint — types, IDs, and endpoint handle.

use gw2_api_macros::gw2_endpoint;
use serde::{Deserialize, Serialize};

use crate::common::Rarity;

// ── Types ─────────────────────────────────────────────────────────────────────

/// A GW2 skin (transmutation appearance).
#[gw2_endpoint(path = "skins", id_type = u32, paged)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skin {
    pub id: SkinId,
    pub name: String,
    #[serde(rename = "type")]
    pub skin_type: String,
    pub icon: Option<String>,
    pub rarity: Rarity,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub restrictions: Vec<String>,
}
