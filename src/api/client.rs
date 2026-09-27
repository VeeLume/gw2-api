// api/client.rs

use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::{
    Url,
    header::{ACCEPT_LANGUAGE, AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::api::limiter::{self, RateLimiter};

pub const DEFAULT_BASE: &str = "https://api.guildwars2.com/v2/";
pub const DEFAULT_SCHEMA_VERSION: &str = "2025-08-29T01:00:00.000Z";

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Invalid token: {0}")]
    InvalidToken(String),
    #[error("Token is missing the `{0}` permission")]
    MissingPermission(String),
    #[error("Not found")]
    NotFound,
    #[error("Rate limited (429) after {attempts} attempts")]
    RateLimited { attempts: u32 },
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error(transparent)]
    Net(#[from] reqwest::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

impl ApiError {
    /// Rebuild an owned error from a shared one.
    ///
    /// moka hands cache-miss failures to every waiter as the *same*
    /// `Arc<ApiError>`, so `Arc::try_unwrap` normally fails and the typed
    /// variant would be flattened into `Other` — callers would see
    /// `Other("Not found")` instead of `NotFound`. Transport and
    /// deserialization errors genuinely cannot be cloned, so those two collapse
    /// to `Other`; every variant callers actually match on survives.
    pub fn from_shared(e: &ApiError) -> ApiError {
        match e {
            ApiError::BadRequest(s) => ApiError::BadRequest(s.clone()),
            ApiError::InvalidToken(s) => ApiError::InvalidToken(s.clone()),
            ApiError::MissingPermission(s) => ApiError::MissingPermission(s.clone()),
            ApiError::NotFound => ApiError::NotFound,
            ApiError::RateLimited { attempts } => ApiError::RateLimited {
                attempts: *attempts,
            },
            ApiError::Http { status, body } => ApiError::Http {
                status: *status,
                body: body.clone(),
            },
            ApiError::Other(s) => ApiError::Other(s.clone()),
            other => ApiError::Other(other.to_string()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Language {
    #[default]
    En,
    Es,
    De,
    Fr,
    Zh,
}

impl Language {
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Es => "es",
            Language::De => "de",
            Language::Fr => "fr",
            Language::Zh => "zh",
        }
    }
}

/// Which kind of transient failure we are backing off from. 429 and 5xx want
/// different base delays: the bucket refills at 5/sec, so a rate-limit stall is
/// short, while a gateway error is the upstream's problem and worth more time.
#[derive(Clone, Copy)]
enum Transient {
    RateLimited,
    Gateway,
}

pub struct ApiClient {
    base: Url,
    http: reqwest::Client,
    /// Default language for requests. Mutable at runtime — safe now that the
    /// static caches include the language in their key.
    language: RwLock<Language>,
    /// Fixed at construction. An authenticated client is a *different client*
    /// from an unauthenticated one, which is what keeps per-key caches honest.
    api_key: Option<String>,
    /// Fixed at construction, for the same reason as `api_key`.
    schema_version: String,
    limiter: Arc<RateLimiter>,
    max_retries: u32,
}

impl ApiClient {
    pub fn builder() -> ApiClientBuilder {
        ApiClientBuilder::default()
    }

    /// Unauthenticated client against the live API with the process-wide limiter.
    pub fn new() -> Result<Self, ApiError> {
        Self::builder().build()
    }

    pub fn language(&self) -> Language {
        *self.language.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_language(&self, lang: Language) {
        *self.language.write().unwrap_or_else(|e| e.into_inner()) = lang;
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    pub fn limiter(&self) -> &Arc<RateLimiter> {
        &self.limiter
    }

    fn make_headers(&self, lang: Language) -> Result<HeaderMap, ApiError> {
        let mut h = HeaderMap::new();
        h.insert(ACCEPT_LANGUAGE, HeaderValue::from_static(lang.as_str()));
        h.insert(
            "X-Schema-Version",
            HeaderValue::from_str(&self.schema_version)
                .map_err(|e| ApiError::Other(format!("invalid schema version: {e}")))?,
        );
        if let Some(k) = &self.api_key {
            let mut v = HeaderValue::from_str(&format!("Bearer {k}"))
                .map_err(|_| ApiError::InvalidToken("api key is not a valid header".into()))?;
            v.set_sensitive(true);
            h.insert(AUTHORIZATION, v);
        }
        Ok(h)
    }

    /// GET a JSON document, honouring the shared rate budget and retrying
    /// transient failures with jittered exponential backoff.
    pub async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        qp: &[(&str, String)],
    ) -> Result<T, ApiError> {
        let url = self
            .base
            .join(path)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
        let headers = self.make_headers(self.language())?;

        let mut last_err: Option<ApiError> = None;
        for attempt in 0..=self.max_retries {
            self.limiter.acquire().await;

            match self
                .http
                .get(url.clone())
                .headers(headers.clone())
                .query(qp)
                .send()
                .await
            {
                Ok(r) => {
                    let status = r.status();
                    let body = r.text().await?;
                    if status.is_success() {
                        return Ok(serde_json::from_str::<T>(&body)?);
                    }
                    match status.as_u16() {
                        400 => return Err(ApiError::BadRequest(body)),
                        401 => return Err(ApiError::InvalidToken(body)),
                        403 => return Err(forbidden(body)),
                        404 => return Err(ApiError::NotFound),
                        429 => {
                            // The server just proved our bucket model wrong.
                            // Zero it so every caller in this process backs off,
                            // not only this one.
                            self.limiter.penalize();
                            last_err = Some(ApiError::RateLimited {
                                attempts: attempt + 1,
                            });
                            tokio::time::sleep(backoff(attempt, Transient::RateLimited)).await;
                        }
                        502..=504 => {
                            last_err = Some(ApiError::Http {
                                status: status.as_u16(),
                                body,
                            });
                            tokio::time::sleep(backoff(attempt, Transient::Gateway)).await;
                        }
                        _ => {
                            return Err(ApiError::Http {
                                status: status.as_u16(),
                                body,
                            });
                        }
                    }
                }
                Err(e) => {
                    last_err = Some(ApiError::Net(e));
                    tokio::time::sleep(backoff(attempt, Transient::Gateway)).await;
                }
            }
        }
        Err(last_err.unwrap_or_else(|| ApiError::Other("Max retries exceeded".into())))
    }
}

/// Split a 403 into "valid key, missing scope" and "bad key".
///
/// The API answers a key that lacks a scope with
/// `{"text": "requires scope wallet"}`. Reporting that as `InvalidToken` would
/// send the user off to replace a key that is fine and only needs one more box
/// ticked.
fn forbidden(body: String) -> ApiError {
    #[derive(serde::Deserialize)]
    struct ErrorText {
        text: String,
    }
    let scope = serde_json::from_str::<ErrorText>(&body)
        .ok()
        .and_then(|e| e.text.strip_prefix("requires scope ").map(str::to_owned));
    match scope {
        Some(s) => ApiError::MissingPermission(s),
        None => ApiError::InvalidToken(body),
    }
}

/// Half-fixed, half-random backoff.
///
/// The jitter is the load-bearing part: without it, two of our own apps that
/// get 429'd in the same second retry in lockstep and collide again.
fn backoff(attempt: u32, kind: Transient) -> Duration {
    const CAP_MS: u64 = 30_000;
    let base_ms: u64 = match kind {
        Transient::RateLimited => 1_000,
        Transient::Gateway => 2_000,
    };
    let exp = base_ms.saturating_mul(1u64 << attempt.min(10)).min(CAP_MS);
    let half = exp / 2;
    Duration::from_millis(half + (half as f64 * jitter_frac()) as u64)
}

/// Cheap pseudo-random fraction in `[0, 1)` from the clock's sub-millisecond
/// noise. Not a PRNG, and deliberately not worth a `rand` dependency — it only
/// needs to de-correlate two processes that happen to fail at the same moment.
fn jitter_frac() -> f64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 1_000_000) as f64 / 1_000_000.0
}

pub struct ApiClientBuilder {
    base: String,
    language: Language,
    api_key: Option<String>,
    schema_version: String,
    limiter: Option<Arc<RateLimiter>>,
    max_retries: u32,
    timeout: Duration,
}

impl Default for ApiClientBuilder {
    fn default() -> Self {
        Self {
            base: DEFAULT_BASE.to_string(),
            language: Language::En,
            api_key: None,
            schema_version: DEFAULT_SCHEMA_VERSION.to_string(),
            limiter: None,
            max_retries: 5,
            timeout: Duration::from_secs(20),
        }
    }
}

impl ApiClientBuilder {
    /// Point the client at a different origin. The only reason this exists is
    /// testability: without it the crate can only be exercised against the live
    /// API.
    pub fn base(mut self, base: impl Into<String>) -> Self {
        self.base = base.into();
        self
    }

    pub fn language(mut self, lang: Language) -> Self {
        self.language = lang;
        self
    }

    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn schema_version(mut self, v: impl Into<String>) -> Self {
        self.schema_version = v.into();
        self
    }

    /// Share a limiter with other clients, or hand this client a partition of
    /// the budget. Defaults to the process-wide limiter.
    pub fn limiter(mut self, limiter: Arc<RateLimiter>) -> Self {
        self.limiter = Some(limiter);
        self
    }

    pub fn max_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }

    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }

    pub fn build(self) -> Result<ApiClient, ApiError> {
        // A trailing slash matters: `Url::join` replaces the last segment
        // without it, which would silently drop the `/v2` prefix.
        let base = if self.base.ends_with('/') {
            self.base
        } else {
            format!("{}/", self.base)
        };
        Ok(ApiClient {
            base: Url::parse(&base).map_err(|e| ApiError::Other(e.to_string()))?,
            http: reqwest::Client::builder().timeout(self.timeout).build()?,
            language: RwLock::new(self.language),
            api_key: self.api_key,
            schema_version: self.schema_version,
            limiter: self.limiter.unwrap_or_else(limiter::global),
            max_retries: self.max_retries,
        })
    }
}

static STATIC_API: LazyLock<ApiClient> =
    LazyLock::new(|| ApiClient::new().expect("default client config is valid"));

/// The process-wide, **unauthenticated** client used for static game data.
///
/// Static data is identical for every account, so one client and one cache per
/// process is correct rather than a compromise. Account data does not come from
/// here — see [`Gw2Session`](crate::api::session::Gw2Session).
pub fn static_client() -> &'static ApiClient {
    &STATIC_API
}
