//! GW2 chat link encoding and decoding.
//!
//! Chat links are Base64-encoded binary codes wrapped in `[&...]` that create
//! clickable in-game links. The first byte is a type header; remaining bytes
//! encode IDs and flags in little-endian format.
//!
//! Typed IDs (`ItemId`, `RecipeId`, `SkinId`, …) from the rest of the library are used
//! throughout, so a decoded link slots directly into the rest of the API without manual
//! ID conversion.
//!
//! # Example
//! ```
//! use gw2_api::chat_link::ChatLink;
//!
//! // Decode a chat link string
//! let link: ChatLink = "[&AgEAWgAA]".parse().unwrap();
//!
//! // Access the typed item ID
//! if let ChatLink::Item { item_id, .. } = &link {
//!     println!("item #{}", item_id.0);  // 23040 → Basic Salvage Kit
//! }
//!
//! // Encode back to the original string
//! assert_eq!(link.to_string(), "[&AgEAWgAA]");
//! ```

use std::fmt;
use std::str::FromStr;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::endpoints::achievements::AchievementId;
use crate::endpoints::items::ItemId;
use crate::endpoints::recipes::RecipeId;
use crate::endpoints::skills::SkillId;
use crate::endpoints::skins::SkinId;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChatLinkError {
    #[error("invalid format: expected [&<base64>]")]
    InvalidFormat,
    #[error("base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("empty payload")]
    Empty,
    #[error("unknown link type: 0x{0:02X}")]
    UnknownType(u8),
    #[error("truncated data: expected {expected} bytes, got {got}")]
    Truncated { expected: usize, got: usize },
}

// ── ChatLink ──────────────────────────────────────────────────────────────────

/// A decoded GW2 chat link.
///
/// Each variant corresponds to one of the known link type bytes (0x01–0x0F).
/// ID fields use the same typed newtypes as the rest of the library
/// (e.g. [`ItemId`], [`RecipeId`], [`SkinId`]), and the [`ChatLink::fetch`]
/// convenience method resolves the link to a full resource where applicable.
///
/// Encoding back to `[&...]` format is supported via [`fmt::Display`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatLink {
    /// `0x01` — Coin link (currently disabled in-game).
    ///
    /// The value is in copper; divide by 100 for silver, 10 000 for gold.
    Coin { copper: u32 },

    /// `0x02` — Item link.
    ///
    /// `quantity` is a single byte (1–255). Optional `skin_id`, `upgrade1_id`,
    /// and `upgrade2_id` are present only when the corresponding flag bits are set.
    Item {
        quantity: u8,
        item_id: ItemId,
        /// Optional wardrobe skin override (`0x80` flag).
        skin_id: Option<SkinId>,
        /// First upgrade slot (`0x40` flag).
        upgrade1_id: Option<ItemId>,
        /// Second upgrade slot (`0x20` flag).
        upgrade2_id: Option<ItemId>,
    },

    /// `0x03` — NPC text string (currently disabled in-game).
    NpcText { string_id: u32 },

    /// `0x04` — Map link (waypoints, points of interest, vistas).
    Map { poi_id: u32 },

    /// `0x06` — Skill link.
    Skill { skill_id: SkillId },

    /// `0x07` — Trait link.
    Trait { trait_id: u32 },

    /// `0x09` — Recipe link.
    Recipe { recipe_id: RecipeId },

    /// `0x0A` — Wardrobe skin link.
    Skin { skin_id: SkinId },

    /// `0x0B` — Outfit link.
    Outfit { outfit_id: u32 },

    /// `0x0C` — WvW objective link.
    ///
    /// The GW2 API identifies objectives as `"<map_id>-<objective_id>"`.
    WvwObjective { objective_id: u32, map_id: u32 },

    /// `0x0D` — Build template link.
    Build(BuildLink),

    /// `0x0E` — Achievement link (currently disabled in-game).
    Achievement { achievement_id: AchievementId },

    /// `0x0F` — Wardrobe template link.
    WardrobeTemplate(WardrobeTemplateLink),

    /// An unknown or undocumented link type with its raw payload bytes.
    Unknown { type_byte: u8, data: Vec<u8> },
}

