// models/currencies.rs
//
// `/v2/currencies` — static game data. It is what turns a wallet's bare ids
// into names and icons, so the wallet lives on the session while this lives in
// the process-wide cache.
use serde::{Deserialize, Serialize};

/// Id 1 is Coin, whose wallet value is in copper.
pub const COIN: i32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Currency {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub icon: String,
    /// Sort order the game UI uses for the wallet.
    pub order: i32,
}

crate::ghost_id!(Currency, "currencies", i32);
crate::cached_resource!(Currency, 256);
crate::id_type!(Currency => CurrencyId);
