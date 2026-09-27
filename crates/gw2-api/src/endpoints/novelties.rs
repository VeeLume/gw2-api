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
    /// Items that unlock this novelty. The API field is singular (`unlock_item`)
    /// although it holds a list.
    #[serde(default, rename = "unlock_item")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlock_items_come_from_unlock_item() {
        let json = r#"{"id":1,"name":"Embellished Kite","description":"","icon":"i","slot":"HeldItem","unlock_item":[88124]}"#;
        let n: Novelty = serde_json::from_str(json).unwrap();
        assert_eq!(n.unlock_items.iter().map(|i| i.0).collect::<Vec<_>>(), [88124]);
    }
}
