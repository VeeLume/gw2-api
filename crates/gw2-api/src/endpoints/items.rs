//! `/v2/items` endpoint — types, IDs, and endpoint handle.

use gw2_api_macros::{gw2_endpoint, gw2_enum};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::coin::{Coin, coin_from_u32};
use crate::common::{GameType, ItemFlag, Rarity};

// ── Item ──────────────────────────────────────────────────────────────────────

/// A GW2 item.
///
/// `details` is parsed according to the outer `type` (see [`ItemDetails::for_item_type`]),
/// because several inner detail types (`"Immediate"`, `"Default"`) are shared between
/// item categories. `remote = "Self"` turns the derives into inherent functions that
/// the hand-written trait impls below wrap.
#[gw2_endpoint(path = "items", id_type = u32, paged)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(remote = "Self")]
pub struct Item {
    pub id: ItemId,
    pub chat_link: String,
    pub name: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub item_type: ItemType,
    pub rarity: Rarity,
    pub level: u32,
    #[serde(deserialize_with = "coin_from_u32")]
    pub vendor_value: Coin,
    #[serde(default)]
    pub default_skin: Option<crate::endpoints::skins::SkinId>,
    #[serde(default)]
    pub flags: Vec<ItemFlag>,
    #[serde(default)]
    pub game_types: Vec<GameType>,
    #[serde(default)]
    pub restrictions: Vec<ItemRestriction>,
    #[serde(default)]
    pub upgrades_into: Vec<ItemUpgrade>,
    #[serde(default)]
    pub upgrades_from: Vec<ItemUpgrade>,
    /// Type-specific details, keyed by `item_type`.
    #[serde(default)]
    pub details: Option<ItemDetails>,
}

impl Serialize for Item {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        Item::serialize(self, s)
    }
}

impl<'de> Deserialize<'de> for Item {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let mut v = Value::deserialize(d)?;
        let raw_details = v.as_object_mut().and_then(|m| m.remove("details"));
        let mut item = if crate::ignored::active() {
            let mut report = |path: serde_ignored::Path| crate::ignored::report(format!("Item.{path}"));
            Item::deserialize(serde_ignored::Deserializer::new(v, &mut report))
        } else {
            Item::deserialize(v)
        }
        .map_err(serde::de::Error::custom)?;
        item.details = raw_details
            .filter(|d| !d.is_null())
            .map(|d| ItemDetails::for_item_type(&item.item_type, d))
            .transpose()
            .map_err(serde::de::Error::custom)?;
        Ok(item)
    }
}

/// Item type discriminator (from the item's top-level `"type"` field).
#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ItemType {
    Armor,
    Back,
    Bag,
    Consumable,
    Container,
    CraftingMaterial,
    Gathering,
    Gizmo,
    JadeTechModule,
    Key,
    MiniPet,
    PowerCore,
    Relic,
    SensoryArray,
    ServiceChip,
    Tool,
    Trait,
    Trinket,
    Trophy,
    UpgradeComponent,
    Weapon,
}

/// Race/profession restrictions on an item.
#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ItemRestriction {
    Asura,
    Charr,
    Female,
    Human,
    Norn,
    Revenant,
    Sylvari,
    Elementalist,
    Engineer,
    Guardian,
    Mesmer,
    Necromancer,
    Ranger,
    Thief,
    Warrior,
}

/// How an item can be upgraded into another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemUpgrade {
    pub upgrade: UpgradeMethod,
    #[serde(rename = "item_id")]
    pub item: ItemId,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UpgradeMethod {
    Attunement,
    Infusion,
}

// ── Subobjects ────────────────────────────────────────────────────────────────

/// An infusion slot on gear.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfusionSlot {
    #[serde(default)]
    pub flags: Vec<InfusionSlotFlag>,
    #[serde(default, rename = "item_id")]
    pub item: Option<ItemId>,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InfusionSlotFlag {
    Enrichment,
    Infusion,
}

