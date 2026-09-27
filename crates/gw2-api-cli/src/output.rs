//! Terminal rendering — tables, progress bars, and result summaries.

use indicatif::{ProgressBar, ProgressStyle};
use tabled::Table;
use tabled::settings::object::Rows;
use tabled::settings::themes::Colorization;
use tabled::settings::{Color, Style};
use tabled::Tabled;

use gw2_api::registry::ENDPOINTS;

use crate::manifest::CoverageDiff;
use crate::runner::{FullOutcome, SingleOutcome};

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

/// Return all registry entries for a path (may be multiple, e.g. `recipes/search`).
fn registry_entries(path: &str) -> Vec<(String, String)> {
    ENDPOINTS
        .iter()
        .filter(|e| e.path == path)
        .map(|e| (e.call.to_string(), e.type_name.to_string()))
        .collect()
}

fn diff_table(rows: Vec<DiffRow>, row_colors: Vec<Color>) -> Table {
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
    println!("{}", diff_table(rows, colors));

    // ── Table 2: missing ─────────────────────────────────────────────────────
    #[derive(Tabled)]
    struct MissingRow {
        #[tabled(rename = "Path")]
        path: String,
        #[tabled(rename = "Status")]
        status: &'static str,
    }
    let mut missing_rows: Vec<MissingRow> = diff
        .missing
        .iter()
        .map(|p| MissingRow { path: p.clone(), status: "missing" })
        .collect();
    missing_rows.sort_by(|a, b| a.path.cmp(&b.path));

    let mut missing_table = Table::new(missing_rows);
    missing_table.with(Style::modern());
    for i in 0..diff.missing.len() {
        missing_table.with(Colorization::exact([Color::FG_RED], Rows::single(i + 1)));
    }

    println!("\nNot yet implemented ({})", diff.missing.len());
    println!("{}", missing_table);

    println!(
        "\n{GREEN}✓ {} covered{RESET}  |  {YELLOW}~ {} extra{RESET}  |  {RED}✗ {} missing{RESET}",
        diff.covered.len(),
        diff.extra.len(),
        diff.missing.len(),
    );
}

// ── Single ────────────────────────────────────────────────────────────────────

/// Create a spinner progress bar for a single endpoint test.
pub fn single_progress() -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} [{elapsed_precise}] {msg}")
            .unwrap(),
    );
    pb
}

pub fn print_single_results(outcomes: &[SingleOutcome]) {
    let mut pass = 0usize;
    let mut fail = 0usize;
    for o in outcomes {
        match &o.result {
            Ok(()) => {
                println!("  {GREEN}✓{RESET}  {}", o.path);
                pass += 1;
            }
            Err(e) => {
                println!("  {RED}✗{RESET}  {}  — {}", o.path, e);
                fail += 1;
            }
        }
    }
    println!();
    println!("{GREEN}{pass} passed{RESET}  |  {RED}{fail} failed{RESET}");
}

// ── Full ──────────────────────────────────────────────────────────────────────

/// Create a progress bar for full fetch (one tick per completed endpoint).
pub fn full_progress(total: u64) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=> "),
    );
    pb
}

pub fn print_full_results(outcomes: &[FullOutcome]) {
    let mut total_ok = 0usize;
    let mut total_err = 0usize;

    for o in outcomes {
        let err_count = o.result.errors.len();
        total_ok += o.result.ok_count;
        total_err += err_count;

        if err_count == 0 {
            println!("  {GREEN}✓{RESET}  {}  ({} records)", o.path, o.result.ok_count);
        } else {
            println!(
                "  {RED}✗{RESET}  {}  ({} ok, {} errors)",
                o.path, o.result.ok_count, err_count
            );
            for (id, msg) in &o.result.errors {
                println!("      [{id}] {msg}");
            }
        }
    }

    println!();
    println!("{GREEN}{total_ok} records ok{RESET}  |  {RED}{total_err} errors{RESET}");
}
