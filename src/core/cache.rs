// core/cache.rs
use moka::future::Cache;
use std::{fmt, hash::Hash, sync::Arc, time::Duration};

use crate::api::client::{API, ApiError};
use crate::core::ghost::Ghost;
use crate::core::ghost_id::{GhostId, Identifiable};

/// ====================== GhostId caches (ID-based) ======================

pub struct TypeCache<T>
where
    T: GhostId + Identifiable + Clone + Send + Sync + 'static,
    T::Id: Clone + Eq + Hash + fmt::Display + Send + Sync + 'static,
{
    inner: Cache<T::Id, Arc<T>>,
}

impl<T> TypeCache<T>
where
    T: GhostId + Identifiable + Clone + Send + Sync + 'static,
    T::Id: Clone + Eq + Hash + fmt::Display + Send + Sync + 'static,
{
    pub fn new(capacity: u64, ttl: Option<Duration>) -> Self {
        let mut builder = Cache::builder().max_capacity(capacity);
        if let Some(d) = ttl {
            builder = builder.time_to_live(d);
        }
        Self {
            inner: builder.build(),
        }
    }

    /// Get one. `force = true` bypasses cache and overwrites it.
    pub async fn get(&self, id: T::Id, force: bool) -> Result<Arc<T>, ApiError> {
        if force {
            let fresh: T = API.get_json(&T::url_for_id(&id), &[]).await?;
            let arc = Arc::new(fresh);
            self.inner.insert(id, arc.clone()).await;
            return Ok(arc);
        }

        // cache-first; errors come back as Arc<ApiError>
        let res: Result<Arc<T>, Arc<ApiError>> = self
            .inner
            .try_get_with(id.clone(), async move {
                let v: T = API.get_json(&T::url_for_id(&id), &[]).await?;
                Ok::<Arc<T>, ApiError>(Arc::new(v))
            })
            .await;

        match res {
            Ok(v) => Ok(v),
            Err(e) => match Arc::try_unwrap(e) {
                Ok(err) => Err(err),
                Err(shared) => Err(ApiError::Other(shared.to_string())),
            },
        }
    }

    /// Get many. If `force`, refetch all given ids; else cache-first then batch fetch misses.
    pub async fn get_many(&self, ids: &[T::Id], force: bool) -> Result<Vec<Arc<T>>, ApiError> {
        use std::collections::{HashMap, HashSet};

        // Keep order of input ids; dedup for network work
        let mut uniq = Vec::new();
        let mut seen = HashSet::new();
        for id in ids.iter().cloned() {
            if seen.insert(id.clone()) {
                uniq.push(id);
            }
        }

        if force {
            // fetch all in one ids= call
            let fetched: Vec<T> = {
                let joined = uniq
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                API.get_json::<Vec<T>>(T::URL, &[("ids", joined)]).await?
            };

            let mut by_id: HashMap<T::Id, Arc<T>> = HashMap::with_capacity(fetched.len());
            for it in fetched {
                let key = it.id().clone();
                let arc = Arc::new(it);
                self.inner.insert(key.clone(), arc.clone()).await;
                by_id.insert(key, arc);
            }

            let mut out = Vec::with_capacity(ids.len());
            for id in ids.iter() {
                if let Some(v) = by_id.get(id) {
                    out.push(v.clone());
                }
            }
            return Ok(out);
        }

        // cache-first path
        let mut out = Vec::with_capacity(ids.len());
        let mut miss = Vec::new();

        for id in uniq.into_iter() {
            if let Some(v) = self.inner.get(&id).await {
                out.push(v);
            } else {
                miss.push(id);
            }
        }

        if !miss.is_empty() {
            let fetched: Vec<T> = {
                let joined = miss
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                API.get_json::<Vec<T>>(T::URL, &[("ids", joined)]).await?
            };
            let mut by_id = std::collections::HashMap::with_capacity(fetched.len());
            for it in fetched {
                let key = it.id().clone();
                let arc = Arc::new(it);
                self.inner.insert(key.clone(), arc.clone()).await;
                by_id.insert(key, arc);
            }
            for id in miss {
                if let Some(v) = self.inner.get(&id).await {
                    out.push(v);
                } else if let Some(v) = by_id.get(&id) {
                    out.push(v.clone());
                }
            }
        }

        Ok(out)
    }

    /// Get one page (always fetch; then populate cache). `force` kept for symmetry.
    pub async fn get_page(
        &self,
        page: usize,
        page_size: usize,
        _force: bool,
    ) -> Result<Vec<Arc<T>>, ApiError> {
        let items: Vec<T> = API
            .get_json(
                T::URL,
                &[
                    ("page", page.to_string()),
                    ("page_size", page_size.to_string()),
                ],
            )
            .await?;
        Ok(self.put_many(items).await)
    }

    /// Get page range (always fetch; then populate cache).
    pub async fn get_pages(
        &self,
        start: usize,
        stop: usize,
        page_size: usize,
        _force: bool,
    ) -> Result<Vec<Arc<T>>, ApiError> {
        use futures::stream::{FuturesUnordered, StreamExt};
        let mut futs = FuturesUnordered::new();
        for p in start..stop {
            let p = p;
            futs.push(async move {
                let params = vec![
                    ("page", p.to_string()),
                    ("page_size", page_size.to_string()),
                ];
                API.get_json::<Vec<T>>(T::URL, &params).await
            });
        }
        let mut collected = Vec::new();
        while let Some(batch) = futs.next().await {
            collected.extend(batch?);
        }
        Ok(self.put_many(collected).await)
    }

    /// Get all (expanded if supported). Always fetch fresh; then populate cache.
    pub async fn get_all(&self, expanded: bool, _force: bool) -> Result<Vec<Arc<T>>, ApiError> {
        let items: Vec<T> = if expanded {
            match API
                .get_json::<Vec<T>>(T::URL, &[("ids", "all".into())])
                .await
            {
                Ok(v) => v,
                Err(ApiError::BadRequest(_)) => {
                    // fall back to pagination
                    let mut out = Vec::new();
                    let mut page = 0usize;
                    loop {
                        let batch: Vec<T> = API
                            .get_json(
                                T::URL,
                                &[("page", page.to_string()), ("page_size", 200.to_string())],
                            )
                            .await?;
                        if batch.is_empty() {
                            break;
                        }
                        out.extend(batch);
                        page += 1;
                    }
                    out
                }
                Err(e) => return Err(e),
            }
        } else {
            let mut out = Vec::new();
            let mut page = 0usize;
            loop {
                let batch: Vec<T> = API
                    .get_json(
                        T::URL,
                        &[("page", page.to_string()), ("page_size", 200.to_string())],
                    )
                    .await?;
                if batch.is_empty() {
                    break;
                }
                out.extend(batch);
                page += 1;
            }
            out
        };

        Ok(self.put_many(items).await)
    }

    async fn put_many(&self, items: Vec<T>) -> Vec<Arc<T>> {
        let mut arcs = Vec::with_capacity(items.len());
        for item in items {
            let key = item.id().clone();
            let arc = Arc::new(item);
            self.inner.insert(key, arc.clone()).await;
            arcs.push(arc);
        }
        arcs
    }

    pub async fn cache_len(&self) -> u64 {
        self.inner.weighted_size()
    }

    pub async fn clear(&self) {
        self.inner.invalidate_all();
    }

    pub async fn is_cached(&self, id: &T::Id) -> bool {
        self.inner.contains_key(id)
    }
}

