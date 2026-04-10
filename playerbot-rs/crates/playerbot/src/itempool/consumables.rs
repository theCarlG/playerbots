//! Ammo / potion / food / trade caches — Rust port of
//! `RandomItemMgr::BuildAmmoCache`, `BuildPotionCache`, `BuildFoodCache`,
//! `BuildTradeCache`, and their `Get*` / `GetRandom*` accessors
//! (RandomItemMgr.cpp 3485-3801).
//!
//! The legacy C++ caches are keyed by `level / 10` buckets. A player of
//! level `L` maps to bucket `(L - 1) / 10`:
//!
//! * L=1..10  → bucket 0
//! * L=11..20 → bucket 1
//! * …
//!
//! On the build side, the loop iterates `level = 1, 11, 21, …` and
//! stores at `level / 10`, which produces the same 0, 1, 2, … indices.
//! Items qualify for a bucket when their `required_level` is either
//! zero or satisfies `level - 10 <= required <= level`, matching the
//! C++ skip condition inverted.
//!
//! [`ConsumablesCache::build`] rebuilds every bucket from a slice of
//! [`BotItemPrototype`]. It takes a `SpellLookup` closure so tests can
//! install their own spell fixtures without pulling in the full
//! [`cmangos::ItemWorld`] trait; production code forwards
//! `|id| world.lookup_spell_entry(id)`.

use std::collections::HashMap;

use cmangos::{BotItemPrototype, BotSpellEntryInfo};

// ── Game-data constants ────────────────────────────────────────────────────

/// `ItemClass` values consulted by the cache builders.
pub mod item_class {
    pub const CONSUMABLE: u32 = 0;
    pub const WEAPON: u32 = 2;
    pub const PROJECTILE: u32 = 6;
    pub const TRADE_GOODS: u32 = 7;
}

/// `ItemSubclass*` values consulted by the cache builders.
pub mod item_sub {
    // Projectile subclasses.
    pub const ARROW: u32 = 2;
    pub const BULLET: u32 = 3;

    // Weapon subclasses (only thrown is relevant to the ammo cache).
    pub const WEAPON_THROWN: u32 = 7;

    // Consumable subclasses.
    pub const CONSUMABLE: u32 = 0;
    pub const POTION: u32 = 1;
    pub const FOOD: u32 = 5;
    pub const FLASK: u32 = 7;
}

/// `SpellEffects` values consulted by the potion cache builder.
pub mod spell_effect {
    pub const HEAL: u32 = 10;
    pub const ENERGIZE: u32 = 30;
}

/// `ItemBondingType::NO_BIND`.
pub const NO_BIND: u32 = 0;

/// High bit of `ItemPrototype::Duration` — legacy percentage-duration
/// marker. Items with this bit set are skipped by the build loops.
pub const DURATION_PERCENT_BIT: u32 = 0x8000_0000;

/// Food category used for solid food (`SpellCategory = 11`).
pub const FOOD_CATEGORY: u32 = 11;
/// Food category used for drinks (`SpellCategory = 59`).
pub const DRINK_CATEGORY: u32 = 59;

// ── Bucket helpers ─────────────────────────────────────────────────────────

/// Convert a player level into the cache bucket index used by the
/// lookup functions. Mirrors the C++ `(level - 1) / 10` formula,
/// clamped at zero so `level == 0` does not underflow.
#[must_use]
pub const fn lookup_bucket(level: u32) -> u32 {
    if level == 0 {
        0
    } else {
        (level - 1) / 10
    }
}

/// Upper bound (inclusive) of the level range a bucket covers.
/// Bucket `k` holds items whose `required_level` ∈ `[10·k, 10·k + 10]`.
const fn build_level(bucket: u32) -> u32 {
    bucket * 10 + 1
}

