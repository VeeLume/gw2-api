//! Terminal rendering — tables, progress bars, and result summaries.

use indicatif::{ProgressBar, ProgressStyle};
use tabled::Table;
use tabled::Tabled;
use tabled::settings::object::Rows;
use tabled::settings::themes::Colorization;
use tabled::settings::{Color, Style};

use gw2_api::registry::{ENDPOINTS, EndpointEntry};

use crate::check::{JobResult, Outcome, Schema};
use crate::manifest::CoverageDiff;
use crate::runner::{FullOutcome, SingleOutcome, Status};

// ANSI color constants for non-table output (single/full results, summary line).
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";

// ── List ──────────────────────────────────────────────────────────────────────

#[derive(Tabled)]
struct EndpointRow {
    #[tabled(rename = "Path")]
    path: &'static str,
    #[tabled(rename = "Call")]
    call: &'static str,
    #[tabled(rename = "Type")]
    type_name: &'static str,
    #[tabled(rename = "Auth")]
    auth: &'static str,
}

pub fn print_list() {
    let mut rows: Vec<EndpointRow> = ENDPOINTS
        .iter()
        .map(|e| EndpointRow {
            path: e.path,
            call: e.call,
            type_name: e.type_name,
            auth: if e.auth { "yes" } else { "" },
        })
        .collect();
    rows.sort_by_key(|r| (r.path, r.call));

    let table = Table::new(rows).with(Style::modern()).to_string();
    println!("{table}");
    println!("{} endpoints registered.", ENDPOINTS.len());
}

// ── List --diff ───────────────────────────────────────────────────────────────

#[derive(Tabled)]
struct DiffRow {
    #[tabled(rename = "Path")]
    path: String,
    #[tabled(rename = "Call")]
    call: String,
    #[tabled(rename = "Type")]
    type_name: String,
    #[tabled(rename = "Status")]
    status: &'static str,
}

#[derive(Tabled)]
struct MissingRow {
    #[tabled(rename = "Path")]
    path: String,
    #[tabled(rename = "Auth")]
    auth: &'static str,
    #[tabled(rename = "Lang")]
    lang: &'static str,
}

/// Return all registry entries for a path (may be multiple, e.g. `recipes/search`).
fn registry_entries(path: &str) -> Vec<(String, String)> {
    ENDPOINTS
        .iter()
        .filter(|e| e.path == path)
        .map(|e| (e.call.to_string(), e.type_name.to_string()))
        .collect()
}

fn colored_table<T: Tabled>(rows: Vec<T>, row_colors: Vec<Color>) -> Table {
    let mut table = Table::new(rows);
    table.with(Style::modern());
    // Row 0 is the header — data rows start at index 1.
    for (i, color) in row_colors.into_iter().enumerate() {
        table.with(Colorization::exact([color], Rows::single(i + 1)));
    }
    table
}

pub fn print_diff(diff: &CoverageDiff) {
    // ── Table 1: implemented (covered + extras) ──────────────────────────────
    let mut impl_rows: Vec<(DiffRow, Color)> = Vec::new();
    for p in &diff.covered {
        for (call, type_name) in registry_entries(p) {
            impl_rows.push((DiffRow { path: p.clone(), call, type_name, status: "covered" }, Color::FG_GREEN));
        }
    }
    for p in &diff.extra {
        for (call, type_name) in registry_entries(p) {
            impl_rows.push((DiffRow { path: p.clone(), call, type_name, status: "extra" }, Color::FG_YELLOW));
        }
    }
    impl_rows.sort_by(|(a, _), (b, _)| a.path.cmp(&b.path));
    let (rows, colors): (Vec<_>, Vec<_>) = impl_rows.into_iter().unzip();

    println!(
        "Implemented ({} covered, {} not in manifest)",
        diff.covered.len(),
        diff.extra.len()
    );
    println!("{}", colored_table(rows, colors));

    // ── Table 2: missing ─────────────────────────────────────────────────────
    let missing_rows: Vec<MissingRow> = diff
        .missing
        .iter()
        .map(|r| MissingRow {
            path: r.path.clone(),
            auth: if r.auth { "yes" } else { "" },
            lang: if r.lang { "yes" } else { "" },
        })
        .collect();
    let colors = vec![Color::FG_RED; missing_rows.len()];
    println!("\nNot yet implemented ({})", diff.missing.len());
    println!("{}", colored_table(missing_rows, colors));

    if !diff.auth_mismatch.is_empty() {
        println!("\n{YELLOW}Auth flag disagrees with the manifest:{RESET}");
        for (path, ours, theirs) in &diff.auth_mismatch {
            println!("  {path}: registry auth={ours}, manifest auth={theirs}");
        }
    }

    println!(
        "\n{GREEN}✓ {} covered{RESET}  |  {YELLOW}~ {} extra{RESET}  |  {RED}✗ {} missing{RESET}",
        diff.covered.len(),
        diff.extra.len(),
        diff.missing.len(),
    );
}

// ── Progress ──────────────────────────────────────────────────────────────────

/// A progress bar with one tick per endpoint (or check job).
pub fn progress(total: usize) -> ProgressBar {
    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40.cyan/blue} {pos}/{len} [{elapsed_precise}] {msg}")
            .unwrap()
            .progress_chars("=> "),
    );
    pb
}

/// The path, plus the call for paths with several entries (e.g. `recipes/search`).
fn label(entry: &EndpointEntry) -> String {
    if ENDPOINTS.iter().filter(|e| e.path == entry.path).count() > 1 {
        format!("{}  {}", entry.path, entry.call)
    } else {
        entry.path.to_string()
    }
}

