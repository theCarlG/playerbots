//! `BuildEquipCache` — Rust port of `RandomItemMgr::BuildEquipCache`
//! (`RandomItemMgr.cpp` lines 3308-3475).
//!
//! This is the "what items can a bot equip at (class, spec, level, slot,
//! quality)" cache. It is the first thing consumed by the runtime
//! equipment query path — `Query(level, clazz, spec, slot, quality)`
//! simply returns the vector at that key.
//!
//! # Two code paths
//!
//! * **DB-loaded** — if the `ai_playerbot_equip_cache` table is
//!   populated, the legacy code skips the expensive build and just
//!   streams the rows into `equipCache[key]`. [`load_equip_cache_rows`]
//!   is the Rust port of that path.
//! * **Built fresh** — when the table is empty, the legacy code iterates
//!   `class × level × slot × spec × quality × prototypes` and applies a
//!   dozen filter rules. [`build_equip_cache`] is the Rust port of that
//!   path; it depends on the item-info cache (for cached `min_level` and
//!   `GetStatWeight`) built earlier by
//!   [`super::item_info::build_item_info_cache`].
//!
//! # Body / tabard special case
//!
//! Shirts (`EQUIPMENT_SLOT_BODY`) and tabards (`EQUIPMENT_SLOT_TABARD`)
//! sit outside the normal stat-weight gate — the legacy code adds every
//! prototype whose `InventoryType` matches the slot, deduped, and writes
//! them to the two fixed keys `(60, 1, 1, BODY, 1)` and
//! `(60, 1, 1, TABARD, 1)`. We replicate that exactly: the Rust port
//! scans prototypes once up front, then inserts the two lists at the
//! same fixed keys after the main loop finishes.

use std::collections::BTreeMap;

use cmangos::BotItemPrototype;

use super::equip_filter::{can_equip_armor, can_equip_item, class, item_class};
use super::item_info::ItemInfoCache;
use super::slots::{self, slot};
use super::types::BotEquipKey;
use super::unavailable_ids::is_unavailable;

// ── Constants mirroring the C++ ────────────────────────────────────────────

/// `EQUIPMENT_SLOT_END` from cmangos `Player.h` — the exclusive upper
/// bound on equip-slot iteration. Slots `0..=18` are valid targets.
pub const EQUIPMENT_SLOT_END: u8 = 19;

/// `ITEM_QUALITY_POOR` through `ITEM_QUALITY_ARTIFACT` — the inclusive
/// range the legacy loop iterates over.
pub const ITEM_QUALITY_POOR: u32 = 0;
/// Inclusive upper bound for the quality iteration.
pub const ITEM_QUALITY_ARTIFACT: u32 = 6;

/// Matches the legacy `MAX_STAT_SCALES` — weight-scale ids run 1..=32.
pub const MAX_STAT_SCALES: u32 = 32;

/// `CLASSMASK_ALL_PLAYABLE`. `WotLK` includes death knight (bit 6),
/// Classic and TBC do not. Mirrors the same constant in
/// [`super::item_info`].
#[cfg(feature = "wotlk")]
const CLASSMASK_ALL_PLAYABLE: u32 = 0x5FF;
#[cfg(not(feature = "wotlk"))]
const CLASSMASK_ALL_PLAYABLE: u32 = 0x5DF;

/// `MAX_CLASSES` exclusive upper bound for the class iteration
/// (`CLASS_WARRIOR..MAX_CLASSES`). `CMaNGOS` classes are numbered 1..=11.
const MAX_CLASSES: u8 = 12;

/// The fixed level used by the body/tabard special-case keys. The legacy
/// code hardcodes `60` here (the vanilla max level) even on TBC/WotLK —
/// shirts and tabards are level-agnostic so the key just needs to be
/// stable.
const BODY_TABARD_FIXED_LEVEL: u32 = 60;

// ── Cache types ────────────────────────────────────────────────────────────

