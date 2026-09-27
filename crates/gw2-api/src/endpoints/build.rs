//! `/v2/build` endpoint — the current game build id.
//!
//! The build id changes with every game patch, which makes it the cheap check for
//! whether cached static data may be stale.

use gw2_api_macros::gw2_endpoint;
use serde::{Deserialize, Serialize};

/// The current game build. From `/v2/build`.
#[gw2_endpoint(path = "build")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Build {
    pub id: u32,
}
