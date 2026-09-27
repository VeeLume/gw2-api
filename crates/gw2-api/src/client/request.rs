//! Internal HTTP request building and execution.
//!
//! All HTTP calls go through [`Gw2Client::execute`], which:
//! 1. Acquires a rate-limiter token
//! 2. Sends the request
//! 3. On 429, 502–504 or a network error: backs off with jitter and retries
//! 4. On success: deserializes the body

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::RequestBuilder as ReqwestBuilder;
use serde::de::DeserializeOwned;
use tokio::time::sleep;
use tracing::warn;

use super::{AuthState, Gw2Client};
use crate::error::Gw2ApiError;

// ── Internal request builder ────────────────────────────────────────────────

/// Internal fluent builder for GW2 API requests.
///
/// Not part of the public API — endpoint methods use it internally.
pub(crate) struct RequestBuilder<'c, S: AuthState> {
    client: &'c Gw2Client<S>,
    path: String,
    params: Vec<(&'static str, String)>,
}

impl<'c, S: AuthState> RequestBuilder<'c, S> {
    pub(crate) fn new(client: &'c Gw2Client<S>, path: impl Into<String>) -> Self {
        Self {
            client,
            path: path.into(),
            params: Vec::new(),
        }
    }

    /// Add a query parameter. The value can be anything that implements `Display`.
    pub(crate) fn param(mut self, key: &'static str, value: impl fmt::Display) -> Self {
        self.params.push((key, value.to_string()));
        self
    }

    /// Add an `ids` query parameter from an iterator of displayable values.
    pub(crate) fn ids(mut self, ids: impl IntoIterator<Item = impl fmt::Display>) -> Self {
        let ids_str = ids
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        self.params.push(("ids", ids_str));
        self
    }

    /// Add `page` and `page_size` query parameters.
    pub(crate) fn page(mut self, page: u32, page_size: u32) -> Self {
        self.params.push(("page", page.to_string()));
        self.params.push(("page_size", page_size.to_string()));
        self
    }

    /// Execute the request and deserialize the response as `T`.
    pub(crate) async fn send<T: DeserializeOwned>(self) -> Result<T, Gw2ApiError> {
        let url = format!("{}{}", self.client.inner.base_url, self.path);

        // Language is always included
        let mut all_params: Vec<(&str, String)> =
            vec![("lang", self.client.inner.lang.as_str().to_string())];

        // Schema version if set
        if let Some(v) = &self.client.inner.schema_version {
            all_params.push(("v", v.clone()));
        }

        all_params.extend(self.params.into_iter());

        let req = self.client.inner.http.get(&url).query(&all_params);

        let req = if let Some(key) = &self.client.inner.api_key {
            req.bearer_auth(key)
        } else {
            req
        };

        self.client.execute(req).await
    }
}

// ── Core execution with retry ───────────────────────────────────────────────

/// Which backoff schedule a retryable failure uses.
#[derive(Clone, Copy)]
enum Backoff {
    /// 429: the whole process is over budget, so start at 1 s.
    RateLimited,
    /// 502–504 or a network error: give the server longer, starting at 2 s.
    Gateway,
}

impl<S: AuthState> Gw2Client<S> {
    /// Execute a reqwest request with rate limiting and jittered exponential-backoff retry.
    ///
    /// Retries 429 (after [penalizing][crate::rate_limit::RateLimiter::penalize] the shared
    /// limiter), 502–504 and transport errors, up to `max_retries` times. Everything else
    /// is returned immediately.
    pub(crate) async fn execute<T: DeserializeOwned>(
        &self,
        req: ReqwestBuilder,
    ) -> Result<T, Gw2ApiError> {
        let max_retries = self.inner.max_retries;
        let mut attempt = 0;
        loop {
            // Acquire a rate-limiter token before each attempt, retries included
            self.inner.rate_limiter.acquire().await;

            let req_clone = req
                .try_clone()
                .expect("GW2 API requests are always GET with no body — try_clone must succeed");

            let err = match Self::attempt(req_clone).await {
                Ok(body) => return Ok(serde_json::from_str::<T>(&body)?),
                Err(e) => e,
            };

            let rate_limited = matches!(err, Gw2ApiError::RateLimited);
            if rate_limited {
                // Every client sharing this limiter backs off, not just this request.
                self.inner.rate_limiter.penalize();
            }
            if !err.is_retryable() || attempt >= max_retries {
                return Err(err);
            }
            let schedule = if rate_limited {
                Backoff::RateLimited
            } else {
                Backoff::Gateway
            };

            let wait = backoff(schedule, attempt);
            warn!(
                attempt = attempt + 1,
                wait_ms = wait.as_millis() as u64,
                error = %err,
                "Retryable API error, backing off",
            );
            sleep(wait).await;
            attempt += 1;
        }
    }

    /// One HTTP round trip: the body on success, a typed error otherwise.
    async fn attempt(req: ReqwestBuilder) -> Result<String, Gw2ApiError> {
        let response = req.send().await?;
        let status = response.status();
        if status.is_success() {
            return Ok(response.text().await?);
        }
        let body = response.text().await.unwrap_or_default();
        Err(Gw2ApiError::from_status(status, body))
    }

    /// Create a [`RequestBuilder`] for the given path.
    pub(crate) fn request(&self, path: impl Into<String>) -> RequestBuilder<'_, S> {
        RequestBuilder::new(self, path)
    }
}

/// `base * 2^attempt`, capped at 30 s, half of it fixed and half random.
///
/// The random half keeps two processes that hit a 429 in the same second from
/// retrying in lockstep and colliding again.
fn backoff(schedule: Backoff, attempt: u32) -> Duration {
    let base_ms: u64 = match schedule {
        Backoff::RateLimited => 1_000,
        Backoff::Gateway => 2_000,
    };
    let exp = (base_ms << attempt.min(10)).min(30_000);
    let half = exp / 2;
    Duration::from_millis(half + (half as f64 * jitter_frac()) as u64)
}

/// A value in `[0, 1)` from the clock's sub-second nanos. Good enough to spread
/// retries; avoids a `rand` dependency.
fn jitter_frac() -> f64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    f64::from(nanos % 1_000_000) / 1_000_000.0
}
