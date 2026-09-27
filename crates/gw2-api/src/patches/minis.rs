//! Corrections for known bugs in the `/v2/minis` endpoint.
//!
//! ## Bug: 33 miniatures return `name: "((208738))"` and `item_id: 6`
//!
//! `((208738))` is an unresolved localization placeholder — the API fails to
//! substitute the actual string. Item ID 6 is a generic sentinel value.
//! The correct data is available from the reverse direction (`/v2/items`),
//! where the item's `details.minipet_id` points back to the correct mini.
//!
//! Source: <https://wiki.guildwars2.com/wiki/API:2/minis>
//!
//! ## Bug: Mini Mosquito and "Mini Revenant Rytlock" share ID 265
//!
//! According to the API, mini ID 265 is "Mini Revenant Rytlock" (unobtainable).
//! Mini Mosquito (item #64216) is listed under the same ID. The `/v2/minis`
//! endpoint resolves 265 to the Revenant Rytlock entry.
//!
//! Source: <https://wiki.guildwars2.com/wiki/API:2/minis>

use crate::resource::Patchable;
use crate::endpoints::items::ItemId;
use crate::endpoints::minis::Mini;

/// Corrections for minis with broken name and item_id.
///
/// Each entry is `(mini_id, correct_name, correct_item_id)`.
/// Verified against `/v2/items` responses and the GW2 wiki bug table.
///
/// Last verified: 2026-03-02 (33 affected minis, unchanged since September 2021).
const NAME_AND_ITEM_PATCHES: &[(u32, &str, u32)] = &[
    (599,  "Mini Spooky Skimmer",               85467),
    (602,  "Mini Kormeerkat",                    85517),
    (605,  "Mini Spooky Raptor",                 85435),
    (609,  "Mini Spooky Springer",               85458),
    (610,  "Mini Spooky Griffon",                85461),
    (611,  "Mini Spooky Jackal",                 85401),
    (627,  "Mini Cozy Wintersday Raptor",         86693),
    (629,  "Mini Cozy Wintersday Springer",       86644),
    (631,  "Mini Cozy Wintersday Skimmer",        86609),
    (633,  "Mini Cozy Wintersday Jackal",         86692),
    (635,  "Mini Cozy Wintersday Griffon",        86581),
    (648,  "Mini Resplendent Avialan Raptor",     86950),
    (649,  "Mini Summit Wildhorn Springer",       86939),
    (652,  "Mini Grand Lion Griffon",             86927),
    (653,  "Mini Reforged Warhound Jackal",       86956),
    (654,  "Mini Lucky Lantern Puppy",            86958),
    (655,  "Mini Umbral Demon Skimmer",           86948),
    (657,  "Mini Branded Griffon",                87133),
    (660,  "Mini Branded Raptor",                 87236),
    (661,  "Mini Branded Springer",               87156),
    (662,  "Mini Branded Jackal",                 87141),
    (663,  "Mini Branded Skimmer",                87203),
    (689,  "Mini Awakened Griffon",               87934),
    (691,  "Mini Awakened Springer",              87760),
    (693,  "Mini Awakened Jackal",                87626),
    (694,  "Mini Awakened Skimmer",               87753),
    (699,  "Mini Awakened Raptor",                87778),
    (709,  "Mini Exo-Suit Griffon",               88389),
    (713,  "Mini Exo-Suit Jackal",                88469),
    (714,  "Mini Exo-Suit Skimmer",               88437),
    (715,  "Mini Exo-Suit Raptor",                88402),
    (717,  "Mini Exo-Suit Springer",              88464),
    (747,  "Mini Shrine Guardian",                90009),
];

impl Patchable for Mini {
    fn patch(&mut self) {
        let raw_id = self.id.0;
        if let Some(&(_, name, item_id)) = NAME_AND_ITEM_PATCHES
            .iter()
            .find(|&&(id, _, _)| id == raw_id)
        {
            self.name = name.to_owned();
            self.unlock_item = ItemId(item_id);
        }
    }
}

// ── Patch verification tests ─────────────────────────────────────────────────
//
// Run with: cargo test -p gw2-api --features verify-patches
//
// These tests hit the live GW2 API and assert that:
//   1. The bug still exists (entries in the table still return wrong data).
//   2. The correct values in the table still match what /v2/items reports.
//   3. No new bugged minis have appeared that the table doesn't cover.
//
// They are compiled and run only under the `verify-patches` feature flag so
// they never slow down normal `cargo test` runs.

#[cfg(all(test, feature = "verify-patches"))]
mod verify {
    use super::NAME_AND_ITEM_PATCHES;

    const BUGGY_NAME: &str = "((208738))";
    const BUGGY_ITEM_ID: u32 = 6;
    const API_BASE: &str = "https://api.guildwars2.com/v2";