/// Infix upgrade — stat bonuses applied by an upgrade component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfixUpgrade {
    /// Itemstat id resolvable via `/v2/itemstats`.
    #[serde(default, rename = "id")]
    pub itemstat: Option<crate::endpoints::itemstats::ItemStatId>,
    #[serde(default)]
    pub attributes: Vec<AttributeBonus>,
    #[serde(default)]
    pub buff: Option<InfixBuff>,
}

/// A single attribute bonus within an infix upgrade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeBonus {
    pub attribute: AttributeType,
    pub modifier: i32,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AttributeType {
    AgonyResistance,
    BoonDuration,
    ConditionDamage,
    ConditionDuration,
    CritDamage,
    Healing,
    Power,
    Precision,
    Toughness,
    Vitality,
}

/// Additional buff effect in an infix upgrade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfixBuff {
    #[serde(default, rename = "skill_id")]
    pub skill: Option<crate::endpoints::skills::SkillId>,
    pub description: Option<String>,
}

/// `Option<ItemId>` that also accepts `""` as `None`.
fn item_id_or_empty<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<ItemId>, D::Error> {
    match Option::<Value>::deserialize(d)? {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.is_empty() => Ok(None),
        Some(v) => ItemId::deserialize(v).map(Some).map_err(serde::de::Error::custom),
    }
}

// ── ItemDetails ───────────────────────────────────────────────────────────────

/// Type-specific item details.
///
/// Serialized as `{"type":"<subtype>", ...}` within the `details` JSON object.
/// The outer item `"type"` field (e.g. `"Weapon"`, `"Armor"`) selects which
/// variant to use; the inner `"type"` field carries the sub-discriminator
/// (e.g. `"LongBow"`, `"Coat"`).
///
/// Since serde cannot propagate the outer `item_type` into deserialization of
/// `details`, this type implements `Deserialize` manually: each variant's inner
/// struct reads the `"type"` field itself.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ItemDetails {
    Armor(ArmorDetails),
    Back(BackDetails),
    Bag(BagDetails),
    Consumable(ConsumableDetails),
    Container(ContainerDetails),
    Gathering(GatheringDetails),
    Gizmo(GizmoDetails),
    MiniPet(MiniPetDetails),
    Tool(ToolDetails),
    Trinket(TrinketDetails),
    UpgradeComponent(UpgradeComponentDetails),
    Weapon(WeaponDetails),
    /// An unrecognised detail object returned by the API.
    Unknown(Value),
}

impl ItemDetails {
    /// Parse a `details` object for an item of the given outer type.
    ///
    /// This is how [`Item`] parses its details. The inner `"type"` alone is ambiguous:
    /// `"Immediate"` is used by both consumables and containers, `"Default"` by
    /// containers, gizmos and upgrade components. Item types without a dedicated
    /// details struct fall back to the inner-type dispatch of the `Deserialize` impl.
    pub fn for_item_type(item_type: &ItemType, v: Value) -> Result<Self, serde_json::Error> {
        use crate::ignored::from_value;
        Ok(match item_type {
            ItemType::Armor => Self::Armor(from_value("ItemDetails::Armor", v)?),
            ItemType::Back => Self::Back(from_value("ItemDetails::Back", v)?),
            ItemType::Bag => Self::Bag(from_value("ItemDetails::Bag", v)?),
            ItemType::Consumable => Self::Consumable(from_value("ItemDetails::Consumable", v)?),
            ItemType::Container => Self::Container(from_value("ItemDetails::Container", v)?),
            ItemType::Gathering => Self::Gathering(from_value("ItemDetails::Gathering", v)?),
            ItemType::Gizmo => Self::Gizmo(from_value("ItemDetails::Gizmo", v)?),
            ItemType::MiniPet => Self::MiniPet(from_value("ItemDetails::MiniPet", v)?),
            ItemType::Tool => Self::Tool(from_value("ItemDetails::Tool", v)?),
            ItemType::Trinket => Self::Trinket(from_value("ItemDetails::Trinket", v)?),
            ItemType::UpgradeComponent => Self::UpgradeComponent(from_value("ItemDetails::UpgradeComponent", v)?),
            ItemType::Weapon => Self::Weapon(from_value("ItemDetails::Weapon", v)?),
            _ => serde_json::from_value(v)?,
        })
    }
}

