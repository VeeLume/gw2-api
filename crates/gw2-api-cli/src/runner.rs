//! Executes `single` and `full` tests against registered endpoints.

use anyhow::Result;
use futures::stream::{self, StreamExt};
use gw2_api::{
    Gw2Client,
    client::auth::{Authenticated, Unauthenticated},
    registry::{ENDPOINTS, EndpointEntry, FullFetchResult},
};
use indicatif::ProgressBar;

/// The clients every command runs through: built once, so all requests share one
/// rate limiter and one cache.
#[derive(Clone)]
pub struct Clients {
    pub unauth: Gw2Client<Unauthenticated>,
    pub auth: Option<Gw2Client<Authenticated>>,
}

impl Clients {
    pub fn new(key: Option<&str>) -> Self {
        let unauth = Gw2Client::new();
        let auth = key.and_then(|k| unauth.authenticate(k).ok());
        Self { unauth, auth }
    }

    /// Why `entry` cannot run with these clients, if it cannot.
    pub fn skip_reason(&self, entry: &EndpointEntry) -> Option<&'static str> {
        (entry.auth && self.auth.is_none()).then_some("needs an API key (--key or GW2_API_KEY)")
    }
}

/// Result of one endpoint test.
pub enum Status {
    Passed,
    Failed(String),
    Skipped(String),
}

/// Outcome of one `single` test.
pub struct SingleOutcome {
    pub entry: &'static EndpointEntry,
    pub status: Status,
}

/// Outcome of one `full` fetch.
pub struct FullOutcome {
    pub entry: &'static EndpointEntry,
    /// `None` when skipped.
    pub result: Option<FullFetchResult>,
}

/// Every registry entry for a path (e.g. `recipes/search` has two). Errors with
/// the list of valid paths if there is none.
pub fn find_endpoints(path: &str) -> Result<Vec<&'static EndpointEntry>> {
    // Normalize: strip leading /v2/ prefix
    let norm = path.trim_start_matches('/').trim_start_matches("v2/");

    let found: Vec<_> = ENDPOINTS.iter().filter(|e| e.path == norm).collect();
    if found.is_empty() {
        let mut valid: Vec<&str> = ENDPOINTS.iter().map(|e| e.path).collect();
        valid.sort_unstable();
        valid.dedup();
        anyhow::bail!("Unknown endpoint {:?}. Valid paths:\n  {}", norm, valid.join("\n  "));
    }
    Ok(found)
}

/// The endpoints selected by `--endpoint` / `--all`, sorted by path.
pub fn select(endpoint: Option<&str>, all: bool) -> Result<Vec<&'static EndpointEntry>> {
    let mut entries = match (endpoint, all) {
        (Some(path), _) => find_endpoints(path)?,
        (None, true) => ENDPOINTS.iter().collect(),
        (None, false) => anyhow::bail!("Specify --endpoint <path> or --all"),
    };
    entries.sort_by_key(|e| (e.path, e.call));
    Ok(entries)
}

/// Run the `single` test for each entry, in order. Stops at the first failure
/// unless `keep_going`.
pub async fn run_single(
    entries: &[&'static EndpointEntry],
    clients: &Clients,
    keep_going: bool,
    pb: &ProgressBar,
) -> Vec<SingleOutcome> {
    let mut outcomes = Vec::new();
    for &entry in entries {
        pb.set_message(entry.path);
        let status = match clients.skip_reason(entry) {
            Some(reason) => Status::Skipped(reason.into()),
            None => match (entry.single)(clients.unauth.clone(), clients.auth.clone()).await {
                Ok(()) => Status::Passed,
                Err(e) => Status::Failed(e),
            },
        };
        pb.inc(1);
        let failed = matches!(status, Status::Failed(_));
        outcomes.push(SingleOutcome { entry, status });
        if failed && !keep_going {
            break;
        }
    }
    outcomes
}

/// Run the `full` fetch for each entry, with bounded concurrency.
pub async fn run_full(
    entries: &[&'static EndpointEntry],
    clients: &Clients,
    concurrency: usize,
    pb: &ProgressBar,
) -> Vec<FullOutcome> {
    let mut outcomes: Vec<FullOutcome> = stream::iter(entries.iter().copied())
        .map(|entry| async move {
            let result = match clients.skip_reason(entry) {
                Some(_) => None,
                None => Some((entry.full)(clients.unauth.clone(), clients.auth.clone()).await),
            };
            pb.inc(1);
            FullOutcome { entry, result }
        })
        .buffer_unordered(concurrency.max(1))
        .collect()
        .await;
    outcomes.sort_by_key(|o| (o.entry.path, o.entry.call));
    outcomes
}