// ── Single ────────────────────────────────────────────────────────────────────

/// Print results; returns the number of failures.
pub fn print_single_results(outcomes: &[SingleOutcome]) -> usize {
    let (mut pass, mut fail, mut skip) = (0usize, 0usize, 0usize);
    for o in outcomes {
        match &o.status {
            Status::Passed => {
                println!("  {GREEN}✓{RESET}  {}", label(o.entry));
                pass += 1;
            }
            Status::Failed(e) => {
                println!("  {RED}✗{RESET}  {}  — {}", label(o.entry), e);
                fail += 1;
            }
            Status::Skipped(why) => {
                println!("  {YELLOW}-{RESET}  {}  — skipped: {}", label(o.entry), why);
                skip += 1;
            }
        }
    }
    println!();
    println!("{GREEN}{pass} passed{RESET}  |  {RED}{fail} failed{RESET}  |  {YELLOW}{skip} skipped{RESET}");
    fail
}

// ── Full ──────────────────────────────────────────────────────────────────────

/// Print results; returns the number of errors.
pub fn print_full_results(outcomes: &[FullOutcome]) -> usize {
    let (mut total_ok, mut total_err, mut skipped) = (0usize, 0usize, 0usize);

    for o in outcomes {
        let Some(result) = &o.result else {
            println!("  {YELLOW}-{RESET}  {}  — skipped: needs an API key", label(o.entry));
            skipped += 1;
            continue;
        };
        let err_count = result.errors.len();
        total_ok += result.ok_count;
        total_err += err_count;

        if err_count == 0 {
            println!("  {GREEN}✓{RESET}  {}  ({} records)", label(o.entry), result.ok_count);
        } else {
            println!(
                "  {RED}✗{RESET}  {}  ({} ok, {} errors)",
                label(o.entry),
                result.ok_count,
                err_count
            );
            for (id, msg) in &result.errors {
                println!("      [{id}] {msg}");
            }
        }
    }

    println!();
    println!(
        "{GREEN}{total_ok} records ok{RESET}  |  {RED}{total_err} errors{RESET}  |  {YELLOW}{skipped} skipped{RESET}"
    );
    total_err
}

// ── Check ─────────────────────────────────────────────────────────────────────

const MAX_FAILURES: usize = 3;
const MAX_IGNORED: usize = 10;
const MAX_MISSING: usize = 10;

fn job_name(r: &JobResult) -> String {
    let schema = match r.job.schema {
        Schema::Pinned => "",
        Schema::Latest => " @latest",
    };
    let lang = if r.job.lang == gw2_api::Language::En {
        String::new()
    } else {
        format!(" [{}]", r.job.lang.as_str())
    };
    format!("{}{schema}{lang}", label(r.job.entry))
}

/// Print check results; returns the number of failures at the pinned schema.
pub fn print_check_results(results: &[JobResult], verbose: bool) -> usize {
    let mut pinned_failures = 0usize;
    let mut latest_breaks: Vec<String> = Vec::new();

    for r in results {
        let name = job_name(r);
        if r.outcome.is_failure() {
            match r.job.schema {
                Schema::Pinned => pinned_failures += 1,
                Schema::Latest => latest_breaks.push(name.clone()),
            }
        }
        match &r.outcome {
            Outcome::Skipped(why) => println!("  {YELLOW}-{RESET}  {name}  — skipped: {why}"),
            Outcome::FetchFailed(e) => println!("  {RED}✗{RESET}  {name}  — fetch failed: {e}"),
            Outcome::Checked { items, failures, ignored, missing } => {
                if failures.is_empty() {
                    println!("  {GREEN}✓{RESET}  {name}  ({items} items)");
                } else {
                    println!(
                        "  {RED}✗{RESET}  {name}  ({} of {items} items failed to parse)",
                        failures.len()
                    );
                    for (id, e) in failures.iter().take(MAX_FAILURES) {
                        println!("      [{id}] {e}");
                    }
                    if failures.len() > MAX_FAILURES {
                        println!("      … and {} more", failures.len() - MAX_FAILURES);
                    }
                }
                if !ignored.is_empty() {
                    let mut fields: Vec<_> = ignored.iter().collect();
                    fields.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
                    let shown = if verbose { fields.len() } else { MAX_IGNORED };
                    println!("      {YELLOW}ignored fields: {}{RESET}", fields.len());
                    for (field, count) in fields.iter().take(shown) {
                        println!("        {field} ({count} items)");
                    }
                    if fields.len() > shown {
                        println!("        … and {} more (--verbose)", fields.len() - shown);
                    }
                }
                if !missing.is_empty() {
                    let ids: Vec<&str> = missing.iter().take(MAX_MISSING).map(String::as_str).collect();
                    println!(
                        "      {YELLOW}listed but not returned: {}{RESET} ({}{})",
                        missing.len(),
                        ids.join(", "),
                        if missing.len() > MAX_MISSING { ", …" } else { "" }
                    );
                }
            }
        }
    }

    println!();
    if !latest_breaks.is_empty() {
        println!("{YELLOW}Would break at schema `latest`:{RESET}");
        for name in &latest_breaks {
            println!("  {name}");
        }
        println!();
    }
    println!(
        "{} failed at the pinned schema  |  {} would break at `latest`",
        pinned_failures,
        latest_breaks.len()
    );
    pinned_failures
}