impl ChatLink {
    /// Return the `ItemId` for `Item` links, or `None` for all other variants.
    pub fn item_id(&self) -> Option<ItemId> {
        match self {
            ChatLink::Item { item_id, .. } => Some(item_id.clone()),
            _ => None,
        }
    }

    /// Return the `RecipeId` for `Recipe` links, or `None` for all other variants.
    pub fn recipe_id(&self) -> Option<RecipeId> {
        match self {
            ChatLink::Recipe { recipe_id } => Some(recipe_id.clone()),
            _ => None,
        }
    }

    /// Return the `SkinId` for `Skin` links, or `None` for all other variants.
    pub fn skin_id(&self) -> Option<SkinId> {
        match self {
            ChatLink::Skin { skin_id } => Some(skin_id.clone()),
            _ => None,
        }
    }

    /// Return the `SkillId` for `Skill` links, or `None` for all other variants.
    pub fn skill_id(&self) -> Option<SkillId> {
        match self {
            ChatLink::Skill { skill_id } => Some(skill_id.clone()),
            _ => None,
        }
    }

    /// Return the `AchievementId` for `Achievement` links, or `None` for all other variants.
    pub fn achievement_id(&self) -> Option<AchievementId> {
        match self {
            ChatLink::Achievement { achievement_id } => Some(achievement_id.clone()),
            _ => None,
        }
    }

