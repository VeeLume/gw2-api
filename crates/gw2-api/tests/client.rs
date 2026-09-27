//! Client behaviour against a mock server: auth, error mapping, caching, retry.
//!
//! Every test builds its own cache and rate limiter, so tests never share state.

use std::sync::Arc;

use futures::TryStreamExt;
use gw2_api::{
    Authenticated, Gw2ApiError, Gw2Client, Language, PageOptions, RateLimiter, ResourceCache,
    Unauthenticated,
};
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn builder(server: &MockServer) -> gw2_api::client::ClientBuilder<Unauthenticated> {
    Gw2Client::builder()
        .base_url(format!("{}/v2", server.uri()))
        .cache_capacity(1_000)
        .rate_limiter(Arc::new(RateLimiter::new(300, 5)))
        .max_retries(0)
}

fn client(server: &MockServer) -> Gw2Client<Unauthenticated> {
    builder(server).build()
}

fn authed(server: &MockServer) -> Gw2Client<Authenticated> {
    client(server).authenticate("test-key").unwrap()
}

fn mini(id: u32, name: &str, item_id: u32) -> serde_json::Value {
    json!({ "id": id, "name": name, "icon": "i", "order": 1, "item_id": item_id })
}

fn currency(id: u32) -> serde_json::Value {
    json!({ "id": id, "name": format!("c{id}"), "description": "", "icon": "i", "order": id })
}

async fn request_count(server: &MockServer) -> usize {
    server.received_requests().await.unwrap_or_default().len()
}

#[tokio::test]
async fn sends_bearer_key_language_and_schema() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/tokeninfo"))
        .and(header("authorization", "Bearer test-key"))
        .and(query_param("lang", "en"))
        .and(query_param("v", gw2_api::DEFAULT_SCHEMA_VERSION))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "abc", "name": "key", "permissions": ["account", "wallet", "homestead"], "type": "APIKey"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let info = authed(&server).tokeninfo().get().await.unwrap();
    assert_eq!(info.name, "key");
    assert_eq!(info.permissions.len(), 3);
}

#[tokio::test]
async fn missing_scope_and_invalid_key_are_distinct() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/account/wallet"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({ "text": "requires scope wallet" })))
        .mount(&server)
        .await;
    Mock::given(path("/v2/account"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({ "text": "Invalid access token" })))
        .mount(&server)
        .await;

    let client = authed(&server);
    let err = client.account().wallet().get().await.unwrap_err();
    assert!(matches!(err, Gw2ApiError::MissingPermission(ref s) if s == "wallet"), "{err:?}");
    let err = client.account().get().await.unwrap_err();
    assert!(matches!(err, Gw2ApiError::InvalidToken(_)), "{err:?}");
}

#[tokio::test]
async fn language_is_part_of_the_cache_key() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/minis/3"))
        .and(query_param("lang", "en"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mini(3, "Mini Rytlock", 1)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/v2/minis/3"))
        .and(query_param("lang", "de"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mini(3, "Mini-Rytlock", 1)))
        .expect(1)
        .mount(&server)
        .await;

    let en = client(&server);
    let de = en.with_language(Language::De);
    assert_eq!(en.minis().get(3).await.unwrap().name, "Mini Rytlock");
    assert_eq!(de.minis().get(3).await.unwrap().name, "Mini-Rytlock");
    assert_eq!(en.minis().get(3).await.unwrap().name, "Mini Rytlock");
    assert_eq!(request_count(&server).await, 2);
}

#[tokio::test]
async fn cache_hits_return_patched_values() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/minis/747"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mini(747, "((208738))", 6)))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);
    for _ in 0..2 {
        let m = client.minis().get(747).await.unwrap();
        assert_eq!(m.name, "Mini Shrine Guardian");
        assert_eq!(m.unlock_item.0, 90009);
    }
}

#[tokio::test]
async fn get_many_dedups_keeps_order_and_caches() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/currencies"))
        .and(query_param("ids", "2,1,999"))
        .respond_with(ResponseTemplate::new(206).set_body_json(json!([currency(1), currency(2)])))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);
    let ids = [2, 1, 2, 999].map(Into::into);
    let got: Vec<u32> = client.currencies().get_many(ids.clone()).await.unwrap().iter().map(|c| c.id.0).collect();
    assert_eq!(got, [2, 1, 2]);

    // Served from cache: no further requests.
    let again: Vec<u32> = client.currencies().get_many([2, 1].map(Into::into)).await.unwrap().iter().map(|c| c.id.0).collect();
    assert_eq!(again, [2, 1]);
    assert_eq!(client.currencies().get(1).await.unwrap().name, "c1");
    assert_eq!(request_count(&server).await, 1);
}

#[tokio::test]
async fn not_found_is_typed_and_not_cached() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/currencies/404"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "text": "no such id" })))
        .expect(2)
        .mount(&server)
        .await;

    let client = client(&server);
    for _ in 0..2 {
        let err = client.currencies().get(404).await.unwrap_err();
        assert!(matches!(err, Gw2ApiError::NotFound(ref s) if s == "no such id"), "{err:?}");
    }
}