    /// Raw mini as returned by /v2/minis (before any patch is applied).
    #[derive(serde::Deserialize, Debug)]
    struct RawMini {
        id: u32,
        name: String,
        item_id: u32,
    }

    /// Relevant fields from /v2/items for a mini unlock item.
    ///
    /// These are Consumable/Unlock items — they carry the correct mini name but
    /// do not have a back-reference `minipet_id` field in their details.
    #[derive(serde::Deserialize, Debug)]
    struct RawItem {
        id: u32,
        name: String,
    }

    async fn get_json<T: serde::de::DeserializeOwned>(url: &str) -> T {
        let body = reqwest::get(url)
            .await
            .unwrap_or_else(|e| panic!("GET {url} failed: {e}"))
            .text()
            .await
            .unwrap();
        serde_json::from_str(&body)
            .unwrap_or_else(|e| panic!("Failed to parse response from {url}: {e}\nBody: {body}"))
    }

    /// Assert that all table entries still return the buggy sentinel values from
    /// /v2/minis. If any entry has been fixed by ANet, the table entry should be
    /// removed and this test updated.
    #[tokio::test]
    async fn bug_still_present_in_minis_endpoint() {
        let patched_ids: Vec<u32> = NAME_AND_ITEM_PATCHES.iter().map(|&(id, _, _)| id).collect();
        let ids_param = patched_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let url = format!("{API_BASE}/minis?ids={ids_param}");
        let minis: Vec<RawMini> = get_json(&url).await;

        let mut fixed = Vec::new();
        for mini in &minis {
            if mini.name != BUGGY_NAME || mini.item_id != BUGGY_ITEM_ID {
                fixed.push(format!(
                    "  mini {} now returns name={:?}, item_id={} (no longer bugged?)",
                    mini.id, mini.name, mini.item_id
                ));
            }
        }

        assert!(
            fixed.is_empty(),
            "Some patched minis appear to have been fixed by ANet. \
             Remove them from NAME_AND_ITEM_PATCHES and update the 'Last verified' date:\n{}",
            fixed.join("\n")
        );
    }

    /// Assert that the correct names and item IDs in our table still match what
    /// /v2/items reports for those item IDs. Catches the case where ANet updates
    /// an item name without fixing the minis endpoint.
    #[tokio::test]
    async fn correct_values_still_match_items_endpoint() {
        let item_ids: Vec<u32> = NAME_AND_ITEM_PATCHES
            .iter()
            .map(|&(_, _, item_id)| item_id)
            .collect();
        let ids_param = item_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let url = format!("{API_BASE}/items?ids={ids_param}");
        let items: Vec<RawItem> = get_json(&url).await;

        let mut mismatches = Vec::new();
        for &(mini_id, expected_name, expected_item_id) in NAME_AND_ITEM_PATCHES {
            let item = items.iter().find(|i| i.id == expected_item_id);
            match item {
                None => mismatches.push(format!(
                    "  mini {mini_id}: item {expected_item_id} not found in /v2/items"
                )),
                Some(item) => {
                    if item.name != expected_name {
                        mismatches.push(format!(
                            "  mini {mini_id}: expected name {:?} but /v2/items/{} now returns {:?}",
                            expected_name, expected_item_id, item.name
                        ));
                    }
                }
            }
        }

        assert!(
            mismatches.is_empty(),
            "Patch table values no longer match /v2/items. \
             Update NAME_AND_ITEM_PATCHES and the 'Last verified' date:\n{}",
            mismatches.join("\n")
        );
    }

    /// Assert that no new bugged minis have appeared that our table doesn't cover.
    /// Fetches all minis and reports any with the buggy sentinel values that are
    /// absent from NAME_AND_ITEM_PATCHES.
    #[tokio::test]
    async fn no_new_bugged_minis() {
        let url = format!("{API_BASE}/minis?ids=all");
        let all_minis: Vec<RawMini> = get_json(&url).await;

        let patched_ids: std::collections::HashSet<u32> =
            NAME_AND_ITEM_PATCHES.iter().map(|&(id, _, _)| id).collect();

        let uncovered: Vec<String> = all_minis
            .iter()
            .filter(|m| m.name == BUGGY_NAME && m.item_id == BUGGY_ITEM_ID)
            .filter(|m| !patched_ids.contains(&m.id))
            .map(|m| format!("  mini {} (name={:?}, item_id={})", m.id, m.name, m.item_id))
            .collect();

        assert!(
            uncovered.is_empty(),
            "New bugged minis found that are not in NAME_AND_ITEM_PATCHES. \
             Add them to the table (correct data available via /v2/items):\n{}",
            uncovered.join("\n")
        );
    }
}