/// ====================== Ghost singletons ======================

pub struct SingleCache<T>
where
    T: Ghost + Clone + Send + Sync + 'static,
{
    inner: Cache<(), Arc<T>>,
}

impl<T> SingleCache<T>
where
    T: Ghost + Clone + Send + Sync + 'static,
{
    pub fn new(ttl: Option<Duration>) -> Self {
        let mut builder = Cache::builder().max_capacity(1);
        if let Some(d) = ttl {
            builder = builder.time_to_live(d);
        }
        Self {
            inner: builder.build(),
        }
    }

    /// `force = true` refetches from API and overwrites cache.
    pub async fn get(&self, force: bool) -> Result<Arc<T>, ApiError> {
        if force {
            let fresh: T = API.get_json(T::URL, &[]).await?;
            let arc = Arc::new(fresh);
            self.inner.insert((), arc.clone()).await;
            return Ok(arc);
        }

        let res: Result<Arc<T>, Arc<ApiError>> = self
            .inner
            .try_get_with((), async move {
                let v: T = API.get_json(T::URL, &[]).await?;
                Ok::<Arc<T>, ApiError>(Arc::new(v))
            })
            .await;

        match res {
            Ok(v) => Ok(v),
            Err(e) => match Arc::try_unwrap(e) {
                Ok(err) => Err(err),
                Err(shared) => Err(ApiError::Other(shared.to_string())),
            },
        }
    }

    pub async fn clear(&self) {
        self.inner.invalidate_all();
    }

    pub async fn is_cached(&self) -> bool {
        self.inner.contains_key(&())
    }
}

