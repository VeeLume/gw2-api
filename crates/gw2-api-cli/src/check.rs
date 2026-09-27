//! `check`: fetch every raw item of an endpoint and parse it against the model.
//!
//! Unlike `full`, which goes through the typed client and stops at the first bad
//! batch, `check` parses each item on its own, so one bad item is reported by id.
//! It also reports fields the model silently drops, ids the index lists but `?ids=`
//! does not return, and runs at both the pinned and the `latest` schema, so a
//! schema change shows up before the pin moves.

use std::collections::{BTreeMap, BTreeSet};

use futures::stream::{self, StreamExt};
use gw2_api::client::auth::AuthState;
use gw2_api::registry::{CheckSpec, EndpointEntry, Shape};
use gw2_api::{Gw2ApiError, Gw2Client, Language};
use indicatif::ProgressBar;
use serde_json::Value;

use crate::runner::Clients;

/// IDs per `?ids=` request. The API's documented maximum.
const CHUNK: usize = 200;
/// Chunk requests in flight at once. The shared limiter paces them anyway; this
/// only bounds how far ahead requests are queued.
const IN_FLIGHT: usize = 8;

/// Which schema a run used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Schema {
    Pinned,
    Latest,
}

/// One endpoint × schema × language to check.
pub struct Job {
    pub entry: &'static EndpointEntry,
    pub schema: Schema,
    pub lang: Language,
}

pub enum Outcome {
    Skipped(String),
    FetchFailed(String),
    Checked {
        items: usize,
        /// `(id, error)` per item that failed to parse.
        failures: Vec<(String, String)>,
        /// Ignored field → how many items had it.
        ignored: BTreeMap<String, usize>,
        /// IDs the index listed but `?ids=` did not return.
        missing: Vec<String>,
    },
}

impl Outcome {
    pub fn is_failure(&self) -> bool {
        match self {
            Outcome::Skipped(_) => false,
            Outcome::FetchFailed(_) => true,
            Outcome::Checked { failures, .. } => !failures.is_empty(),
        }
    }
}

pub struct JobResult {
    pub job: Job,
    pub outcome: Outcome,
}

fn id_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

struct Fetched {
    items: Vec<Value>,
    missing: Vec<String>,
}

async fn fetch<S: AuthState>(client: &Gw2Client<S>, path: &str, shape: Shape) -> Result<Fetched, Gw2ApiError> {
    let as_array = |v: Value| match v {
        Value::Array(items) => Ok(items),
        other => Err(Gw2ApiError::Other(format!("expected an array, got {other}"))),
    };
    match shape {
        Shape::One => Ok(Fetched { items: vec![client.get_raw(path, &[]).await?], missing: vec![] }),
        Shape::Array => Ok(Fetched {
            items: as_array(client.get_raw(path, &[]).await?)?
                .into_iter()
                .filter(|v| !v.is_null())
                .collect(),
            missing: vec![],
        }),
        Shape::Pages => {
            let mut items = Vec::new();
            for page in 0u32.. {
                let query = [("page", page.to_string()), ("page_size", CHUNK.to_string())];
                let batch = match client.get_raw(path, &query).await {
                    Ok(v) => as_array(v)?,
                    Err(Gw2ApiError::BadRequest(_)) if page > 0 => break,
                    Err(e) => return Err(e),
                };
                let short = batch.len() < CHUNK;
                items.extend(batch);
                if short {
                    break;
                }
            }
            Ok(Fetched { items, missing: vec![] })
        }
        Shape::Bulk => {
            let ids: Vec<String> = as_array(client.get_raw(path, &[]).await?)?.iter().map(id_string).collect();
            let batches: Vec<Result<Value, Gw2ApiError>> = stream::iter(ids.chunks(CHUNK))
                .map(|chunk| async move { client.get_raw(path, &[("ids", chunk.join(","))]).await })
                .buffer_unordered(IN_FLIGHT)
                .collect()
                .await;

            let mut items = Vec::with_capacity(ids.len());
            for batch in batches {
                items.extend(as_array(batch?)?);
            }
            let returned: BTreeSet<String> = items.iter().filter_map(|v| v.get("id").map(id_string)).collect();
            let missing = ids.into_iter().filter(|i| !returned.contains(i)).collect();
            Ok(Fetched { items, missing })
        }
    }
}

async fn check_with<S: AuthState>(client: &Gw2Client<S>, entry: &EndpointEntry, spec: &CheckSpec) -> Outcome {
    let path = format!("/{}", entry.path);
    let fetched = match fetch(client, &path, spec.shape).await {
        Ok(f) => f,
        Err(Gw2ApiError::MissingPermission(p)) => return Outcome::Skipped(format!("key lacks the `{p}` scope")),
        Err(e) => return Outcome::FetchFailed(e.to_string()),
    };

    let mut failures = Vec::new();
    let mut ignored: BTreeMap<String, usize> = BTreeMap::new();
    let items = fetched.items.len();
    for v in fetched.items {
        let id = v.get("id").map(id_string).unwrap_or_else(|| "-".into());
        match (spec.parse)(v) {
            Ok(fields) => {
                // Count each field once per item.
                for f in fields.into_iter().collect::<BTreeSet<_>>() {
                    *ignored.entry(f).or_default() += 1;
                }
            }
            Err(e) => failures.push((id, e)),
        }
    }
    Outcome::Checked { items, failures, ignored, missing: fetched.missing }
}

async fn run_job(clients: &Clients, job: &Job) -> Outcome {
    let Some(spec) = &job.entry.check else {
        return Outcome::Skipped("no raw check for this endpoint".into());
    };
    if let Some(reason) = clients.skip_reason(job.entry) {
        return Outcome::Skipped(reason.into());
    }

    let schema = match job.schema {
        Schema::Pinned => None,
        Schema::Latest => Some("latest"),
    };
    match (&clients.auth, job.entry.auth) {
        (Some(auth), true) => {
            let mut c = auth.with_language(job.lang);
            if let Some(v) = schema {
                c = c.with_schema_version(v);
            }
            check_with(&c, job.entry, spec).await
        }
        _ => {
            let mut c = clients.unauth.with_language(job.lang);
            if let Some(v) = schema {
                c = c.with_schema_version(v);
            }
            check_with(&c, job.entry, spec).await
        }
    }
}

/// Run all jobs one after another (each job already fans out its own requests).
pub async fn run(clients: &Clients, jobs: Vec<Job>, pb: &ProgressBar) -> Vec<JobResult> {
    let mut results = Vec::with_capacity(jobs.len());
    for job in jobs {
        pb.set_message(format!("{} [{:?} {}]", job.entry.path, job.schema, job.lang.as_str()));
        let outcome = run_job(clients, &job).await;
        pb.inc(1);
        results.push(JobResult { job, outcome });
    }
    results
}
