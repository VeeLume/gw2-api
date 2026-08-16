// examples/main.rs
use gw2_api::models::{build::Build, minis::Mini};
use gw2_api::prelude::*; // <-- your crate name here // adjust path to your crate

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Ghost (singleton)
    let build = Build::get(false).await?; // cache-first
    println!("Current build ID: {}", build.id);
    let build_fresh = Build::get(true).await?; // force refetch
    println!("Current build ID (fresh): {}", build_fresh.id);
    println!(
        "Builds are the same instance: {}",
        std::ptr::eq(&build, &build_fresh)
    );

    // GhostId
    let m = Mini::get(747, false).await?;
    println!("Mini ID 747: {} (unlock item: {:?})", m.name, m.unlock_item);
    let m_fresh = Mini::get(747, true).await?;
    println!(
        "Mini ID 747 (fresh): {} (unlock item: {:?})",
        m_fresh.name, m_fresh.unlock_item
    );
    println!(
        "Minis are the same instance: {}",
        std::ptr::eq(&m, &m_fresh)
    );

    let many = Mini::get_many(&[717, 747, 999], false).await?;
    for mini in many {
        println!("Mini ID {}: {}", mini.id, mini.name);
    }
    let page = Mini::get_page(0, 200, false).await?;
    println!(
        "First mini on page 0: {}",
        page.first().map(|m| &m.name).unwrap_or(&"none".to_string())
    );
    let pages = Mini::get_pages(0, 3, 200, false).await?;
    println!("Number of minis in first 3 pages: {}", pages.len());
    let all = Mini::get_all(true, false).await?;
    println!("Total number of minis: {}", all.len());

    // Cache inspection and clearing
    println!("Number of cached Minis: {}", Mini::cache_len().await);
    println!(
        "First Mini ID 747 is not in the cache: {}",
        std::ptr::eq(&m, &Mini::get(747, false).await?)
    );
    println!(
        "Fresh Mini ID 747 is in the cache: {}",
        !std::ptr::eq(&m, &Mini::get(747, true).await?)
    );
    println!("Mini ID 747 is cached: {}", Mini::is_cached(&747).await);

    println!("Build is cached: {}", Build::is_cached().await);

    Mini::cache_len().await;
    Mini::cache_clear().await;
    Build::cache_clear().await;

    println!("Caches cleared.");
    println!("Number of cached Minis: {}", Mini::cache_len().await);

    println!("Mini ID 747 is cached: {}", Mini::is_cached(&747).await);
    println!("Build is cached: {}", Build::is_cached().await);

    // Mounts:

    let m = gw2_api::models::mounts::MountType::get("raptor".to_string(), false).await?;
    println!("Mount 'raptor': {}", m.id);
    println!("Mount ID 1: {}", m.name);
    println!("Default skin ID: {}", m.default_skin);
    println!("Default skin ID: {}", m.default_skin.id());

    let default_skin = m.default_skin.get(false).await?;

    println!("Default skin for Mount ID 1: {}", default_skin.name);

    Ok(())
}
