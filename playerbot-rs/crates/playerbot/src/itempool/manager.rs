//! Top-level orchestrator for the item pool — the Rust counterpart to
//! `RandomItemMgr` as a whole.
//!
//! [`ItemPoolManager`] owns every cache the legacy C++ class manages
//! and exposes a small query surface mirroring the public
//! `RandomItemMgr::*` methods. The individual caches are built inside
//! the submodules of [`super`]; this module just fuses them together
//! under a single lifetime and provides the rebuild entry point.
//!
//! # Init flow
//!
//! [`ItemPoolManager::init`] mirrors `RandomItemMgr::Init` at
//! `RandomItemMgr.cpp:155` verbatim except for the order-independent
//! sequence within each block (the Rust builders share a single
//! prototype scan where possible):
//!
//! 1. Query prototypes + weight-scale rows + quest / vendor / enchant
//!    tables from [`ItemWorld`].
//! 2. Build the [`item_info`](super::item_info) cache.
//! 3. Build the [`equip_cache`](super::equip_cache) cache (depends on
//!    item-info + weight scales).
//! 4. Build consumables (ammo / potion / food / trade) in one pass
//!    (see [`super::consumables`]).
//! 5. Load random enchantments from
//!    `ai_playerbot_item_enchantment_template`.
//! 6. Build the [`random_cache`](super::random_cache).
//! 7. Load the rarity cache from `ai_playerbot_rarity_cache` (or leave
//!    it empty if the table is missing).
//!
//! The legacy `BuildGlyphCache` step is WotLK-only and is a separate
//! subsystem not yet ported — it will plug into this manager when the
//! port lands.

use std::collections::HashMap;

use cmangos::{
    BotItemEnchantmentRow, BotItemPrototype, BotItemRarityRow, BotQuestItemRow, BotVendorItemRow,
    BotWeightScaleRow, BotWeightScaleStatRow, ItemWorld,
};

use super::consumables::ConsumablesCache;
use super::enchant::{self, EnchantCtx, RandomEnchantsCache};
use super::equip_cache::{self, BotEquipCache, EquipCacheBuildCtx, EquipCacheRow};
use super::item_info::{self, ItemInfoBuildCtx, ItemInfoCache, SourceClassifier};
use super::random_cache::{
    self, RandomCacheBuildCtx, RandomCachePredicates, RandomCacheRow, RandomItemCache,
};
use super::rarity::RarityCache;
use super::types::{RandomItemType, WeightScale, WeightScaleInfo, WeightScaleStat};

// ── Snapshot inputs ────────────────────────────────────────────────────────

/// Optional pre-loaded DB rows that let the manager skip the expensive
/// `build_*` pass when the legacy cache tables are populated. Mirrors
/// the "if (results)" branches in `BuildEquipCache` /
/// `BuildRandomItemCache`.
#[derive(Default)]
pub struct PreloadedCaches {
    pub equip_rows: Vec<EquipCacheRow>,
    pub random_rows: Vec<RandomCacheRow>,
    pub rarity_rows: Vec<BotItemRarityRow>,
}

/// DB-facing inputs that don't come from the [`ItemWorld`] trait
/// directly — callers assemble these from the character/world DB rows.
#[derive(Default)]
pub struct ManagerInputs {
    pub quest_items: Vec<BotQuestItemRow>,
    pub vendor_items: Vec<BotVendorItemRow>,
    pub enchant_rows: Vec<BotItemEnchantmentRow>,
    pub preloaded: PreloadedCaches,
}

// ── Manager ────────────────────────────────────────────────────────────────

