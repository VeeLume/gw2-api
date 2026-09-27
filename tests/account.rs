// tests/account.rs
//
// `/v2/account` and `/v2/account/wallet` on a session, and the one bridge back
// to static data: a wallet entry resolving its currency through the shared
// cache.

use std::sync::Arc;

use gw2_api::api::client::{ApiClient, ApiError};
use gw2_api::api::session::Gw2Session;
use gw2_api::models::account::Access;
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client_for(server: &MockServer, key: &str) -> Arc<ApiClient> {
    Arc::new(
        ApiClient::builder()
            .base(format!("{}/v2", server.uri()))
            .api_key(key)
            .max_retries(0)
            .build()
            .expect("test client builds"),
    )
}

#[tokio::test]
async fn account_parses_and_tolerates_unknown_access() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .and(header("authorization", "Bearer key-account"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "A1B2-C3D4",
            "name": "Example.1234",
            "age": 3_600_000,
            "world": 2202,
            "guilds": ["GUILD-1"],
            "created": "2015-08-28T00:00:00Z",
            "access": ["GuildWars2", "HeartOfThorns", "SomeFutureExpansion"],
            "commander": true
        })))
        .mount(&server)
        .await;

    let session = Gw2Session::from_client(client_for(&server, "key-account"));
    let account = session.account().await.unwrap();

    assert_eq!(account.name, "Example.1234");
    assert!(account.has_access(&Access::HeartOfThorns));
    assert!(account.has_access(&Access::Unknown("SomeFutureExpansion".into())));
    // Without the `progression` scope these are simply absent.
    assert_eq!(account.fractal_level, None);
}

#[tokio::test]
async fn wallet_without_scope_is_missing_permission_not_invalid_token() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/account/wallet"))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(json!({ "text": "requires scope wallet" })),
        )
        .mount(&server)
        .await;

    let session = Gw2Session::from_client(client_for(&server, "key-no-wallet"));
    let err = session.wallet().await.unwrap_err();

    match err {
        ApiError::MissingPermission(p) => assert_eq!(p, "wallet"),
        other => panic!("expected MissingPermission, got {other:?}"),
    }
}

#[tokio::test]
async fn other_403s_stay_invalid_token() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/account"))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(json!({ "text": "Invalid access token" })),
        )
        .mount(&server)
        .await;

    let session = Gw2Session::from_client(client_for(&server, "key-bad"));
    let err = session.account().await.unwrap_err();

    assert!(matches!(err, ApiError::InvalidToken(_)), "got {err:?}");
}

// The only test in the process that touches the `Currency` cache, so nothing
// races on it.
#[tokio::test]
async fn wallet_entries_resolve_their_currency_names() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/account/wallet"))
        .and(header("authorization", "Bearer key-wallet"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": 1, "value": 1_234_567 },
            { "id": 4, "value": 800 }
        ])))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v2/currencies"))
        .and(query_param("ids", "1,4"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": 1, "name": "Coin", "description": "", "icon": "", "order": 101 },
            { "id": 4, "name": "Gem", "description": "", "icon": "", "order": 102 }
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server, "key-wallet");
    let session = Gw2Session::from_client(client.clone());
    let wallet = session.wallet().await.unwrap();
    assert_eq!(wallet.len(), 2);
    assert_eq!(wallet[0].value, 1_234_567);

    let ids: Vec<i32> = wallet.iter().map(|e| *e.id.id()).collect();
    let currencies = gw2_api::models::currencies::Currency::get_many_with(&client, &ids, false)
        .await
        .unwrap();
    let names: Vec<&str> = currencies.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Coin", "Gem"]);

    // The typed id now resolves from cache; the mock's `expect(1)` holds.
    let gem = wallet[1].id.get_with(&client, false).await.unwrap();
    assert_eq!(gem.name, "Gem");
}
