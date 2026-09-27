// examples/basic.rs — run with: cargo run --example basic
//
// Hits the live API. The account section only runs if GW2_API_KEY is set.

use std::sync::Arc;

use gw2_api::models::{build::Build, minis::Mini, mounts::MountType, tokeninfo::Permission};
use gw2_api::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ---------- static game data: no key, no session, process-wide cache ----------

    let build = Build::get(false).await?; // cache-first
    println!("Current build ID: {}", build.id);
    let build_fresh = Build::get(true).await?; // force refetch
    println!(
        "Same Arc after a forced refetch: {}",
        Arc::ptr_eq(&build, &build_fresh)
    );

    let m = Mini::get(747, false).await?;
    println!("Mini 747: {} (unlock item: {:?})", m.name, m.unlock_item);

    let many = Mini::get_many(&[717, 747, 999_999], false).await?;
    println!(
        "Asked for 3 ids, got {} back (unknown ids are simply absent)",
        many.len()
    );

    let all = Mini::get_all(true).await?;
    println!("Total minis: {}", all.len());
    println!("Cached minis: {}", Mini::cache_len().await);

    // The cache is keyed by (Language, Id), so switching language re-fetches
    // instead of handing back the English name.
    //
    // Note: not mini 747 — that one is in the BUGGED_NAMES table, where the API
    // returns a placeholder in every language and the crate substitutes a
    // hardcoded English name. It would look like the language key was broken.
    let m_en = Mini::get(3, false).await?;
    println!("Mini 3 in English: {}", m_en.name);
    static_client().set_language(Language::De);
    let m_de = Mini::get(3, false).await?;
    println!("Mini 3 auf Deutsch:  {}", m_de.name);
    static_client().set_language(Language::En);

    // Typed ids still fetch themselves.
    let raptor = MountType::get("raptor".to_string(), false).await?;
    let skin = raptor.default_skin.get(false).await?;
    println!("Raptor default skin: {}", skin.name);

    println!(
        "Modelled rate budget left: {:.0}",
        static_client().limiter().modelled_remaining()
    );

    // ---------- account data: needs a session ----------

    match std::env::var("GW2_API_KEY") {
        Ok(key) if !key.is_empty() => {
            let session = Gw2Session::new(key)?;
            let info = session.tokeninfo().await?;
            println!(
                "Key \"{}\" has {} permissions",
                info.name,
                info.permissions.len()
            );
            match info.require(&Permission::Wallet) {
                Ok(()) => println!("  wallet scope: present"),
                Err(e) => println!("  {e}"),
            }
        }
        _ => println!("Set GW2_API_KEY to exercise the authenticated path."),
    }

    Ok(())
}
