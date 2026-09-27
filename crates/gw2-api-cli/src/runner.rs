//! Executes `single` and `full` tests against registered endpoints.

use anyhow::Result;
use futures::stream::{self, StreamExt};
use gw2_api::{
    Gw2Client,
    client::auth::{Authenticated, Unauthenticated},
    registry::{EndpointEntry, FullFetchResult, ENDPOINTS},
};

/// Outcome of one `single` test.
pub struct SingleOutcome {
    pub path: &'static str,
    pub result: Result<(), String>,
}

/// Outcome of one `full` fetch.
pub struct FullOutcome {
    pub path: &'static str,
    pub result: FullFetchResult,
}

fn make_clients(key: Option<&str>) -> (Gw2Client<Unauthenticated>, Option<Gw2Client<Authenticated>>) {
    let unauth = Gw2Client::new();
    let auth = key.and_then(|k| {
        if k.is_empty() {
            None
        } else {
            Gw2Client::builder().api_key(k).build().ok()
        }
    });
    (unauth, auth)
}

/// Find an endpoint by path. Returns an error listing valid paths if not found.
pub fn find_endpoint(path: &str) -> Result<&'static EndpointEntry> {
    // Normalize: strip leading /v2/ prefix
    let norm = path
        .trim_start_matches('/')
        .trim_start_matches("v2/");

    ENDPOINTS
        .iter()
        .find(|e| e.path == norm)
        .ok_or_else(|| {
            let mut valid: Vec<&str> = ENDPOINTS.iter().map(|e| e.path).collect();
            valid.sort_unstable();
            anyhow::anyhow!(
                "Unknown endpoint {:?}. Valid paths:\n  {}",
                norm,
                valid.join("\n  ")
            )
        })
}

/// Run the `single` test for every registered endpoint, in registration order.
///
/// Returns a `Vec<SingleOutcome>` — one entry per endpoint.
/// If `keep_going` is false, stops on the first failure.
pub async fn run_single_all(key: Option<&str>, keep_going: bool) -> Vec<SingleOutcome> {
    let mut outcomes = Vec::new();
    for entry in ENDPOINTS.iter() {
        let (unauth, auth) = make_clients(key);
        let result = (entry.single)(unauth, auth).await;
        let failed = result.is_err();
        outcomes.push(SingleOutcome { path: entry.path, result });
        if failed && !keep_going {
            break;
        }
    }
    outcomes
}

/// Run the `single` test for a single endpoint.
pub async fn run_single_one(entry: &'static EndpointEntry, key: Option<&str>) -> SingleOutcome {
    let (unauth, auth) = make_clients(key);
    let result = (entry.single)(unauth, auth).await;
    SingleOutcome { path: entry.path, result }
}

/// Run the `full` fetch for every registered endpoint, with bounded concurrency.
pub async fn run_full_all(key: Option<&str>, concurrency: usize) -> Vec<FullOutcome> {
    stream::iter(ENDPOINTS.iter())
        .map(|entry| {
            let key = key.map(str::to_string);
            async move {
                let (unauth, auth) = make_clients(key.as_deref());
                let result = (entry.full)(unauth, auth).await;
                FullOutcome { path: entry.path, result }
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await
}

/// Run the `full` fetch for a single endpoint.
pub async fn run_full_one(entry: &'static EndpointEntry, key: Option<&str>) -> FullOutcome {
    let (unauth, auth) = make_clients(key);
    let result = (entry.full)(unauth, auth).await;
    FullOutcome { path: entry.path, result }
}
