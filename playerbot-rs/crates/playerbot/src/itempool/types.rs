//! Core types for the item pool — the Rust port of `RandomItemMgr.h`.
//!
//! Pure data. No cmangos vtable calls here; that lives in
//! [`super::manager`]. The types mirror the C++ `RandomItemType`,
//! `ItemSpecType`, `ItemInfoEntry`, `WeightScale*`, and `BotEquipKey`
//! structures 1:1 so the stat-weight and equip-filter ports stay
//! auditable against the legacy code.

use std::collections::HashMap;

// ── RandomItemType ─────────────────────────────────────────────────────────

/// Mirrors the C++ `RandomItemType` enum. Used to index
/// `RandomItemCache` entries inside [`super::manager::ItemPoolManager`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum RandomItemType {
    GuildTask = 0,
    GuildTaskRewardEquipBlue = 1,
    GuildTaskRewardEquipGreen = 2,
    GuildTaskRewardTrade = 3,
    GuildTaskRewardTradeRare = 4,
}

impl RandomItemType {
    /// Decode a raw discriminant from the FFI boundary.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            0 => Some(Self::GuildTask),
            1 => Some(Self::GuildTaskRewardEquipBlue),
            2 => Some(Self::GuildTaskRewardEquipGreen),
            3 => Some(Self::GuildTaskRewardTrade),
            4 => Some(Self::GuildTaskRewardTradeRare),
            _ => None,
        }
    }
}

// ── ItemSpecType ───────────────────────────────────────────────────────────

/// Bitmask mirroring the C++ `ItemSpecType` enum. An item can satisfy
/// multiple specs at once (e.g. a caster dagger with spell damage +
/// haste rating).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemSpecType(pub u32);

impl ItemSpecType {
    pub const NONE: Self = Self(0x0);
    pub const SPELL_DAMAGE: Self = Self(0x01);
    pub const SPELL_HEALING: Self = Self(0x02);
    pub const ATTACK: Self = Self(0x04);
    pub const TANK: Self = Self(0x08);
    pub const CASTER: Self = Self(0x10);

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl core::ops::BitOr for ItemSpecType {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitOrAssign for ItemSpecType {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

// ── ItemSource ─────────────────────────────────────────────────────────────

/// Mirrors the C++ `ItemSource` enum — the provenance tag recorded by
/// `BuildItemInfoCache` for every item in the info cache.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum ItemSource {
    #[default]
    None = 0,
    Drop = 1,
    Vendor = 2,
    Quest = 3,
    Craft = 4,
    Pvp = 5,
}

impl ItemSource {
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Self::Drop,
            2 => Self::Vendor,
            3 => Self::Quest,
            4 => Self::Craft,
            5 => Self::Pvp,
            _ => Self::None,
        }
    }
}

// ── Weight scales ──────────────────────────────────────────────────────────

/// Mirrors the C++ `WeightScaleInfo` struct — the scale-level metadata
/// that identifies which (class, spec) combo a weight vector applies to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeightScaleInfo {
    pub id: u32,
    pub name: String,
    pub class_id: u8,
}

/// A single (stat-name → weight) entry inside a [`WeightScale`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeightScaleStat {
    pub stat: String,
    pub weight: u32,
}

/// The per-scale (class, spec) stat weight vector.
#[derive(Clone, Debug, Default)]
pub struct WeightScale {
    pub info: WeightScaleInfo,
    pub stats: Vec<WeightScaleStat>,
}

// ── ItemInfoEntry ──────────────────────────────────────────────────────────

/// Per-item cached metadata. Mirrors the C++ `ItemInfoEntry` struct.
/// The `weights` map holds one entry per `spec_id` (1..=32).
#[derive(Clone, Debug, Default)]
pub struct ItemInfoEntry {
    /// `spec_id` -> base stat weight (pre-computed during `BuildItemInfoCache`).
    pub weights: HashMap<u32, u32>,
    pub min_level: u32,
    pub item_level: u32,
    pub source: u32,
    pub source_ids: Vec<u32>,
    pub team: u32,
    pub rep_rank: u32,
    pub rep_faction: u32,
    pub req_skill: u32,
    pub req_skill_rank: u32,
    pub pvp_rank: u32,
    pub quality: u32,
    pub slot: u32,
    pub item_id: u32,
    pub item_spec: ItemSpecType,
}

