// core/cache.rs
//
// Caches for **static game data** only: values that are identical for every
// account and change only when the game patches. That is why they are keyed by
// `(Language, Id)` and shared process-wide.
//
// The language is part of the key deliberately. Before it was, fetching a mini
// in English and then switching the client to German returned the English name
// forever — the cache had no idea the request had changed.
//
// Account data does not belong here. It is per-key, it changes every session,
// and it lives on `Gw2Session` instead.

use moka::future::Cache;
use std::{fmt, hash::Hash, sync::Arc, time::Duration};

use crate::api::client::{ApiClient, ApiError, Language};
use crate::core::ghost::Ghost;
use crate::core::ghost_id::{GhostId, Identifiable};

fn unwrap_cache_err(e: Arc<ApiError>) -> ApiError {
    Arc::try_unwrap(e).unwrap_or_else(|shared| ApiError::from_shared(&shared))
}

// ====================== GhostId caches (ID-based) ======================

pub struct TypeCache<T>
where
    T: GhostId + Identifiable + Clone + Send + Sync + 'static,
    T::Id: Clone + Eq + Hash + fmt::Display + Send + Sync + 'static,
{
    inner: Cache<(Language, T::Id), Arc<T>>,
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

    /// Get one. `force = true` bypasses the cache and overwrites it.
    pub async fn get(
        &self,
        client: &ApiClient,
        id: T::Id,
        force: bool,
    ) -> Result<Arc<T>, ApiError> {
        let lang = client.language();

        if force {
            let fresh: T = client.get_json(&T::url_for_id(&id), &[]).await?;
            let arc = Arc::new(fresh);
            self.inner.insert((lang, id), arc.clone()).await;
            return Ok(arc);
        }

        let fetch_id = id.clone();
        self.inner
            .try_get_with((lang, id), async move {
                let v: T = client.get_json(&T::url_for_id(&fetch_id), &[]).await?;
                Ok::<Arc<T>, ApiError>(Arc::new(v))
            })
            .await
            .map_err(unwrap_cache_err)
    }

    /// Get many. If `force`, refetch all given ids; otherwise cache-first, then
    /// one batched `?ids=` call for the misses.
    ///
    /// Ids the API does not know are simply absent from the result, so the
    /// returned vector can be shorter than `ids`.
    pub async fn get_many(
        &self,
        client: &ApiClient,
        ids: &[T::Id],
        force: bool,
    ) -> Result<Vec<Arc<T>>, ApiError> {
        use std::collections::{HashMap, HashSet};

        let lang = client.language();

        // Dedup for the network work, but answer in the caller's order.
        let mut uniq = Vec::new();
        let mut seen = HashSet::new();
        for id in ids.iter().cloned() {
            if seen.insert(id.clone()) {
                uniq.push(id);
            }
        }

        let mut by_id: HashMap<T::Id, Arc<T>> = HashMap::new();
        let mut to_fetch = Vec::new();

        if force {
            to_fetch = uniq;
        } else {
            for id in uniq {
                match self.inner.get(&(lang, id.clone())).await {
                    Some(v) => {
                        by_id.insert(id, v);
                    }
                    None => to_fetch.push(id),
                }
            }
        }

        if !to_fetch.is_empty() {
            let joined = to_fetch
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let fetched: Vec<T> = client.get_json(T::URL, &[("ids", joined)]).await?;
            for arc in self.put_many(lang, fetched).await {
                by_id.insert(arc.id().clone(), arc);
            }
        }

        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(v) = by_id.get(id) {
                out.push(v.clone());
            }
        }
        Ok(out)
    }

    /// Get one page (always fetches, then populates the cache).
    pub async fn get_page(
        &self,
        client: &ApiClient,
        page: usize,
        page_size: usize,
    ) -> Result<Vec<Arc<T>>, ApiError> {
        let lang = client.language();
        let items: Vec<T> = client
            .get_json(
                T::URL,
                &[
                    ("page", page.to_string()),
                    ("page_size", page_size.to_string()),
                ],
            )
            .await?;
        Ok(self.put_many(lang, items).await)
    }

    /// Get a page range (always fetches, then populates the cache).
    pub async fn get_pages(
        &self,
        client: &ApiClient,
        start: usize,
        stop: usize,
        page_size: usize,
    ) -> Result<Vec<Arc<T>>, ApiError> {
        use futures::stream::{FuturesUnordered, StreamExt};

        let lang = client.language();
        let mut futs = FuturesUnordered::new();
        for p in start..stop {
            futs.push(async move {
                client
                    .get_json::<Vec<T>>(
                        T::URL,
                        &[
                            ("page", p.to_string()),
                            ("page_size", page_size.to_string()),
                        ],
                    )
                    .await
            });
        }
        let mut collected = Vec::new();
        while let Some(batch) = futs.next().await {
            collected.extend(batch?);
        }
        Ok(self.put_many(lang, collected).await)
    }

    /// Get everything (expanded if the endpoint supports `ids=all`).
    ///
    /// This is the expensive one — on a large endpoint it is hundreds of
    /// requests, which is most of a minute's budget. Anything that calls it
    /// wants the persistent store in front of it.
    pub async fn get_all(
        &self,
        client: &ApiClient,
        expanded: bool,
    ) -> Result<Vec<Arc<T>>, ApiError> {
        let lang = client.language();
        let items: Vec<T> = if expanded {
            match client
                .get_json::<Vec<T>>(T::URL, &[("ids", "all".into())])
                .await
            {
                Ok(v) => v,
                Err(ApiError::BadRequest(_)) => self.paginate_all(client).await?,
                Err(e) => return Err(e),
            }
        } else {
            self.paginate_all(client).await?
        };
        Ok(self.put_many(lang, items).await)
    }

    async fn paginate_all(&self, client: &ApiClient) -> Result<Vec<T>, ApiError> {
        const PAGE_SIZE: usize = 200;
        let mut out = Vec::new();
        let mut page = 0usize;
        loop {
            // The API never answers an empty page: one past the end is a 400
            // ("page out of range"), so that is the end marker, not an error.
            let batch: Vec<T> = match client
                .get_json(
                    T::URL,
                    &[
                        ("page", page.to_string()),
                        ("page_size", PAGE_SIZE.to_string()),
                    ],
                )
                .await
            {
                Ok(b) => b,
                Err(ApiError::BadRequest(_)) if page > 0 => break,
                Err(e) => return Err(e),
            };
            let short = batch.len() < PAGE_SIZE;
            out.extend(batch);
            if short {
                break;
            }
            page += 1;
        }
        Ok(out)
    }

    async fn put_many(&self, lang: Language, items: Vec<T>) -> Vec<Arc<T>> {
        let mut arcs = Vec::with_capacity(items.len());
        for item in items {
            let key = item.id().clone();
            let arc = Arc::new(item);
            self.inner.insert((lang, key), arc.clone()).await;
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

    pub async fn is_cached(&self, lang: Language, id: &T::Id) -> bool {
        self.inner.contains_key(&(lang, id.clone()))
    }
}

// ====================== Ghost singletons ======================

pub struct SingleCache<T>
where
    T: Ghost + Clone + Send + Sync + 'static,
{
    inner: Cache<Language, Arc<T>>,
}

impl<T> SingleCache<T>
where
    T: Ghost + Clone + Send + Sync + 'static,
{
    pub fn new(ttl: Option<Duration>) -> Self {
        let mut builder = Cache::builder().max_capacity(8);
        if let Some(d) = ttl {
            builder = builder.time_to_live(d);
        }
        Self {
            inner: builder.build(),
        }
    }

    pub async fn get(&self, client: &ApiClient, force: bool) -> Result<Arc<T>, ApiError> {
        let lang = client.language();

        if force {
            let fresh: T = client.get_json(T::URL, &[]).await?;
            let arc = Arc::new(fresh);
            self.inner.insert(lang, arc.clone()).await;
            return Ok(arc);
        }

        self.inner
            .try_get_with(lang, async move {
                let v: T = client.get_json(T::URL, &[]).await?;
                Ok::<Arc<T>, ApiError>(Arc::new(v))
            })
            .await
            .map_err(unwrap_cache_err)
    }

    pub async fn clear(&self) {
        self.inner.invalidate_all();
    }

    pub async fn is_cached(&self, lang: Language) -> bool {
        self.inner.contains_key(&lang)
    }
}

// ====================== Macros (public API) ======================

/// For ID-based endpoints (`GhostId`).
///
/// Generates a process-wide cache plus two flavours of every fetcher: the plain
/// one, which uses the static unauthenticated client, and a `_with` twin taking
/// an explicit `&ApiClient`. The twins are what make the crate testable against
/// a mock server.
#[macro_export]
macro_rules! cached_resource {
    ($ty:ty, $capacity:expr) => {
        $crate::cached_resource!(@impl $ty, $capacity, None::<::std::time::Duration>);
    };
    ($ty:ty, $capacity:expr, ttl = $ttl:expr) => {
        $crate::cached_resource!(@impl $ty, $capacity, Some($ttl));
    };
    (@impl $ty:ty, $capacity:expr, $ttl:expr) => {
        impl $ty {
            #[inline]
            fn __cache() -> &'static $crate::core::cache::TypeCache<$ty> {
                static CELL: ::std::sync::OnceLock<$crate::core::cache::TypeCache<$ty>> =
                    ::std::sync::OnceLock::new();
                CELL.get_or_init(|| {
                    $crate::core::cache::TypeCache::<$ty>::new($capacity, $ttl)
                })
            }

            pub async fn get(
                id: <Self as $crate::core::ghost_id::Identifiable>::Id,
                force: bool,
            ) -> ::std::result::Result<::std::sync::Arc<Self>, $crate::api::client::ApiError> {
                Self::get_with($crate::api::client::static_client(), id, force).await
            }

            pub async fn get_with(
                client: &$crate::api::client::ApiClient,
                id: <Self as $crate::core::ghost_id::Identifiable>::Id,
                force: bool,
            ) -> ::std::result::Result<::std::sync::Arc<Self>, $crate::api::client::ApiError> {
                Self::__cache().get(client, id, force).await
            }

            pub async fn get_many(
                ids: &[<Self as $crate::core::ghost_id::Identifiable>::Id],
                force: bool,
            ) -> ::std::result::Result<Vec<::std::sync::Arc<Self>>, $crate::api::client::ApiError> {
                Self::get_many_with($crate::api::client::static_client(), ids, force).await
            }

            pub async fn get_many_with(
                client: &$crate::api::client::ApiClient,
                ids: &[<Self as $crate::core::ghost_id::Identifiable>::Id],
                force: bool,
            ) -> ::std::result::Result<Vec<::std::sync::Arc<Self>>, $crate::api::client::ApiError> {
                Self::__cache().get_many(client, ids, force).await
            }

            pub async fn get_page(
                page: usize,
                page_size: usize,
            ) -> ::std::result::Result<Vec<::std::sync::Arc<Self>>, $crate::api::client::ApiError> {
                Self::get_page_with($crate::api::client::static_client(), page, page_size).await
            }

            pub async fn get_page_with(
                client: &$crate::api::client::ApiClient,
                page: usize,
                page_size: usize,
            ) -> ::std::result::Result<Vec<::std::sync::Arc<Self>>, $crate::api::client::ApiError> {
                Self::__cache().get_page(client, page, page_size).await
            }

            pub async fn get_pages(
                start: usize,
                stop: usize,
                page_size: usize,
            ) -> ::std::result::Result<Vec<::std::sync::Arc<Self>>, $crate::api::client::ApiError> {
                Self::get_pages_with(
                    $crate::api::client::static_client(),
                    start,
                    stop,
                    page_size,
                )
                .await
            }

            pub async fn get_pages_with(
                client: &$crate::api::client::ApiClient,
                start: usize,
                stop: usize,
                page_size: usize,
            ) -> ::std::result::Result<Vec<::std::sync::Arc<Self>>, $crate::api::client::ApiError> {
                Self::__cache().get_pages(client, start, stop, page_size).await
            }

            pub async fn get_all(
                expanded: bool,
            ) -> ::std::result::Result<Vec<::std::sync::Arc<Self>>, $crate::api::client::ApiError> {
                Self::get_all_with($crate::api::client::static_client(), expanded).await
            }

            pub async fn get_all_with(
                client: &$crate::api::client::ApiClient,
                expanded: bool,
            ) -> ::std::result::Result<Vec<::std::sync::Arc<Self>>, $crate::api::client::ApiError> {
                Self::__cache().get_all(client, expanded).await
            }

            pub async fn cache_clear() {
                Self::__cache().clear().await
            }

            pub async fn cache_len() -> u64 {
                Self::__cache().cache_len().await
            }

            /// Is this id cached for the static client's current language?
            pub async fn is_cached(
                id: &<Self as $crate::core::ghost_id::Identifiable>::Id,
            ) -> bool {
                let lang = $crate::api::client::static_client().language();
                Self::__cache().is_cached(lang, id).await
            }

            pub async fn is_cached_in(
                lang: $crate::api::client::Language,
                id: &<Self as $crate::core::ghost_id::Identifiable>::Id,
            ) -> bool {
                Self::__cache().is_cached(lang, id).await
            }
        }
    };
}