    /// Encode this chat link to its raw binary payload (including the type byte).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            ChatLink::Coin { copper } => {
                buf.push(0x01);
                push_u32_le(&mut buf, *copper);
            }
            ChatLink::Item { quantity, item_id, skin_id, upgrade1_id, upgrade2_id } => {
                buf.push(0x02);
                buf.push(*quantity);
                push_u24_le(&mut buf, item_id.0);
                let flags: u8 = if skin_id.is_some() { 0x80 } else { 0 }
                    | if upgrade1_id.is_some() { 0x40 } else { 0 }
                    | if upgrade2_id.is_some() { 0x20 } else { 0 };
                buf.push(flags);
                if let Some(id) = skin_id {
                    push_u24_le(&mut buf, id.0);
                    buf.push(0x00);
                }
                if let Some(id) = upgrade1_id {
                    push_u24_le(&mut buf, id.0);
                    buf.push(0x00);
                }
                if let Some(id) = upgrade2_id {
                    push_u24_le(&mut buf, id.0);
                    buf.push(0x00);
                }
            }
            ChatLink::NpcText { string_id } => {
                buf.push(0x03);
                push_u24_le(&mut buf, *string_id);
                buf.push(0x00);
            }
            ChatLink::Map { poi_id } => {
                buf.push(0x04);
                push_u24_le(&mut buf, *poi_id);
                buf.push(0x00);
            }
            ChatLink::Skill { skill_id } => {
                buf.push(0x06);
                push_u24_le(&mut buf, skill_id.0);
                buf.push(0x00);
            }
            ChatLink::Trait { trait_id } => {
                buf.push(0x07);
                push_u24_le(&mut buf, *trait_id);
                buf.push(0x00);
            }
            ChatLink::Recipe { recipe_id } => {
                buf.push(0x09);
                push_u24_le(&mut buf, recipe_id.0);
                buf.push(0x00);
            }
            ChatLink::Skin { skin_id } => {
                buf.push(0x0A);
                push_u24_le(&mut buf, skin_id.0);
                buf.push(0x00);
            }
            ChatLink::Outfit { outfit_id } => {
                buf.push(0x0B);
                push_u24_le(&mut buf, *outfit_id);
                buf.push(0x00);
            }
            ChatLink::WvwObjective { objective_id, map_id } => {
                buf.push(0x0C);
                push_u24_le(&mut buf, *objective_id);
                buf.push(0x00);
                push_u24_le(&mut buf, *map_id);
                buf.push(0x00);
            }
            ChatLink::Build(b) => {
                buf.push(0x0D);
                buf.push(b.profession);
                for (spec_id, traits) in &b.specializations {
                    buf.push(*spec_id);
                    buf.push(*traits);
                }
                for s in &b.skills {
                    push_u16_le(&mut buf, *s);
                }
                match &b.profession_data {
                    ProfessionData::Ranger { pets } => {
                        buf.extend_from_slice(pets);
                        buf.extend_from_slice(&[0u8; 12]);
                    }
                    ProfessionData::Revenant { legends, inactive_skills } => {
                        buf.extend_from_slice(legends);
                        for s in inactive_skills {
                            push_u16_le(&mut buf, *s);
                        }
                    }
                    ProfessionData::Other(raw) => {
                        buf.extend_from_slice(raw);
                    }
                }
                buf.push(b.weapons.len() as u8);
                for w in &b.weapons {
                    push_u16_le(&mut buf, *w);
                }
                buf.push(b.skill_overrides.len() as u8);
                for s in &b.skill_overrides {
                    push_u32_le(&mut buf, *s);
                }
            }
            ChatLink::Achievement { achievement_id } => {
                buf.push(0x0E);
                push_u24_le(&mut buf, achievement_id.0);
                buf.push(0x00);
            }
            ChatLink::WardrobeTemplate(t) => {
                buf.push(0x0F);
                push_u16_le(&mut buf, t.aquabreather_skin.0 as u16);
                push_u16_le(&mut buf, t.backpack_skin.0 as u16);
                for d in &t.backpack_dyes { push_u16_le(&mut buf, *d); }
                push_u16_le(&mut buf, t.chest_skin.0 as u16);
                for d in &t.chest_dyes { push_u16_le(&mut buf, *d); }
                push_u16_le(&mut buf, t.boots_skin.0 as u16);
                for d in &t.boot_dyes { push_u16_le(&mut buf, *d); }
                push_u16_le(&mut buf, t.gloves_skin.0 as u16);
                for d in &t.glove_dyes { push_u16_le(&mut buf, *d); }
                push_u16_le(&mut buf, t.helm_skin.0 as u16);
                for d in &t.helm_dyes { push_u16_le(&mut buf, *d); }
                push_u16_le(&mut buf, t.leggings_skin.0 as u16);
                for d in &t.legging_dyes { push_u16_le(&mut buf, *d); }
                push_u16_le(&mut buf, t.shoulders_skin.0 as u16);
                for d in &t.shoulder_dyes { push_u16_le(&mut buf, *d); }
                push_u16_le(&mut buf, t.outfit_id);
                for d in &t.outfit_dyes { push_u16_le(&mut buf, *d); }
                push_u16_le(&mut buf, t.aquatic_weapon_a_skin.0 as u16);
                push_u16_le(&mut buf, t.aquatic_weapon_b_skin.0 as u16);
                push_u16_le(&mut buf, t.weapon_set_a_mainhand_skin.0 as u16);
                push_u16_le(&mut buf, t.weapon_set_a_offhand_skin.0 as u16);
                push_u16_le(&mut buf, t.weapon_set_b_mainhand_skin.0 as u16);
                push_u16_le(&mut buf, t.weapon_set_b_offhand_skin.0 as u16);
                push_u16_le(&mut buf, t.visibility_flags);
            }
            ChatLink::Unknown { type_byte, data } => {
                buf.push(*type_byte);
                buf.extend_from_slice(data);
            }
        }
        buf
    }
}

// ── Build template ─────────────────────────────────────────────────────────────

/// A decoded build template link (`0x0D`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildLink {
    /// Profession code 1–9 (Guardian=1 … Revenant=9).
    pub profession: u8,
    /// Three specialization slots; each is `(specialization_id, trait_choices)`.
    /// `trait_choices` encodes three 2-bit selections (first choice in bits 1–0).
    pub specializations: [(u8, u8); 3],
    /// Ten skill palette IDs in terrestrial/aquatic pairs:
    /// heal(t), heal(a), util1(t), util1(a), util2(t), util2(a),
    /// util3(t), util3(a), elite(t), elite(a).
    pub skills: [u16; 10],
    /// Profession-specific extension data.
    pub profession_data: ProfessionData,
    /// Terrestrial weapon type IDs (added by Secrets of the Obscure).
    pub weapons: Vec<u16>,
    /// Skill overrides: full 32-bit skill IDs that differ from defaults.
    pub skill_overrides: Vec<u32>,
}

