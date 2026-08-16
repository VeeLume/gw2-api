// lib.rs
pub mod api {
    pub mod client;
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
}
pub mod shared {
    pub mod enums;
}

// “prelude” so your examples don’t need a pile of imports
pub mod prelude {
    pub use crate::api::client::{API, ApiClient, ApiError};
    pub use crate::core::base::Base;
    pub use crate::core::ghost::Ghost;
    pub use crate::core::ghost_id::{GhostId, Identifiable};
    // macros are #[macro_export], available at crate root; this keeps trait names handy
}
