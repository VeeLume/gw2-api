//! Transparent resource caching via [`moka`].
//!
//! The cache stores raw `serde_json::Value` keyed by `(TypeId, id_string)`, so a single
//! cache serves all resource types without generic parameters. Values are deserialized
//! on cache hits — negligible overhead compared to network latency.

use std::any::TypeId;
use std::time::Duration;

use moka::future::Cache;
use serde::de::DeserializeOwned;

use crate::resource::Resource;

type CacheKey = (TypeId, String);

/// Async LRU cache for API resources, backed by [`moka`].
pub struct ResourceCache {
    store: Cache<CacheKey, serde_json::Value>,
}

impl ResourceCache {
    /// Create a cache with the given maximum entry count and a 5-minute TTL.
    pub fn new(capacity: u64) -> Self {
        Self {
            store: Cache::builder()
                .max_capacity(capacity)
                .time_to_live(Duration::from_secs(300))
                .build(),
        }
    }

    /// Create a disabled (no-op) cache with zero capacity.
    pub fn disabled() -> Self {
        Self::new(0)
    }

    /// Look up a resource by type + ID. Returns `None` on cache miss or deserialization failure.
    pub async fn get<R: Resource>(&self, id: &R::Id) -> Option<R>
    where
        R: DeserializeOwned,
    {
        let key = (TypeId::of::<R>(), id.to_string());
        let value = self.store.get(&key).await?;
        serde_json::from_value(value).ok()
    }

    /// Insert a raw JSON value into the cache for the given resource type + ID.
    pub async fn insert<R: Resource>(&self, id: &R::Id, value: serde_json::Value) {
        let key = (TypeId::of::<R>(), id.to_string());
        self.store.insert(key, value).await;
    }
}