/// Does `proto`'s `required_level` satisfy the legacy build-loop range
/// for a bucket whose representative level is `level`?
///
/// The C++ code uses:
/// `if (proto->RequiredLevel && (proto->RequiredLevel > level || proto->RequiredLevel < level - 10)) continue;`
///
/// on plain `uint32_t`, so `level - 10` wraps around for `level < 10`
/// and the `<` comparison then trips for any finite required level,
/// which effectively restricts bucket 0 (`level = 1`) to items with
/// `required_level == 0`. We preserve that semantic with
/// [`u32::wrapping_sub`] — it is surprising but matches the legacy
/// behavior exactly, and `required_level == 0` short-circuits through
/// the outer `&&` just like in C++.
const fn required_level_fits(required_level: u32, level: u32) -> bool {
    if required_level == 0 {
        return true;
    }
    if required_level > level {
        return false;
    }
    let lo = level.wrapping_sub(10);
    required_level >= lo
}

// ── Closure aliases ────────────────────────────────────────────────────────

/// Closure-typed alias used by [`ConsumablesCache::build`] so callers
/// can plug in either the `ItemWorld` vtable or a test fixture.
pub type SpellLookup<'a> = &'a dyn Fn(i32) -> Option<BotSpellEntryInfo>;

/// Closure-typed alias used by every `get_random_*` function.
pub type UrandFn<'a> = &'a mut dyn FnMut(u32, u32) -> u32;

// ── Cache ──────────────────────────────────────────────────────────────────

/// Container holding every consumables sub-cache the `ItemPoolManager`
/// needs. Keyed by bucket index so the maps stay tiny — even the widest
/// bucket set (wotlk's 9 buckets for level 1..80) is only 9 entries.
#[derive(Default, Debug, Clone)]
pub struct ConsumablesCache {
    /// `ammo[bucket][sub_class]` → chosen item id. `sub_class` is one
    /// of [`item_sub::ARROW`], [`item_sub::BULLET`], or
    /// [`item_sub::WEAPON_THROWN`]. Missing entries map to zero.
    pub ammo: HashMap<u32, HashMap<u32, u32>>,
    /// `potion[bucket][effect]` → pool of item ids. `effect` is
    /// [`spell_effect::HEAL`] or [`spell_effect::ENERGIZE`].
    pub potion: HashMap<u32, HashMap<u32, Vec<u32>>>,
    /// `food[bucket][category]` → pool of item ids. `category` is
    /// [`FOOD_CATEGORY`] (solid food) or [`DRINK_CATEGORY`].
    pub food: HashMap<u32, HashMap<u32, Vec<u32>>>,
    /// `trade[bucket]` → pool of item ids.
    pub trade: HashMap<u32, Vec<u32>>,
}

impl ConsumablesCache {
    /// Rebuild every sub-cache from a prototype slice.
    ///
    /// `max_level` is the server's `randomBotMaxLevel` (capped to
    /// `MaxPlayerLevel`). `spell_lookup` resolves spell ids during the
    /// potion builder; it can return `None` for unknown ids.
    ///
    /// The C++ equivalents run four independent build functions that
    /// each walk `sItemStorage`. We fuse them into a single pass for a
    /// speed win and to keep the iteration order deterministic.
    pub fn build(
        &mut self,
        protos: &[BotItemPrototype],
        max_level: u32,
        spell_lookup: SpellLookup<'_>,
    ) {
        self.clear();

        let max_bucket = max_level / 10;
        for bucket in 0..=max_bucket {
            let level = build_level(bucket);

            // Rebuild all sub-caches for this bucket in a single
            // prototype scan. `build_*` returns on the first qualifying
            // item for ammo (highest required-level wins) and
            // accumulates into vectors for potions/food/trade.
            let mut best_ammo: HashMap<u32, (u32, u32)> = HashMap::new();
            let mut bucket_potions: HashMap<u32, Vec<u32>> = HashMap::new();
            let mut bucket_food: HashMap<u32, Vec<u32>> = HashMap::new();
            let mut bucket_trade: Vec<u32> = Vec::new();

            for proto in protos {
                Self::visit_ammo(proto, level, &mut best_ammo);
                Self::visit_potion(proto, level, spell_lookup, &mut bucket_potions);
                Self::visit_food(proto, level, &mut bucket_food);
                Self::visit_trade(proto, level, &mut bucket_trade);
            }

            // Collapse the ammo tracker into the flat HashMap shape the
            // legacy C++ exposes (one chosen item id per subclass).
            if !best_ammo.is_empty() {
                let mut flat = HashMap::new();
                for (sub_class, (_required_level, item_id)) in best_ammo {
                    flat.insert(sub_class, item_id);
                }
                self.ammo.insert(bucket, flat);
            }

            if !bucket_potions.is_empty() {
                self.potion.insert(bucket, bucket_potions);
            }
            if !bucket_food.is_empty() {
                self.food.insert(bucket, bucket_food);
            }
            if !bucket_trade.is_empty() {
                self.trade.insert(bucket, bucket_trade);
            }
        }
    }

