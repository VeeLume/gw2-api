// tests/static_cache.rs
//
// These tests define their own models through the public macros, which means
// they double as a macro-hygiene check: everything the macros generate has to
// resolve from *outside* the crate, with no `once_cell` in the consumer's
// dependency list.
//
// One model per test — the caches the macros generate are process-wide statics,
// so two tests sharing a model would share its cache and race.

use gw2_api::api::client::{ApiClient, ApiError, Language};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client_for(server: &MockServer, lang: Language) -> ApiClient {
    ApiClient::builder()
        .base(format!("{}/v2", server.uri()))
        .language(lang)
        .max_retries(0)
        .build()
        .expect("test client builds")
}

// ---------------------------------------------------------------- language

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LangItem {
    id: i32,
    name: String,
}
gw2_api::ghost_id!(LangItem, "langitems", i32);
gw2_api::cached_resource!(LangItem, 128);

#[tokio::test]
async fn cache_is_keyed_by_language() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/langitems/1"))
        .and(header("accept-language", "en"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1, "name": "Mini Shrine Guardian"
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v2/langitems/1"))
        .and(header("accept-language", "de"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1, "name": "Mini-Schreinwächter"
        })))
        .mount(&server)
        .await;

    let en = client_for(&server, Language::En);
    let de = client_for(&server, Language::De);

    let first = LangItem::get_with(&en, 1, false).await.unwrap();
    assert_eq!(first.name, "Mini Shrine Guardian");

    // The regression this whole key change exists for: before the language was
    // part of the key, this returned the English name from cache.
    let second = LangItem::get_with(&de, 1, false).await.unwrap();
    assert_eq!(second.name, "Mini-Schreinwächter");

    // And the English entry is still cached under its own key.
    assert!(LangItem::is_cached_in(Language::En, &1).await);
    assert!(LangItem::is_cached_in(Language::De, &1).await);

    let again = LangItem::get_with(&en, 1, false).await.unwrap();
    assert_eq!(again.name, "Mini Shrine Guardian");

    // Two requests total, not three: the last call was a cache hit.
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

// ---------------------------------------------------------------- get_many

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BulkItem {
    id: i32,
    name: String,
}
gw2_api::ghost_id!(BulkItem, "bulkitems", i32);
gw2_api::cached_resource!(BulkItem, 128);

#[tokio::test]
async fn get_many_dedups_requests_and_preserves_caller_order() {
    let server = MockServer::start().await;

    // Deduped, first-seen order — and 999 is a real id as far as we know until
    // the API declines to return it.
    Mock::given(method("GET"))
        .and(path("/v2/bulkitems"))
        .and(query_param("ids", "2,1,999"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 1, "name": "one"},
            {"id": 2, "name": "two"}
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server, Language::En);
    let got = BulkItem::get_many_with(&client, &[2, 1, 2, 999], false)
        .await
        .unwrap();

    // Caller order is preserved, duplicates are echoed, and unknown ids are
    // simply absent rather than an error.
    let ids: Vec<i32> = got.iter().map(|i| i.id).collect();
    assert_eq!(ids, vec![2, 1, 2]);

    // A second call is fully served from cache for the two known ids.
    let cached = BulkItem::get_many_with(&client, &[1, 2], false)
        .await
        .unwrap();
    assert_eq!(cached.len(), 2);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

// ---------------------------------------------------------------- errors

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MissingItem {
    id: i32,
    name: String,
}
gw2_api::ghost_id!(MissingItem, "missingitems", i32);
gw2_api::cached_resource!(MissingItem, 128);

#[tokio::test]
async fn unknown_id_maps_to_not_found_and_is_not_cached() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/missingitems/404"))
        .respond_with(ResponseTemplate::new(404).set_body_string("no such id"))
        .mount(&server)
        .await;

    let client = client_for(&server, Language::En);
    let err = MissingItem::get_with(&client, 404, false)
        .await
        .unwrap_err();

    assert!(matches!(err, ApiError::NotFound), "got {err:?}");
    assert!(!MissingItem::is_cached_in(Language::En, &404).await);
}