/// ====================== Macros (public API) ======================

/// For ID-based endpoints (GhostId).
/// Exposes: get(id, force), get_many(ids, force), get_page(page, size, force),
/// get_pages(start, stop, size, force), get_all(expanded, force), cache_clear(), cache_len().
#[macro_export]
macro_rules! cached_resource {
    ($ty:ty, $capacity:expr) => {
        $crate::cached_resource!(@impl $ty, $capacity, None::<std::time::Duration>);
    };
    ($ty:ty, $capacity:expr, ttl = $ttl:expr) => {
        $crate::cached_resource!(@impl $ty, $capacity, Some($ttl));
    };
    (@impl $ty:ty, $capacity:expr, $ttl:expr) => {
        impl $ty
        where
            Self: $crate::core::ghost_id::GhostId
                + $crate::core::ghost_id::Identifiable
                + Clone + Send + Sync + 'static,
            <Self as $crate::core::ghost_id::Identifiable>::Id: Clone
                + Eq + std::hash::Hash + std::fmt::Display + Send + Sync + 'static,
        {
            #[inline]
            fn __cache() -> &'static $crate::core::cache::TypeCache<Self> {
                use once_cell::sync::OnceCell;
                static CELL: OnceCell<$crate::core::cache::TypeCache::<$ty>> = OnceCell::new();
                CELL.get_or_init(|| $crate::core::cache::TypeCache::<Self>::new($capacity, $ttl))
            }

            pub async fn get(
                id: <Self as $crate::core::ghost_id::Identifiable>::Id,
                force: bool
            ) -> Result<std::sync::Arc<Self>, $crate::api::client::ApiError> {
                Self::__cache().get(id, force).await
            }

            pub async fn get_many(
                ids: &[<Self as $crate::core::ghost_id::Identifiable>::Id],
                force: bool
            ) -> Result<Vec<std::sync::Arc<Self>>, $crate::api::client::ApiError> {
                Self::__cache().get_many(ids, force).await
            }

            pub async fn get_page(
                page: usize,
                page_size: usize,
                force: bool
            ) -> Result<Vec<std::sync::Arc<Self>>, $crate::api::client::ApiError> {
                Self::__cache().get_page(page, page_size, force).await
            }

            pub async fn get_pages(
                start: usize,
                stop: usize,
                page_size: usize,
                force: bool
            ) -> Result<Vec<std::sync::Arc<Self>>, $crate::api::client::ApiError> {
                Self::__cache().get_pages(start, stop, page_size, force).await
            }

            pub async fn get_all(
                expanded: bool,
                force: bool
            ) -> Result<Vec<std::sync::Arc<Self>>, $crate::api::client::ApiError> {
                Self::__cache().get_all(expanded, force).await
            }

            pub async fn cache_clear() { Self::__cache().clear().await }
            pub async fn cache_len() -> u64 { Self::__cache().cache_len().await }
            pub async fn is_cached(
                id: &<Self as $crate::core::ghost_id::Identifiable>::Id
            ) -> bool {
                Self::__cache().is_cached(id).await
            }
        }
    };
}

/// For singleton endpoints (Ghost).
/// Exposes: get(force), cache_clear().
#[macro_export]
macro_rules! cached_singleton {
    ($ty:ty) => {
        $crate::cached_singleton!(@impl $ty, None::<std::time::Duration>);
    };
    ($ty:ty, ttl = $ttl:expr) => {
        $crate::cached_singleton!(@impl $ty, Some($ttl));
    };
    (@impl $ty:ty, $ttl:expr) => {
        mod __cache_single {
            use super::*;
            use once_cell::sync::Lazy;
            use crate::core::cache::SingleCache;
            pub static CACHE: Lazy<SingleCache<$ty>> =
                Lazy::new(|| SingleCache::<$ty>::new($ttl));
        }

        impl $ty
        where
            Self: $crate::core::ghost::Ghost + Clone + Send + Sync + 'static,
        {
            pub async fn get(force: bool) -> Result<std::sync::Arc<Self>, $crate::api::client::ApiError> {
                __cache_single::CACHE.get(force).await
            }
            pub async fn cache_clear() { __cache_single::CACHE.clear().await }
            pub async fn is_cached() -> bool { __cache_single::CACHE.is_cached().await }
        }
    };
}
