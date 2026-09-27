//! `auth` on an ID resource: only reachable from an authenticated client, and its
//! values are account data, so they must not enter the shared static cache.

use std::sync::Arc;

use gw2_api_macros::gw2_endpoint;
use serde::{Deserialize, Serialize};
use serde_json::json;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::{Gw2Client, RateLimiter, ResourceCache};

/// Stand-in for `/v2/characters`.
#[gw2_endpoint(path = "testchars", id_type = String, auth, no_registry)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestChar {
    pub id: TestCharId,
    pub level: u32,
}

#[tokio::test]
async fn auth_resources_bypass_the_static_cache() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/testchars/Hero"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "Hero", "level": 80 })))
        .expect(2)
        .mount(&server)
        .await;

    let cache = Arc::new(ResourceCache::new(100, None));
    let client = Gw2Client::builder()
        .base_url(format!("{}/v2", server.uri()))
        .cache(cache.clone())
        .rate_limiter(Arc::new(RateLimiter::new(300, 5)))
        .api_key("key")
        .build()
        .unwrap();

    // `client.testchars()` only exists on Gw2Client<Authenticated>.
    for _ in 0..2 {
        let c = client.testchars().get("Hero".to_string()).await.unwrap();
        assert_eq!(c.level, 80);
    }
    assert_eq!(cache.entry_count().await, 0);
}
