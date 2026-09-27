use gw2_api_macros::gw2_endpoint;
use serde::{Deserialize, Serialize};

/// A GW2 Currency.
#[gw2_endpoint(path = "currencies", id_type = u32, paged)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Currency {
    pub id: CurrencyId,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub order: u32,
}
