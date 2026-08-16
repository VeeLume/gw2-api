// models/build.rs
use serde::{Deserialize, Serialize};

use crate::cached_singleton;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Build {
    pub id: i32,
}

crate::ghost!(Build, "build");
cached_singleton!(Build);
