//! `/v2/novelties` endpoint — types, IDs, and endpoint handle.

use gw2_api_macros::{gw2_endpoint, gw2_enum};
use serde::{Deserialize, Serialize};

use crate::endpoints::items::ItemId;

// ── Types ─────────────────────────────────────────────────────────────────────

/// A GW2 novelty.
#[gw2_endpoint(path = "novelties", id_type = u32, paged)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Novelty {
    pub id: NoveltyId,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub slot: Option<NoveltySlot>,
    #[serde(default)]
    pub unlock_items: Vec<ItemId>,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NoveltySlot {
    Chair,
    Music,
    HeldItem,
    Miscellaneous,
    Tonic,
}
