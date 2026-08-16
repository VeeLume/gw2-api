// core/base.rs
use serde::{Deserialize, Serialize};

pub trait Base: Serialize + for<'de> Deserialize<'de> {}
impl<T> Base for T where T: Serialize + for<'de> Deserialize<'de> {}