/// Profession-specific build data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfessionData {
    /// Ranger: four pet IDs [terrestrial1, terrestrial2, aquatic1, aquatic2].
    Ranger { pets: [u8; 4] },
    /// Revenant: legend codes + inactive legend skill palette IDs.
    Revenant {
        /// [terrestrial_active, terrestrial_inactive, aquatic_active, aquatic_inactive]
        legends: [u8; 4],
        /// Inactive legend skill palette IDs (3 terrestrial, 3 aquatic).
        inactive_skills: [u16; 6],
    },
    /// All other professions: 16 bytes of unused data.
    Other([u8; 16]),
}

// ── Wardrobe template ─────────────────────────────────────────────────────────

/// A decoded wardrobe template link (`0x0F`).
///
/// All skin fields use [`SkinId`]. Dye IDs are plain `u16` (no `ColorId` type
/// exists yet in the library). Unoccupied skin slots are `SkinId(0)`;
/// unused dye slots are `1` (Dye Remover).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WardrobeTemplateLink {
    pub aquabreather_skin: SkinId,
    pub backpack_skin: SkinId,
    pub backpack_dyes: [u16; 4],
    pub chest_skin: SkinId,
    pub chest_dyes: [u16; 4],
    pub boots_skin: SkinId,
    pub boot_dyes: [u16; 4],
    pub gloves_skin: SkinId,
    pub glove_dyes: [u16; 4],
    pub helm_skin: SkinId,
    pub helm_dyes: [u16; 4],
    pub leggings_skin: SkinId,
    pub legging_dyes: [u16; 4],
    pub shoulders_skin: SkinId,
    pub shoulder_dyes: [u16; 4],
    /// Outfit ID (plain `u16`; no typed `OutfitId` exists yet).
    pub outfit_id: u16,
    pub outfit_dyes: [u16; 4],
    pub aquatic_weapon_a_skin: SkinId,
    pub aquatic_weapon_b_skin: SkinId,
    pub weapon_set_a_mainhand_skin: SkinId,
    pub weapon_set_a_offhand_skin: SkinId,
    pub weapon_set_b_mainhand_skin: SkinId,
    pub weapon_set_b_offhand_skin: SkinId,
    pub visibility_flags: u16,
}

impl WardrobeTemplateLink {
    /// Return the `SkinId` for each skin slot that is occupied (non-zero).
    pub fn skin_ids(&self) -> impl Iterator<Item = SkinId> + '_ {
        [
            self.aquabreather_skin.clone(),
            self.backpack_skin.clone(),
            self.chest_skin.clone(),
            self.boots_skin.clone(),
            self.gloves_skin.clone(),
            self.helm_skin.clone(),
            self.leggings_skin.clone(),
            self.shoulders_skin.clone(),
            self.aquatic_weapon_a_skin.clone(),
            self.aquatic_weapon_b_skin.clone(),
            self.weapon_set_a_mainhand_skin.clone(),
            self.weapon_set_a_offhand_skin.clone(),
            self.weapon_set_b_mainhand_skin.clone(),
            self.weapon_set_b_offhand_skin.clone(),
        ]
        .into_iter()
        .filter(|id| id.0 != 0)
    }
}