// TODO: best effort without the outer item type: "Immediate" consumables become Container,
//       "Default" containers become Gizmo. `Item` uses `for_item_type` and is correct.
impl<'de> Deserialize<'de> for ItemDetails {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;

        // Dispatch on the inner "type" field value, which identifies the sub-type
        // within each item category. Types with no inner "type" field are detected
        // by their unique required fields.
        let type_str = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match type_str {
            // Armor slots
            "Boots" | "Coat" | "Gloves" | "Helm" | "HelmAquatic" | "Leggings" | "Shoulders" => {
                crate::ignored::from_value::<ArmorDetails>("ItemDetails::Armor", v)
                    .map(ItemDetails::Armor)
                    .map_err(serde::de::Error::custom)
            }
            // Weapon types (one-handed, two-handed, aquatic, other)
            "Axe" | "Dagger" | "Focus" | "Greatsword" | "Hammer" | "Harpoon" | "LargeBundle"
            | "LongBow" | "Mace" | "Pistol" | "Rifle" | "Scepter" | "Shield" | "ShortBow"
            | "SmallBundle" | "Speargun" | "Staff" | "Sword" | "Torch" | "Toy" | "ToyTwoHanded"
            | "Trident" | "Warhorn" => crate::ignored::from_value::<WeaponDetails>("ItemDetails::Weapon", v)
                .map(ItemDetails::Weapon)
                .map_err(serde::de::Error::custom),
            // Consumable sub-types
            "AppearanceChange" | "Booze" | "ContractNpc" | "Currency" | "Food" | "Generic"
            | "Halloween" | "Megaphone" | "MountRandomUnlock" | "RandomUnlock"
            | "TeleportToFriend" | "Transmutation" | "Unlock" | "UpgradeRemoval" | "Utility" => {
                crate::ignored::from_value::<ConsumableDetails>("ItemDetails::Consumable", v)
                    .map(ItemDetails::Consumable)
                    .map_err(serde::de::Error::custom)
            }
            // "Immediate" is shared by Container and Consumable — distinguish by required fields.
            "Immediate" => {
                // ConsumableDetails has many optional fields; ContainerDetails has only "type".
                // Try Container first (stricter), then Consumable.
                if let Ok(x) = crate::ignored::from_value::<ContainerDetails>("ItemDetails::Container", v.clone()) {
                    Ok(ItemDetails::Container(x))
                } else {
                    crate::ignored::from_value::<ConsumableDetails>("ItemDetails::Consumable", v)
                        .map(ItemDetails::Consumable)
                        .map_err(serde::de::Error::custom)
                }
            }
            // Container sub-types (excluding "Immediate" handled above)
            "GiftBox" | "OpenUI" => crate::ignored::from_value::<ContainerDetails>("ItemDetails::Container", v)
                .map(ItemDetails::Container)
                .map_err(serde::de::Error::custom),
            // Gathering sub-types
            "Bait" | "Fishing" | "Foraging" | "Logging" | "Lure" | "Mining" => {
                crate::ignored::from_value::<GatheringDetails>("ItemDetails::Gathering", v)
                    .map(ItemDetails::Gathering)
                    .map_err(serde::de::Error::custom)
            }
            // "ContainerKey", "RentableContractNpc", "UnlimitedConsumable" are Gizmo-only.
            "ContainerKey" | "RentableContractNpc" | "UnlimitedConsumable" => {
                crate::ignored::from_value::<GizmoDetails>("ItemDetails::Gizmo", v)
                    .map(ItemDetails::Gizmo)
                    .map_err(serde::de::Error::custom)
            }
            // Trinket sub-types
            "Accessory" | "Amulet" | "Ring" => crate::ignored::from_value::<TrinketDetails>("ItemDetails::Trinket", v)
                .map(ItemDetails::Trinket)
                .map_err(serde::de::Error::custom),
            // Upgrade component sub-types (excluding "Default" handled below)
            "Gem" | "Rune" | "Sigil" => crate::ignored::from_value::<UpgradeComponentDetails>("ItemDetails::UpgradeComponent", v)
                .map(ItemDetails::UpgradeComponent)
                .map_err(serde::de::Error::custom),
            // Tool (always "Salvage")
            "Salvage" => crate::ignored::from_value::<ToolDetails>("ItemDetails::Tool", v)
                .map(ItemDetails::Tool)
                .map_err(serde::de::Error::custom),
            // "Default" is shared by Container, Gizmo, and UpgradeComponent.
            // Distinguish by required fields: UpgradeComponent requires "suffix" + "infix_upgrade";
            // Gizmo has optional vendor_ids; Container has only "type".
            "Default" => {
                if let Ok(x) = crate::ignored::from_value::<UpgradeComponentDetails>("ItemDetails::UpgradeComponent", v.clone()) {
                    Ok(ItemDetails::UpgradeComponent(x))
                } else if let Ok(x) = crate::ignored::from_value::<GizmoDetails>("ItemDetails::Gizmo", v.clone()) {
                    Ok(ItemDetails::Gizmo(x))
                } else {
                    crate::ignored::from_value::<ContainerDetails>("ItemDetails::Container", v)
                        .map(ItemDetails::Container)
                        .map_err(serde::de::Error::custom)
                }
            }
            // No "type" field — distinguish by unique required fields.
            "" => {
                if v.get("minipet_id").is_some() {
                    crate::ignored::from_value::<MiniPetDetails>("ItemDetails::MiniPet", v)
                        .map(ItemDetails::MiniPet)
                        .map_err(serde::de::Error::custom)
                } else if v.get("size").is_some() {
                    crate::ignored::from_value::<BagDetails>("ItemDetails::Bag", v)
                        .map(ItemDetails::Bag)
                        .map_err(serde::de::Error::custom)
                } else {
                    // Back item — all fields optional
                    crate::ignored::from_value::<BackDetails>("ItemDetails::Back", v)
                        .map(ItemDetails::Back)
                        .map_err(serde::de::Error::custom)
                }
            }
            other => {
                tracing::warn!(type_ = other, "unknown ItemDetails type from GW2 API");
                Ok(ItemDetails::Unknown(v))
            }
        }
    }
}