impl ItemInfoEntry {
    /// Fresh entry for `item_id`, all other fields zero / empty.
    #[must_use]
    pub fn new(item_id: u32) -> Self {
        Self {
            item_id,
            ..Self::default()
        }
    }

    /// Lookup helper mirroring C++'s `info->weights[specId]` with its
    /// implicit zero-fill behaviour.
    #[must_use]
    pub fn weight(&self, spec_id: u32) -> u32 {
        self.weights.get(&spec_id).copied().unwrap_or(0)
    }
}

// ── BotEquipKey ────────────────────────────────────────────────────────────

/// Composite key indexing the equip cache. Mirrors C++ `BotEquipKey` —
/// the `key` field is the 64-bit formula packed from `(level, clazz,
/// spec, slot, quality)` so it sorts identically to the legacy code.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BotEquipKey {
    pub level: u32,
    pub clazz: u8,
    pub spec: u8,
    pub slot: u8,
    pub quality: u32,
}

impl BotEquipKey {
    #[must_use]
    pub const fn new(level: u32, clazz: u8, spec: u8, slot: u8, quality: u32) -> Self {
        Self {
            level,
            clazz,
            spec,
            slot,
            quality,
        }
    }

    /// 64-bit packed key used by the legacy cache map. The formula
    /// matches `RandomItemMgr.cpp::BotEquipKey::GetKey`:
    ///
    /// ```text
    /// key = level
    ///     + 100        * clazz
    ///     + 10_000     * spec
    ///     + 1_000_000  * slot
    ///     + 100_000_000* quality
    /// ```
    #[must_use]
    pub const fn packed(self) -> u64 {
        self.level as u64
            + 100 * self.clazz as u64
            + 10_000 * self.spec as u64
            + 1_000_000 * self.slot as u64
            + 100_000_000 * self.quality as u64
    }
}

impl PartialOrd for BotEquipKey {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BotEquipKey {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // Legacy C++ operator<: `other.key < this->key` — reverse order.
        // Rust ordering uses `self.cmp(other)`, so the map sort falls out
        // naturally if we reverse the key comparison here.
        other.packed().cmp(&self.packed())
    }
}

impl core::hash::Hash for BotEquipKey {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.packed().hash(state);
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_equip_key_pack_formula() {
        let k = BotEquipKey::new(60, 1, 2, 7, 3);
        // 60 + 100*1 + 10_000*2 + 1_000_000*7 + 100_000_000*3
        //  = 60 + 100 + 20_000 + 7_000_000 + 300_000_000
        //  = 307_020_160
        assert_eq!(k.packed(), 307_020_160);
    }

    #[test]
    fn bot_equip_key_sort_reverse() {
        let lo = BotEquipKey::new(10, 1, 1, 1, 1);
        let hi = BotEquipKey::new(60, 1, 1, 1, 1);
        assert!(hi < lo, "legacy operator< reverses packed order");
    }

    #[test]
    fn item_spec_type_bitmask() {
        let mut s = ItemSpecType::NONE;
        s |= ItemSpecType::SPELL_DAMAGE;
        s |= ItemSpecType::CASTER;
        assert!(s.contains(ItemSpecType::SPELL_DAMAGE));
        assert!(s.contains(ItemSpecType::CASTER));
        assert!(!s.contains(ItemSpecType::TANK));
        assert_eq!(s.0, 0x11);
    }

    #[test]
    fn random_item_type_roundtrip() {
        for raw in 0..=4 {
            let ty = RandomItemType::from_raw(raw).unwrap();
            assert_eq!(ty as u32, raw);
        }
        assert!(RandomItemType::from_raw(5).is_none());
    }

    #[test]
    fn item_info_entry_weight_defaults_to_zero() {
        let e = ItemInfoEntry::new(1234);
        assert_eq!(e.weight(1), 0);
        assert_eq!(e.item_id, 1234);
    }
}