    /// Drop every cached entry. Used before a rebuild and by the tests.
    pub fn clear(&mut self) {
        self.ammo.clear();
        self.potion.clear();
        self.food.clear();
        self.trade.clear();
    }

    /// Returns the pre-selected ammo item id for `(level, sub_class)`,
    /// matching `RandomItemMgr::GetAmmo`.
    #[must_use]
    pub fn get_ammo(&self, level: u32, sub_class: u32) -> u32 {
        self.ammo
            .get(&lookup_bucket(level))
            .and_then(|row| row.get(&sub_class).copied())
            .unwrap_or(0)
    }

    /// Random potion pick from the cached pool, matching
    /// `RandomItemMgr::GetRandomPotion`. Returns `0` when the pool is
    /// empty.
    pub fn get_random_potion(&self, level: u32, effect: u32, urand: UrandFn<'_>) -> u32 {
        let pool = self
            .potion
            .get(&lookup_bucket(level))
            .and_then(|row| row.get(&effect));
        random_from_pool(pool, urand)
    }

    /// Random food pick from the cached pool, matching
    /// `RandomItemMgr::GetRandomFood`.
    pub fn get_random_food(&self, level: u32, category: u32, urand: UrandFn<'_>) -> u32 {
        let pool = self
            .food
            .get(&lookup_bucket(level))
            .and_then(|row| row.get(&category));
        random_from_pool(pool, urand)
    }

    /// Random trade-goods pick from the cached pool, matching
    /// `RandomItemMgr::GetRandomTrade`.
    pub fn get_random_trade(&self, level: u32, urand: UrandFn<'_>) -> u32 {
        let pool = self.trade.get(&lookup_bucket(level));
        random_from_pool(pool, urand)
    }

    // ── Per-proto visitors ────────────────────────────────────────────────

    fn visit_ammo(
        proto: &BotItemPrototype,
        level: u32,
        best: &mut HashMap<u32, (u32, u32)>,
    ) {
        // Ammo cache accepts: projectile arrows/bullets (via the
        // `item_template` SQL query) plus thrown weapons. It picks the
        // HIGHEST `required_level` that is still ≤ `level` and keeps
        // quality = normal (1).
        let sub_class = match (proto.class_id, proto.sub_class) {
            (item_class::PROJECTILE, item_sub::ARROW) => item_sub::ARROW,
            (item_class::PROJECTILE, item_sub::BULLET) => item_sub::BULLET,
            (item_class::WEAPON, item_sub::WEAPON_THROWN) => item_sub::WEAPON_THROWN,
            _ => return,
        };

        if proto.quality != 1 {
            return;
        }
        if proto.required_level > level {
            return;
        }

        // Keep the item with the highest `required_level` ≤ `level`.
        // Ties fall back to the highest item id so the result is
        // deterministic across rebuilds (the C++ `ORDER BY
        // RequiredLevel desc` picks the first SQL row which is driver-
        // defined on ties; we tie-break explicitly).
        let entry = best
            .entry(sub_class)
            .or_insert((proto.required_level, proto.item_id));
        if proto.required_level > entry.0
            || (proto.required_level == entry.0 && proto.item_id > entry.1)
        {
            *entry = (proto.required_level, proto.item_id);
        }
    }

