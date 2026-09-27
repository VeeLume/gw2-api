// tests/session.rs
//
// The account-data path: a session carries its key, and two sessions do not
// share anything except the rate budget.

use std::sync::Arc;

use gw2_api::api::client::{ApiClient, ApiError};
use gw2_api::api::session::Gw2Session;
use gw2_api::models::tokeninfo::Permission;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn session_for(server: &MockServer, key: &str) -> Gw2Session {
    let client = ApiClient::builder()
        .base(format!("{}/v2", server.uri()))
        .api_key(key)
        .max_retries(0)
        .build()
        .expect("test client builds");
    Gw2Session::from_client(Arc::new(client))
}

#[tokio::test]
async fn tokeninfo_sends_the_bearer_token_and_parses_permissions() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/tokeninfo"))
        .and(header("authorization", "Bearer test-key-a"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "ABCD-1234",
            "name": "companion",
            "permissions": ["account", "wallet", "unlocks"]
        })))
        .mount(&server)
        .await;

    let session = session_for(&server, "test-key-a");
    let info = session.tokeninfo().await.unwrap();

    assert_eq!(info.name, "companion");
    assert!(info.has(&Permission::Wallet));
    assert!(!info.has(&Permission::Characters));
}

#[tokio::test]
async fn unknown_permissions_do_not_break_deserialization() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/tokeninfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "ABCD-1234",
            "name": "future-proof",
            "permissions": ["account", "homestead-decorations"]
        })))
        .mount(&server)
        .await;

    let session = session_for(&server, "test-key-b");
    let info = session.tokeninfo().await.unwrap();

    // A scope ArenaNet adds later must not fail the whole response.
    assert!(info.has(&Permission::Account));
    assert!(info.has(&Permission::Unknown("homestead-decorations".into())));
}

#[tokio::test]
async fn missing_scope_is_a_typed_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/tokeninfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "ABCD-1234",
            "name": "read-only",
            "permissions": ["account"]
        })))
        .mount(&server)
        .await;

    let session = session_for(&server, "test-key-c");
    let err = session
        .require_permission(&Permission::Wallet)
        .await
        .unwrap_err();

    match err {
        ApiError::MissingPermission(p) => assert_eq!(p, "wallet"),
        other => panic!("expected MissingPermission, got {other:?}"),
    }
}

#[tokio::test]
async fn an_invalid_key_is_reported_as_invalid_token() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/tokeninfo"))
        .respond_with(ResponseTemplate::new(401).set_body_string("invalid key"))
        .mount(&server)
        .await;

    let session = session_for(&server, "nonsense");
    let err = session.tokeninfo().await.unwrap_err();

    assert!(matches!(err, ApiError::InvalidToken(_)), "got {err:?}");
}
