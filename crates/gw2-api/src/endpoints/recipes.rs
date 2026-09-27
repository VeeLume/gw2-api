//! `/v2/recipes` endpoint — types, IDs, and endpoint handle.

use gw2_api_macros::{gw2_endpoint, gw2_enum};
use serde::{Deserialize, Serialize};

use crate::common::CraftingDiscipline;
use crate::endpoints::items::ItemId;
use crate::error::Gw2ApiError;

// ── Types ─────────────────────────────────────────────────────────────────────

/// A crafting recipe.
#[gw2_endpoint(path = "recipes", id_type = u32, paged)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub id: RecipeId,
    #[serde(rename = "type")]
    pub recipe_type: String,
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
}

/// An ingredient required for a recipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ingredient {
    pub id: ItemId,
    pub count: u32,
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