    fn visit_potion(
        proto: &BotItemPrototype,
        level: u32,
        spell_lookup: SpellLookup<'_>,
        bucket_potions: &mut HashMap<u32, Vec<u32>>,
    ) {
        if !Self::is_consumable_candidate(proto, level) {
            return;
        }
        if proto.sub_class != item_sub::POTION && proto.sub_class != item_sub::FLASK {
            return;
        }

        // Walk the item's on-use / on-equip spells; whichever effect
        // the spell carries is the effect bucket we append to.
        for spell_ref in &proto.spells {
            if spell_ref.spell_id == 0 {
                continue;
            }
            let Some(spell) = spell_lookup(spell_ref.spell_id) else {
                continue;
            };
            for effect in spell.effect {
                if effect == spell_effect::HEAL || effect == spell_effect::ENERGIZE {
                    bucket_potions
                        .entry(effect)
                        .or_default()
                        .push(proto.item_id);
                    break;
                }
            }
        }
    }

    fn visit_food(
        proto: &BotItemPrototype,
        level: u32,
        bucket_food: &mut HashMap<u32, Vec<u32>>,
    ) {
        if !Self::is_consumable_candidate(proto, level) {
            return;
        }
        if proto.sub_class != item_sub::FOOD && proto.sub_class != item_sub::CONSUMABLE {
            return;
        }

        // Food discrimination uses `spells[0].spell_category`:
        // category 11 = solid food, 59 = drink. Anything else is a
        // non-food consumable (e.g. bandage) and gets skipped.
        let Some(first) = proto.spells.first() else {
            return;
        };
        let category = first.spell_category;
        if category != FOOD_CATEGORY && category != DRINK_CATEGORY {
            return;
        }

        bucket_food
            .entry(category)
            .or_default()
            .push(proto.item_id);
    }

    fn visit_trade(
        proto: &BotItemPrototype,
        level: u32,
        bucket_trade: &mut Vec<u32>,
    ) {
        if proto.class_id != item_class::TRADE_GOODS {
            return;
        }
        if proto.bonding != NO_BIND {
            return;
        }
        if proto.item_level < level {
            return;
        }
        if !required_level_fits(proto.required_level, level) {
            return;
        }
        if proto.required_skill != 0 {
            return;
        }

        bucket_trade.push(proto.item_id);
    }

    /// Shared predicate used by the potion and food visitors. Mirrors
    /// the C++ skip-list at the top of `BuildPotionCache`:
    ///
    /// * must be a consumable
    /// * must not bind
    /// * required-level must fit the bucket range
    /// * no required-skill gate
    /// * no area / map / city-rank / honor-rank gate
    /// * duration must not carry the "percentage" high bit
    fn is_consumable_candidate(proto: &BotItemPrototype, level: u32) -> bool {
        if proto.class_id != item_class::CONSUMABLE {
            return false;
        }
        if proto.bonding != NO_BIND {
            return false;
        }
        if !required_level_fits(proto.required_level, level) {
            return false;
        }
        if proto.required_skill != 0 {
            return false;
        }
        if proto.area != 0
            || proto.map != 0
            || proto.required_city_rank != 0
            || proto.required_honor_rank != 0
        {
            return false;
        }
        if (proto.duration & DURATION_PERCENT_BIT) != 0 {
            return false;
        }
        true
    }
}

// ── Hardcoded level → food/drink tables ────────────────────────────────────

