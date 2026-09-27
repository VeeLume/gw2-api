//! `/v2/recipes` endpoint — types, IDs, and endpoint handle.

use gw2_api_macros::{gw2_endpoint, gw2_enum, gw2_tagged_union};
use serde::{Deserialize, Serialize};

use crate::common::CraftingDiscipline;
use crate::endpoints::currencies::CurrencyId;
use crate::endpoints::items::ItemId;
use crate::error::Gw2ApiError;

// ── Types ─────────────────────────────────────────────────────────────────────

/// A crafting recipe.
#[gw2_endpoint(path = "recipes", id_type = u32, paged)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub id: RecipeId,
    #[serde(rename = "type")]
    pub recipe_type: RecipeType,
    #[serde(rename = "output_item_id")]
    pub output_item: ItemId,
    pub output_item_count: u32,
    pub min_rating: u32,
    pub time_to_craft_ms: u64,
    #[serde(default)]
    pub disciplines: Vec<CraftingDiscipline>,
    #[serde(default)]
    pub flags: Vec<RecipeFlag>,
    #[serde(default)]
    pub ingredients: Vec<Ingredient>,
    /// The guild upgrade a guild recipe produces (decorations, WvW consumables).
    // TODO: GuildUpgradeId once /v2/guild/upgrades is modelled.
    #[serde(default)]
    pub output_upgrade_id: Option<u32>,
    pub chat_link: String,
}

/// An ingredient required for a recipe.
///
/// Since schema 2022-03 an ingredient can be a currency (e.g. recipe 13513 needs
/// 100 of currency 61) or a guild upgrade (guild decorations), not only an item.
/// Serializes with a `"type"` field, like the API.
#[gw2_tagged_union]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum Ingredient {
    Item { id: ItemId, count: u32 },
    Currency { id: CurrencyId, count: u32 },
    /// A guild upgrade id (`/v2/guild/upgrades`, not modelled yet).
    // TODO: GuildUpgradeId once /v2/guild/upgrades is modelled.
    GuildUpgrade { id: u32, count: u32 },
    Unknown { type_: String },
}

impl Ingredient {
    /// How many are needed, `None` for an unrecognised ingredient type.
    pub fn count(&self) -> Option<u32> {
        match self {
            Self::Item { count, .. } | Self::Currency { count, .. } | Self::GuildUpgrade { count, .. } => {
                Some(*count)
            }
            Self::Unknown { .. } => None,
        }
    }
}

/// What a recipe produces. Values from the wiki; all live values are covered.
#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RecipeType {
    // Weapons
    Axe,
    Dagger,
    Focus,
    Greatsword,
    Hammer,
    Harpoon,
    LongBow,
    Mace,
    Pistol,
    Rifle,
    Scepter,
    Shield,
    ShortBow,
    Speargun,
    Staff,
    Sword,
    Torch,
    Trident,
    Warhorn,
    // Armor
    Boots,
    Coat,
    Gloves,
    Helm,
    Leggings,
    Shoulders,
    // Trinkets
    Amulet,
    Earring,
    Ring,
    // Food
    Dessert,
    Feast,
    Food,
    IngredientCooking,
    Meal,
    Seasoning,
    Snack,
    Soup,
    // Crafting components
    Component,
    Inscription,
    Insignia,
    LegendaryComponent,
    // Refinement
    Refinement,
    RefinementEctoplasm,
    RefinementObsidian,
    // Guild
    GuildConsumable,
    GuildConsumableWvw,
    GuildDecoration,
    // Other
    Backpack,
    Bag,
    Bulk,
    Consumable,
    Dye,
    Potion,
    UpgradeComponent,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RecipeFlag {
    AutoLearned,
    LearnedFromItem,
}

// ── Search namespace ──────────────────────────────────────────────────────────

/// Navigation handle for `/v2/recipes/search`.
// recipes/search returns an error without ?input/output params — no registry entry.
// path "recipes/search" → SearchEndpoint on RecipesEndpoint
#[gw2_endpoint(path = "recipes/search", namespace, no_registry)]
pub struct Search;

// ── Search methods ────────────────────────────────────────────────────────────

/// Search for recipes that use a given item as an ingredient.
// fn name "input" != last seg "search" → attaches to SearchEndpoint
#[gw2_endpoint(path = "recipes/search", test_params(input = ItemId(19976u32)))]
pub async fn input(
    &self,
    input: impl ::std::convert::Into<ItemId>,
) -> Result<Vec<RecipeId>, Gw2ApiError> {
    self.0
        .request("/recipes/search")
        .param("input", input.into())
        .send()
        .await
}

/// Search for recipes that produce a given item.
// fn name "output" != last seg "search" → attaches to SearchEndpoint
#[gw2_endpoint(path = "recipes/search", test_params(output = ItemId(19976u32)))]
pub async fn output(
    &self,
    output: impl ::std::convert::Into<ItemId>,
) -> Result<Vec<RecipeId>, Gw2ApiError> {
    self.0
        .request("/recipes/search")
        .param("output", output.into())
        .send()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingredients_keep_their_type() {
        let json = r#"{"id":13513,"type":"Consumable","output_item_id":1,"output_item_count":1,"time_to_craft_ms":0,"disciplines":["Chef"],"min_rating":0,"flags":[],"chat_link":"[&CQEAAAA=]",
            "ingredients":[{"type":"Item","id":19726,"count":2},{"type":"Currency","id":61,"count":100},{"type":"GuildUpgrade","id":279,"count":1}]}"#;
        let r: Recipe = serde_json::from_str(json).unwrap();
        assert!(matches!(r.ingredients[0], Ingredient::Item { ref id, count: 2 } if id.0 == 19726));
        assert!(matches!(r.ingredients[1], Ingredient::Currency { ref id, count: 100 } if id.0 == 61));
        assert!(matches!(r.ingredients[2], Ingredient::GuildUpgrade { id: 279, count: 1 }));
        assert_eq!(
            serde_json::to_value(&r.ingredients[1]).unwrap(),
            serde_json::json!({"type": "Currency", "id": 61, "count": 100})
        );
    }
}
