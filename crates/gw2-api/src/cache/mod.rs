//! Cache for **static game data** via [`moka`].
//!
//! Static data (items, recipes, minis, …) is identical for every account and
//! changes only when the game patches, so one cache per process is correct: by
//! default every client shares [`ResourceCache::global()`], the public client and
//! every authenticated one alike. Account data never enters it — resources marked
//! `auth` bypass the cache entirely.
//!
//! Entries are keyed by `(TypeId, scope, id)`, where the scope is the client's
//! base URL, language and schema version. Without the language in the key, fetching
//! a mini in English and then asking a German client returned the English name;
//! without the schema version, a `latest` client would read values parsed under
//! the pinned schema.
//!
//! Values are stored as `Arc<R>` **after** patching, so a cache hit returns the
//! same corrected value as the fetch that filled it.

use std::any::{Any, TypeId};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use moka::future::Cache;

use crate::error::Gw2ApiError;
use crate::resource::Resource;

type CacheKey = (TypeId, Arc<str>, String);
type CacheValue = Arc<dyn Any + Send + Sync>;

/// Default capacity of the global cache, in entries. Large enough for every item.
pub const DEFAULT_CAPACITY: u64 = 200_000;

/// Default time-to-live. Static data only changes with game builds; the TTL bounds
/// how long a long-running app serves data from before a patch.
pub const DEFAULT_TTL: Duration = Duration::from_secs(60 * 60);

static GLOBAL: LazyLock<Arc<ResourceCache>> =
    LazyLock::new(|| Arc::new(ResourceCache::new(DEFAULT_CAPACITY, Some(DEFAULT_TTL))));

/// Async LRU cache for static API resources.
pub struct ResourceCache {
    store: Option<Cache<CacheKey, CacheValue>>,
}

impl ResourceCache {
    /// Create a cache with the given maximum entry count and optional time-to-live.
    /// A capacity of `0` disables caching.
    pub fn new(capacity: u64, ttl: Option<Duration>) -> Self {
        if capacity == 0 {
            return Self::disabled();
        }
        let mut builder = Cache::builder().max_capacity(capacity);
        if let Some(ttl) = ttl {
            builder = builder.time_to_live(ttl);
        }
        Self {
            store: Some(builder.build()),
        }
    }

    /// A cache that stores nothing.
    pub fn disabled() -> Self {
        Self { store: None }
    }

    /// The process-wide cache every client uses unless given another one.
    pub fn global() -> Arc<ResourceCache> {
        GLOBAL.clone()
    }

    /// Whether this cache stores anything.
    pub fn is_enabled(&self) -> bool {
        self.store.is_some()
    }

    /// Drop every entry.
    pub async fn clear(&self) {
        if let Some(store) = &self.store {
            store.invalidate_all();
            store.run_pending_tasks().await;
        }
    }

    /// Number of entries, after flushing pending maintenance.
    pub async fn entry_count(&self) -> u64 {
        match &self.store {
            Some(store) => {
                store.run_pending_tasks().await;
                store.entry_count()
            }
            None => 0,
        }
    }

    fn key<R: Resource>(scope: &Arc<str>, id: &R::Id) -> CacheKey {
        (TypeId::of::<R>(), scope.clone(), id.to_string())
    }

    /// Look up one resource.
    pub(crate) async fn get<R: Resource + Clone>(&self, scope: &Arc<str>, id: &R::Id) -> Option<R> {
        let value = self.store.as_ref()?.get(&Self::key::<R>(scope, id)).await?;
        value.downcast_ref::<R>().cloned()
    }

    /// Store one (already patched) resource.
    pub(crate) async fn insert<R: Resource + Clone>(&self, scope: &Arc<str>, id: &R::Id, value: R) {
        if let Some(store) = &self.store {
            store.insert(Self::key::<R>(scope, id), Arc::new(value)).await;
        }
    }

    /// Return the cached value, or run `fetch` to fill it.
    ///
    /// Concurrent misses for the same key share one fetch. Errors are not cached.
    pub(crate) async fn get_or_fetch<R, F>(&self, scope: &Arc<str>, id: &R::Id, fetch: F) -> Result<R, Gw2ApiError>
    where
        R: Resource + Clone,
        F: Future<Output = Result<R, Gw2ApiError>>,
    {
        let Some(store) = &self.store else {
            return fetch.await;
        };
        let value = store
            .try_get_with(Self::key::<R>(scope, id), async move {
                fetch.await.map(|v| Arc::new(v) as CacheValue)
            })
            .await
            .map_err(|e| Arc::try_unwrap(e).unwrap_or_else(|shared| Gw2ApiError::from_shared(&shared)))?;
        value
            .downcast_ref::<R>()
            .cloned()
            .ok_or_else(|| Gw2ApiError::Other(format!("cache type mismatch for {}", R::PATH)))
    }
}
