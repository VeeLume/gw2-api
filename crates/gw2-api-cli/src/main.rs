mod cli;
mod manifest;
mod output;
mod runner;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};

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
    let key = cli.key.as_deref();

    match cli.command {
        Command::List { diff } => {
            output::print_list();

            if diff {
                println!("\nFetching live GW2 API manifest…");
                let manifest = manifest::fetch_manifest().await?;
                let implemented: Vec<&str> =
                    gw2_api::registry::ENDPOINTS.iter().map(|e| e.path).collect();
                let d = manifest::compute_diff(&manifest, &implemented);
                output::print_diff(&d);
            }
        }

        Command::Single { endpoint, all, keep_going } => {
            if let Some(path) = endpoint {
                let entry = runner::find_endpoint(&path)?;
                let pb = output::single_progress();
                pb.set_message(format!("testing {}", entry.path));
                let outcome = runner::run_single_one(entry, key).await;
                pb.finish_and_clear();
                output::print_single_results(std::slice::from_ref(&outcome));
            } else if all {
                let outcomes = runner::run_single_all(key, keep_going).await;
                output::print_single_results(&outcomes);
            } else {
                eprintln!("Specify --endpoint <path> or --all");
                std::process::exit(1);
            }
        }

        Command::Full { endpoint, all, concurrency } => {
            if let Some(path) = endpoint {
                let entry = runner::find_endpoint(&path)?;
                let total = gw2_api::registry::ENDPOINTS.len() as u64;
                let pb = output::full_progress(total);
                pb.set_message(entry.path);
                let outcome = runner::run_full_one(entry, key).await;
                pb.finish_and_clear();
                output::print_full_results(std::slice::from_ref(&outcome));
            } else if all {
                let total = gw2_api::registry::ENDPOINTS.len() as u64;
                let pb = output::full_progress(total);
                let outcomes =
                    runner::run_full_all(key, concurrency).await;
                pb.finish_and_clear();
                output::print_full_results(&outcomes);
            } else {
                eprintln!("Specify --endpoint <path> or --all");
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
