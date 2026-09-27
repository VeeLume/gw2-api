// lib.rs
//
// Two data classes, two paths:
//
//   static game data  →  `static_client()` + process-wide caches keyed by
//                        (Language, Id). Identical for every account, so one
//                        cache per process is correct, not a compromise.
//
//   account data      →  `Gw2Session`, which owns a key and its own state.
//
// Both share one `RateLimiter`, because the GW2 rate limit is per IP address.

pub mod api {
    pub mod client;
    pub mod limiter;
    pub mod session;
}
pub mod core {
    pub mod base;
    pub mod cache;
    pub mod ghost;
    pub mod ghost_id;
}
pub mod models {
    pub mod build;
    pub mod minis;
    pub mod mounts;
    pub mod tokeninfo;
}
pub mod shared {
    pub mod enums;
}

/// “prelude” so your examples don’t need a pile of imports
pub mod prelude {
    pub use crate::api::client::{ApiClient, ApiClientBuilder, ApiError, Language, static_client};
    pub use crate::api::limiter::RateLimiter;
    pub use crate::api::session::Gw2Session;
    pub use crate::core::base::Base;
    pub use crate::core::ghost::Ghost;
    pub use crate::core::ghost_id::{GhostId, Identifiable};
    pub use crate::models::tokeninfo::{Permission, TokenInfo};
    // macros are #[macro_export], available at crate root
}