/// Alias for the per-key item list. Mirrors the C++ typedef
/// `typedef std::vector<uint32> RandomItemList`.
pub type RandomItemList = Vec<u32>;

/// Top-level cache — `std::map<BotEquipKey, RandomItemList>`. A
/// [`BTreeMap`] preserves the legacy reverse-packed sort order the
/// [`BotEquipKey`] `Ord` impl encodes.
pub type BotEquipCache = BTreeMap<BotEquipKey, RandomItemList>;

// ── DB-loaded path ─────────────────────────────────────────────────────────

/// One row from `ai_playerbot_equip_cache`. The bridge is responsible
/// for running the `SELECT` and converting the six `uint32` columns into
/// this struct.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct EquipCacheRow {
    pub clazz: u8,
    pub spec: u8,
    pub level: u32,
    pub slot: u8,
    pub quality: u32,
    pub item_id: u32,
}

/// Load a pre-built equip cache from raw DB rows. Mirrors the legacy
/// `while (results->NextRow()) { equipCache[key].push_back(itemId); }`
/// pump at lines 3320-3334.
#[must_use]
pub fn load_equip_cache_rows(rows: &[EquipCacheRow]) -> BotEquipCache {
    let mut cache: BotEquipCache = BTreeMap::new();
    for row in rows {
        let key = BotEquipKey::new(row.level, row.clazz, row.spec, row.slot, row.quality);
        cache.entry(key).or_default().push(row.item_id);
    }
    cache
}

// ── Build path ─────────────────────────────────────────────────────────────

/// Inputs to [`build_equip_cache`]. Keeping this as a struct means the
/// function signature doesn't change when future passes need extra
/// context (e.g. an override unavailable-id set for tests).
pub struct EquipCacheBuildCtx<'a> {
    /// All item prototypes to scan. The legacy code iterates
    /// `sItemStorage`; the manager supplies the sorted list up front.
    pub prototypes: &'a [BotItemPrototype],
    /// Weight-scale slice used only to learn which (class, spec)
    /// combinations exist. The actual stat weights are read from
    /// `item_info`.
    pub scales: &'a [super::types::WeightScale],
    /// Per-item metadata from [`super::item_info::build_item_info_cache`]
    /// — provides `GetStatWeight` and `GetMinLevelFromCache`.
    pub item_info: &'a ItemInfoCache,
    /// Inclusive max bot level (capped by server config in the legacy
    /// code at `sPlayerbotAIConfig.randomBotMaxLevel`).
    pub max_level: u32,
}

/// Fast lookup mirroring the C++
/// `RandomItemMgr::GetStatWeight(itemId, specId)`.
///
/// Returns zero for any combination that doesn't have a cached entry.
#[inline]
#[must_use]
fn get_stat_weight(item_info: &ItemInfoCache, item_id: u32, spec_id: u32) -> u32 {
    if spec_id == 0 || item_id == 0 {
        return 0;
    }
    item_info
        .get(&item_id)
        .map_or(0, |entry| entry.weight(spec_id))
}

/// Fast lookup mirroring the C++
/// `RandomItemMgr::GetMinLevelFromCache(itemId)`.
#[inline]
#[must_use]
fn get_min_level(item_info: &ItemInfoCache, item_id: u32) -> u32 {
    item_info.get(&item_id).map_or(0, |entry| entry.min_level)
}

/// Scan `prototypes` once and collect the body / tabard item lists. The
/// legacy code inlines this inside the main loop and runs it once per
/// `(class, level, spec, quality)` combination — a staggering amount of
/// wasted work, but the resulting lists are identical for every key.
/// The Rust port precomputes them once.
fn collect_body_tabard_lists(prototypes: &[BotItemPrototype]) -> (RandomItemList, RandomItemList) {
    let mut shirts: RandomItemList = Vec::new();
    let mut tabards: RandomItemList = Vec::new();

    for proto in prototypes {
        if is_unavailable(proto.item_id) {
            continue;
        }

        let inv = proto.inventory_type as u8;

        if slots::slot_accepts(slot::BODY, inv)
            && !shirts.contains(&proto.item_id)
        {
            shirts.push(proto.item_id);
        }
        if slots::slot_accepts(slot::TABARD, inv)
            && !tabards.contains(&proto.item_id)
        {
            tabards.push(proto.item_id);
        }
    }

    (shirts, tabards)
}