/// Port of `RandomItemMgr::GetFood(level, category)` — the hand-curated
/// tiered food table used when bots need a guaranteed pick rather than
/// a random roll from the DB-derived cache. `urand` stands in for
/// `urand(0, items.len() - 1)`.
#[must_use]
pub fn hardcoded_food(level: u32, category: u32, urand: UrandFn<'_>) -> u32 {
    let items: &[u32] = match category {
        FOOD_CATEGORY => hardcoded_food_table(level),
        DRINK_CATEGORY => hardcoded_drink_table(level),
        _ => &[],
    };
    if items.is_empty() {
        return 0;
    }
    let idx = urand(0, (items.len() - 1) as u32) as usize;
    items[idx.min(items.len() - 1)]
}

fn hardcoded_food_table(level: u32) -> &'static [u32] {
    // Shared across all three expansions up to the top of the classic
    // bracket. The trailing cfg blocks add the higher-level tiers.
    if level < 5 {
        return &[787, 117, 4540, 2680];
    }
    if level < 15 {
        return &[2287, 4592, 4541, 21072];
    }
    if level < 25 {
        return &[3770, 16170, 4542, 20074];
    }
    if level < 35 {
        return &[4594, 3771, 1707, 4457];
    }
    if level < 45 {
        return &[4599, 4601, 21552, 17222];
    }
    #[cfg(feature = "vanilla")]
    {
        // Classic tops out here — every level ≥ 45 shares the same list.
        &[8950, 8952, 8957, 21023]
    }
    #[cfg(feature = "tbc")]
    {
        if level < 55 {
            return &[8950, 8952, 8957, 21023];
        }
        if level < 65 {
            return &[29292, 27859, 30458, 27662];
        }
        &[29450, 29451, 29452]
    }
    #[cfg(feature = "wotlk")]
    {
        if level < 55 {
            return &[8950, 8952, 8957, 21023];
        }
        if level < 65 {
            return &[29292, 27859, 30458, 27662];
        }
        if level < 75 {
            return &[29450, 29451, 29452];
        }
        &[35947]
    }
}

