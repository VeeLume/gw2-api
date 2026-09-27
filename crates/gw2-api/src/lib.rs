//! # gw2-api
//!
//! Type-safe Guild Wars 2 API v2 client library.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use gw2_api::{Gw2Client, Language};
//! use gw2_api::endpoints::items::ItemId;
//!
//! # async fn example() -> Result<(), gw2_api::error::Gw2ApiError> {
//! // Unauthenticated client — public endpoints only
//! let client = Gw2Client::new();
//! let item = client.items().get(ItemId(19976)).await?;
//! println!("{}: {:?}", item.name, item.rarity);
//!
//! // Authenticated client — also enables account endpoints
//! let client = Gw2Client::builder()
//!     .api_key("your-api-key")
//!     .build()?;
//! let account = client.account().get().await?;
//! # Ok(())
//! # }
//! ```

// Allow proc macros (which emit `::gw2_api::...` paths) to resolve correctly
// when used inside this crate itself.
extern crate self as gw2_api;

pub mod cache;
pub mod chat_link;
pub mod client;
pub mod coin;
pub mod common;
#[macro_use]
pub mod macros;
pub mod endpoints;
pub mod error;
pub mod patches;
pub mod rate_limit;
pub mod registry;
pub mod resource;

pub use client::{
    DEFAULT_SCHEMA_VERSION, Gw2Client, Language,
    auth::{Authenticated, Unauthenticated},
};
pub use coin::Coin;
pub use error::Gw2ApiError;
pub use resource::{
    CollectionSingletonResource, PageOptions, PagedResource, Patchable, Resource, SingletonResource,
};
