pub mod auth;
mod builder;
mod request;

use std::marker::PhantomData;
use std::sync::Arc;

use reqwest::Client;

pub use builder::ClientBuilder;

use crate::cache::ResourceCache;
use crate::error::Gw2ApiError;
use crate::rate_limit::RateLimiter;
use auth::{AuthState, Authenticated, Unauthenticated};

pub(crate) const BASE_URL: &str = "https://api.guildwars2.com/v2";
pub(crate) const USER_AGENT: &str = concat!("gw2-api/", env!("CARGO_PKG_VERSION"));

/// Default GW2 API schema version sent with every request.
///
/// Pinned to the latest known schema at crate development time. This ensures
/// stable behaviour out of the box while allowing callers to opt into newer
/// schemas via [`ClientBuilder::schema_version`].
///
/// Source: <https://wiki.guildwars2.com/wiki/API:2>
pub const DEFAULT_SCHEMA_VERSION: &str = "2025-08-29T01:00:00.000Z";

// ── Internal state ──────────────────────────────────────────────────────────

/// Shared client state behind an `Arc`. Cheap to clone.
#[derive(Clone)]
pub(crate) struct ClientState {
    pub(crate) http: Client,
    /// Without trailing slash; request paths start with `/`.
    pub(crate) base_url: String,
    pub(crate) lang: Language,
    pub(crate) api_key: Option<String>,
    pub(crate) schema_version: Option<String>,
    pub(crate) max_retries: u32,
    pub(crate) cache: Arc<ResourceCache>,
    /// The part of every cache key that depends on this client: base URL,
    /// language and schema version.
    pub(crate) cache_scope: Arc<str>,
    pub(crate) rate_limiter: Arc<RateLimiter>,
}

impl ClientState {
    pub(crate) fn compute_scope(&mut self) {
        self.cache_scope = format!(
            "{}|{}|{}",
            self.base_url,
            self.lang.as_str(),
            self.schema_version.as_deref().unwrap_or("")
        )
        .into();
    }
}

// ── Public client ───────────────────────────────────────────────────────────

/// GW2 API v2 client with type-level authentication state.
///
/// `Gw2Client<Unauthenticated>` can only access public endpoints.
/// `Gw2Client<Authenticated>` can also access account-bound endpoints.
///
/// The client is `Clone` with O(1) cost — all state is behind an `Arc`.
///
/// By default every client in the process shares one rate limiter
/// ([`rate_limit::global`][crate::rate_limit::global]) and one static-data cache
/// ([`ResourceCache::global`]), because the GW2 rate limit is per IP address and
/// static data is the same for every key.
///
/// # Building
///
/// ```rust,ignore
/// // Unauthenticated
/// let client = Gw2Client::new();
///
/// // Authenticated, sharing the public client's limiter and cache
/// let account_client = client.authenticate("your-key-here")?;
///
/// // Custom settings
/// let client = Gw2Client::builder()
///     .language(Language::De)
///     .api_key("your-key-here")
///     .build()?;
/// ```
#[derive(Clone)]
pub struct Gw2Client<S: AuthState = Unauthenticated> {
    pub(crate) inner: Arc<ClientState>,
    pub(crate) _auth: PhantomData<S>,
}

// ── Language ────────────────────────────────────────────────────────────────

/// Supported GW2 API response languages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Language {
    #[default]
    En,
    De,
    Es,
    Fr,
    Zh,
}

impl Language {
    /// All supported languages.
    pub const ALL: [Language; 5] = [Self::En, Self::De, Self::Es, Self::Fr, Self::Zh];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::De => "de",
            Self::Es => "es",
            Self::Fr => "fr",
            Self::Zh => "zh",
        }
    }
}

// ── Constructors ────────────────────────────────────────────────────────────

impl Gw2Client<Unauthenticated> {
    /// Create an unauthenticated client with default settings.
    pub fn new() -> Self {
        Self::builder().build()
    }

    /// Create a [`ClientBuilder`] for fine-grained configuration.
    ///
    /// The builder always starts as [`Unauthenticated`] — call
    /// [`.api_key()`][ClientBuilder::api_key] to get an authenticated builder.
    pub fn builder() -> ClientBuilder<Unauthenticated> {
        ClientBuilder::new()
    }

    /// An authenticated client with the same settings, sharing this client's
    /// HTTP connection pool, rate limiter and static-data cache.
    ///
    /// Returns [`Gw2ApiError::InvalidApiKey`] if the key is empty. The key is not
    /// checked against the API; call `tokeninfo()` for that.
    pub fn authenticate(&self, key: impl Into<String>) -> Result<Gw2Client<Authenticated>, Gw2ApiError> {
        let key = key.into();
        if key.is_empty() {
            return Err(Gw2ApiError::InvalidApiKey);
        }
        let mut state = (*self.inner).clone();
        state.api_key = Some(key);
        Ok(Gw2Client::from_state(state))
    }
}

impl Gw2Client<Authenticated> {
    /// A public client with the same settings and shared limiter and cache, but no key.
    pub fn unauthenticated(&self) -> Gw2Client<Unauthenticated> {
        let mut state = (*self.inner).clone();
        state.api_key = None;
        Gw2Client::from_state(state)
    }
}

impl<S: AuthState> Gw2Client<S> {
    pub(crate) fn from_state(state: ClientState) -> Self {
        Self {
            inner: Arc::new(state),
            _auth: PhantomData,
        }
    }

    /// The language responses are requested in.
    pub fn language(&self) -> Language {
        self.inner.lang
    }

    /// The schema version sent with every request, if any.
    pub fn schema_version(&self) -> Option<&str> {
        self.inner.schema_version.as_deref()
    }

    /// The rate limiter this client draws from.
    pub fn rate_limiter(&self) -> &Arc<RateLimiter> {
        &self.inner.rate_limiter
    }

    /// The static-data cache this client reads from and fills.
    pub fn cache(&self) -> &Arc<ResourceCache> {
        &self.inner.cache
    }

    /// The same client in another language, sharing limiter and cache.
    pub fn with_language(&self, lang: Language) -> Self {
        let mut state = (*self.inner).clone();
        state.lang = lang;
        state.compute_scope();
        Self::from_state(state)
    }

    /// The same client pinned to another schema version (an ISO 8601 date or
    /// `"latest"`), sharing limiter and cache.
    pub fn with_schema_version(&self, version: impl Into<String>) -> Self {
        let mut state = (*self.inner).clone();
        state.schema_version = Some(version.into());
        state.compute_scope();
        Self::from_state(state)
    }
}

impl Default for Gw2Client<Unauthenticated> {
    fn default() -> Self {
        Self::new()
    }
}
