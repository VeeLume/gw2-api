use once_cell::sync::Lazy;
use reqwest::{
    Url,
    header::{ACCEPT_LANGUAGE, AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde::de::DeserializeOwned;
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Invalid token: {0}")]
    InvalidToken(String),
    #[error("Not found")]
    NotFound,
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error(transparent)]
    Net(#[from] reqwest::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

#[derive(Clone, Debug)]
pub enum Language {
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

#[derive(Clone, Copy)]
struct Bucket {
    remaining: f64,
    last_update: Instant,
}

pub struct ApiClient {
    base: Url,
    http: reqwest::Client, // plain Client, no lock
    language: tokio::sync::RwLock<Language>,
    api_key: tokio::sync::RwLock<Option<String>>,
    schema_version: tokio::sync::RwLock<String>,
    bucket: tokio::sync::Mutex<Bucket>,
    trottling_sleep_secs: u64,
    max_retries: u32,
    requests_per_minute: f64,
}

impl ApiClient {
    pub fn new() -> Result<Self, ApiError> {
        Ok(Self {
            base: Url::parse("https://api.guildwars2.com/v2/")
                .map_err(|e| ApiError::Other(e.to_string()))?,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()?, // no default headers here
            language: tokio::sync::RwLock::new(Language::En),
            api_key: tokio::sync::RwLock::new(None),
            schema_version: tokio::sync::RwLock::new("2024-06-08T00:00:00Z".into()),
            bucket: tokio::sync::Mutex::new(Bucket {
                remaining: 300.0,
                last_update: Instant::now(),
            }),
            trottling_sleep_secs: 2,
            max_retries: 5,
            requests_per_minute: 300.0,
        })
    }

    fn make_headers(
        &self,
        lang: &Language,
        schema_version: &str,
        api_key: &Option<String>,
    ) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(ACCEPT_LANGUAGE, HeaderValue::from_static(lang.as_str()));
        h.insert(
            "X-Schema-Version",
            HeaderValue::from_str(schema_version).unwrap(),
        );
        if let Some(k) = api_key {
            let mut v = HeaderValue::from_str(&format!("Bearer {}", k)).unwrap();
            v.set_sensitive(true);
            h.insert(AUTHORIZATION, v);
        }
        h
    }

    async fn throttle(&self) {
        loop {
            // keep the lock very short: read-modify-release
            {
                let mut b = self.bucket.lock().await;
                let elapsed = b.last_update.elapsed().as_secs_f64();
                b.last_update = Instant::now();
                b.remaining += elapsed * (self.requests_per_minute / 60.0);
                if b.remaining > self.requests_per_minute {
                    b.remaining = self.requests_per_minute;
                }
                if b.remaining >= 1.0 {
                    b.remaining -= 1.0;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_secs(self.trottling_sleep_secs)).await;
        }
    }

    pub async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        qp: &[(&str, String)],
    ) -> Result<T, ApiError> {
        let url = self
            .base
            .clone()
            .join(path)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
        let (lang, sv, key) = {
            let lang = self.language.read().await.clone();
            let sv = self.schema_version.read().await.clone();
            let key = self.api_key.read().await.clone();
            (lang, sv, key)
        };
        let headers = self.make_headers(&lang, &sv, &key);

        let mut last_err = None;
        for attempt in 0..=self.max_retries {
            self.throttle().await;

            let resp = self
                .http
                .get(url.clone())
                .headers(headers.clone())
                .query(qp)
                .send()
                .await;
            match resp {
                Ok(r) => {
                    let status = r.status();
                    let body = r.text().await?;
                    if status.is_success() {
                        return Ok(serde_json::from_str::<T>(&body)?);
                    }
                    match status.as_u16() {
                        400 => return Err(ApiError::BadRequest(body)),
                        401 | 403 => return Err(ApiError::InvalidToken(body)),
                        404 => return Err(ApiError::NotFound),
                        429 | 502 | 504 => {
                            tokio::time::sleep(Duration::from_secs(
                                self.trottling_sleep_secs.saturating_pow(attempt.max(1)),
                            ))
                            .await;
                            continue;
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
                    tokio::time::sleep(Duration::from_secs(
                        self.trottling_sleep_secs.saturating_pow(attempt.max(1)),
                    ))
                    .await;
                }
            }
        }
        Err(last_err.unwrap_or_else(|| ApiError::Other("Max retries exceeded".into())))
    }

    // Settings setters don’t touch the Client now.
    pub async fn set_language(&self, lang: Language) {
        *self.language.write().await = lang;
    }
    pub async fn set_api_key(&self, key: Option<String>) {
        *self.api_key.write().await = key;
    }
    pub async fn set_schema_version(&self, v: String) {
        *self.schema_version.write().await = v;
    }
}

pub static API: Lazy<std::sync::Arc<ApiClient>> =
    Lazy::new(|| std::sync::Arc::new(ApiClient::new().expect("valid client")));
