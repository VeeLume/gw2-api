// examples/api_check.rs — run with: cargo run --example api_check [-- options]
//
// Online contract check against the live API. Three questions:
//
//   coverage  — which active routes in /v2.json the crate models, and which not
//   schema    — whether ArenaNet has published schema versions newer than the pin
//   parsing   — whether *every* resource of every covered route deserializes,
//               at the pinned schema and at `latest`
//
// The parsing check fetches everything on purpose. The API is not always what
// the wiki says: single items come back with nulls, placeholder names or extra
// fields, and a sample would miss exactly those. Each item is parsed on its own,
// so one malformed item is reported by id instead of failing its whole batch.
//
// Authenticated routes use GW2_API_KEY from the environment or `.env`, and are
// skipped without one. A key without the route's scope skips just that route.
//
// Options:
//   --only <text>   check only routes containing <text> (repeatable)
//   --uncovered     list every active route the crate does not model
//   --all-langs     parse localized routes in every language, not just English
//
// Exits non-zero when anything fails at the pinned schema. Failures at `latest`
// are warnings: they are what breaks when the pin moves forward.

use std::collections::{BTreeMap, BTreeSet};
use std::process::ExitCode;

use futures::stream::{self, StreamExt};
use gw2_api::api::client::{ApiClient, ApiError, DEFAULT_SCHEMA_VERSION, Language};
use gw2_api::core::ghost::Ghost;
use gw2_api::core::ghost_id::GhostId;
use gw2_api::models::{
    account::{Account, WalletEntry},
    build::Build,
    currencies::Currency,
    minis::Mini,
    mounts::{MountSkin, MountType},
    tokeninfo::TokenInfo,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

// ---------------------------------------------------------------- targets

/// How a route answers.
#[derive(Clone, Copy)]
enum Shape {
    /// One object: `build`, `account`, `tokeninfo`.
    One,
    /// A plain array, answered in full: `account/wallet`.
    Array,
    /// Id-indexed: the bare route lists ids, `?ids=` returns the items.
    Bulk,
}

struct Target {
    path: &'static str,
    shape: Shape,
    auth: bool,
    parse: fn(Value) -> Result<Vec<String>, String>,
}

/// Everything the crate models. This list *is* the crate's coverage: a model
/// that is not registered here is neither counted nor checked.
const TARGETS: &[Target] = &[
    Target {
        path: Build::URL,
        shape: Shape::One,
        auth: false,
        parse: parse::<Build>,
    },
    Target {
        path: Currency::URL,
        shape: Shape::Bulk,
        auth: false,
        parse: parse::<Currency>,
    },
    Target {
        path: Mini::URL,
        shape: Shape::Bulk,
        auth: false,
        parse: parse::<Mini>,
    },
    Target {
        path: MountSkin::URL,
        shape: Shape::Bulk,
        auth: false,
        parse: parse::<MountSkin>,
    },
    Target {
        path: MountType::URL,
        shape: Shape::Bulk,
        auth: false,
        parse: parse::<MountType>,
    },
    Target {
        path: "tokeninfo",
        shape: Shape::One,
        auth: true,
        parse: parse::<TokenInfo>,
    },
    Target {
        path: "account",
        shape: Shape::One,
        auth: true,
        parse: parse::<Account>,
    },
    Target {
        path: "account/wallet",
        shape: Shape::Array,
        auth: true,
        parse: parse::<WalletEntry>,
    },
];

/// Deserialize one item, collecting the fields the model silently drops.
fn parse<T: DeserializeOwned>(v: Value) -> Result<Vec<String>, String> {
    let mut ignored = Vec::new();
    serde_ignored::deserialize::<_, _, T>(v, |p| ignored.push(generalize(&p.to_string())))
        .map(|_| ignored)
        .map_err(|e| e.to_string())
}

/// `dye_slots.3.shade` → `dye_slots[].shade`, so drift is counted per field
/// rather than per array position.
fn generalize(path: &str) -> String {
    let mut out = String::new();
    for seg in path.split('.') {
        if seg.chars().all(|c| c.is_ascii_digit()) {
            out.push_str("[]");
        } else {
            if !out.is_empty() {
                out.push('.');
            }
            out.push_str(seg);
        }
    }
    out
}

// ---------------------------------------------------------------- fetching

/// Ids per `?ids=` request. The API's documented maximum.
const CHUNK: usize = 200;
/// Chunk requests in flight at once. The shared limiter paces them anyway;
/// this only bounds how far ahead we queue.
const IN_FLIGHT: usize = 8;

fn id_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

struct Fetched {
    items: Vec<Value>,
    /// Ids the index listed but `?ids=` did not return.
    missing: Vec<String>,
}

async fn fetch(client: &ApiClient, t: &Target) -> Result<Fetched, ApiError> {
    match t.shape {
        Shape::One => Ok(Fetched {
            items: vec![client.get_json::<Value>(t.path, &[]).await?],
            missing: vec![],
        }),
        Shape::Array => Ok(Fetched {
            items: client.get_json::<Vec<Value>>(t.path, &[]).await?,
            missing: vec![],
        }),
        Shape::Bulk => {
            let ids: Vec<String> = client
                .get_json::<Vec<Value>>(t.path, &[])
                .await?
                .iter()
                .map(id_string)
                .collect();

            let batches: Vec<Result<Vec<Value>, ApiError>> = stream::iter(ids.chunks(CHUNK))
                .map(|chunk| async move {
                    let qp = [("ids", chunk.join(","))];
                    client.get_json::<Vec<Value>>(t.path, &qp).await
                })
                .buffer_unordered(IN_FLIGHT)
                .collect()
                .await;

            let mut items = Vec::with_capacity(ids.len());
            for b in batches {
                items.extend(b?);
            }
            let returned: BTreeSet<String> = items
                .iter()
                .filter_map(|v| v.get("id").map(id_string))
                .collect();
            let missing = ids.into_iter().filter(|i| !returned.contains(i)).collect();
            Ok(Fetched { items, missing })
        }
    }
}

// ---------------------------------------------------------------- checking

enum Outcome {
    Skipped(String),
    FetchFailed(String),
    Checked {
        items: usize,
        failures: Vec<(String, String)>,
        ignored: BTreeMap<String, usize>,
        missing: Vec<String>,
    },
}

impl Outcome {
    fn is_failure(&self) -> bool {
        match self {
            Outcome::Skipped(_) => false,
            Outcome::FetchFailed(_) => true,
            Outcome::Checked { failures, .. } => !failures.is_empty(),
        }
    }
}

async fn check(client: &ApiClient, t: &Target) -> Outcome {
    let fetched = match fetch(client, t).await {
        Ok(f) => f,
        Err(ApiError::MissingPermission(p)) => {
            return Outcome::Skipped(format!("key lacks the `{p}` scope"));
        }
        Err(e) => return Outcome::FetchFailed(e.to_string()),
    };

    let mut failures = Vec::new();
    let mut ignored: BTreeMap<String, usize> = BTreeMap::new();
    let items = fetched.items.len();
    for v in fetched.items {
        let id = v.get("id").map(id_string).unwrap_or_else(|| "-".into());
        match (t.parse)(v) {
            Ok(fields) => {
                for f in fields {
                    *ignored.entry(f).or_default() += 1;
                }
            }
            Err(e) => failures.push((id, e)),
        }
    }
    Outcome::Checked {
        items,
        failures,
        ignored,
        missing: fetched.missing,
    }
}

// ---------------------------------------------------------------- /v2.json

#[derive(Deserialize)]
struct Index {
    routes: Vec<Route>,
    schema_versions: Vec<SchemaVersion>,
}

#[derive(Deserialize)]
struct Route {
    path: String,
    lang: bool,
    auth: bool,
    active: bool,
}

#[derive(Deserialize)]
struct SchemaVersion {
    v: String,
    desc: String,
}

fn route_of(path: &str) -> String {
    format!("/v2/{path}")
}

// ---------------------------------------------------------------- main

struct Options {
    only: Vec<String>,
    uncovered: bool,
    all_langs: bool,
}

fn options() -> Result<Options, String> {
    let mut o = Options {
        only: vec![],
        uncovered: false,
        all_langs: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--only" => o
                .only
                .push(args.next().ok_or("--only needs a value".to_string())?),
            "--uncovered" => o.uncovered = true,
            "--all-langs" => o.all_langs = true,
            other => return Err(format!("unknown option: {other}")),
        }
    }
    Ok(o)
}