// ── Armor ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmorDetails {
    #[serde(rename = "type")]
    pub armor_type: ArmorType,
    pub weight_class: WeightClass,
    pub defense: u32,
    #[serde(default)]
    pub infusion_slots: Vec<InfusionSlot>,
    #[serde(default)]
    pub attribute_adjustment: f64,
    #[serde(default)]
    pub infix_upgrade: Option<InfixUpgrade>,
    #[serde(default, rename = "suffix_item_id")]
    pub suffix_item: Option<ItemId>,
    /// Second upgrade slot (two-handed weapons). Absent, `""` on older schemas, or an item id.
    #[serde(default, deserialize_with = "item_id_or_empty")]
    pub secondary_suffix_item_id: Option<ItemId>,
    #[serde(default)]
    // TODO: Vec<ItemStatId>.
    pub stat_choices: Vec<u32>,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArmorType {
    Boots,
    Coat,
    Gloves,
    Helm,
    HelmAquatic,
    Leggings,
    Shoulders,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WeightClass {
    Heavy,
    Medium,
    Light,
    Clothing,
}

// ── Back item ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackDetails {
    #[serde(default)]
    pub infusion_slots: Vec<InfusionSlot>,
    #[serde(default)]
    pub attribute_adjustment: f64,
    #[serde(default)]
    pub infix_upgrade: Option<InfixUpgrade>,
    #[serde(default)]
    // TODO: Option<ItemId>, and one name across the details structs (ArmorDetails uses `suffix_item`).
    pub suffix_item_id: Option<u32>,
    /// Second upgrade slot (two-handed weapons). Absent, `""` on older schemas, or an item id.
    #[serde(default, deserialize_with = "item_id_or_empty")]
    pub secondary_suffix_item_id: Option<ItemId>,
    #[serde(default)]
    // TODO: Vec<ItemStatId>.
    pub stat_choices: Vec<u32>,
}

