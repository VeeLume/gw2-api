//! Builder for [`Gw2Client`] using the typestate pattern.
//!
//! Calling `.api_key()` on a `ClientBuilder<Unauthenticated>` produces a
//! `ClientBuilder<Authenticated>`, enforcing at compile time that only authenticated
//! builders produce authenticated clients.

use std::marker::PhantomData;
use std::sync::Arc;

use reqwest::Client;

use super::auth::{Authenticated, AuthState, Unauthenticated};
use super::{ClientState, Gw2Client, Language, BASE_URL, DEFAULT_SCHEMA_VERSION, USER_AGENT};
use crate::cache::ResourceCache;
use crate::error::Gw2ApiError;
use crate::rate_limit::{build_rate_limiter, GW2_BURST_SIZE, GW2_REFILL_RATE_PER_SECOND};

/// Builder for [`Gw2Client`].
///
/// Obtain one via [`Gw2Client::builder()`].
pub struct ClientBuilder<S: AuthState> {
    api_key: Option<String>,
    lang: Language,
    cache_capacity: u64,
    /// Steady-state refill rate in requests per second.
    rate_refill_per_second: u32,
    /// Maximum burst size (number of requests that can be sent back-to-back).
    rate_burst: u32,
    schema_version: Option<String>,
    _auth: PhantomData<S>,
}

impl ClientBuilder<Unauthenticated> {
    pub(super) fn new() -> Self {
        Self {
            api_key: None,
            lang: Language::default(),
            cache_capacity: 4_096,
            rate_refill_per_second: GW2_REFILL_RATE_PER_SECOND,
            rate_burst: GW2_BURST_SIZE,
            schema_version: Some(DEFAULT_SCHEMA_VERSION.to_string()),
            _auth: PhantomData,
        }
    }

    /// Provide an API key, upgrading to an authenticated builder.
    pub fn api_key(self, key: impl Into<String>) -> ClientBuilder<Authenticated> {
        ClientBuilder {
            api_key: Some(key.into()),
            lang: self.lang,
            cache_capacity: self.cache_capacity,
            rate_refill_per_second: self.rate_refill_per_second,
            rate_burst: self.rate_burst,
            schema_version: self.schema_version,
            _auth: PhantomData,
        }
    }

    /// Build an unauthenticated client.
    pub fn build(self) -> Gw2Client<Unauthenticated> {
        build_client(self.lang, None, self.cache_capacity, self.rate_refill_per_second, self.rate_burst, self.schema_version)
    }
}

impl ClientBuilder<Authenticated> {
    /// Build an authenticated client.
    ///
    /// Returns an error if the API key is empty.
    pub fn build(self) -> Result<Gw2Client<Authenticated>, Gw2ApiError> {
        let key = self.api_key.unwrap();
        if key.is_empty() {
            return Err(Gw2ApiError::InvalidApiKey);
        }
        Ok(build_client(self.lang, Some(key), self.cache_capacity, self.rate_refill_per_second, self.rate_burst, self.schema_version))
    }
}

// Shared config methods available on all builder states
macro_rules! impl_builder_shared {
    ($($S:ty),+) => {
        $(impl ClientBuilder<$S> {
            /// Set the language for API responses.
            pub fn language(mut self, lang: Language) -> Self {
                self.lang = lang;
                self
            }

            /// Set the cache capacity (number of entries). Use `0` to disable caching.
            pub fn cache_capacity(mut self, capacity: u64) -> Self {
                self.cache_capacity = capacity;
                self
            }

            /// Override the client-side rate limiter parameters.
            ///
            /// `refill_per_second` is the steady-state refill rate; `burst` is how many
            /// requests can be sent back-to-back before throttling begins.
            ///
            /// Defaults match the GW2 API: 5 req/s refill, 300 burst.
            pub fn rate_limit(mut self, refill_per_second: u32, burst: u32) -> Self {
                self.rate_refill_per_second = refill_per_second;
                self.rate_burst = burst;
                self
            }

            /// Set the GW2 API schema version string (ISO 8601 date or `"latest"`).
            pub fn schema_version(mut self, version: impl Into<String>) -> Self {
                self.schema_version = Some(version.into());
                self
            }
        })+
    };
}

impl_builder_shared!(Unauthenticated, Authenticated);

fn build_client<S: AuthState>(
    lang: Language,
    api_key: Option<String>,
    cache_capacity: u64,
    rate_refill_per_second: u32,
    rate_burst: u32,
    schema_version: Option<String>,
) -> Gw2Client<S> {
    let http = Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("failed to build HTTP client");

    let state = ClientState {
        http,
        base_url: BASE_URL.to_string(),
        lang,
        api_key,
        cache: ResourceCache::new(cache_capacity),
        rate_limiter: Arc::new(build_rate_limiter(rate_refill_per_second, rate_burst)),
        schema_version,
    };

    Gw2Client {
        inner: Arc::new(state),
        _auth: PhantomData,
    }
}