const LANGS: [Language; 5] = [
    Language::En,
    Language::De,
    Language::Fr,
    Language::Es,
    Language::Zh,
];

#[tokio::main]
async fn main() -> ExitCode {
    let opts = match options() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    // A missing .env is fine: the key may come from the environment, or the
    // run is static-only.
    let _ = dotenvy::dotenv();
    let key = std::env::var("GW2_API_KEY").ok().filter(|k| !k.is_empty());

    let build = |schema: &str, key: Option<&str>| {
        let mut b = ApiClient::builder().schema_version(schema);
        if let Some(k) = key {
            b = b.api_key(k);
        }
        b.build().expect("client config is valid")
    };
    let schemas = [("pinned", DEFAULT_SCHEMA_VERSION), ("latest", "latest")];

    let mut failed = false;

    // ---------- coverage + schema ----------

    let index: Index = match build(DEFAULT_SCHEMA_VERSION, None)
        .get_json("../v2.json", &[("v", "latest".into())])
        .await
    {
        Ok(i) => i,
        Err(e) => {
            eprintln!("could not fetch /v2.json: {e}");
            return ExitCode::FAILURE;
        }
    };

    let active: Vec<&Route> = index.routes.iter().filter(|r| r.active).collect();
    let covered: BTreeSet<String> = TARGETS.iter().map(|t| route_of(t.path)).collect();
    let count = |auth: bool| {
        let all = active.iter().filter(|r| r.auth == auth).count();
        let done = active
            .iter()
            .filter(|r| r.auth == auth && covered.contains(&r.path))
            .count();
        (done, all)
    };
    let (sd, sa) = count(false);
    let (ad, aa) = count(true);
    println!("== coverage");
    println!(
        "   {} of {} active routes  (static {sd}/{sa}, authenticated {ad}/{aa})",
        sd + ad,
        sa + aa
    );
    for t in TARGETS {
        let r = route_of(t.path);
        match index.routes.iter().find(|x| x.path == r) {
            None => {
                println!("   ! {r} is modelled but not listed in /v2.json");
                failed = true;
            }
            Some(x) if !x.active => println!("   ! {r} is modelled but marked inactive"),
            Some(x) if x.auth != t.auth => {
                println!(
                    "   ! {r}: /v2.json says auth={}, target says {}",
                    x.auth, t.auth
                )
            }
            _ => {}
        }
    }
    if opts.uncovered {
        for r in active.iter().filter(|r| !covered.contains(&r.path)) {
            let tags = match (r.auth, r.lang) {
                (true, true) => "  [auth, lang]",
                (true, false) => "  [auth]",
                (false, true) => "  [lang]",
                (false, false) => "",
            };
            println!("     - {}{tags}", r.path);
        }
    }

    println!("\n== schema");
    println!("   pinned  {DEFAULT_SCHEMA_VERSION}");
    // ISO-8601 timestamps compare correctly as strings on the date prefix,
    // whatever their sub-second precision.
    let newer: Vec<&SchemaVersion> = index
        .schema_versions
        .iter()
        .filter(|s| s.v.as_str() > DEFAULT_SCHEMA_VERSION)
        .collect();
    if newer.is_empty() {
        println!("   up to date");
    } else {
        println!("   {} newer version(s) available:", newer.len());
        for s in newer {
            println!("     {}  {}", s.v, s.desc);
        }
    }

    // ---------- parsing ----------

    println!("\n== parsing (every resource)");
    let mut latest_breaks = Vec::new();
    for t in TARGETS {
        if !opts.only.is_empty() && !opts.only.iter().any(|o| t.path.contains(o.as_str())) {
            continue;
        }
        let localized = index
            .routes
            .iter()
            .any(|r| r.path == route_of(t.path) && r.lang);
        let langs: &[Language] = if localized && opts.all_langs {
            &LANGS
        } else {
            &LANGS[..1]
        };

        for &lang in langs {
            for (label, schema) in schemas {
                let outcome = if t.auth && key.is_none() {
                    Outcome::Skipped("no GW2_API_KEY".into())
                } else {
                    let client = build(schema, if t.auth { key.as_deref() } else { None });
                    client.set_language(lang);
                    check(&client, t).await
                };

                let name = if localized {
                    format!("{} [{}]", t.path, lang.as_str())
                } else {
                    t.path.to_string()
                };
                report(&name, label, &outcome);

                if outcome.is_failure() {
                    if label == "pinned" {
                        failed = true;
                    } else {
                        latest_breaks.push(name);
                    }
                }
            }
        }
    }

    if !latest_breaks.is_empty() {
        println!("\n== would break at `latest`");
        for n in latest_breaks {
            println!("   {n}");
        }
    }

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Failures shown per route and schema; the rest are counted.
const SHOW_FAILURES: usize = 3;

fn report(name: &str, label: &str, o: &Outcome) {
    let head = format!("   {name:<28} {label}");
    match o {
        Outcome::Skipped(why) => println!("{head}  skipped: {why}"),
        Outcome::FetchFailed(e) => println!("{head}  FETCH FAILED: {e}"),
        Outcome::Checked {
            items,
            failures,
            ignored,
            missing,
        } => {
            if failures.is_empty() {
                println!("{head}  ok  {items} item(s)");
            } else {
                println!(
                    "{head}  FAIL  {} of {items} item(s) did not parse",
                    failures.len()
                );
                for (id, e) in failures.iter().take(SHOW_FAILURES) {
                    println!("        id {id}: {e}");
                }
                if failures.len() > SHOW_FAILURES {
                    println!("        ... {} more", failures.len() - SHOW_FAILURES);
                }
            }
            for (field, n) in ignored {
                println!("        ignored field `{field}` in {n} item(s)");
            }
            if !missing.is_empty() {
                let shown: Vec<&str> = missing.iter().take(10).map(String::as_str).collect();
                println!(
                    "        {} listed id(s) not returned: {}{}",
                    missing.len(),
                    shown.join(", "),
                    if missing.len() > 10 { ", ..." } else { "" }
                );
            }
        }
    }
}
