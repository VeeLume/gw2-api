// api/session.rs
//
// The account-data half of the static/account split.
//
// A session owns one API key and, eventually, its own caches. That ownership is
// the whole point: account data is per-key and changes every play session, so
// it must never share the process-wide caches that static game data uses. Two
// sessions for two keys are two independent worlds.
//
// It does share the rate limiter, though — the limit is per IP, so every
// session and the static client draw from the same budget.

use std::sync::Arc;

use crate::api::client::{ApiClient, ApiError, Language};
use crate::api::limiter::RateLimiter;
use crate::models::tokeninfo::{Permission, TokenInfo};

/// An authenticated handle for one API key.
///
/// Cheap to build and cheap to drop, which is what makes "test this key the
/// user just pasted" a throwaway session rather than a mutation of global state.
pub struct Gw2Session {
    client: Arc<ApiClient>,
}

impl Gw2Session {
    /// Build a session against the live API, sharing the process-wide limiter.
    pub fn new(api_key: impl Into<String>) -> Result<Self, ApiError> {
        Ok(Self {
            client: Arc::new(ApiClient::builder().api_key(api_key).build()?),
        })
    }

    /// Build a session with an explicit language.
    pub fn with_language(api_key: impl Into<String>, lang: Language) -> Result<Self, ApiError> {
        Ok(Self {
            client: Arc::new(
                ApiClient::builder()
                    .api_key(api_key)
                    .language(lang)
                    .build()?,
            ),
        })
    }

    /// Build a session with an explicit budget partition — e.g. a Stream Deck
    /// plugin that should not be able to starve a companion app.
    pub fn with_limiter(
        api_key: impl Into<String>,
        limiter: Arc<RateLimiter>,
    ) -> Result<Self, ApiError> {
        Ok(Self {
            client: Arc::new(
                ApiClient::builder()
                    .api_key(api_key)
                    .limiter(limiter)
                    .build()?,
            ),
        })
    }

    /// Adopt a fully configured client. Used by tests to point a session at a
    /// mock server.
    pub fn from_client(client: Arc<ApiClient>) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &ApiClient {
        &self.client
    }

    pub fn client_arc(&self) -> Arc<ApiClient> {
        self.client.clone()
    }

    /// Who this key belongs to and what it may do.
    ///
    /// Deliberately uncached: it is cheap, it is the connection test, and a
    /// stale answer is worse than a request.
    pub async fn tokeninfo(&self) -> Result<TokenInfo, ApiError> {
        self.client.get_json::<TokenInfo>("tokeninfo", &[]).await
    }

    /// Fetch the token's scopes and fail with a typed error if `perm` is absent.
    pub async fn require_permission(&self, perm: &Permission) -> Result<TokenInfo, ApiError> {
        let info = self.tokeninfo().await?;
        info.require(perm)?;
        Ok(info)
    }
}