/// Main entry point — mirrors `RandomItemMgr::BuildEquipCache` in
/// purpose. Returns the populated equip cache keyed by `BotEquipKey`.
#[must_use]
pub fn build_equip_cache(ctx: &EquipCacheBuildCtx<'_>) -> BotEquipCache {
    let mut cache: BotEquipCache = BTreeMap::new();

    // Precompute body/tabard once — see [`collect_body_tabard_lists`].
    let (shirts, tabards) = collect_body_tabard_lists(ctx.prototypes);

    // Build an (class -> Vec<&WeightScale>) index so the innermost loop
    // doesn't rescan the scale slice for every prototype.
    let mut scales_by_class: std::collections::HashMap<u8, Vec<&super::types::WeightScale>> =
        std::collections::HashMap::new();
    for scale in ctx.scales {
        if scale.info.id == 0 {
            continue;
        }
        scales_by_class
            .entry(scale.info.class_id)
            .or_default()
            .push(scale);
    }

    for clazz in (class::WARRIOR)..MAX_CLASSES {
        let class_bit = 1u32 << (u32::from(clazz).saturating_sub(1));
        if (class_bit & CLASSMASK_ALL_PLAYABLE) == 0 {
            continue;
        }
        let Some(class_scales) = scales_by_class.get(&clazz) else {
            continue;
        };

        for level in 1u32..=ctx.max_level {
            for slot_id in 0u8..EQUIPMENT_SLOT_END {
                // Body / tabard slots are handled once at the end of
                // the function via the pre-computed lists — the legacy
                // code's inline loop body for those slots is equivalent
                // to inserting the same list at every key, so we skip
                // them here to keep the cache compact.
                if slot_id == slot::BODY || slot_id == slot::TABARD {
                    continue;
                }

                for scale in class_scales {
                    let spec_id = scale.info.id;
                    if spec_id == 0 || spec_id > MAX_STAT_SCALES {
                        continue;
                    }

                    for quality in ITEM_QUALITY_POOR..=ITEM_QUALITY_ARTIFACT {
                        let key = BotEquipKey::new(level, clazz, spec_id as u8, slot_id, quality);
                        let mut items: RandomItemList = Vec::new();

                        for proto in ctx.prototypes {
                            if proto.quality != key.quality {
                                continue;
                            }
                            if is_unavailable(proto.item_id) {
                                continue;
                            }

                            // Stat-weight gate — cached from item_info.
                            let stat_weight = get_stat_weight(ctx.item_info, proto.item_id, spec_id);
                            if stat_weight == 0 {
                                continue;
                            }
                            // "Useless" filler items (weight == 1) only
                            // survive past level 30 if they have a
                            // random-property roll.
                            if stat_weight == 1 && level > 30 && proto.random_property == 0 {
                                continue;
                            }

                            let min_level = get_min_level(ctx.item_info, proto.item_id);
                            if min_level > level {
                                continue;
                            }
                            // Level window: within 20 levels below `level`.
                            if level.saturating_sub(min_level) > 20 {
                                continue;
                            }

                            // Class gate (weapon/armor/container/projectile only).
                            if proto.class_id != item_class::WEAPON
                                && proto.class_id != item_class::ARMOR
                                && proto.class_id != item_class::CONTAINER
                                && proto.class_id != item_class::PROJECTILE
                            {
                                continue;
                            }

                            if !can_equip_item(key, proto, min_level) {
                                continue;
                            }

                            // Armor class-specific filter (plate at 40+, etc.).
                            if proto.class_id == item_class::ARMOR
                                && is_armor_body_slot(slot_id)
                                && !can_equip_armor(clazz, spec_id, level, proto)
                            {
                                continue;
                            }

                            // Rogue offhand must be a weapon.
                            if slot_id == slot::OFFHAND
                                && key.clazz == class::ROGUE
                                && proto.class_id != item_class::WEAPON
                            {
                                continue;
                            }

                            items.push(proto.item_id);
                        }

                        cache.insert(key, items);
                    }
                }
            }
        }
    }

    // Insert the body / tabard lists at their fixed keys.
    let tabard_key = BotEquipKey::new(BODY_TABARD_FIXED_LEVEL, 1, 1, slot::TABARD, 1);
    let shirt_key = BotEquipKey::new(BODY_TABARD_FIXED_LEVEL, 1, 1, slot::BODY, 1);
    cache.insert(tabard_key, tabards);
    cache.insert(shirt_key, shirts);

    cache
}