#[tokio::test]
async fn retries_429_and_penalizes_the_shared_limiter() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/currencies/1"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/v2/currencies/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(currency(1)))
        .expect(1)
        .mount(&server)
        .await;

    let limiter = Arc::new(RateLimiter::new(300, 5));
    let client = builder(&server).rate_limiter(limiter.clone()).max_retries(1).build();
    assert_eq!(client.currencies().get(1).await.unwrap().name, "c1");
    assert!(limiter.modelled_remaining() < 10.0, "limiter was not penalized");
}

#[tokio::test]
async fn internal_server_error_is_not_retried() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/currencies/1"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&server)
        .await;

    let client = builder(&server).max_retries(3).build();
    let err = client.currencies().get(1).await.unwrap_err();
    assert!(matches!(err, Gw2ApiError::ServerError { status: 500, .. }), "{err:?}");
}

#[tokio::test]
async fn pagination_stops_at_the_400_past_the_end() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/currencies"))
        .and(query_param("page", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([currency(1), currency(2)])))
        .mount(&server)
        .await;
    Mock::given(path("/v2/currencies"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({ "text": "page out of range" })))
        .mount(&server)
        .await;

    let client = client(&server);
    let all: Vec<_> = client.currencies().pages(PageOptions::default().page_size(2)).try_collect().await.unwrap();
    assert_eq!(all.len(), 2);

    // A 400 on the first page is a real error.
    let err = client
        .currencies()
        .pages(PageOptions::default().range(1, 5).page_size(2))
        .try_collect::<Vec<_>>()
        .await
        .unwrap_err();
    assert!(matches!(err, Gw2ApiError::BadRequest(_)), "{err:?}");
}

#[test]
fn authenticate_shares_limiter_and_cache() {
    let public = Gw2Client::builder()
        .cache(Arc::new(ResourceCache::new(10, None)))
        .rate_limiter(Arc::new(RateLimiter::new(10, 1)))
        .build();
    let account = public.authenticate("key").unwrap();
    assert!(Arc::ptr_eq(public.rate_limiter(), account.rate_limiter()));
    assert!(Arc::ptr_eq(public.cache(), account.cache()));
    assert!(Arc::ptr_eq(public.cache(), account.unauthenticated().cache()));
    assert!(matches!(public.authenticate(""), Err(Gw2ApiError::InvalidApiKey)));

    // By default every client shares the process-wide limiter and cache.
    let a = Gw2Client::new();
    let b = Gw2Client::builder().language(Language::Fr).build();
    assert!(Arc::ptr_eq(a.rate_limiter(), b.rate_limiter()));
    assert!(Arc::ptr_eq(a.cache(), b.cache()));
}

fn transactions(from: u64, n: u64) -> serde_json::Value {
    (from..from + n)
        .map(|id| json!({ "id": id, "item_id": 19976, "price": 100, "quantity": 1, "created": "2026-09-27T10:00:00+00:00" }))
        .collect()
}

#[tokio::test]
async fn transaction_lists_fetch_every_page() {
    let server = MockServer::start().await;
    let path_ = "/v2/commerce/transactions/history/sells";
    Mock::given(path(path_))
        .and(query_param("page", "0"))
        .and(query_param("page_size", "200"))
        .respond_with(ResponseTemplate::new(200).set_body_json(transactions(0, 200)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path(path_))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(transactions(200, 3)))
        .expect(1)
        .mount(&server)
        .await;

    let all = authed(&server).commerce().transactions().history().sells().get().await.unwrap();
    assert_eq!(all.len(), 203);
    assert_eq!(all.last().unwrap().id, 202);
}

#[tokio::test]
async fn transaction_paging_stops_at_the_400_past_the_end() {
    let server = MockServer::start().await;
    let path_ = "/v2/commerce/transactions/current/buys";
    Mock::given(path(path_))
        .and(query_param("page", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(transactions(0, 200)))
        .mount(&server)
        .await;
    Mock::given(path(path_))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({ "text": "page out of range" })))
        .mount(&server)
        .await;

    let all = authed(&server).commerce().transactions().current().buys().get().await.unwrap();
    assert_eq!(all.len(), 200);
}

#[tokio::test]
async fn exchange_results_name_what_they_count() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/commerce/exchange/gems"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "coins_per_gem": 2569, "quantity": 256997 })))
        .mount(&server)
        .await;
    Mock::given(path("/v2/commerce/exchange/coins"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "coins_per_gem": 3831, "quantity": 261 })))
        .mount(&server)
        .await;

    let client = client(&server);
    let sold = client.commerce().exchange().gems(100).await.unwrap();
    assert_eq!(sold.coins, gw2_api::Coin(256997));
    let bought = client.commerce().exchange().coins(1_000_000).await.unwrap();
    assert_eq!(bought.gems, 261);
}