// ── Bag ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BagDetails {
    pub size: u32,
    pub no_sell_or_sort: bool,
}

// ── Consumable ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumableDetails {
    #[serde(rename = "type")]
    pub consumable_type: ConsumableType,
    pub description: Option<String>,
    pub duration_ms: Option<u64>,
    pub unlock_type: Option<UnlockType>,
    // TODO: ColorId once /v2/colors is modelled.
    pub color_id: Option<u32>,
    // TODO: Option<RecipeId>.
    pub recipe_id: Option<u32>,
    #[serde(default)]
    // TODO: Vec<RecipeId>.
    pub extra_recipe_ids: Vec<u32>,
    // TODO: GuildUpgradeId once /v2/guild/upgrades is modelled.
    pub guild_upgrade_id: Option<u32>,
    pub apply_count: Option<u32>,
    pub name: Option<String>,
    pub icon: Option<String>,
    #[serde(default)]
    // TODO: Vec<SkinId>.
    pub skins: Vec<u32>,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConsumableType {
    AppearanceChange,
    Booze,
    ContractNpc,
    Currency,
    Food,
    Generic,
    Halloween,
    Immediate,
    Megaphone,
    MountRandomUnlock,
    RandomUnlock,
    TeleportToFriend,
    Transmutation,
    Unlock,
    UpgradeRemoval,
    Utility,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnlockType {
    BagSlot,
    BankTab,
    BuildLibrarySlot,
    BuildLoadoutTab,
    Champion,
    CollectibleCapacity,
    Content,
    CraftingRecipe,
    Dye,
    GearLoadoutTab,
    GliderSkin,
    JadeBotSkin,
    /// Homestead conjured doorway skins (e.g. 105144). Not on the wiki as of 2026-09-27.
    MagicDoorSkin,
    Minipet,
    /// Mount skins.
    Ms,
    Outfit,
    /// Spelled this way on the wiki, which doubts it exists; no item uses it as of 2026-09-27.
    RandomUlock,
    SharedSlot,
    /// Fashion Template Expansion (106996). Not on the wiki as of 2026-09-27.
    WardrobeTemplateTab,
}

// ── Container ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerDetails {
    #[serde(rename = "type")]
    pub container_type: ContainerType,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ContainerType {
    Default,
    GiftBox,
    Immediate,
    OpenUI,
}

// ── Gathering ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatheringDetails {
    #[serde(rename = "type")]
    pub gathering_type: GatheringType,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GatheringType {
    Bait,
    Fishing,
    Foraging,
    Logging,
    Lure,
    Mining,
}

// ── Gizmo ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GizmoDetails {
    #[serde(rename = "type")]
    pub gizmo_type: GizmoType,
    #[serde(default)]
    // TODO: GuildUpgradeId once /v2/guild/upgrades is modelled.
    pub guild_upgrade_id: Option<u32>,
    #[serde(default)]
    /// Undocumented; ids of the vendor NPCs the gizmo opens. No API endpoint resolves them.
    pub vendor_ids: Vec<u32>,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GizmoType {
    ContainerKey,
    Default,
    RentableContractNpc,
    UnlimitedConsumable,
}

// ── Miniature ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiniPetDetails {
    pub minipet_id: crate::endpoints::minis::MiniId,
}

// ── Tool (Salvage kit) ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDetails {
    /// Always `"Salvage"`.
    #[serde(rename = "type")]
    pub tool_type: ToolType,
    pub charges: u32,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ToolType {
    Salvage,
}

