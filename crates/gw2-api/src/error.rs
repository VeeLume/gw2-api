//! Error types for the GW2 API client.

use reqwest::StatusCode;

/// Errors that can occur when interacting with the GW2 API.
#[derive(Debug, thiserror::Error)]
pub enum Gw2ApiError {
    /// HTTP 400: Bad request — often signals that `?ids=all` is not supported by this endpoint,
    /// triggering an automatic fallback to pagination in [`all()`][crate::client::Gw2Client::all].
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// HTTP 401 or 403: The API key is missing, invalid, or lacks required permissions.
    #[error("Authentication error: {0}")]
    AuthError(String),

    /// HTTP 404: The requested resource was not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// HTTP 429: Rate limited by the GW2 API server (after all retries were exhausted).
    #[error("Rate limited by API — all retries exhausted")]
    RateLimited,

    /// HTTP 5xx: Transient server error (after all retries were exhausted).
    #[error("Server error ({status}): {message}")]
    ServerError { status: u16, message: String },

    /// Any other non-success HTTP response.
    #[error("API error ({status}): {message}")]
    ApiError { status: u16, message: String },

    /// HTTP transport error (network failure, timeout, DNS, TLS, …).
    #[error("HTTP transport error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The provided API key was empty.
    #[error("Invalid API key: key must not be empty")]
    InvalidApiKey,

    /// All retry attempts were exhausted without a successful response.
    #[error("Max retries exceeded after {attempts} attempts")]
    MaxRetriesExceeded { attempts: u32 },

    /// Invalid pagination parameters.
    #[error("Invalid pagination parameters: {0}")]
    InvalidPagination(String),
}

impl Gw2ApiError {
    /// Create an appropriate error variant from a non-success HTTP status and response body.
    ///
    /// Parses GW2's `{"text": "..."}` JSON error body when present.
    pub(crate) fn from_status(status: StatusCode, body: String) -> Self {
        let message = parse_gw2_error_body(&body).unwrap_or(body);
        match status.as_u16() {
            400 => Self::BadRequest(message),
            401 | 403 => Self::AuthError(message),
            404 => Self::NotFound(message),
            429 => Self::RateLimited,
            s if s >= 500 => Self::ServerError { status: s, message },
            s => Self::ApiError { status: s, message },
        }
    }

    /// Returns `true` if this error is transient and worth retrying.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited | Self::ServerError { .. } | Self::Http(_)
        )
    }
}

/// Parse GW2's `{"text": "..."}` error body format.
fn parse_gw2_error_body(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("text")?.as_str().map(String::from)
}
