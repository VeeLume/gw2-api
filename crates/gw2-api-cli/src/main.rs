mod check;
mod cli;
mod manifest;
mod output;
mod runner;

use anyhow::Result;
use clap::Parser;
use gw2_api::{DEFAULT_SCHEMA_VERSION, Language, registry::ENDPOINTS};

use check::{Job, Schema};
use cli::{Cli, Command};
use runner::Clients;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    // Load .env file if present — makes GW2_API_KEY available to clap's env reader.
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();
    let clients = Clients::new(cli.key.as_deref().filter(|k| !k.is_empty()));

    let failed = match cli.command {
        Command::List { diff } => {
            output::print_list();
            if diff {
                println!("\nFetching live GW2 API manifest…");
                let manifest = manifest::fetch_manifest(DEFAULT_SCHEMA_VERSION).await?;
                let implemented: Vec<_> = ENDPOINTS.iter().collect();
                output::print_diff(&manifest::compute_diff(&manifest, &implemented));
                print_newer_schemas(&manifest);
            }
            false
        }

        Command::Single { endpoint, all, keep_going } => {
            let entries = runner::select(endpoint.as_deref(), all)?;
            let pb = output::progress(entries.len());
            let outcomes = runner::run_single(&entries, &clients, keep_going, &pb).await;
            pb.finish_and_clear();
            output::print_single_results(&outcomes) > 0
        }

        Command::Full { endpoint, all, concurrency } => {
            let entries = runner::select(endpoint.as_deref(), all)?;
            let pb = output::progress(entries.len());
            let outcomes = runner::run_full(&entries, &clients, concurrency, &pb).await;
            pb.finish_and_clear();
            output::print_full_results(&outcomes) > 0
        }

        Command::Check { endpoint, all, pinned_only, all_langs, verbose } => {
            let entries = runner::select(endpoint.as_deref(), all)?;
            let manifest = manifest::fetch_manifest(DEFAULT_SCHEMA_VERSION).await?;
            print_newer_schemas(&manifest);
            let routes = manifest.by_path();

            let schemas: &[Schema] = if pinned_only {
                &[Schema::Pinned]
            } else {
                &[Schema::Pinned, Schema::Latest]
            };
            let mut jobs = Vec::new();
            for &entry in &entries {
                let localized = routes.get(entry.path).is_some_and(|r| r.lang);
                let langs: &[Language] = if all_langs && localized { &Language::ALL } else { &[Language::En] };
                for &schema in schemas {
                    for &lang in langs {
                        jobs.push(Job { entry, schema, lang });
                    }
                }
            }

            let pb = output::progress(jobs.len());
            let results = check::run(&clients, jobs, &pb).await;
            pb.finish_and_clear();
            output::print_check_results(&results, verbose) > 0
        }
    };

    if failed {
        std::process::exit(1);
    }
    Ok(())
}

fn print_newer_schemas(manifest: &manifest::LiveManifest) {
    if manifest.newer_schemas.is_empty() {
        println!("Schema pin {DEFAULT_SCHEMA_VERSION} is the latest.\n");
        return;
    }
    println!("Schema versions newer than the pin {DEFAULT_SCHEMA_VERSION}:");
    for (v, desc) in &manifest.newer_schemas {
        println!("  {v}  {desc}");
    }
    println!();
}
