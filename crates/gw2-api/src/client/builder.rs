//! Builder for [`Gw2Client`] using the typestate pattern.
//!
//! Calling `.api_key()` on a `ClientBuilder<Unauthenticated>` produces a
//! `ClientBuilder<Authenticated>`, enforcing at compile time that only authenticated
//! builders produce authenticated clients.

use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;

use super::auth::{AuthState, Authenticated, Unauthenticated};
use super::{BASE_URL, ClientState, DEFAULT_SCHEMA_VERSION, Gw2Client, Language, USER_AGENT};
use crate::cache::ResourceCache;
use crate::error::Gw2ApiError;
use crate::rate_limit::{self, RateLimiter};

const DEFAULT_MAX_RETRIES: u32 = 5;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

/// Builder for [`Gw2Client`].
///
/// Obtain one via [`Gw2Client::builder()`].
pub struct ClientBuilder<S: AuthState> {
    api_key: Option<String>,
    lang: Language,
    base_url: String,
    schema_version: Option<String>,
    max_retries: u32,
    timeout: Duration,
    cache: Option<Arc<ResourceCache>>,
    rate_limiter: Option<Arc<RateLimiter>>,
    _auth: PhantomData<S>,
}

impl ClientBuilder<Unauthenticated> {
    pub(super) fn new() -> Self {
        Self {
            api_key: None,
            lang: Language::default(),
            base_url: BASE_URL.to_string(),
            schema_version: Some(DEFAULT_SCHEMA_VERSION.to_string()),
            max_retries: DEFAULT_MAX_RETRIES,
            timeout: DEFAULT_TIMEOUT,
            cache: None,
            rate_limiter: None,
            _auth: PhantomData,
        }
    }

    /// Provide an API key, upgrading to an authenticated builder.
    pub fn api_key(self, key: impl Into<String>) -> ClientBuilder<Authenticated> {
        ClientBuilder {
            api_key: Some(key.into()),
            lang: self.lang,
            base_url: self.base_url,
            schema_version: self.schema_version,
            max_retries: self.max_retries,
            timeout: self.timeout,
            cache: self.cache,
            rate_limiter: self.rate_limiter,
            _auth: PhantomData,
        }
    }

    /// Build an unauthenticated client.
    pub fn build(self) -> Gw2Client<Unauthenticated> {
        self.build_client()
    }
}

impl ClientBuilder<Authenticated> {
    /// Build an authenticated client.
    ///
    /// Returns an error if the API key is empty.
    pub fn build(self) -> Result<Gw2Client<Authenticated>, Gw2ApiError> {
        if self.api_key.as_deref().is_none_or(str::is_empty) {
            return Err(Gw2ApiError::InvalidApiKey);
        }
        Ok(self.build_client())
    }
}

impl<S: AuthState> ClientBuilder<S> {
    /// Set the language for API responses.
    pub fn language(mut self, lang: Language) -> Self {
        self.lang = lang;
        self
    }

    /// Set the GW2 API schema version string (ISO 8601 date or `"latest"`).
    pub fn schema_version(mut self, version: impl Into<String>) -> Self {
        self.schema_version = Some(version.into());
        self
    }

    /// Override the API base URL, e.g. to point at a mock server in tests.
    /// Default: `https://api.guildwars2.com/v2`.
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into().trim_end_matches('/').to_string();
        self
    }

    /// How often a request is retried after a 429, a 502–504 or a network error.
    /// Default: 5.
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Per-request timeout. Default: 20 s.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Use this static-data cache instead of the process-wide [`ResourceCache::global`].
    pub fn cache(mut self, cache: Arc<ResourceCache>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Give this client its own cache with the given capacity instead of sharing the
    /// process-wide one. Use `0` to disable caching.
    pub fn cache_capacity(mut self, capacity: u64) -> Self {
        self.cache = Some(Arc::new(ResourceCache::new(
            capacity,
            Some(crate::cache::DEFAULT_TTL),
        )));
        self
    }

    /// Use this rate limiter instead of the process-wide [`rate_limit::global`] —
    /// e.g. a [`RateLimiter::gw2_share`] partition.
    pub fn rate_limiter(mut self, limiter: Arc<RateLimiter>) -> Self {
        self.rate_limiter = Some(limiter);
        self
    }

    /// Give this client its own rate limiter with the given parameters instead of
    /// sharing the process-wide one.
    ///
    /// `refill_per_second` is the steady-state refill rate; `burst` is how many
    /// requests can be sent back-to-back before throttling begins.
    pub fn rate_limit(self, refill_per_second: u32, burst: u32) -> Self {
        self.rate_limiter(Arc::new(RateLimiter::new(burst, refill_per_second)))
    }

    fn build_client<T: AuthState>(self) -> Gw2Client<T> {
        let http = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(self.timeout)
            .build()
            .expect("failed to build HTTP client");

        let mut state = ClientState {
            http,
            base_url: self.base_url,
            lang: self.lang,
            api_key: self.api_key,
            schema_version: self.schema_version,
            max_retries: self.max_retries,
            cache: self.cache.unwrap_or_else(ResourceCache::global),
            cache_scope: Arc::from(""),
            rate_limiter: self.rate_limiter.unwrap_or_else(rate_limit::global),
        };
        state.compute_scope();
        Gw2Client::from_state(state)
    }
}
