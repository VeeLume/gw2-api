//! `/v2/itemstats` endpoint — itemstat IDs and stub type.

use gw2_api_macros::gw2_endpoint;
use serde::{Deserialize, Serialize};

// ── Types ─────────────────────────────────────────────────────────────────────

/// A GW2 itemstat (attribute set, e.g. "Berserker's").
#[gw2_endpoint(path = "itemstats", id_type = u32, paged)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemStat {
    pub id: ItemStatId,
    pub name: String,
}