// ── Parsing helpers ───────────────────────────────────────────────────────────

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn read_u8(&mut self) -> Result<u8, ChatLinkError> {
        if self.pos >= self.data.len() {
            return Err(ChatLinkError::Truncated { expected: self.pos + 1, got: self.data.len() });
        }
        let b = self.data[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn read_u16_le(&mut self) -> Result<u16, ChatLinkError> {
        let lo = self.read_u8()? as u16;
        let hi = self.read_u8()? as u16;
        Ok(lo | (hi << 8))
    }

    fn read_u24_le(&mut self) -> Result<u32, ChatLinkError> {
        let b0 = self.read_u8()? as u32;
        let b1 = self.read_u8()? as u32;
        let b2 = self.read_u8()? as u32;
        Ok(b0 | (b1 << 8) | (b2 << 16))
    }

    fn read_u32_le(&mut self) -> Result<u32, ChatLinkError> {
        let b0 = self.read_u8()? as u32;
        let b1 = self.read_u8()? as u32;
        let b2 = self.read_u8()? as u32;
        let b3 = self.read_u8()? as u32;
        Ok(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
    }

    fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, ChatLinkError> {
        if self.pos + n > self.data.len() {
            return Err(ChatLinkError::Truncated { expected: self.pos + n, got: self.data.len() });
        }
        let slice = self.data[self.pos..self.pos + n].to_vec();
        self.pos += n;
        Ok(slice)
    }
}

// ── Encoding helpers ──────────────────────────────────────────────────────────

fn push_u24_le(buf: &mut Vec<u8>, v: u32) {
    buf.push((v & 0xFF) as u8);
    buf.push(((v >> 8) & 0xFF) as u8);
    buf.push(((v >> 16) & 0xFF) as u8);
}

fn push_u16_le(buf: &mut Vec<u8>, v: u16) {
    buf.push((v & 0xFF) as u8);
    buf.push((v >> 8) as u8);
}

fn push_u32_le(buf: &mut Vec<u8>, v: u32) {
    buf.push((v & 0xFF) as u8);
    buf.push(((v >> 8) & 0xFF) as u8);
    buf.push(((v >> 16) & 0xFF) as u8);
    buf.push(((v >> 24) & 0xFF) as u8);
}

// ── FromStr ───────────────────────────────────────────────────────────────────

impl FromStr for ChatLink {
    type Err = ChatLinkError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let inner = s
            .strip_prefix("[&")
            .and_then(|s| s.strip_suffix(']'))
            .ok_or(ChatLinkError::InvalidFormat)?;

        let bytes = BASE64.decode(inner)?;
        if bytes.is_empty() {
            return Err(ChatLinkError::Empty);
        }

        let type_byte = bytes[0];
        let mut r = Reader::new(&bytes[1..]);

        match type_byte {
            0x01 => {
                let copper = r.read_u32_le()?;
                Ok(ChatLink::Coin { copper })
            }
            0x02 => parse_item(&mut r),
            0x03 => {
                let string_id = r.read_u24_le()?;
                let _ = r.read_u8();
                Ok(ChatLink::NpcText { string_id })
            }
            0x04 => {
                let poi_id = r.read_u24_le()?;
                let _ = r.read_u8();
                Ok(ChatLink::Map { poi_id })
            }
            0x06 => {
                let skill_id = SkillId(r.read_u24_le()?);
                let _ = r.read_u8();
                Ok(ChatLink::Skill { skill_id })
            }
            0x07 => {
                let trait_id = r.read_u24_le()?;
                let _ = r.read_u8();
                Ok(ChatLink::Trait { trait_id })
            }
            0x09 => {
                let recipe_id = RecipeId(r.read_u24_le()?);
                let _ = r.read_u8();
                Ok(ChatLink::Recipe { recipe_id })
            }
            0x0A => {
                let skin_id = SkinId(r.read_u24_le()?);
                let _ = r.read_u8();
                Ok(ChatLink::Skin { skin_id })
            }
            0x0B => {
                let outfit_id = r.read_u24_le()?;
                let _ = r.read_u8();
                Ok(ChatLink::Outfit { outfit_id })
            }
            0x0C => {
                let objective_id = r.read_u24_le()?;
                let _ = r.read_u8();
                let map_id = r.read_u24_le()?;
                let _ = r.read_u8();
                Ok(ChatLink::WvwObjective { objective_id, map_id })
            }
            0x0D => parse_build(&mut r),
            0x0E => {
                let achievement_id = AchievementId(r.read_u24_le()?);
                let _ = r.read_u8();
                Ok(ChatLink::Achievement { achievement_id })
            }
            0x0F => parse_wardrobe_template(&mut r),
            other => {
                let data = r.read_bytes(r.remaining())?;
                Ok(ChatLink::Unknown { type_byte: other, data })
            }
        }
    }
}

fn parse_item(r: &mut Reader) -> Result<ChatLink, ChatLinkError> {
    let quantity = r.read_u8()?;
    let item_id = ItemId(r.read_u24_le()?);
    let flags = r.read_u8()?;

    let skin_id = if flags & 0x80 != 0 {
        let id = SkinId(r.read_u24_le()?);
        let _ = r.read_u8();
        Some(id)
    } else {
        None
    };

    let upgrade1_id = if flags & 0x40 != 0 {
        let id = ItemId(r.read_u24_le()?);
        let _ = r.read_u8();
        Some(id)
    } else {
        None
    };

    let upgrade2_id = if flags & 0x20 != 0 {
        let id = ItemId(r.read_u24_le()?);
        let _ = r.read_u8();
        Some(id)
    } else {
        None
    };

    Ok(ChatLink::Item { quantity, item_id, skin_id, upgrade1_id, upgrade2_id })
}

fn parse_build(r: &mut Reader) -> Result<ChatLink, ChatLinkError> {
    let profession = r.read_u8()?;

    let mut specializations = [(0u8, 0u8); 3];
    for slot in &mut specializations {
        slot.0 = r.read_u8()?;
        slot.1 = r.read_u8()?;
    }

    let mut skills = [0u16; 10];
    for s in &mut skills {
        *s = r.read_u16_le()?;
    }

    let profession_data = match profession {
        4 => {
            // Ranger
            let pets = [r.read_u8()?, r.read_u8()?, r.read_u8()?, r.read_u8()?];
            let _unused = r.read_bytes(12)?;
            ProfessionData::Ranger { pets }
        }
        9 => {
            // Revenant
            let legends = [r.read_u8()?, r.read_u8()?, r.read_u8()?, r.read_u8()?];
            let mut inactive_skills = [0u16; 6];
            for s in &mut inactive_skills {
                *s = r.read_u16_le()?;
            }
            ProfessionData::Revenant { legends, inactive_skills }
        }
        _ => {
            let mut buf = [0u8; 16];
            for b in &mut buf {
                *b = r.read_u8()?;
            }
            ProfessionData::Other(buf)
        }
    };

    // Variable-length tail (Secrets of the Obscure addition)
    let weapon_count = if r.remaining() > 0 { r.read_u8()? as usize } else { 0 };
    let mut weapons = Vec::with_capacity(weapon_count);
    for _ in 0..weapon_count {
        weapons.push(r.read_u16_le()?);
    }

    let override_count = if r.remaining() > 0 { r.read_u8()? as usize } else { 0 };
    let mut skill_overrides = Vec::with_capacity(override_count);
    for _ in 0..override_count {
        skill_overrides.push(r.read_u32_le()?);
    }

    Ok(ChatLink::Build(BuildLink { profession, specializations, skills, profession_data, weapons, skill_overrides }))
}

fn parse_wardrobe_template(r: &mut Reader) -> Result<ChatLink, ChatLinkError> {
    let read_skin = |r: &mut Reader| -> Result<SkinId, ChatLinkError> {
        Ok(SkinId(r.read_u16_le()? as u32))
    };
    let read_dyes = |r: &mut Reader| -> Result<[u16; 4], ChatLinkError> {
        Ok([r.read_u16_le()?, r.read_u16_le()?, r.read_u16_le()?, r.read_u16_le()?])
    };

    Ok(ChatLink::WardrobeTemplate(WardrobeTemplateLink {
        aquabreather_skin: read_skin(r)?,
        backpack_skin: read_skin(r)?,
        backpack_dyes: read_dyes(r)?,
        chest_skin: read_skin(r)?,
        chest_dyes: read_dyes(r)?,
        boots_skin: read_skin(r)?,
        boot_dyes: read_dyes(r)?,
        gloves_skin: read_skin(r)?,
        glove_dyes: read_dyes(r)?,
        helm_skin: read_skin(r)?,
        helm_dyes: read_dyes(r)?,
        leggings_skin: read_skin(r)?,
        legging_dyes: read_dyes(r)?,
        shoulders_skin: read_skin(r)?,
        shoulder_dyes: read_dyes(r)?,
        outfit_id: r.read_u16_le()?,
        outfit_dyes: read_dyes(r)?,
        aquatic_weapon_a_skin: read_skin(r)?,
        aquatic_weapon_b_skin: read_skin(r)?,
        weapon_set_a_mainhand_skin: read_skin(r)?,
        weapon_set_a_offhand_skin: read_skin(r)?,
        weapon_set_b_mainhand_skin: read_skin(r)?,
        weapon_set_b_offhand_skin: read_skin(r)?,
        visibility_flags: r.read_u16_le()?,
    }))
}

// ── Display ───────────────────────────────────────────────────────────────────

impl fmt::Display for ChatLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[&{}]", BASE64.encode(self.encode()))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(s: &str) {
        let link: ChatLink = s.parse().unwrap();
        assert_eq!(link.to_string(), s, "roundtrip failed for {s}");
    }

    #[test]
    fn item_basic_salvage_kit() {
        // [&AgEAWgAA] — Basic Salvage Kit (Id: 23040)
        let link: ChatLink = "[&AgEAWgAA]".parse().unwrap();
        assert_eq!(
            link,
            ChatLink::Item {
                quantity: 1,
                item_id: ItemId(23040),
                skin_id: None,
                upgrade1_id: None,
                upgrade2_id: None,
            }
        );
        // Typed ID is accessible directly
        if let ChatLink::Item { item_id, .. } = &link {
            assert_eq!(item_id.0, 23040);
        }
        roundtrip("[&AgEAWgAA]");
    }

    #[test]
    fn item_id_helper() {
        let link: ChatLink = "[&AgEAWgAA]".parse().unwrap();
        assert_eq!(link.item_id().unwrap(), ItemId(23040));
    }

    #[test]
    fn item_copper_sickle() {
        // [&AgH1WQAA] — Copper Harvesting Sickle (Id: 23029)
        let link: ChatLink = "[&AgH1WQAA]".parse().unwrap();
        assert!(matches!(link, ChatLink::Item { item_id: ItemId(23029), .. }));
        roundtrip("[&AgH1WQAA]");
    }

    #[test]
    fn item_with_skin_only() {
        roundtrip("[&AgGqtgCAfQ4AAA==]");
        let link: ChatLink = "[&AgGqtgCAfQ4AAA==]".parse().unwrap();
        if let ChatLink::Item { skin_id: Some(SkinId(id)), .. } = link {
            assert_eq!(id, 3709); // Dreamthistle Greatsword Skin wardrobe id
        } else {
            panic!("expected item with skin");
        }
    }

    #[test]
    fn item_with_sigil1_only() {
        roundtrip("[&AgGqtgBA/18AAA==]");
    }

    #[test]
    fn item_with_both_sigils() {
        roundtrip("[&AgGqtgBg/18AACdgAAA=]");
        let link: ChatLink = "[&AgGqtgBg/18AACdgAAA=]".parse().unwrap();
        if let ChatLink::Item { upgrade1_id: Some(ItemId(u1)), upgrade2_id: Some(ItemId(u2)), .. } = link {
            assert_eq!(u1, 24575); // Superior Sigil of Bloodlust
            assert_eq!(u2, 24615); // Superior Sigil of Force
        } else {
            panic!("expected two upgrades");
        }
    }

    #[test]
    fn item_with_skin_and_sigil1() {
        roundtrip("[&AgGqtgDAfQ4AAP9fAAA=]");
    }

    #[test]
    fn item_with_skin_and_both_sigils() {
        roundtrip("[&AgGqtgDgfQ4AAP9fAAAnYAAA]");
    }

    #[test]
    fn map_link() {
        let link: ChatLink = "[&BDgAAAA=]".parse().unwrap();
        assert_eq!(link, ChatLink::Map { poi_id: 56 });
        roundtrip("[&BDgAAAA=]");
    }

    #[test]
    fn skill_link() {
        // Aegis skill Id: 743
        let buf = vec![0x06u8, 0xE7, 0x02, 0x00, 0x00];
        let s = format!("[&{}]", BASE64.encode(&buf));
        let link: ChatLink = s.parse().unwrap();
        assert_eq!(link, ChatLink::Skill { skill_id: SkillId(743) });
        assert_eq!(link.to_string(), s);
        assert_eq!(link.skill_id().unwrap(), SkillId(743));
    }

    #[test]
    fn recipe_link() {
        // Recipe: Soft Wood Plank (Id: 1)
        let buf = vec![0x09u8, 0x01, 0x00, 0x00, 0x00];
        let s = format!("[&{}]", BASE64.encode(&buf));
        roundtrip(&s);
        let link: ChatLink = s.parse().unwrap();
        assert_eq!(link, ChatLink::Recipe { recipe_id: RecipeId(1) });
        assert_eq!(link.recipe_id().unwrap(), RecipeId(1));
    }

    #[test]
    fn skin_link() {
        // Apprentice Coat (Id: 4)
        let buf = vec![0x0Au8, 0x04, 0x00, 0x00, 0x00];
        let s = format!("[&{}]", BASE64.encode(&buf));
        let link: ChatLink = s.parse().unwrap();
        assert_eq!(link, ChatLink::Skin { skin_id: SkinId(4) });
        assert_eq!(link.skin_id().unwrap(), SkinId(4));
        roundtrip(&s);
    }

    #[test]
    fn coin_link() {
        // 10203 copper = 0x27DB → little-endian: 0xDB 0x27 0x00 0x00
        let buf = vec![0x01u8, 0xDB, 0x27, 0x00, 0x00];
        let s = format!("[&{}]", BASE64.encode(&buf));
        let link: ChatLink = s.parse().unwrap();
        assert_eq!(link, ChatLink::Coin { copper: 10203 });
        assert_eq!(link.to_string(), s);
    }

    #[test]
    fn wvw_objective_link() {
        // Speldan Clearcut: objective 6, map 38
        let buf = vec![0x0Cu8, 0x06, 0x00, 0x00, 0x00, 0x26, 0x00, 0x00, 0x00];
        let s = format!("[&{}]", BASE64.encode(&buf));
        let link: ChatLink = s.parse().unwrap();
        assert_eq!(link, ChatLink::WvwObjective { objective_id: 6, map_id: 38 });
        roundtrip(&s);
    }

    #[test]
    fn achievement_link() {
        // Bar Brawl Combos (Id: 182)
        let link: ChatLink = "[&DrYAAAA=]".parse().unwrap();
        assert_eq!(link, ChatLink::Achievement { achievement_id: AchievementId(182) });
        assert_eq!(link.achievement_id().unwrap(), AchievementId(182));
        roundtrip("[&DrYAAAA=]");
    }

    #[test]
    fn unknown_type_preserved() {
        let buf = vec![0x05u8, 0xAB, 0xCD];
        let s = format!("[&{}]", BASE64.encode(&buf));
        let link: ChatLink = s.parse().unwrap();
        assert!(matches!(link, ChatLink::Unknown { type_byte: 0x05, .. }));
        assert_eq!(link.to_string(), s);
    }

    #[test]
    fn invalid_format_rejected() {
        assert!("[AgEAWgAA]".parse::<ChatLink>().is_err()); // missing &
        assert!("[&AgEAWgAA".parse::<ChatLink>().is_err());  // missing ]
        assert!("[&]".parse::<ChatLink>().is_err());         // empty base64
    }
}