/// Owns every itempool cache. Cheap to `Default::default()`; all state
/// is filled in by [`ItemPoolManager::init`].
#[derive(Default)]
pub struct ItemPoolManager {
    /// Sorted prototype slice fetched from [`ItemWorld::query_item_prototypes`].
    pub prototypes: Vec<BotItemPrototype>,
    /// Weight-scale entries indexed by spec id. A scale id of `0` is
    /// never used (the legacy code reserves it as the sentinel empty
    /// scale).
    pub scales: Vec<WeightScale>,
    /// Per-item info (minlevel, source, weights, ...).
    pub item_info: ItemInfoCache,
    /// (class, spec, level, slot, quality) → `item_id` list.
    pub equip_cache: BotEquipCache,
    /// Decade-bucketed random-item pool (guild-task rewards).
    pub random_cache: RandomItemCache,
    /// Ammo / potion / food / trade caches.
    pub consumables: ConsumablesCache,
    /// Random enchantment roll pool.
    pub enchants: RandomEnchantsCache,
    /// `ai_playerbot_rarity_cache` snapshot.
    pub rarity: RarityCache,
    /// `randomBotMaxLevel` after the sanity cap from
    /// `sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL)`.
    pub max_level: u32,
}

impl ItemPoolManager {
    /// Fresh empty manager.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuild every cache from a fresh [`ItemWorld`] snapshot.
    ///
    /// `max_level` is the server's `sPlayerbotAIConfig.randomBotMaxLevel`.
    /// `inputs` supplies the DB rows the trait doesn't already expose
    /// (quest / vendor / enchant / rarity tables).
    pub fn init(&mut self, world: &dyn ItemWorld, max_level: u32, inputs: &ManagerInputs) {
        self.max_level = max_level;
        self.prototypes = world.query_item_prototypes();

        // Weight scales (rows + stats → structs).
        let scale_rows = world.query_weight_scales();
        let scale_stat_rows = world.query_weight_scale_stats();
        self.scales = build_weight_scales(&scale_rows, &scale_stat_rows);

        // Item info cache.
        let classifier = build_source_classifier(&inputs.quest_items, &inputs.vendor_items);
        let info_ctx = ItemInfoBuildCtx {
            world,
            prototypes: &self.prototypes,
            scales: &self.scales,
            classifier: &classifier,
            whitelist: &[],
        };
        self.item_info = item_info::build_item_info_cache(&info_ctx);

        // Equip cache — either loaded from DB or built fresh.
        self.equip_cache = if inputs.preloaded.equip_rows.is_empty() {
            let ctx = EquipCacheBuildCtx {
                prototypes: &self.prototypes,
                scales: &self.scales,
                item_info: &self.item_info,
                max_level,
            };
            equip_cache::build_equip_cache(&ctx)
        } else {
            equip_cache::load_equip_cache_rows(&inputs.preloaded.equip_rows)
        };

        // Consumables — single pass across prototypes.
        let spell_lookup: super::consumables::SpellLookup<'_> =
            &|spell_id: i32| world.lookup_spell_entry(spell_id as u32);
        self.consumables.build(&self.prototypes, max_level, spell_lookup);

        // Random enchantments.
        self.enchants.load(&inputs.enchant_rows);

        // Random-item cache — DB rows or fresh build.
        self.random_cache = if inputs.preloaded.random_rows.is_empty() {
            let predicates = RandomCachePredicates::new();
            let ctx = RandomCacheBuildCtx {
                prototypes: &self.prototypes,
                predicates: &predicates,
            };
            random_cache::build_random_cache(&ctx)
        } else {
            random_cache::load_random_cache_rows(&inputs.preloaded.random_rows)
        };

        // Rarity cache.
        self.rarity.load(&inputs.preloaded.rarity_rows);
    }

    /// Query the equip cache — mirrors
    /// `RandomItemMgr::Query(level, clazz, spec, slot, quality)`.
    #[must_use]
    pub fn query_equip(
        &self,
        level: u32,
        clazz: u8,
        spec: u8,
        slot: u8,
        quality: u32,
    ) -> &[u32] {
        equip_cache::query_equip_cache(&self.equip_cache, level, clazz, spec, slot, quality)
    }

    /// Query the random-item pool — mirrors
    /// `RandomItemMgr::Query(level, type, predicate)`. Allocates a
    /// filtered copy so the result outlives any borrow on `self`.
    #[must_use]
    pub fn query_random(&self, level: u32, kind: RandomItemType) -> Vec<u32> {
        let index = self.prototypes_by_id();
        random_cache::query_random_cache(&self.random_cache, level, kind, None, &index)
    }

