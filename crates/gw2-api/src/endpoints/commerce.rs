//! `/v2/commerce` endpoints — types, IDs, and endpoint handles.
//!
//! Call chain:
//!   `client.commerce()`               → `CommerceEndpoint`
//!   `.prices()`                        → `PricesEndpoint`   (bulk resource)
//!   `.listings()`                      → `ListingsEndpoint` (bulk resource)
//!   `.delivery()`                      → `DeliveryEndpoint.get()` (singleton, auth)
//!   `.exchange().coins(qty)`           → `Result<CoinsToGems>`
//!   `.exchange().gems(qty)`            → `Result<GemsToCoins>`
//!   `.transactions().current().buys()` → `Result<Vec<Transaction>>`
//!   `.transactions().current().sells()`→ `Result<Vec<Transaction>>`
//!   `.transactions().history().buys()` → `Result<Vec<Transaction>>`
//!   `.transactions().history().sells()`→ `Result<Vec<Transaction>>`

use gw2_api_macros::gw2_endpoint;
use serde::{Deserialize, Serialize};

use crate::coin::Coin;
use crate::endpoints::items::ItemId;
use crate::error::Gw2ApiError;

// ── Trading post bulk resources ───────────────────────────────────────────────

/// Trading post price listing for an item.
// path "commerce/prices" → PricesEndpoint, commerce.prices()
// id_type = ItemId (existing type, no new ID generated)
#[gw2_endpoint(path = "commerce/prices", id_type = ItemId, paged)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemPrice {
    pub id: ItemId,
    pub whitelisted: bool,
    pub buys: PriceInfo,
    pub sells: PriceInfo,
}

/// Buy or sell side price info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceInfo {
    pub quantity: u32,
    pub unit_price: Coin,
}

/// A trading post listing (individual buy/sell orders).
// path "commerce/listings" → ListingsEndpoint, commerce.listings()
// id_type = ItemId (existing type, no new ID generated)
#[gw2_endpoint(path = "commerce/listings", id_type = ItemId, paged)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listing {
    pub id: ItemId,
    pub buys: Vec<ListingEntry>,
    pub sells: Vec<ListingEntry>,
}

/// Individual buy or sell order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListingEntry {
    pub listings: u32,
    pub unit_price: Coin,
    pub quantity: u32,
}

// ── Delivery (auth singleton) ─────────────────────────────────────────────────

/// Delivery box contents (coins + items waiting for pickup).
// path "commerce/delivery" → DeliveryEndpoint, commerce.delivery()
#[gw2_endpoint(path = "commerce/delivery", auth)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delivery {
    pub coins: Coin,
    pub items: Vec<DeliveryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryItem {
    pub id: ItemId,
    pub count: u32,
}

// ── Shared response types ─────────────────────────────────────────────────────

/// Result of `/v2/commerce/exchange/coins`: what the given coins buy in gems.
///
/// The API calls the result `quantity` on both exchange endpoints, but it counts
/// gems here and coins in [`GemsToCoins`], so the two get their own types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinsToGems {
    pub coins_per_gem: Coin,
    /// Gems received.
    #[serde(rename = "quantity")]
    pub gems: u32,
}

/// Result of `/v2/commerce/exchange/gems`: what the given gems sell for in coins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GemsToCoins {
    pub coins_per_gem: Coin,
    /// Coins received.
    #[serde(rename = "quantity")]
    pub coins: Coin,
}

/// A trading post transaction (current or historical).
///
/// The lists are paginated; `.get()` fetches every page.
#[gw2_endpoint(path = "commerce/transactions/current/buys", auth, collection, paged, also(
    "commerce/transactions/current/sells",
    "commerce/transactions/history/buys",
    "commerce/transactions/history/sells",
))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: u64,
    pub item_id: ItemId,
    pub price: Coin,
    pub quantity: u32,
    // TODO: DateTime<Utc> (also `purchased`).
    pub created: String,
    #[serde(default)]
    pub purchased: Option<String>,
}

// ── Namespace handles ─────────────────────────────────────────────────────────

/// Handle for all `/v2/commerce/*` sub-endpoints. Access via `client.commerce()`.
// commerce returns {"text":"API not active"}, not Vec<String> — no registry entry.
#[gw2_endpoint(path = "commerce", namespace, no_registry)]
pub struct Commerce;

/// Handle for `/v2/commerce/exchange/*`. Access via `commerce.exchange()`.
#[gw2_endpoint(path = "commerce/exchange", namespace, no_registry)]
pub struct Exchange;

/// Handle for `/v2/commerce/transactions/*`. Access via `commerce.transactions()`.
#[gw2_endpoint(path = "commerce/transactions", namespace, auth, no_registry)]
pub struct Transactions;

/// Handle for `/v2/commerce/transactions/current/*`. Access via `transactions.current()`.
#[gw2_endpoint(path = "commerce/transactions/current", namespace, auth, no_registry)]
pub struct Current;

/// Handle for `/v2/commerce/transactions/history/*`. Access via `transactions.history()`.
#[gw2_endpoint(path = "commerce/transactions/history", namespace, auth, no_registry)]
pub struct History;

// ── Exchange endpoint methods ─────────────────────────────────────────────────

/// How many gems `quantity` coins buy.
// path "commerce/exchange/coins", fn name "coins" == last seg → attaches to ExchangeEndpoint
#[gw2_endpoint(path = "commerce/exchange/coins", test_params(quantity = 1000000u32))]
pub async fn coins(&self, quantity: u32) -> Result<CoinsToGems, Gw2ApiError> {
    self.0
        .request("/commerce/exchange/coins")
        .param("quantity", quantity)
        .send()
        .await
}

/// How many coins `quantity` gems sell for.
// path "commerce/exchange/gems", fn name "gems" == last seg → attaches to ExchangeEndpoint
#[gw2_endpoint(path = "commerce/exchange/gems", test_params(quantity = 100u32))]
pub async fn gems(&self, quantity: u32) -> Result<GemsToCoins, Gw2ApiError> {
    self.0
        .request("/commerce/exchange/gems")
        .param("quantity", quantity)
        .send()
        .await
}