/// For singleton endpoints (`Ghost`).
///
/// Uses a scoped `OnceLock` inside the accessor rather than a named module, so
/// two singletons can live in the same module — the previous version generated
/// `mod __cache_single` and collided.
#[macro_export]
macro_rules! cached_singleton {
    ($ty:ty) => {
        $crate::cached_singleton!(@impl $ty, None::<::std::time::Duration>);
    };
    ($ty:ty, ttl = $ttl:expr) => {
        $crate::cached_singleton!(@impl $ty, Some($ttl));
    };
    (@impl $ty:ty, $ttl:expr) => {
        impl $ty {
            #[inline]
            fn __single_cache() -> &'static $crate::core::cache::SingleCache<$ty> {
                static CELL: ::std::sync::OnceLock<$crate::core::cache::SingleCache<$ty>> =
                    ::std::sync::OnceLock::new();
                CELL.get_or_init(|| $crate::core::cache::SingleCache::<$ty>::new($ttl))
            }

            pub async fn get(
                force: bool,
            ) -> ::std::result::Result<::std::sync::Arc<Self>, $crate::api::client::ApiError> {
                Self::get_with($crate::api::client::static_client(), force).await
            }

            pub async fn get_with(
                client: &$crate::api::client::ApiClient,
                force: bool,
            ) -> ::std::result::Result<::std::sync::Arc<Self>, $crate::api::client::ApiError> {
                Self::__single_cache().get(client, force).await
            }

            pub async fn cache_clear() {
                Self::__single_cache().clear().await
            }

            pub async fn is_cached() -> bool {
                let lang = $crate::api::client::static_client().language();
                Self::__single_cache().is_cached(lang).await
            }
        }
    };
}
