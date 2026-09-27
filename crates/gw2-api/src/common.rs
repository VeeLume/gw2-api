//! Common types shared across multiple endpoints.

use gw2_api_macros::gw2_enum;

/// Rarity levels for items, skins, etc.
#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Rarity {
    Junk,
    Basic,
    Fine,
    Masterwork,
    Rare,
    Exotic,
    Ascended,
    Legendary,
}

/// Game disciplines (crafting professions).
#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CraftingDiscipline {
    Armorsmith,
    Artificer,
    Chef,
    /// Homestead crafting (Janthir Wilds).
    Homesteader,
    Huntsman,
    Jeweler,
    Leatherworker,
    Scribe,
    Tailor,
    Weaponsmith,
}

/// Item flags from the API.
#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ItemFlag {
    AccountBindOnUse,
    AccountBound,
    Attuned,
    BulkConsume,
    DeleteWarning,
    HideSuffix,
    Infused,
    MonsterOnly,
    NoMysticForge,
    NoSalvage,
    NoSell,
    NotUpgradeable,
    NoUnderwater,
    SoulbindOnAcquire,
    SoulBindOnUse,
    Tonic,
    Unique,
}

/// Game type restrictions.
#[gw2_enum]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameType {
    Activity,
    Dungeon,
    Pve,
    Pvp,
    PvpLobby,
    Wvw,
}