/// True if `slot_id` is one of the armor body slots the legacy
/// `BuildEquipCache` checks for the `CanEquipArmor` gate (head, shoulders,
/// chest, waist, legs, feet, wrists, hands).
#[inline]
#[must_use]
fn is_armor_body_slot(slot_id: u8) -> bool {
    matches!(
        slot_id,
        slot::HEAD
            | slot::SHOULDERS
            | slot::CHEST
            | slot::WAIST
            | slot::LEGS
            | slot::FEET
            | slot::WRISTS
            | slot::HANDS
    )
}

// ── Query ──────────────────────────────────────────────────────────────────

/// Mirrors `RandomItemMgr::Query(level, clazz, spec, slot, quality)`.
/// Returns an empty slice when the key is absent from the cache.
#[must_use]
pub fn query_equip_cache(
    cache: &BotEquipCache,
    level: u32,
    clazz: u8,
    spec: u8,
    slot: u8,
    quality: u32,
) -> &[u32] {
    let key = BotEquipKey::new(level, clazz, spec, slot, quality);
    cache.get(&key).map_or(&[][..], Vec::as_slice)
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itempool::equip_filter::{armor_sub, bonding};
    use crate::itempool::types::{ItemInfoEntry, WeightScale, WeightScaleInfo};

    fn scale(id: u32, name: &str, class_id: u8) -> WeightScale {
        WeightScale {
            info: WeightScaleInfo {
                id,
                name: name.to_string(),
                class_id,
            },
            stats: Vec::new(),
        }
    }

    fn warrior_scales() -> Vec<WeightScale> {
        vec![scale(1, "arms", class::WARRIOR)]
    }

    /// Build a minimally-valid mail chest prototype at the given item id
    /// and required level, quality `NORMAL` (2). Mail is used so the
    /// warrior `CanEquipArmor` gate (warriors <40 need mail) accepts it.
    fn chest_proto(item_id: u32, required_level: u32) -> BotItemPrototype {
        let mut proto = BotItemPrototype::default();
        proto.item_id = item_id;
        proto.class_id = item_class::ARMOR;
        proto.sub_class = armor_sub::MAIL;
        proto.inventory_type = u32::from(slots::inv::CHEST);
        proto.quality = 2;
        proto.required_level = required_level;
        proto.allowable_class = -1;
        proto.bonding = bonding::NO_BIND;
        proto
    }

    /// Convenience: build an item-info cache populated with one entry
    /// per prototype, each claiming a fixed stat weight of 10 for
    /// spec 1 and a `min_level` matching `required_level`.
    fn make_item_info(prototypes: &[BotItemPrototype], spec_id: u32) -> ItemInfoCache {
        let mut cache: ItemInfoCache = ItemInfoCache::new();
        for proto in prototypes {
            let mut entry = ItemInfoEntry::new(proto.item_id);
            entry.min_level = proto.required_level;
            entry.weights.insert(spec_id, 10);
            cache.insert(proto.item_id, entry);
        }
        cache
    }

    #[test]
    fn packed_key_order_matches_legacy() {
        let lo = BotEquipKey::new(10, 1, 1, 0, 1);
        let hi = BotEquipKey::new(60, 1, 1, 0, 1);
        // Legacy operator< returns `other.key < this->key` — reverse.
        assert!(hi < lo);
    }

    #[test]
    fn load_equip_cache_rows_groups_items_per_key() {
        let rows = vec![
            EquipCacheRow {
                clazz: 1,
                spec: 1,
                level: 60,
                slot: slot::HEAD,
                quality: 3,
                item_id: 1001,
            },
            EquipCacheRow {
                clazz: 1,
                spec: 1,
                level: 60,
                slot: slot::HEAD,
                quality: 3,
                item_id: 1002,
            },
            EquipCacheRow {
                clazz: 1,
                spec: 1,
                level: 60,
                slot: slot::CHEST,
                quality: 3,
                item_id: 2001,
            },
        ];
        let cache = load_equip_cache_rows(&rows);
        assert_eq!(cache.len(), 2);
        let head = query_equip_cache(&cache, 60, 1, 1, slot::HEAD, 3);
        assert_eq!(head, &[1001, 1002][..]);
        let chest = query_equip_cache(&cache, 60, 1, 1, slot::CHEST, 3);
        assert_eq!(chest, &[2001][..]);
    }

    #[test]
    fn query_missing_key_returns_empty() {
        let cache: BotEquipCache = BTreeMap::new();
        assert!(query_equip_cache(&cache, 60, 1, 1, slot::HEAD, 3).is_empty());
    }

    #[test]
    fn build_empty_prototypes_still_inserts_body_tabard_fixed_keys() {
        let scales = warrior_scales();
        let item_info = ItemInfoCache::new();
        let ctx = EquipCacheBuildCtx {
            prototypes: &[],
            scales: &scales,
            item_info: &item_info,
            max_level: 1,
        };
        let cache = build_equip_cache(&ctx);
        // Even with no prototypes, the fixed keys exist with empty lists.
        let tabard = query_equip_cache(&cache, BODY_TABARD_FIXED_LEVEL, 1, 1, slot::TABARD, 1);
        let shirt = query_equip_cache(&cache, BODY_TABARD_FIXED_LEVEL, 1, 1, slot::BODY, 1);
        assert_eq!(tabard, &[] as &[u32]);
        assert_eq!(shirt, &[] as &[u32]);
    }

    #[test]
    fn build_places_shirt_and_tabard_at_fixed_keys() {
        let mut shirt = BotItemPrototype::default();
        shirt.item_id = 10;
        shirt.class_id = item_class::ARMOR;
        shirt.sub_class = armor_sub::CLOTH;
        shirt.inventory_type = u32::from(slots::inv::BODY);
        shirt.quality = 1;

        let mut tabard = BotItemPrototype::default();
        tabard.item_id = 20;
        tabard.class_id = item_class::ARMOR;
        tabard.sub_class = armor_sub::CLOTH;
        tabard.inventory_type = u32::from(slots::inv::TABARD);
        tabard.quality = 1;

        let prototypes = vec![shirt, tabard];
        let scales = warrior_scales();
        let item_info = ItemInfoCache::new();
        let ctx = EquipCacheBuildCtx {
            prototypes: &prototypes,
            scales: &scales,
            item_info: &item_info,
            max_level: 1,
        };
        let cache = build_equip_cache(&ctx);
        let shirts = query_equip_cache(&cache, BODY_TABARD_FIXED_LEVEL, 1, 1, slot::BODY, 1);
        let tabards = query_equip_cache(&cache, BODY_TABARD_FIXED_LEVEL, 1, 1, slot::TABARD, 1);
        assert_eq!(shirts, &[10][..]);
        assert_eq!(tabards, &[20][..]);
    }

    #[test]
    fn build_includes_eligible_chest_in_warrior_key() {
        let proto = chest_proto(500, 20);
        let prototypes = vec![proto];
        let scales = warrior_scales();
        let item_info = make_item_info(&prototypes, 1);

        let ctx = EquipCacheBuildCtx {
            prototypes: &prototypes,
            scales: &scales,
            item_info: &item_info,
            max_level: 25,
        };
        let cache = build_equip_cache(&ctx);
        // (level=25, warrior, spec=1, CHEST, quality=2) — the proto
        // has required_level=20 (within the 20-level window), quality 2
        // and is plate (warrior-friendly even at 40+, though we're at 25).
        let items = query_equip_cache(&cache, 25, class::WARRIOR, 1, slot::CHEST, 2);
        assert!(items.contains(&500), "chest should be in cache, got {items:?}");
    }

    #[test]
    fn build_skips_out_of_window_min_level() {
        // min_level 5 vs target level 40 — delta > 20, should be skipped.
        let proto = chest_proto(501, 5);
        let prototypes = vec![proto];
        let scales = warrior_scales();
        let item_info = make_item_info(&prototypes, 1);

        let ctx = EquipCacheBuildCtx {
            prototypes: &prototypes,
            scales: &scales,
            item_info: &item_info,
            max_level: 45,
        };
        let cache = build_equip_cache(&ctx);
        let items = query_equip_cache(&cache, 40, class::WARRIOR, 1, slot::CHEST, 2);
        assert!(!items.contains(&501));
    }

    #[test]
    fn build_skips_when_min_level_above_target() {
        let proto = chest_proto(502, 50);
        let prototypes = vec![proto];
        let scales = warrior_scales();
        let item_info = make_item_info(&prototypes, 1);

        let ctx = EquipCacheBuildCtx {
            prototypes: &prototypes,
            scales: &scales,
            item_info: &item_info,
            max_level: 45,
        };
        let cache = build_equip_cache(&ctx);
        let items = query_equip_cache(&cache, 45, class::WARRIOR, 1, slot::CHEST, 2);
        assert!(!items.contains(&502));
    }

    #[test]
    fn build_skips_quality_mismatch() {
        let mut proto = chest_proto(503, 20);
        proto.quality = 3; // uncommon vs queried quality 2
        let prototypes = vec![proto];
        let scales = warrior_scales();
        let item_info = make_item_info(&prototypes, 1);

        let ctx = EquipCacheBuildCtx {
            prototypes: &prototypes,
            scales: &scales,
            item_info: &item_info,
            max_level: 25,
        };
        let cache = build_equip_cache(&ctx);
        let items = query_equip_cache(&cache, 25, class::WARRIOR, 1, slot::CHEST, 2);
        assert!(!items.contains(&503));
        // But the quality-3 key should contain it.
        let items3 = query_equip_cache(&cache, 25, class::WARRIOR, 1, slot::CHEST, 3);
        assert!(items3.contains(&503));
    }

    #[test]
    fn build_skips_zero_stat_weight() {
        // item_info cache without a weight entry → GetStatWeight returns 0.
        let proto = chest_proto(504, 20);
        let prototypes = vec![proto];
        let scales = warrior_scales();
        let item_info: ItemInfoCache = {
            let mut c = ItemInfoCache::new();
            let mut e = ItemInfoEntry::new(504);
            e.min_level = 20;
            // no weights inserted — get returns 0
            c.insert(504, e);
            c
        };

        let ctx = EquipCacheBuildCtx {
            prototypes: &prototypes,
            scales: &scales,
            item_info: &item_info,
            max_level: 25,
        };
        let cache = build_equip_cache(&ctx);
        let items = query_equip_cache(&cache, 25, class::WARRIOR, 1, slot::CHEST, 2);
        assert!(!items.contains(&504));
    }

    #[test]
    fn build_skips_weight_one_above_level_30_without_random_property() {
        let proto = chest_proto(505, 30);
        let prototypes = vec![proto];
        let scales = warrior_scales();
        // weight of exactly 1 → culled at levels > 30 unless random_property.
        let mut item_info: ItemInfoCache = ItemInfoCache::new();
        let mut e = ItemInfoEntry::new(505);
        e.min_level = 30;
        e.weights.insert(1, 1);
        item_info.insert(505, e);

        let ctx = EquipCacheBuildCtx {
            prototypes: &prototypes,
            scales: &scales,
            item_info: &item_info,
            max_level: 35,
        };
        let cache = build_equip_cache(&ctx);
        // level 31 is > 30 → should be skipped
        let items = query_equip_cache(&cache, 31, class::WARRIOR, 1, slot::CHEST, 2);
        assert!(!items.contains(&505));
        // level 30 is not > 30 → still included
        let items30 = query_equip_cache(&cache, 30, class::WARRIOR, 1, slot::CHEST, 2);
        assert!(items30.contains(&505));
    }

    #[test]
    fn build_weight_one_with_random_property_survives_past_30() {
        let mut proto = chest_proto(506, 30);
        proto.random_property = 42;
        let prototypes = vec![proto];
        let scales = warrior_scales();
        let mut item_info: ItemInfoCache = ItemInfoCache::new();
        let mut e = ItemInfoEntry::new(506);
        e.min_level = 30;
        e.weights.insert(1, 1);
        item_info.insert(506, e);

        let ctx = EquipCacheBuildCtx {
            prototypes: &prototypes,
            scales: &scales,
            item_info: &item_info,
            max_level: 35,
        };
        let cache = build_equip_cache(&ctx);
        let items = query_equip_cache(&cache, 35, class::WARRIOR, 1, slot::CHEST, 2);
        assert!(items.contains(&506));
    }

    #[test]
    fn build_filters_rogue_offhand_non_weapon() {
        // A shield prototype offered to a rogue in OFFHAND — rejected.
        let mut shield = BotItemPrototype::default();
        shield.item_id = 700;
        shield.class_id = item_class::ARMOR;
        shield.sub_class = armor_sub::SHIELD;
        shield.inventory_type = u32::from(slots::inv::SHIELD);
        shield.quality = 2;
        shield.required_level = 20;
        shield.allowable_class = -1;
        shield.bonding = bonding::NO_BIND;

        let prototypes = vec![shield];
        let scales = vec![scale(1, "assassination", class::ROGUE)];
        let item_info = make_item_info(&prototypes, 1);

        let ctx = EquipCacheBuildCtx {
            prototypes: &prototypes,
            scales: &scales,
            item_info: &item_info,
            max_level: 25,
        };
        let cache = build_equip_cache(&ctx);
        let items = query_equip_cache(&cache, 25, class::ROGUE, 1, slot::OFFHAND, 2);
        assert!(!items.contains(&700));
    }

    #[test]
    fn build_filters_armor_by_can_equip_armor_for_warrior_at_40_plus() {
        // A cloth chest offered to a warrior at level 40+ — CanEquipArmor
        // rejects it because warriors need plate (or cloak) at 40+.
        let mut cloth = BotItemPrototype::default();
        cloth.item_id = 800;
        cloth.class_id = item_class::ARMOR;
        cloth.sub_class = armor_sub::CLOTH;
        cloth.inventory_type = u32::from(slots::inv::CHEST);
        cloth.quality = 2;
        cloth.required_level = 40;
        cloth.allowable_class = -1;
        cloth.bonding = bonding::NO_BIND;

        let prototypes = vec![cloth];
        let scales = warrior_scales();
        let item_info = make_item_info(&prototypes, 1);

        let ctx = EquipCacheBuildCtx {
            prototypes: &prototypes,
            scales: &scales,
            item_info: &item_info,
            max_level: 45,
        };
        let cache = build_equip_cache(&ctx);
        let items = query_equip_cache(&cache, 45, class::WARRIOR, 1, slot::CHEST, 2);
        assert!(!items.contains(&800));
    }
}