fn hardcoded_drink_table(level: u32) -> &'static [u32] {
    if level < 5 {
        return &[159, 117];
    }
    if level < 15 {
        return &[1179, 21072];
    }
    if level < 25 {
        return &[1205];
    }
    if level < 35 {
        return &[1708];
    }
    if level < 45 {
        return &[1645];
    }
    #[cfg(feature = "vanilla")]
    {
        &[8766]
    }
    #[cfg(feature = "tbc")]
    {
        if level < 55 {
            return &[8766];
        }
        if level < 65 {
            return &[28399];
        }
        &[27860]
    }
    #[cfg(feature = "wotlk")]
    {
        if level < 55 {
            return &[8766];
        }
        if level < 65 {
            return &[28399];
        }
        if level < 75 {
            return &[27860];
        }
        &[33445]
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn random_from_pool(pool: Option<&Vec<u32>>, urand: UrandFn<'_>) -> u32 {
    let Some(pool) = pool else { return 0 };
    if pool.is_empty() {
        return 0;
    }
    let max = (pool.len() - 1) as u32;
    let idx = urand(0, max) as usize;
    pool[idx.min(pool.len() - 1)]
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_proto(item_id: u32) -> BotItemPrototype {
        let mut p = BotItemPrototype::default();
        p.item_id = item_id;
        p.quality = 1;
        p
    }

    #[test]
    fn bucket_math_matches_legacy_formula() {
        assert_eq!(lookup_bucket(1), 0);
        assert_eq!(lookup_bucket(10), 0);
        assert_eq!(lookup_bucket(11), 1);
        assert_eq!(lookup_bucket(20), 1);
        assert_eq!(lookup_bucket(21), 2);
        assert_eq!(lookup_bucket(60), 5);
    }

    #[test]
    fn required_level_range_accepts_zero_and_bucket_range() {
        // level=11 covers required_level in [1, 11].
        assert!(required_level_fits(0, 11));
        assert!(required_level_fits(1, 11));
        assert!(required_level_fits(11, 11));
        assert!(!required_level_fits(12, 11));
        // level=1 only accepts required_level == 0: the C++ `level - 10`
        // underflows on uint32 and the `<` comparison then rejects any
        // finite value.
        assert!(required_level_fits(0, 1));
        assert!(!required_level_fits(1, 1));
        assert!(!required_level_fits(2, 1));
    }

    #[test]
    fn visit_ammo_picks_highest_required_level() {
        let mut p1 = make_proto(100);
        p1.class_id = item_class::PROJECTILE;
        p1.sub_class = item_sub::ARROW;
        p1.required_level = 5;

        let mut p2 = make_proto(200);
        p2.class_id = item_class::PROJECTILE;
        p2.sub_class = item_sub::ARROW;
        p2.required_level = 10;

        let mut p3 = make_proto(300);
        p3.class_id = item_class::PROJECTILE;
        p3.sub_class = item_sub::ARROW;
        p3.required_level = 20;

        // max_level = 20 so the builder walks level = 1, 11, 21 and
        // populates buckets 0, 1, 2. Bucket 1 (built with level=11)
        // accepts p1 (req=5) and p2 (req=10); p2 wins because it has
        // the higher required_level. Bucket 2 accepts p3 (req=20).
        let mut cache = ConsumablesCache::default();
        cache.build(&[p1, p2, p3], 20, &|_| None);

        assert_eq!(cache.get_ammo(11, item_sub::ARROW), 200);
        assert_eq!(cache.get_ammo(20, item_sub::ARROW), 200);
        assert_eq!(cache.get_ammo(21, item_sub::ARROW), 300);
    }

    #[test]
    fn visit_ammo_ignores_non_normal_quality() {
        let mut p = make_proto(42);
        p.class_id = item_class::PROJECTILE;
        p.sub_class = item_sub::BULLET;
        p.required_level = 1;
        p.quality = 2; // not normal

        let mut cache = ConsumablesCache::default();
        cache.build(&[p], 10, &|_| None);
        assert_eq!(cache.get_ammo(1, item_sub::BULLET), 0);
    }

    #[test]
    fn visit_ammo_captures_thrown_weapons() {
        let mut p = make_proto(999);
        p.class_id = item_class::WEAPON;
        p.sub_class = item_sub::WEAPON_THROWN;
        p.required_level = 1;

        // Build with max_level=20 so bucket 1 is built with level=11
        // and accepts required_level=1. `get_ammo(11)` → lookup_bucket = 1.
        let mut cache = ConsumablesCache::default();
        cache.build(&[p], 20, &|_| None);
        assert_eq!(cache.get_ammo(11, item_sub::WEAPON_THROWN), 999);
    }

    #[test]
    fn visit_potion_uses_spell_effect() {
        let mut p = make_proto(7);
        p.class_id = item_class::CONSUMABLE;
        p.sub_class = item_sub::POTION;
        p.bonding = NO_BIND;
        // required_level=0 keeps the item eligible for every bucket,
        // including bucket 0 which the wrapping-sub semantics restrict
        // to `required_level == 0` only.
        p.required_level = 0;
        p.spells[0].spell_id = 42;

        let spell_lookup = |spell_id: i32| {
            if spell_id == 42 {
                let mut s = BotSpellEntryInfo::default();
                s.effect[0] = spell_effect::HEAL as _;
                Some(s)
            } else {
                None
            }
        };

        let mut cache = ConsumablesCache::default();
        cache.build(&[p], 10, &spell_lookup);

        let mut urand = |_min: u32, _max: u32| 0u32;
        let result = cache.get_random_potion(10, spell_effect::HEAL, &mut urand);
        assert_eq!(result, 7);
    }

    #[test]
    fn visit_potion_skips_bound_items() {
        let mut p = make_proto(8);
        p.class_id = item_class::CONSUMABLE;
        p.sub_class = item_sub::POTION;
        p.bonding = 1; // bind on pickup
        p.spells[0].spell_id = 42;

        let lookup = |_: i32| {
            let mut s = BotSpellEntryInfo::default();
            s.effect[0] = spell_effect::HEAL as _;
            Some(s)
        };

        let mut cache = ConsumablesCache::default();
        cache.build(&[p], 10, &lookup);
        assert!(cache.potion.is_empty());
    }

    #[test]
    fn visit_food_routes_by_spell_category() {
        let mut food = make_proto(100);
        food.class_id = item_class::CONSUMABLE;
        food.sub_class = item_sub::FOOD;
        food.bonding = NO_BIND;
        food.spells[0].spell_category = FOOD_CATEGORY;

        let mut drink = make_proto(200);
        drink.class_id = item_class::CONSUMABLE;
        drink.sub_class = item_sub::FOOD;
        drink.bonding = NO_BIND;
        drink.spells[0].spell_category = DRINK_CATEGORY;

        // Non-food consumable with a stray category should be ignored.
        let mut bandage = make_proto(300);
        bandage.class_id = item_class::CONSUMABLE;
        bandage.sub_class = item_sub::CONSUMABLE;
        bandage.bonding = NO_BIND;
        bandage.spells[0].spell_category = 150;

        let mut cache = ConsumablesCache::default();
        cache.build(&[food, drink, bandage], 10, &|_| None);

        let mut urand = |_min: u32, _max: u32| 0u32;
        assert_eq!(cache.get_random_food(10, FOOD_CATEGORY, &mut urand), 100);
        assert_eq!(cache.get_random_food(10, DRINK_CATEGORY, &mut urand), 200);
    }

    #[test]
    fn visit_trade_filters_by_item_level_and_skill() {
        // All three items target bucket 0 (built with level=1). That
        // bucket's legacy `required_level_fits` check only accepts
        // items with `required_level == 0`, so we keep `ok` at 0.
        let mut ok = make_proto(1);
        ok.class_id = item_class::TRADE_GOODS;
        ok.bonding = NO_BIND;
        ok.item_level = 1;
        ok.required_level = 0;

        let mut skill_gated = make_proto(2);
        skill_gated.class_id = item_class::TRADE_GOODS;
        skill_gated.bonding = NO_BIND;
        skill_gated.item_level = 1;
        skill_gated.required_level = 0;
        skill_gated.required_skill = 50;

        let mut too_low_level = make_proto(3);
        too_low_level.class_id = item_class::TRADE_GOODS;
        too_low_level.bonding = NO_BIND;
        too_low_level.item_level = 0; // < bucket level
        too_low_level.required_level = 0;

        let mut cache = ConsumablesCache::default();
        cache.build(&[ok, skill_gated, too_low_level], 10, &|_| None);

        let mut urand = |_min: u32, _max: u32| 0u32;
        assert_eq!(cache.get_random_trade(1, &mut urand), 1);
        assert_eq!(cache.trade.get(&0).map(|v| v.len()), Some(1));
    }

    #[test]
    fn hardcoded_food_low_level_returns_level_one_list() {
        let mut urand = |_min: u32, _max: u32| 0u32;
        assert_eq!(hardcoded_food(1, FOOD_CATEGORY, &mut urand), 787);
        assert_eq!(hardcoded_food(1, DRINK_CATEGORY, &mut urand), 159);
    }

    #[test]
    fn hardcoded_food_unknown_category_is_zero() {
        let mut urand = |_min: u32, _max: u32| 0u32;
        assert_eq!(hardcoded_food(60, 999, &mut urand), 0);
    }

    #[test]
    fn random_from_empty_pool_is_zero() {
        let mut urand = |_min: u32, _max: u32| 0u32;
        let pool: Vec<u32> = Vec::new();
        assert_eq!(random_from_pool(Some(&pool), &mut urand), 0);
        assert_eq!(random_from_pool(None, &mut urand), 0);
    }

    #[test]
    fn random_from_pool_respects_urand_index() {
        let pool = vec![10u32, 20, 30, 40];
        let mut urand = |_min: u32, max: u32| max; // always last element
        assert_eq!(random_from_pool(Some(&pool), &mut urand), 40);
    }
}