    /// Pick a random item id from the bucket — mirrors
    /// `RandomItemMgr::GetRandomItem`. Returns `0` when the bucket is
    /// empty.
    pub fn get_random_item(
        &self,
        world: &dyn ItemWorld,
        level: u32,
        kind: RandomItemType,
    ) -> u32 {
        let index = self.prototypes_by_id();
        random_cache::get_random_item(&self.random_cache, world, level, kind, None, &index)
    }

    /// Pre-computed stat weight lookup — mirrors
    /// `RandomItemMgr::GetStatWeight(itemId, specId)` on the legacy
    /// class.
    #[must_use]
    pub fn stat_weight(&self, item_id: u32, spec_id: u32) -> u32 {
        if spec_id == 0 || item_id == 0 {
            return 0;
        }
        self.item_info
            .get(&item_id)
            .map_or(0, |entry| entry.weight(spec_id))
    }

    /// Cached minimum level for `item_id`, or `0` when unknown.
    /// Mirrors `RandomItemMgr::GetMinLevelFromCache`.
    #[must_use]
    pub fn min_level(&self, item_id: u32) -> u32 {
        self.item_info
            .get(&item_id)
            .map_or(0, |entry| entry.min_level)
    }

    /// Resolve the rarity for `item_id`. Returns the DB-cached value if
    /// present, otherwise falls back to
    /// [`rarity::quality_heuristic`] keyed on the item's quality from
    /// the info cache. Mirrors `RandomItemMgr::GetItemRarity`.
    #[must_use]
    pub fn item_rarity(&self, item_id: u32) -> f32 {
        if item_id == 0 {
            return 0.0;
        }
        let quality = self
            .item_info
            .get(&item_id)
            .map_or(0, |entry| entry.quality);
        self.rarity.get_or_heuristic(item_id, quality)
    }

    /// Query the ammo cache. Mirrors `RandomItemMgr::GetAmmo`.
    #[must_use]
    pub fn ammo(&self, level: u32, sub_class: u32) -> u32 {
        self.consumables.get_ammo(level, sub_class)
    }

    /// Pick a random potion from the cache. Mirrors
    /// `RandomItemMgr::GetRandomPotion`. Returns `0` when the bucket is
    /// empty.
    pub fn random_potion(&self, world: &dyn ItemWorld, level: u32, effect: u32) -> u32 {
        let mut urand_fn = |min, max| world.urand_range(min, max);
        self.consumables
            .get_random_potion(level, effect, &mut urand_fn)
    }

    /// Pick a random food from the cache. Mirrors
    /// `RandomItemMgr::GetRandomFood`. Returns `0` when the bucket is
    /// empty.
    pub fn random_food(&self, world: &dyn ItemWorld, level: u32, category: u32) -> u32 {
        let mut urand_fn = |min, max| world.urand_range(min, max);
        self.consumables
            .get_random_food(level, category, &mut urand_fn)
    }

    /// Pick a random trade good from the cache. Mirrors
    /// `RandomItemMgr::GetRandomTrade`. Returns `0` when the bucket is
    /// empty.
    pub fn random_trade(&self, world: &dyn ItemWorld, level: u32) -> u32 {
        let mut urand_fn = |min, max| world.urand_range(min, max);
        self.consumables.get_random_trade(level, &mut urand_fn)
    }

    /// Score an enchant id for `(class, spec)`. Returns `0` when the
    /// scale does not exist. Mirrors
    /// `RandomItemMgr::CalculateEnchantWeight`.
    pub fn calculate_enchant_weight(
        &self,
        world: &dyn ItemWorld,
        class_id: u8,
        spec_id: u32,
        enchant_id: u32,
    ) -> u32 {
        let Some(scale) = self.scale_for(spec_id) else {
            return 0;
        };
        let ctx = EnchantCtx {
            player_class: class_id,
            spec_id,
            scale,
            world,
        };
        enchant::calculate_enchant_weight(&ctx, enchant_id)
    }