// ── Trinket ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrinketDetails {
    #[serde(rename = "type")]
    pub trinket_type: TrinketType,
    #[serde(default)]
    pub infusion_slots: Vec<InfusionSlot>,
    #[serde(default)]
    pub attribute_adjustment: f64,
    #[serde(default)]
    pub infix_upgrade: Option<InfixUpgrade>,
    #[serde(default)]
    // TODO: Option<ItemId>, and one name across the details structs (ArmorDetails uses `suffix_item`).
    pub suffix_item_id: Option<u32>,
    /// Second upgrade slot (two-handed weapons). Absent, `""` on older schemas, or an item id.
    #[serde(default, deserialize_with = "item_id_or_empty")]
    pub secondary_suffix_item_id: Option<ItemId>,
    #[serde(default)]
    // TODO: Vec<ItemStatId>.
    pub stat_choices: Vec<u32>,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TrinketType {
    Accessory,
    Amulet,
    Ring,
}

// ── Upgrade component ─────────────────────────────────────────────────────────

// TODO: missing `attribute_adjustment` (undocumented, sent on all upgrade components).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeComponentDetails {
    #[serde(rename = "type")]
    pub upgrade_type: UpgradeComponentType,
    #[serde(default)]
    pub flags: Vec<UpgradeComponentFlag>,
    #[serde(default)]
    pub infusion_upgrade_flags: Vec<InfusionUpgradeFlag>,
    pub suffix: String,
    pub infix_upgrade: InfixUpgrade,
    #[serde(default)]
    pub bonuses: Vec<String>,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UpgradeComponentType {
    Default,
    Gem,
    Rune,
    Sigil,
}

/// Which item slots accept this upgrade component.
#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UpgradeComponentFlag {
    Axe,
    Dagger,
    Focus,
    Greatsword,
    Hammer,
    Harpoon,
    HeavyArmor,
    LightArmor,
    LongBow,
    Mace,
    MediumArmor,
    Pistol,
    Rifle,
    Scepter,
    Shield,
    ShortBow,
    Speargun,
    Staff,
    Sword,
    Torch,
    Trinket,
    Trident,
    Warhorn,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InfusionUpgradeFlag {
    Enrichment,
    Infusion,
}

// ── Weapon ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponDetails {
    #[serde(rename = "type")]
    pub weapon_type: WeaponType,
    pub damage_type: DamageType,
    pub min_power: u32,
    pub max_power: u32,
    pub defense: u32,
    #[serde(default)]
    pub infusion_slots: Vec<InfusionSlot>,
    #[serde(default)]
    pub attribute_adjustment: f64,
    #[serde(default)]
    pub infix_upgrade: Option<InfixUpgrade>,
    #[serde(default)]
    // TODO: Option<ItemId>, and one name across the details structs (ArmorDetails uses `suffix_item`).
    pub suffix_item_id: Option<u32>,
    /// Second upgrade slot (two-handed weapons). Absent, `""` on older schemas, or an item id.
    #[serde(default, deserialize_with = "item_id_or_empty")]
    pub secondary_suffix_item_id: Option<ItemId>,
    #[serde(default)]
    // TODO: Vec<ItemStatId>.
    pub stat_choices: Vec<u32>,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WeaponType {
    Axe,
    Dagger,
    Focus,
    Greatsword,
    Hammer,
    Harpoon,
    LargeBundle,
    LongBow,
    Mace,
    Pistol,
    Rifle,
    Scepter,
    Shield,
    ShortBow,
    SmallBundle,
    Speargun,
    Staff,
    Sword,
    Torch,
    Toy,
    ToyTwoHanded,
    Trident,
    Warhorn,
}

