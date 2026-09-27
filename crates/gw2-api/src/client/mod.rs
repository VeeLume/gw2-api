pub mod auth;
mod builder;
mod request;

use std::marker::PhantomData;
use std::sync::Arc;

use reqwest::Client;

pub use builder::ClientBuilder;

use auth::{AuthState, Unauthenticated};
use crate::cache::ResourceCache;
use crate::rate_limit::GovRateLimiter;

pub(crate) const BASE_URL: &str = "https://api.guildwars2.com/v2";
pub(crate) const USER_AGENT: &str = concat!("gw2-companion/", env!("CARGO_PKG_VERSION"));

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
pub(crate) struct ClientState {
    pub(crate) http: Client,
    pub(crate) base_url: String,
    pub(crate) lang: Language,
    pub(crate) api_key: Option<String>,
    pub(crate) cache: ResourceCache,
    pub(crate) rate_limiter: Arc<GovRateLimiter>,
    pub(crate) schema_version: Option<String>,
}

// ── Public client ───────────────────────────────────────────────────────────

/// GW2 API v2 client with type-level authentication state.
///
/// `Gw2Client<Unauthenticated>` can only access public endpoints.
/// `Gw2Client<Authenticated>` can also access account-bound endpoints.
///
/// The client is `Clone` with O(1) cost — all state is behind an `Arc`.
///
/// # Building
///
/// ```rust,ignore
/// // Unauthenticated
/// let client = Gw2Client::new();
///
/// // Authenticated, with custom settings
/// let client = Gw2Client::builder()
///     .api_key("your-key-here")
///     .language(Language::De)
///     .cache_capacity(10_000)
///     .build()?;
/// ```
#[derive(Clone)]
pub struct Gw2Client<S: AuthState = Unauthenticated> {
    pub(crate) inner: Arc<ClientState>,
    pub(crate) _auth: PhantomData<S>,
}

// ── Language ────────────────────────────────────────────────────────────────

/// Supported GW2 API response languages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    En,
    De,
    Es,
    Fr,
    Zh,
}

impl Language {
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
}

impl Default for Gw2Client<Unauthenticated> {
    fn default() -> Self {
        Self::new()
    }
}