    /// Pick the best random enchant id rollable for `item_id` under
    /// `(class, spec)`. Mirrors
    /// `RandomItemMgr::CalculateBestRandomEnchantId`. Returns `0` when
    /// the item has no random property entry or every roll scores 0.
    pub fn calculate_best_random_enchant_id(
        &self,
        world: &dyn ItemWorld,
        class_id: u8,
        spec_id: u32,
        item_id: u32,
    ) -> u32 {
        let Some(scale) = self.scale_for(spec_id) else {
            return 0;
        };
        let Some(proto) = self.prototype(item_id) else {
            return 0;
        };
        let ctx = EnchantCtx {
            player_class: class_id,
            spec_id,
            scale,
            world,
        };
        enchant::calculate_best_random_enchant_id(&ctx, &self.enchants, proto)
    }

    /// Look up a prototype by id. Returns `None` when the id is not in
    /// the cache.
    #[must_use]
    pub fn prototype(&self, item_id: u32) -> Option<&BotItemPrototype> {
        self.prototypes.iter().find(|p| p.item_id == item_id)
    }

    /// Look up a weight scale by id. Returns `None` when the id is
    /// `0` (the sentinel) or not loaded.
    #[must_use]
    pub fn scale_for(&self, spec_id: u32) -> Option<&WeightScale> {
        if spec_id == 0 {
            return None;
        }
        self.scales.iter().find(|s| s.info.id == spec_id)
    }

    /// Build a `HashMap<item_id, &proto>` over the current prototype
    /// slice. Used internally by the random-cache query paths that
    /// re-apply predicates.
    fn prototypes_by_id(&self) -> HashMap<u32, &BotItemPrototype> {
        let mut map = HashMap::with_capacity(self.prototypes.len());
        for proto in &self.prototypes {
            map.insert(proto.item_id, proto);
        }
        map
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Convert raw DB rows into the [`WeightScale`] vector used by the
/// `item_info` and `equip_cache` builders. Rows are indexed by scale id
/// so stats rows can attach to the right scale in a single pass.
fn build_weight_scales(
    scale_rows: &[BotWeightScaleRow],
    stat_rows: &[BotWeightScaleStatRow],
) -> Vec<WeightScale> {
    let mut scales: HashMap<u32, WeightScale> = HashMap::with_capacity(scale_rows.len());
    for row in scale_rows {
        let info = WeightScaleInfo {
            id: row.id,
            name: c_fixed_str(&row.name).to_string(),
            class_id: row.class_id as u8,
        };
        scales.insert(
            row.id,
            WeightScale {
                info,
                stats: Vec::new(),
            },
        );
    }
    for row in stat_rows {
        if let Some(scale) = scales.get_mut(&row.scale_id) {
            scale.stats.push(WeightScaleStat {
                stat: c_fixed_str(&row.stat).to_string(),
                weight: row.weight.max(0) as u32,
            });
        }
    }

    let mut out: Vec<WeightScale> = scales.into_values().collect();
    out.sort_by_key(|scale| scale.info.id);
    out
}

/// Build the source classifier from the DB rows the caller already has
/// on hand. Empty drop tables are the norm today (the drop-map loader
/// was stripped from the legacy module) so the classifier only
/// populates the quest / vendor maps.
fn build_source_classifier<'a>(
    quest_rows: &'a [BotQuestItemRow],
    vendor_rows: &'a [BotVendorItemRow],
) -> SourceClassifier<'a> {
    // Leak small empty boxes for the PvP / drop sets so the classifier
    // struct has a stable lifetime. The size is negligible (< 200 B
    // per boot) and the manager is a long-lived singleton.
    use std::collections::HashSet;

    let alliance_pvp: &'a HashSet<u32> = Box::leak(Box::<HashSet<u32>>::default());
    let horde_pvp: &'a HashSet<u32> = Box::leak(Box::<HashSet<u32>>::default());
    let creature_drops: &'a HashMap<u32, Vec<u32>> =
        Box::leak(Box::<HashMap<u32, Vec<u32>>>::default());
    let gameobject_drops: &'a HashMap<u32, Vec<u32>> =
        Box::leak(Box::<HashMap<u32, Vec<u32>>>::default());