#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DamageType {
    Choking,
    Fire,
    Ice,
    Lightning,
    Physical,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::ItemFlag;

    #[test]
    fn unknown_item_flag_deserializes_to_unknown() {
        let flags: Vec<ItemFlag> =
            serde_json::from_str(r#"["AccountBound","BrandNewFlagFromAnet"]"#).unwrap();
        assert_eq!(flags[0], ItemFlag::AccountBound);
        assert_eq!(
            flags[1],
            ItemFlag::Unknown("BrandNewFlagFromAnet".to_string())
        );
    }

    #[test]
    fn unknown_item_flag_serializes_raw_string() {
        let flag = ItemFlag::Unknown("FutureFlag".to_string());
        assert_eq!(serde_json::to_string(&flag).unwrap(), r#""FutureFlag""#);
    }

    // Details JSON is the `details` sub-object from the GW2 API.
    // The `"type"` field within details is the sub-type discriminator.

    #[test]
    fn armor_details_deserializes() {
        let json = r#"{"type":"Coat","weight_class":"Heavy","defense":300,"infusion_slots":[],"attribute_adjustment":0.0,"secondary_suffix_item_id":""}"#;
        let details: ItemDetails = serde_json::from_str(json).unwrap();
        match details {
            ItemDetails::Armor(a) => {
                assert_eq!(a.armor_type, ArmorType::Coat);
                assert_eq!(a.weight_class, WeightClass::Heavy);
            }
            other => panic!("expected Armor, got {:?}", other),
        }
    }

    #[test]
    fn weapon_details_deserializes() {
        // The details object "type" is the weapon slot (e.g. "LongBow").
        let json = r#"{
            "type": "LongBow",
            "damage_type": "Physical",
            "min_power": 385,
            "max_power": 452,
            "defense": 0,
            "infusion_slots": [],
            "infix_upgrade": {
                "attributes": [
                    { "attribute": "Power", "modifier": 62 },
                    { "attribute": "Precision", "modifier": 44 }
                ]
            },
            "suffix_item_id": 24547,
            "secondary_suffix_item_id": ""
        }"#;
        let details: ItemDetails = serde_json::from_str(json).unwrap();
        match details {
            ItemDetails::Weapon(w) => {
                assert_eq!(w.weapon_type, WeaponType::LongBow);
                assert_eq!(w.damage_type, DamageType::Physical);
                assert_eq!(w.min_power, 385);
            }
            other => panic!("expected Weapon, got {:?}", other),
        }
    }

    #[test]
    fn consumable_food_deserializes() {
        // The details object "type" is the consumable sub-type.
        let json = r#"{
            "type": "Food",
            "duration_ms": 1800000,
            "apply_count": 1,
            "name": "Nourishment",
            "icon": "https://render.guildwars2.com/file/foo/bar.png",
            "description": "30% Magic Find"
        }"#;
        let details: ItemDetails = serde_json::from_str(json).unwrap();
        match details {
            ItemDetails::Consumable(c) => {
                assert_eq!(c.consumable_type, ConsumableType::Food);
                assert_eq!(c.duration_ms, Some(1800000));
            }
            other => panic!("expected Consumable, got {:?}", other),
        }
    }

    #[test]
    fn bag_details_deserializes() {
        let json = r#"{"size":20,"no_sell_or_sort":false}"#;
        let details: ItemDetails = serde_json::from_str(json).unwrap();
        match details {
            ItemDetails::Bag(b) => assert_eq!(b.size, 20),
            other => panic!("expected Bag, got {:?}", other),
        }
    }

    #[test]
    fn trinket_details_deserializes() {
        let json = r#"{"type":"Ring","infusion_slots":[],"attribute_adjustment":0.0,"secondary_suffix_item_id":""}"#;
        let details: ItemDetails = serde_json::from_str(json).unwrap();
        match details {
            ItemDetails::Trinket(t) => assert_eq!(t.trinket_type, TrinketType::Ring),
            other => panic!("expected Trinket, got {:?}", other),
        }
    }

    #[test]
    fn upgrade_component_deserializes() {
        let json = r#"{"type":"Sigil","flags":["Axe","Sword"],"infusion_upgrade_flags":[],"suffix":"of Fire","infix_upgrade":{"attributes":[]}}"#;
        let details: ItemDetails = serde_json::from_str(json).unwrap();
        match details {
            ItemDetails::UpgradeComponent(u) => {
                assert_eq!(u.upgrade_type, UpgradeComponentType::Sigil);
                assert_eq!(u.suffix, "of Fire");
            }
            other => panic!("expected UpgradeComponent, got {:?}", other),
        }
    }

    #[test]
    fn container_details_deserializes() {
        let json = r#"{"type":"GiftBox"}"#;
        let details: ItemDetails = serde_json::from_str(json).unwrap();
        assert!(matches!(details, ItemDetails::Container(_)));
    }

    #[test]
    fn gathering_details_deserializes() {
        let json = r#"{"type":"Mining"}"#;
        let details: ItemDetails = serde_json::from_str(json).unwrap();
        assert!(matches!(details, ItemDetails::Gathering(_)));
    }

    #[test]
    fn tool_details_deserializes() {
        let json = r#"{"type":"Salvage","charges":25}"#;
        let details: ItemDetails = serde_json::from_str(json).unwrap();
        match details {
            ItemDetails::Tool(t) => assert_eq!(t.charges, 25),
            other => panic!("expected Tool, got {:?}", other),
        }
    }

    fn item_json(item_type: &str, details: &str) -> String {
        format!(
            r#"{{"id":1,"chat_link":"[&AgEBAAAA]","name":"x","type":"{item_type}","rarity":"Fine","level":0,"vendor_value":0,"details":{details}}}"#
        )
    }

    #[test]
    fn immediate_consumable_is_not_a_container() {
        let json = item_json("Consumable", r#"{"type":"Immediate","description":"Grants a buff","duration_ms":1000}"#);
        let item: Item = serde_json::from_str(&json).unwrap();
        match item.details {
            Some(ItemDetails::Consumable(c)) => assert_eq!(c.duration_ms, Some(1000)),
            other => panic!("expected Consumable, got {other:?}"),
        }
    }

    #[test]
    fn default_container_is_not_a_gizmo() {
        let item: Item = serde_json::from_str(&item_json("Container", r#"{"type":"Default"}"#)).unwrap();
        assert!(matches!(item.details, Some(ItemDetails::Container(_))), "{:?}", item.details);

        let item: Item = serde_json::from_str(&item_json("Gizmo", r#"{"type":"Default","vendor_ids":[1]}"#)).unwrap();
        assert!(matches!(item.details, Some(ItemDetails::Gizmo(_))), "{:?}", item.details);
    }

    #[test]
    fn item_round_trips_through_serialize() {
        let item: Item = serde_json::from_str(&item_json("Container", r#"{"type":"Default"}"#)).unwrap();
        let back: Item = serde_json::from_value(serde_json::to_value(&item).unwrap()).unwrap();
        assert_eq!(back.id, item.id);
        assert!(matches!(back.details, Some(ItemDetails::Container(_))));
    }

    #[test]
    fn secondary_suffix_is_an_item_id_or_empty() {
        let two_sigils = r#"{"type":"LongBow","damage_type":"Physical","min_power":920,"max_power":1080,"defense":0,"infusion_slots":[],"attribute_adjustment":682.88,"suffix_item_id":24615,"secondary_suffix_item_id":24599}"#;
        match serde_json::from_str::<ItemDetails>(two_sigils).unwrap() {
            ItemDetails::Weapon(w) => assert_eq!(w.secondary_suffix_item_id.map(|i| i.0), Some(24599)),
            other => panic!("expected Weapon, got {other:?}"),
        }
        let empty = r#"{"type":"Ring","infusion_slots":[],"attribute_adjustment":0.0,"secondary_suffix_item_id":""}"#;
        match serde_json::from_str::<ItemDetails>(empty).unwrap() {
            ItemDetails::Trinket(t) => assert!(t.secondary_suffix_item_id.is_none()),
            other => panic!("expected Trinket, got {other:?}"),
        }
    }
}
