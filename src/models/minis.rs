// models/minis.rs
use phf::phf_map;
use serde::{Deserialize, Serialize};

use crate::cached_resource;

// Bugged name data
static BUGGED_NAMES: phf::Map<i32, (&'static str, i32)> = phf_map! {
    747i32 => ("Mini Shrine Guardian", 90009),
    717i32 => ("Mini Exo-Suit Springer", 88464),
    // ...fill the rest...
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawMini {
    pub id: i32,
    pub name: String,
    pub unlock: Option<String>,
    pub icon: String,
    pub order: i32,
    #[serde(rename = "item_id")]
    pub unlock_item: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "RawMini")]
pub struct Mini {
    pub id: i32,
    pub name: String,
    pub unlock: Option<String>,
    pub icon: String,
    pub order: i32,
    pub unlock_item: Option<i32>,
}

impl From<RawMini> for Mini {
    fn from(mut r: RawMini) -> Self {
        if r.name == "((208738))" {
            if let Some((fixed_name, fixed_item)) = BUGGED_NAMES.get(&r.id).copied() {
                r.name = fixed_name.to_string();
                r.unlock_item = Some(fixed_item);
            }
        }
        if r.unlock_item == Some(6) {
            if let Some((_, fixed_item)) = BUGGED_NAMES.get(&r.id).copied() {
                r.unlock_item = Some(fixed_item);
            }
        }
        Mini {
            id: r.id,
            name: r.name,
            unlock: r.unlock,
            icon: r.icon,
            order: r.order,
            unlock_item: r.unlock_item,
        }
    }
}

// Wire up GhostId + cache
crate::ghost_id!(Mini, "minis", i32);
cached_resource!(Mini, 128, ttl = std::time::Duration::from_secs(300));
