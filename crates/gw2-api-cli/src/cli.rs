//! CLI argument definitions using clap derive.

use clap::{Parser, Subcommand};

/// Developer tooling for the GW2 API wrapper.
///
/// Reads, tests, and validates registered endpoints via the real Gw2Client code path.
#[derive(Debug, Parser)]
#[command(name = "gw2-api-cli", version, about, long_about = None)]
pub struct Cli {
    /// GW2 API key. Reads `GW2_API_KEY` env var when omitted.
    #[arg(long, short = 'k', global = true, env = "GW2_API_KEY")]
    pub key: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List all registered endpoints with path, Rust type, kind, and auth status.
    ///
    /// Pass --diff to also fetch the live /v2.json manifest and show coverage gaps.
    List {
        /// Compare against the live GW2 API manifest and show missing/extra endpoints.
        #[arg(long)]
        diff: bool,
    },

    /// Run one representative call per endpoint through the real Gw2Client.
    ///
    /// Fast — validates the full request stack (rate limiter, retry, auth, patch).
    Single {
        /// Only test this endpoint (exact path, e.g. "items" or "account/wallet").
        #[arg(long, conflicts_with = "all")]
        endpoint: Option<String>,

        /// Test all registered endpoints.
        #[arg(long, conflicts_with = "endpoint")]
        all: bool,

        /// Continue testing after a failure instead of stopping.
        #[arg(long)]
        keep_going: bool,
    },

    /// Fetch and deserialize all data for one or more endpoints.
    ///
    /// Slow — uses the same bulk deserialization path as the library.
    Full {
        /// Only fetch this endpoint (exact path, e.g. "achievements").
        #[arg(long, conflicts_with = "all")]
        endpoint: Option<String>,

        /// Fetch all registered endpoints.
        #[arg(long, conflicts_with = "endpoint")]
        all: bool,

        /// Maximum number of endpoints to fetch concurrently.
        #[arg(long, default_value = "4")]
        concurrency: usize,
    },

    /// Parse every raw item against the model, at the pinned and the `latest` schema.
    ///
    /// Reports parse failures per item id, fields the model ignores, and ids the
    /// index lists but `?ids=` does not return. Exits 1 on failures at the pinned
    /// schema; failures at `latest` only warn.
    Check {
        /// Only check this endpoint (exact path, e.g. "mounts/skins").
        #[arg(long, conflicts_with = "all")]
        endpoint: Option<String>,

        /// Check all registered endpoints.
        #[arg(long, conflicts_with = "endpoint")]
        all: bool,

        /// Skip the run at schema `latest`.
        #[arg(long)]
        pinned_only: bool,

        /// Also check every other language, for routes the manifest marks as localized.
        #[arg(long)]
        all_langs: bool,

        /// Show every ignored field instead of the first 10 per endpoint.
        #[arg(long)]
        verbose: bool,
    },
}