    let mut quests_by_item: HashMap<u32, Vec<&'a BotQuestItemRow>> = HashMap::new();
    for row in quest_rows {
        quests_by_item.entry(row.item_id).or_default().push(row);
    }
    let mut vendors_by_item: HashMap<u32, Vec<&'a BotVendorItemRow>> = HashMap::new();
    for row in vendor_rows {
        vendors_by_item.entry(row.item_id).or_default().push(row);
    }

    // Leak the two item-id lookup maps too — they store references to
    // the caller's row slices, which must outlive the classifier. The
    // caller owns the rows for the whole `init` call so this is safe.
    let quests_by_item: &'a HashMap<u32, Vec<&'a BotQuestItemRow>> =
        Box::leak(Box::new(quests_by_item));
    let vendors_by_item: &'a HashMap<u32, Vec<&'a BotVendorItemRow>> =
        Box::leak(Box::new(vendors_by_item));

    SourceClassifier {
        quests_by_item,
        vendors_by_item,
        alliance_pvp_items: alliance_pvp,
        horde_pvp_items: horde_pvp,
        creature_drops,
        gameobject_drops,
    }
}

/// Decode a nul-terminated `[c_char; N]` into a borrowed `&str`. Falls
/// back to `""` on invalid UTF-8 — matches how `item_info` handles
/// prototype names.
fn c_fixed_str(buf: &[core::ffi::c_char]) -> &str {
    #[allow(unsafe_code)]
    let bytes: &[u8] = unsafe {
        core::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), buf.len())
    };
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..end]).unwrap_or("")
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::MockItemWorld;

    #[test]
    fn empty_manager_default() {
        let mgr = ItemPoolManager::new();
        assert_eq!(mgr.max_level, 0);
        assert!(mgr.prototypes.is_empty());
        assert!(mgr.scales.is_empty());
        assert!(mgr.item_info.is_empty());
        assert!(mgr.equip_cache.is_empty());
        assert!(mgr.random_cache.is_empty());
    }

    #[test]
    fn init_with_empty_world_produces_empty_caches() {
        let world = MockItemWorld::new();
        let mut mgr = ItemPoolManager::new();
        let inputs = ManagerInputs::default();
        mgr.init(&world, 60, &inputs);
        assert_eq!(mgr.max_level, 60);
        assert!(mgr.prototypes.is_empty());
        // equip_cache still has the body/tabard fixed keys even with
        // zero prototypes.
        assert!(!mgr.equip_cache.is_empty());
        assert!(mgr.random_cache.is_empty());
    }

    #[test]
    fn stat_weight_returns_zero_for_missing_entries() {
        let mgr = ItemPoolManager::new();
        assert_eq!(mgr.stat_weight(0, 1), 0);
        assert_eq!(mgr.stat_weight(1234, 0), 0);
        assert_eq!(mgr.stat_weight(1234, 1), 0);
    }

    #[test]
    fn min_level_returns_zero_when_cache_empty() {
        let mgr = ItemPoolManager::new();
        assert_eq!(mgr.min_level(100), 0);
    }

    #[test]
    fn init_with_preloaded_equip_rows_skips_build() {
        let world = MockItemWorld::new();
        let mut mgr = ItemPoolManager::new();
        let inputs = ManagerInputs {
            preloaded: PreloadedCaches {
                equip_rows: vec![EquipCacheRow {
                    clazz: 1,
                    spec: 1,
                    level: 60,
                    slot: 4, // CHEST
                    quality: 3,
                    item_id: 4242,
                }],
                ..PreloadedCaches::default()
            },
            ..ManagerInputs::default()
        };
        mgr.init(&world, 60, &inputs);
        let items = mgr.query_equip(60, 1, 1, 4, 3);
        assert_eq!(items, &[4242][..]);
    }
}
