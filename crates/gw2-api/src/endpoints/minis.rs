//! `/v2/minis` endpoint — types, IDs, and endpoint handle.

use gw2_api_macros::gw2_endpoint;
use serde::{Deserialize, Serialize};

use crate::endpoints::items::ItemId;

// ── Types ─────────────────────────────────────────────────────────────────────

/// A GW2 mini-pet.
#[gw2_endpoint(path = "minis", id_type = u32, paged, no_default_patch)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mini {
    pub id: MiniId,
    pub name: String,
    /// How to obtain the mini, where the API says.
    #[serde(default)]
    pub unlock: Option<String>,
    pub icon: String,
    pub order: u32,
    #[serde(rename = "item_id")]
    pub unlock_item: ItemId,
}
