//! Internal HTTP request building and execution.
//!
//! All HTTP calls go through [`Gw2Client::execute`], which:
//! 1. Acquires a rate-limiter token
//! 2. Sends the request
//! 3. On 429 or 5xx: applies exponential backoff and retries (up to 5 times)
//! 4. On success: returns the raw response

use std::fmt;
use std::time::Duration;

use reqwest::RequestBuilder as ReqwestBuilder;
use serde::de::DeserializeOwned;
use tokio::time::sleep;
use tracing::warn;

use super::{AuthState, Gw2Client};
use crate::error::Gw2ApiError;

const MAX_RETRIES: u32 = 5;

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

        all_params.extend(self.params.into_iter().map(|(k, v)| (k, v)));

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

impl<S: AuthState> Gw2Client<S> {
    /// Execute a reqwest request with rate limiting and exponential-backoff retry.
    pub(crate) async fn execute<T: DeserializeOwned>(
        &self,
        req: ReqwestBuilder,
    ) -> Result<T, Gw2ApiError> {
        for attempt in 0..MAX_RETRIES {
            // Acquire a rate-limiter token before each attempt
            self.inner.rate_limiter.until_ready().await;

            // Clone the request so we can retry it
            let req_clone = req
                .try_clone()
                .expect("GW2 API requests are always GET with no body — try_clone must succeed");

            let response = req_clone.send().await?;
            let status = response.status();

            if status.is_success() {
                return Ok(response.json::<T>().await?);
            }

            let body = response.text().await.unwrap_or_default();
            let err = Gw2ApiError::from_status(status, body);

            if !err.is_retryable() || attempt == MAX_RETRIES - 1 {
                return Err(err);
            }

            let wait = Duration::from_secs(2u64.pow(attempt));
            warn!(
                attempt = attempt + 1,
                wait_secs = wait.as_secs(),
                "Retryable API error, backing off",
            );
            sleep(wait).await;
        }

        Err(Gw2ApiError::MaxRetriesExceeded {
            attempts: MAX_RETRIES,
        })
    }

    /// Create a [`RequestBuilder`] for the given path.
    pub(crate) fn request(&self, path: impl Into<String>) -> RequestBuilder<'_, S> {
        RequestBuilder::new(self, path)
    }
}
