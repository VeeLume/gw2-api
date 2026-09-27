//! Error types for the GW2 API client.

use reqwest::StatusCode;

/// Errors that can occur when interacting with the GW2 API.
#[derive(Debug, thiserror::Error)]
pub enum Gw2ApiError {
    /// HTTP 400: Bad request — often signals that `?ids=all` is not supported by this endpoint,
    /// triggering an automatic fallback to pagination in `all()`.
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// HTTP 401, or a 403 that is not about a missing scope: the key is invalid,
    /// expired or revoked.
    #[error("Invalid API key: {0}")]
    InvalidToken(String),

    /// HTTP 403 `"requires scope X"`: the key is valid but lacks the named permission.
    ///
    /// Kept apart from [`InvalidToken`][Self::InvalidToken] so a UI can ask for one more
    /// scope instead of telling the user to replace a working key.
    #[error("API key is missing the `{0}` permission")]
    MissingPermission(String),

    /// HTTP 404: The requested resource was not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// HTTP 429: Rate limited by the GW2 API server (after all retries were exhausted).
    #[error("Rate limited by API — all retries exhausted")]
    RateLimited,

    /// HTTP 5xx: Server error. 502–504 are retried first; other 5xx are returned directly.
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

    /// Invalid pagination parameters.
    #[error("Invalid pagination parameters: {0}")]
    InvalidPagination(String),

    /// A transport or JSON error that reached several callers at once through the
    /// cache, flattened to its message because those errors cannot be cloned.
    #[error("{0}")]
    Other(String),
}

impl Gw2ApiError {
    /// Create an appropriate error variant from a non-success HTTP status and response body.
    ///
    /// Parses GW2's `{"text": "..."}` JSON error body when present.
    pub(crate) fn from_status(status: StatusCode, body: String) -> Self {
        let message = parse_gw2_error_body(&body).unwrap_or(body);
        match status.as_u16() {
            400 => Self::BadRequest(message),
            401 => Self::InvalidToken(message),
            403 => match message.strip_prefix("requires scope ") {
                Some(scope) => Self::MissingPermission(scope.trim().to_string()),
                None => Self::InvalidToken(message),
            },
            404 => Self::NotFound(message),
            429 => Self::RateLimited,
            s if s >= 500 => Self::ServerError { status: s, message },
            s => Self::ApiError { status: s, message },
        }
    }

    /// Returns `true` if this error is transient and worth retrying.
    ///
    /// 500 and 501 are not: on the GW2 API they are usually permanent for that
    /// request, so retrying only burns rate-limit budget.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::RateLimited | Self::Http(_) => true,
            Self::ServerError { status, .. } => matches!(status, 502..=504),
            _ => false,
        }
    }

    /// Rebuild an owned error from a shared one.
    ///
    /// The cache hands a failed fetch to every waiter as the *same* `Arc<Gw2ApiError>`.
    /// Every variant callers match on survives; only the two that genuinely
    /// cannot be cloned (transport and JSON errors) collapse to [`Other`][Self::Other].
    pub fn from_shared(e: &Gw2ApiError) -> Gw2ApiError {
        match e {
            Self::BadRequest(s) => Self::BadRequest(s.clone()),
            Self::InvalidToken(s) => Self::InvalidToken(s.clone()),
            Self::MissingPermission(s) => Self::MissingPermission(s.clone()),
            Self::NotFound(s) => Self::NotFound(s.clone()),
            Self::RateLimited => Self::RateLimited,
            Self::ServerError { status, message } => Self::ServerError {
                status: *status,
                message: message.clone(),
            },
            Self::ApiError { status, message } => Self::ApiError {
                status: *status,
                message: message.clone(),
            },
            Self::InvalidApiKey => Self::InvalidApiKey,
            Self::InvalidPagination(s) => Self::InvalidPagination(s.clone()),
            Self::Other(s) => Self::Other(s.clone()),
            other @ (Self::Http(_) | Self::Json(_)) => Self::Other(other.to_string()),
        }
    }
}

/// Parse GW2's `{"text": "..."}` error body format.
fn parse_gw2_error_body(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("text")?.as_str().map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(status: u16, text: &str) -> Gw2ApiError {
        Gw2ApiError::from_status(
            StatusCode::from_u16(status).unwrap(),
            format!(r#"{{"text":"{text}"}}"#),
        )
    }

    #[test]
    fn missing_scope_is_not_an_invalid_key() {
        assert!(matches!(err(403, "requires scope wallet"), Gw2ApiError::MissingPermission(s) if s == "wallet"));
        assert!(matches!(err(403, "Invalid access token"), Gw2ApiError::InvalidToken(_)));
        assert!(matches!(err(401, "Invalid access token"), Gw2ApiError::InvalidToken(_)));
    }

    #[test]
    fn only_gateway_errors_are_retryable() {
        assert!(err(502, "x").is_retryable());
        assert!(err(504, "x").is_retryable());
        assert!(!err(500, "x").is_retryable());
        assert!(err(429, "x").is_retryable());
        assert!(!err(404, "x").is_retryable());
    }

    #[test]
    fn from_shared_keeps_typed_variants() {
        assert!(matches!(Gw2ApiError::from_shared(&err(404, "no such id")), Gw2ApiError::NotFound(s) if s == "no such id"));
        let json = serde_json::from_str::<u32>("x").unwrap_err();
        assert!(matches!(Gw2ApiError::from_shared(&Gw2ApiError::Json(json)), Gw2ApiError::Other(_)));
    }
}
