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
use super::equip_filter::{gem_sub, item_class};
use super::item_info::{self, ItemInfoBuildCtx, ItemInfoCache, SourceClassifier, team};
use super::player_spec;
use super::random_cache::{
    self, RandomCacheBuildCtx, RandomCachePredicates, RandomCacheRow, RandomItemCache,
};
use super::rarity::RarityCache;
use super::slots::slot;
use super::types::{ItemSource, RandomItemType, WeightScale, WeightScaleInfo, WeightScaleStat};

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
    /// Forward index `quest_id → reward item ids` built from the quest
    /// table during [`Self::init`]. Mirrors the per-quest reward lookups
    /// that `HasSameQuestRewards` issues against `sObjectMgr` in the
    /// legacy code — pre-computed so the runtime check stays pure.
    pub quest_rewards: HashMap<u32, Vec<u32>>,
    /// Gem item ids — every non-"simple" prototype from `item_class::GEM`.
    /// Empty on vanilla (gems don't exist); populated on TBC/WotLK. Used
    /// by the factory's socket-enchant roll.
    pub gems: Vec<u32>,
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

        // Forward quest-reward index. `BotQuestItemRow` is already
        // (item, quest) — bucket by quest so `has_same_quest_rewards`
        // can enumerate all reward items for a given quest in O(1).
        self.quest_rewards = HashMap::with_capacity(inputs.quest_items.len());
        for row in &inputs.quest_items {
            self.quest_rewards
                .entry(row.quest_id)
                .or_default()
                .push(row.item_id);
        }

        // Gem list — empty on vanilla (no gems), populated on TBC/WotLK
        // by walking the prototype table. Matches `GetGemsList` in the
        // legacy C++.
        self.gems.clear();
        if cfg!(not(feature = "vanilla")) {
            for proto in &self.prototypes {
                if proto.class_id == item_class::GEM && proto.sub_class != gem_sub::SIMPLE {
                    self.gems.push(proto.item_id);
                }
            }
        }
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

    /// Full gem list for socket rolls. Mirrors
    /// `RandomItemMgr::GetGemsList` — empty on vanilla, populated from
    /// the prototype table on TBC/WotLK.
    #[must_use]
    pub fn gems_list(&self) -> &[u32] {
        &self.gems
    }

    /// True when the player already holds at least one reward item from
    /// any quest that can give `item_id`. Mirrors
    /// `RandomItemMgr::HasSameQuestRewards`.
    ///
    /// Returns `false` when `item_id` is not in the info cache, when
    /// its source is not `Quest`, or when none of the quests reward an
    /// item the player holds.
    pub fn has_same_quest_rewards(
        &self,
        world: &dyn ItemWorld,
        guid_low: u32,
        item_id: u32,
    ) -> bool {
        let Some(info) = self.item_info.get(&item_id) else {
            return false;
        };
        if ItemSource::from_raw(info.source) != ItemSource::Quest {
            return false;
        }
        for quest_id in &info.source_ids {
            let Some(rewards) = self.quest_rewards.get(quest_id) else {
                continue;
            };
            for &reward_id in rewards {
                if reward_id != 0 && world.player_has_item(guid_low, reward_id) {
                    return true;
                }
            }
        }
        false
    }

    /// Resolve the talent-tab-based spec id for a logged-in player.
    /// Mirrors `RandomItemMgr::GetPlayerSpecId`, which looks up the
    /// best tab via `GetPlayerSpecTab`, maps it to a spec name, then
    /// does a linear scan through the weight scales.
    ///
    /// Returns `0` when the player context is missing, the spec name
    /// can't be derived (e.g. an unsupported class id), or no scale
    /// row matches `(class, name)`.
    pub fn player_spec_id(&self, world: &dyn ItemWorld, guid_low: u32) -> u32 {
        let Some(ctx) = world.player_item_ctx(guid_low) else {
            return 0;
        };
        let tab_info = world.player_talent_tab(guid_low);
        let mut urand = |min, max| world.urand_range(min, max);
        let Some(name) =
            player_spec::player_spec_name(ctx.class_id as u8, tab_info.best_tab, ctx.level, &mut urand)
        else {
            return 0;
        };
        player_spec::player_spec_id(&self.scales, ctx.class_id as u8, name)
    }

    /// Runtime stat weight for an item against a live player — the
    /// filtered counterpart to [`Self::stat_weight`]. Mirrors
    /// `RandomItemMgr::GetLiveStatWeight` verbatim, including the
    /// vanilla-only `×5` boost for PvP-rank items and the Classic-only
    /// "no-stats" trinket guard.
    ///
    /// Passing `spec_id == 0` is supported — the method falls back to
    /// [`Self::player_spec_id`] just like the legacy C++ does.
    pub fn live_stat_weight(
        &self,
        world: &dyn ItemWorld,
        guid_low: u32,
        item_id: u32,
        mut spec_id: u32,
    ) -> u32 {
        if item_id == 0 {
            return 0;
        }
        let Some(info) = self.item_info.get(&item_id) else {
            return 0;
        };
        let Some(ctx) = world.player_item_ctx(guid_low) else {
            return 0;
        };

        // Fall back to the talent-tab lookup if the caller didn't
        // supply a spec explicitly — legacy `GetLiveStatWeight` does
        // the same short-circuit.
        if spec_id == 0 {
            spec_id = self.player_spec_id(world, guid_low);
            if spec_id == 0 {
                return 0;
            }
        }
        let Some(scale) = self.scale_for(spec_id) else {
            return 0;
        };
        // Unused binding silenced — we only needed the lookup to
        // verify the scale exists, matching the C++ early-exit.
        let _ = scale;

        let stat_weight = info.weight(spec_id);

        // Level gate: skip items whose min level is above the player.
        if info.min_level > ctx.level {
            return 0;
        }

        // Faction gate: `ctx.team == 0` is Alliance, `1` is Horde
        // (see `BotPlayerItemCtx` documentation in botffi.h).
        let ctx_team = match ctx.team {
            0 => team::ALLIANCE,
            1 => team::HORDE,
            _ => team::NONE,
        };
        if info.team != team::NONE && info.team != team::BOTH && info.team != ctx_team {
            return 0;
        }

        // Quest gate: the item must come from a quest the player can
        // actually do (class/race/level satisfied, event active) and
        // hasn't been rewarded yet. `cmangos::QuestStatus` bitflags
        // mirror the `SatisfyQuest*` predicates in the C++.
        if ItemSource::from_raw(info.source) == ItemSource::Quest && !info.source_ids.is_empty() {
            let mut can_do_quest = false;
            for &quest_id in &info.source_ids {
                let status = world.player_quest_status(guid_low, quest_id);
                let satisfies = status.contains(cmangos::QuestStatus::SATISFIES_CLASS)
                    && status.contains(cmangos::QuestStatus::SATISFIES_RACE)
                    && status.contains(cmangos::QuestStatus::SATISFIES_LEVEL);
                if !satisfies {
                    continue;
                }
                // C++ also requires `quest->IsActive()` — the bridge
                // folds that into the status bitmask.
                if !status.contains(cmangos::QuestStatus::IS_ACTIVE) {
                    continue;
                }
                can_do_quest = true;
                break;
            }
            if !can_do_quest {
                return 0;
            }
        }

        // Reputation gate.
        if info.rep_faction != 0 {
            let rank = world.player_reputation_rank(guid_low, info.rep_faction);
            if (rank as i64) < i64::from(info.rep_rank) {
                return 0;
            }
        }

        // PvP-rank gate. Classic has the extra "not-top-rank
        // off-slots" guard.
        #[cfg(feature = "vanilla")]
        {
            if info.pvp_rank != 0 && ctx.pvp_rank < info.pvp_rank {
                return 0;
            }
            if info.pvp_rank != 0 && info.pvp_rank < 16 && ctx.pvp_rank == 18 {
                return 0;
            }

            // Non-pvp items for specs that don't care about pvp gear.
            if info.pvp_rank < 16 && ctx.pvp_rank == 18 {
                let allowed_slot = info.slot == u32::from(slot::WAIST)
                    || info.slot == u32::from(slot::WRISTS)
                    || info.slot == u32::from(slot::BACK)
                    || info.slot == u32::from(slot::NECK)
                    || info.slot == u32::from(slot::TRINKET1)
                    || info.slot == u32::from(slot::TRINKET2)
                    || info.slot == u32::from(slot::FINGER1)
                    || info.slot == u32::from(slot::FINGER2);
                let is_shaman_totem_or_relic = self.prototype(item_id).is_some_and(|proto| {
                    use super::equip_filter::armor_sub;
                    proto.sub_class == armor_sub::TOTEM
                        || proto.sub_class == armor_sub::LIBRAM
                        || proto.sub_class == armor_sub::IDOL
                });
                let healing_spec = matches!(spec_id, 3 | 4 | 5 | 14 | 22 | 31);
                if !allowed_slot && !is_shaman_totem_or_relic && !healing_spec {
                    return 0;
                }
            }
        }
        #[cfg(not(feature = "vanilla"))]
        {
            if info.pvp_rank != 0 && ctx.pvp_rank < info.pvp_rank {
                return 0;
            }
        }

        // Skill gate.
        if info.req_skill != 0
            && world.player_skill_value(guid_low, info.req_skill) < info.req_skill_rank
        {
            return 0;
        }

        // "No-stats" trinket / off-slot guard — the legacy code treats
        // a `weight == 1` filler as "has stats but none the spec cares
        // about" and skips it for accessory slots.
        if stat_weight == 1
            && (info.slot == u32::from(slot::NECK)
                || info.slot == u32::from(slot::TRINKET1)
                || info.slot == u32::from(slot::TRINKET2)
                || info.slot == u32::from(slot::FINGER1)
                || info.slot == u32::from(slot::FINGER2))
        {
            return 0;
        }

        // Classic only: boost PvP-rank items so they compete with
        // comparable raid drops.
        #[cfg(feature = "vanilla")]
        {
            if info.pvp_rank != 0 {
                return stat_weight.saturating_mul(5);
            }
        }

        stat_weight
    }

    /// Best enchant score an item can roll under `(spec, item)`.
    /// Mirrors `RandomItemMgr::GetBestRandomEnchantStatWeight`, which
    /// walks the scale table to pull the class id from the spec, calls
    /// `CalculateBestRandomEnchantId`, then scores the resulting
    /// enchant. Returns `0` when any step in the chain fails.
    pub fn best_random_enchant_stat_weight(
        &self,
        world: &dyn ItemWorld,
        item_id: u32,
        spec_id: u32,
    ) -> u32 {
        if item_id == 0 || spec_id == 0 {
            return 0;
        }
        if !self.item_info.contains_key(&item_id) {
            return 0;
        }
        let Some(scale) = self.scale_for(spec_id) else {
            return 0;
        };
        let class_id = scale.info.class_id;
        if class_id == 0 {
            return 0;
        }
        let best_enchant = self.calculate_best_random_enchant_id(world, class_id, spec_id, item_id);
        if best_enchant == 0 {
            return 0;
        }
        self.calculate_enchant_weight(world, class_id, spec_id, best_enchant)
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

    #[test]
    fn init_populates_quest_rewards_index() {
        use cmangos::BotQuestItemRow;
        let world = MockItemWorld::new();
        let mut mgr = ItemPoolManager::new();
        let inputs = ManagerInputs {
            quest_items: vec![
                BotQuestItemRow {
                    item_id: 1111,
                    quest_id: 42,
                    ..BotQuestItemRow::default()
                },
                BotQuestItemRow {
                    item_id: 2222,
                    quest_id: 42,
                    ..BotQuestItemRow::default()
                },
                BotQuestItemRow {
                    item_id: 3333,
                    quest_id: 99,
                    ..BotQuestItemRow::default()
                },
            ],
            ..ManagerInputs::default()
        };
        mgr.init(&world, 60, &inputs);
        assert_eq!(mgr.quest_rewards.len(), 2);
        let mut quest42 = mgr.quest_rewards.get(&42).cloned().unwrap();
        quest42.sort_unstable();
        assert_eq!(quest42, vec![1111, 2222]);
        assert_eq!(mgr.quest_rewards.get(&99), Some(&vec![3333]));
    }

    #[test]
    #[cfg(feature = "vanilla")]
    fn init_leaves_gems_empty_on_vanilla() {
        use cmangos::BotItemPrototype;
        let world = MockItemWorld::new();
        world.set_prototypes(vec![BotItemPrototype {
            item_id: 555,
            class_id: item_class::GEM,
            sub_class: 2, // not "simple"
            ..BotItemPrototype::default()
        }]);
        let mut mgr = ItemPoolManager::new();
        mgr.init(&world, 60, &ManagerInputs::default());
        assert!(mgr.gems_list().is_empty());
    }

    #[test]
    #[cfg(not(feature = "vanilla"))]
    fn init_populates_gems_on_tbc_plus() {
        use cmangos::BotItemPrototype;
        let world = MockItemWorld::new();
        world.set_prototypes(vec![
            BotItemPrototype {
                item_id: 100,
                class_id: item_class::GEM,
                sub_class: 2, // red
                ..BotItemPrototype::default()
            },
            BotItemPrototype {
                item_id: 101,
                class_id: item_class::GEM,
                sub_class: gem_sub::SIMPLE,
                ..BotItemPrototype::default()
            },
            BotItemPrototype {
                item_id: 200,
                class_id: item_class::ARMOR,
                ..BotItemPrototype::default()
            },
        ]);
        let mut mgr = ItemPoolManager::new();
        mgr.init(&world, 70, &ManagerInputs::default());
        assert_eq!(mgr.gems_list(), &[100][..]);
    }

    #[test]
    fn has_same_quest_rewards_false_when_item_missing() {
        let mgr = ItemPoolManager::new();
        let world = MockItemWorld::new();
        assert!(!mgr.has_same_quest_rewards(&world, 42, 9999));
    }

    #[test]
    fn has_same_quest_rewards_false_for_non_quest_items() {
        use super::super::types::ItemInfoEntry;
        let mut mgr = ItemPoolManager::new();
        mgr.item_info.insert(
            123,
            ItemInfoEntry {
                source: ItemSource::Vendor.as_u32(),
                source_ids: vec![1],
                ..ItemInfoEntry::new(123)
            },
        );
        let world = MockItemWorld::new();
        assert!(!mgr.has_same_quest_rewards(&world, 42, 123));
    }

    #[test]
    fn has_same_quest_rewards_true_when_player_holds_reward() {
        use super::super::types::ItemInfoEntry;
        let mut mgr = ItemPoolManager::new();
        mgr.item_info.insert(
            123,
            ItemInfoEntry {
                source: ItemSource::Quest.as_u32(),
                source_ids: vec![42],
                ..ItemInfoEntry::new(123)
            },
        );
        // Quest 42 rewards items 100 and 200.
        mgr.quest_rewards.insert(42, vec![100, 200]);
        let world = MockItemWorld::new();
        world.set_player_has_item(7, 100, true);
        assert!(mgr.has_same_quest_rewards(&world, 7, 123));
        // Different guid — no duplicate.
        assert!(!mgr.has_same_quest_rewards(&world, 8, 123));
    }

    #[test]
    fn has_same_quest_rewards_false_when_quest_not_in_index() {
        use super::super::types::ItemInfoEntry;
        let mut mgr = ItemPoolManager::new();
        mgr.item_info.insert(
            123,
            ItemInfoEntry {
                source: ItemSource::Quest.as_u32(),
                source_ids: vec![42],
                ..ItemInfoEntry::new(123)
            },
        );
        // quest_rewards intentionally empty
        let world = MockItemWorld::new();
        world.set_player_has_item(7, 100, true);
        assert!(!mgr.has_same_quest_rewards(&world, 7, 123));
    }

    /// Shared helper — build a one-scale manager for `class_id=1, spec=1`.
    fn warrior_fury_manager() -> ItemPoolManager {
        let mut mgr = ItemPoolManager::new();
        mgr.scales.push(WeightScale {
            info: WeightScaleInfo {
                id: 1,
                name: "fury".into(),
                class_id: 1,
            },
            stats: Vec::new(),
        });
        mgr
    }

    /// Shared helper — build a warrior `BotPlayerItemCtx` with the
    /// given `(level, team, pvp_rank)`.
    fn warrior_ctx(guid: u32, level: u32, team: u32, pvp_rank: u32) -> cmangos::BotPlayerItemCtx {
        cmangos::BotPlayerItemCtx {
            guid_low: guid,
            level,
            class_id: 1,
            race_id: 1,
            team,
            pvp_rank,
            map_id: 0,
            in_world: true,
        }
    }

    #[test]
    fn live_stat_weight_zero_for_missing_player_ctx() {
        use super::super::types::ItemInfoEntry;
        let mut mgr = warrior_fury_manager();
        mgr.item_info.insert(
            123,
            ItemInfoEntry {
                weights: [(1u32, 100u32)].into(),
                ..ItemInfoEntry::new(123)
            },
        );
        let world = MockItemWorld::new();
        // No ctx seeded -> returns 0.
        assert_eq!(mgr.live_stat_weight(&world, 7, 123, 1), 0);
    }

    #[test]
    fn live_stat_weight_applies_level_and_team_gates() {
        use super::super::types::ItemInfoEntry;
        let mut mgr = warrior_fury_manager();
        // Item requires level 60 and is horde-locked.
        mgr.item_info.insert(
            123,
            ItemInfoEntry {
                min_level: 60,
                team: team::HORDE,
                weights: [(1u32, 100u32)].into(),
                ..ItemInfoEntry::new(123)
            },
        );

        let world = MockItemWorld::new();
        // Level 50 alliance player — both gates fail.
        world.set_player_ctx(7, warrior_ctx(7, 50, 0, 0));
        assert_eq!(mgr.live_stat_weight(&world, 7, 123, 1), 0);

        // Level 60 horde player — both gates pass.
        world.set_player_ctx(8, warrior_ctx(8, 60, 1, 0));
        assert_eq!(mgr.live_stat_weight(&world, 8, 123, 1), 100);
    }

    #[test]
    fn live_stat_weight_zero_for_no_stats_trinket() {
        use super::super::types::ItemInfoEntry;
        let mut mgr = warrior_fury_manager();
        mgr.item_info.insert(
            123,
            ItemInfoEntry {
                weights: [(1u32, 1u32)].into(), // filler weight
                slot: u32::from(slot::TRINKET1),
                ..ItemInfoEntry::new(123)
            },
        );
        let world = MockItemWorld::new();
        world.set_player_ctx(7, warrior_ctx(7, 60, 0, 0));
        assert_eq!(mgr.live_stat_weight(&world, 7, 123, 1), 0);
    }

    #[test]
    #[cfg(feature = "vanilla")]
    fn live_stat_weight_boosts_pvp_items_on_vanilla() {
        use super::super::types::ItemInfoEntry;
        let mut mgr = warrior_fury_manager();
        mgr.item_info.insert(
            123,
            ItemInfoEntry {
                weights: [(1u32, 20u32)].into(),
                pvp_rank: 5,
                ..ItemInfoEntry::new(123)
            },
        );
        let world = MockItemWorld::new();
        world.set_player_ctx(7, warrior_ctx(7, 60, 0, 10));
        assert_eq!(mgr.live_stat_weight(&world, 7, 123, 1), 100); // 20 * 5
    }

    #[test]
    fn best_random_enchant_stat_weight_zero_without_scale() {
        let mgr = ItemPoolManager::new();
        let world = MockItemWorld::new();
        assert_eq!(mgr.best_random_enchant_stat_weight(&world, 0, 1), 0);
        assert_eq!(mgr.best_random_enchant_stat_weight(&world, 123, 0), 0);
        assert_eq!(mgr.best_random_enchant_stat_weight(&world, 123, 1), 0);
    }

    #[test]
    fn player_spec_id_zero_without_ctx() {
        let mgr = ItemPoolManager::new();
        let world = MockItemWorld::new();
        assert_eq!(mgr.player_spec_id(&world, 7), 0);
    }

    #[test]
    fn player_spec_id_resolves_warrior_arms() {
        use cmangos::TalentTabInfo;
        let mut mgr = ItemPoolManager::new();
        mgr.scales.push(WeightScale {
            info: WeightScaleInfo {
                id: 1,
                name: "arms".into(),
                class_id: 1,
            },
            stats: Vec::new(),
        });
        mgr.scales.push(WeightScale {
            info: WeightScaleInfo {
                id: 2,
                name: "fury".into(),
                class_id: 1,
            },
            stats: Vec::new(),
        });
        let world = MockItemWorld::new();
        world.set_player_ctx(7, warrior_ctx(7, 60, 0, 0));
        world.set_player_talent_tab(
            7,
            TalentTabInfo {
                best_tab: 0, // arms
                meditation_rank: 0,
            },
        );
        assert_eq!(mgr.player_spec_id(&world, 7), 1);
    }
}
