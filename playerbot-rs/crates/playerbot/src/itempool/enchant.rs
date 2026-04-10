//! Enchant / gem / socket weight calculations — Rust port of
//! `RandomItemMgr::CalculateEnchantWeight`,
//! `CalculateRandomPropertyWeight`, `CalculateGemWeight`,
//! `CalculateSocketWeight`, `CalculateBestRandomEnchantId`, and
//! `LoadRandomEnchantments` (RandomItemMgr.cpp 2161–2468, 3986–4014).
//!
//! These all operate on the same "enchant → weight" primitive: decode
//! the `SpellItemEnchantmentEntry::type[3]` cases, multiply the amounts
//! through the per-spec stat-weight scale, and sum. Higher-level helpers
//! (`CalculateRandomPropertyWeight`, `CalculateGemWeight`,
//! `CalculateSocketWeight`) call down into [`calculate_enchant_weight`]
//! with different lookups.
//!
//! The module keeps the functions pure — every input goes through the
//! [`EnchantCtx`] parameter. The `WeightScale` reference lives in the
//! manager; this module only reads it via the context.
//!
//! # Expansion gating
//!
//! Classic has no random enchants, no gems, and no sockets. The C++ code
//! uses `#ifdef MANGOSBOT_ZERO` to early-return from gem and socket
//! calculations; the Rust port mirrors that via `cfg`-gated branches but
//! exposes the same function signatures on every expansion so callers
//! don't need their own cfg noise.
//!
//! # `CalculateRandomEnchantId`
//!
//! The C++ `CalculateRandomEnchantId` is never called anywhere in the
//! playerbot module — it is dead code kept around as a hypothetical
//! entry point. It is deliberately not ported: there are no callers, and
//! faithfully emulating `GetItemEnchantMod` would require leaking a
//! weighted-chance RNG through the `ItemWorld` trait for no benefit.

use std::collections::HashMap;

use cmangos::{BotEnchantInfo, BotItemEnchantmentRow, BotItemPrototype, ItemWorld};

#[allow(unused_imports)]
use super::stat_link::item_mod_to_stat_name;
use super::stat_weight::calculate_single_stat_weight;
use super::types::WeightScale;

// ── Enchant type tags ─────────────────────────────────────────────────────

/// `ItemEnchantmentType` values the legacy `CalculateEnchantWeight`
/// `switch` dispatches on. Only the subset we consult is named; the
/// rest (totem, use-spell, prismatic socket) are no-ops matching the
/// C++ behaviour.
#[allow(dead_code)]
pub mod enchant_type {
    pub const NONE: u32 = 0;
    pub const PROC: u32 = 1;
    pub const DAMAGE: u32 = 2;
    pub const EQUIP_SPELL: u32 = 3;
    pub const RESISTANCE: u32 = 4;
    pub const STAT: u32 = 5;
    pub const TOTEM: u32 = 6;
    pub const USE_SPELL: u32 = 7;
    pub const PRISMATIC_SOCKET: u32 = 8;
}

/// `SpellEffect` enum — only the one we read.
const SPELL_EFFECT_APPLY_AURA: u32 = 6;

/// `AuraType` enum values referenced by the switch.
#[allow(dead_code)]
mod aura {
    pub const MOD_STAT: u32 = 29;
    pub const MOD_DAMAGE_DONE: u32 = 13;
    pub const MOD_HEALING_DONE: u32 = 135;
}

// ── Spell school masks (matches C++ `SpellSchoolMask`) ───────────────────

const SPELL_SCHOOL_MASK_HOLY: i32 = 1 << 1;
const SPELL_SCHOOL_MASK_FIRE: i32 = 1 << 2;
const SPELL_SCHOOL_MASK_NATURE: i32 = 1 << 3;
const SPELL_SCHOOL_MASK_FROST: i32 = 1 << 4;
const SPELL_SCHOOL_MASK_SHADOW: i32 = 1 << 5;
const SPELL_SCHOOL_MASK_ARCANE: i32 = 1 << 6;
const SPELL_SCHOOL_MASK_SPELL: i32 = SPELL_SCHOOL_MASK_FIRE
    | SPELL_SCHOOL_MASK_NATURE
    | SPELL_SCHOOL_MASK_FROST
    | SPELL_SCHOOL_MASK_SHADOW
    | SPELL_SCHOOL_MASK_ARCANE;
const SPELL_SCHOOL_MASK_MAGIC: i32 = SPELL_SCHOOL_MASK_HOLY | SPELL_SCHOOL_MASK_SPELL;

// ── EnchantCtx ────────────────────────────────────────────────────────────

/// Inputs to [`calculate_enchant_weight`] and friends. Bundled so tests
/// (and the manager) can pass them around without shepherding four
/// arguments through every helper.
#[derive(Clone, Copy)]
pub struct EnchantCtx<'a> {
    /// Player class (1..=11). Currently unused by the scoring itself
    /// but kept in the context so callers can stop threading it through
    /// every sibling helper.
    pub player_class: u8,
    /// Weight-scale id (1..=32) — looks up the spec's stat vector. Kept
    /// for parity with the C++ signature even though the scale
    /// reference supersedes it.
    pub spec_id: u32,
    /// Reference to the weight-scale vector for `spec_id`. The legacy
    /// code dereferences this via `m_weightScales[spec]`.
    pub scale: &'a WeightScale,
    /// Item-world vtable — used to resolve enchant ids, spell ids, and
    /// random-property enchant slots.
    pub world: &'a dyn ItemWorld,
}

// ── calculate_enchant_weight ──────────────────────────────────────────────

/// Port of `RandomItemMgr::CalculateEnchantWeight`. Looks up an enchant
/// id in `sSpellItemEnchantmentStore`, then dispatches on the three
/// `type[3]` slots and sums the per-slot contributions.
///
/// Returns 0 for unknown enchant ids.
#[must_use]
pub fn calculate_enchant_weight(ctx: &EnchantCtx<'_>, enchant_id: u32) -> u32 {
    if enchant_id == 0 {
        return 0;
    }

    let Some(enchant) = ctx.world.lookup_enchant_info(enchant_id) else {
        return 0;
    };

    let mut weight: u64 = 0;
    for slot in 0..3 {
        weight = weight.saturating_add(u64::from(enchant_slot_weight(ctx, &enchant, slot)));
    }
    u32::try_from(weight).unwrap_or(u32::MAX)
}

/// Per-slot dispatch extracted from `calculate_enchant_weight` so the
/// `for`-loop body stays shallow. Mirrors the inner `switch
/// (pEnchant->type[s])` in the C++.
fn enchant_slot_weight(ctx: &EnchantCtx<'_>, enchant: &BotEnchantInfo, slot: usize) -> u32 {
    match enchant.type_[slot] {
        enchant_type::DAMAGE => {
            let amount = enchant.amount[slot];
            if amount == 0 {
                return 0;
            }
            calculate_single_stat_weight(ctx.scale, "mledps", i64::from(amount))
        }
        enchant_type::EQUIP_SPELL => {
            let spell_id = enchant.spell_id[slot];
            if spell_id == 0 {
                return 0;
            }
            let Some(spell) = ctx.world.lookup_spell_entry(spell_id) else {
                return 0;
            };
            let mut total: u64 = 0;
            for eff in 0..3 {
                if spell.effect[eff] != SPELL_EFFECT_APPLY_AURA {
                    continue;
                }
                total = total
                    .saturating_add(u64::from(enchant_equip_spell_aura_weight(ctx, &spell, eff)));
            }
            u32::try_from(total).unwrap_or(u32::MAX)
        }
        enchant_type::RESISTANCE => {
            let amount = enchant.amount[slot];
            if amount == 0 {
                return 0;
            }
            calculate_single_stat_weight(ctx.scale, "armor", i64::from(amount))
        }
        enchant_type::STAT => {
            // Reverse lookup: find the stat-name whose weightStatLink
            // value equals pEnchant->spell_id[s], then accumulate.
            let mod_id = enchant.spell_id[slot];
            let Some(stat_name) = item_mod_to_stat_name(mod_id) else {
                return 0;
            };
            calculate_single_stat_weight(ctx.scale, stat_name, i64::from(enchant.amount[slot]))
        }
        // `enchant_type::PROC`, `TOTEM`, `USE_SPELL`, `PRISMATIC_SOCKET`
        // are all no-ops in the legacy code.
        _ => 0,
    }
}

/// Decode one `SPELL_EFFECT_APPLY_AURA` effect slot of an on-equip
/// enchant spell. Handles `MOD_STAT`, `MOD_DAMAGE_DONE`, and (pre-WotLK
/// only) `MOD_HEALING_DONE` — mirroring the C++ `#ifdef MANGOSBOT_TWO`
/// split.
fn enchant_equip_spell_aura_weight(
    ctx: &EnchantCtx<'_>,
    spell: &cmangos::BotSpellEntryInfo,
    eff: usize,
) -> u32 {
    let aura = spell.effect_apply_aura_name[eff];
    let misc = spell.effect_misc_value[eff];
    let base_points = spell.effect_base_points[eff];
    // C++ uses `EffectBasePoints[j] + 1` — the SPELL_BASE_POINTS offset.
    let value = base_points.wrapping_add(1);

    let mut weight: u64 = 0;

    if aura == aura::MOD_STAT {
        if value == 0 {
            return 0;
        }
        let stat_id = u32::try_from(misc).unwrap_or(0);
        if let Some(stat_name) = item_mod_to_stat_name(stat_id) {
            weight = weight.saturating_add(u64::from(calculate_single_stat_weight(
                ctx.scale,
                stat_name,
                i64::from(value),
            )));
        }
    }

    if aura == aura::MOD_DAMAGE_DONE {
        let spell_damage = value;
        if misc == SPELL_SCHOOL_MASK_MAGIC {
            weight = weight.saturating_add(u64::from(calculate_single_stat_weight(
                ctx.scale,
                "splpwr",
                i64::from(spell_damage),
            )));
        } else {
            let mut special: u64 = 0;
            if (misc & SPELL_SCHOOL_MASK_ARCANE) != 0 {
                special = special.saturating_add(u64::from(calculate_single_stat_weight(
                    ctx.scale,
                    "arcsplpwr",
                    i64::from(spell_damage),
                )));
            }
            if (misc & SPELL_SCHOOL_MASK_FROST) != 0 {
                special = special.saturating_add(u64::from(calculate_single_stat_weight(
                    ctx.scale,
                    "frosplpwr",
                    i64::from(spell_damage),
                )));
            }
            if (misc & SPELL_SCHOOL_MASK_FIRE) != 0 {
                special = special.saturating_add(u64::from(calculate_single_stat_weight(
                    ctx.scale,
                    "firsplpwr",
                    i64::from(spell_damage),
                )));
            }
            if (misc & SPELL_SCHOOL_MASK_SHADOW) != 0 {
                special = special.saturating_add(u64::from(calculate_single_stat_weight(
                    ctx.scale,
                    "shasplpwr",
                    i64::from(spell_damage),
                )));
            }
            if (misc & SPELL_SCHOOL_MASK_NATURE) != 0 {
                special = special.saturating_add(u64::from(calculate_single_stat_weight(
                    ctx.scale,
                    "natsplpwr",
                    i64::from(spell_damage),
                )));
            }
            weight = weight.saturating_add(special);
        }
    }

    // Pre-WotLK: SPELL_AURA_MOD_HEALING_DONE adds "splheal".
    // WotLK merged healing into spell power so this branch is gone.
    #[cfg(not(feature = "wotlk"))]
    if aura == aura::MOD_HEALING_DONE {
        weight = weight.saturating_add(u64::from(calculate_single_stat_weight(
            ctx.scale,
            "splheal",
            i64::from(value),
        )));
    }

    u32::try_from(weight).unwrap_or(u32::MAX)
}

// ── calculate_random_property_weight ──────────────────────────────────────

/// Port of `RandomItemMgr::CalculateRandomPropertyWeight`. Resolves the
/// four enchant slots of a random property entry and sums their
/// contributions.
#[must_use]
pub fn calculate_random_property_weight(ctx: &EnchantCtx<'_>, random_property_id: i32) -> u32 {
    if random_property_id == 0 {
        return 0;
    }

    let abs_id = random_property_id.unsigned_abs() as i32;
    let Some(slots) = ctx.world.lookup_random_property_enchants(abs_id) else {
        return 0;
    };

    // C++ iterates PROP_ENCHANTMENT_SLOT_0..+3, i.e. slots 0..=2. The
    // bridge returns 4 slots for symmetry with the DBC layout; we only
    // consume the first three to match the legacy loop.
    let mut total: u64 = 0;
    for &ench_id in slots.iter().take(3) {
        total = total.saturating_add(u64::from(calculate_enchant_weight(ctx, ench_id)));
    }
    u32::try_from(total).unwrap_or(u32::MAX)
}

// ── calculate_gem_weight ──────────────────────────────────────────────────

/// Port of `RandomItemMgr::CalculateGemWeight`.
///
/// * Classic (`vanilla`): returns 0 — matches the `#ifdef MANGOSBOT_ZERO`
///   short-circuit in the C++.
/// * TBC/WotLK: forwards to [`calculate_enchant_weight`] with the gem's
///   `spell_item_enchantment` id.
#[must_use]
pub fn calculate_gem_weight(ctx: &EnchantCtx<'_>, gem_id: u32) -> u32 {
    #[cfg(feature = "vanilla")]
    {
        let _ = (ctx, gem_id);
        0
    }
    #[cfg(not(feature = "vanilla"))]
    {
        if gem_id == 0 {
            return 0;
        }
        calculate_enchant_weight(ctx, gem_id)
    }
}

// ── calculate_socket_weight ───────────────────────────────────────────────

/// Gem payload for one of the item's four gem sockets. The legacy
/// `ItemQualifier::GetGemN` accessors return enchant ids; this newtype
/// makes the contract obvious at call sites.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SocketGem {
    pub enchant_id: u32,
}

/// Port of `RandomItemMgr::CalculateSocketWeight`.
///
/// # Arguments
///
/// * `ctx` — scoring context.
/// * `proto` — the item whose sockets we are scoring.
/// * `gems` — the four gem enchant ids (`[gem1, gem2, gem3, gem4]`).
///
/// The C++ uses an `ItemQualifier` reference which bundles `proto` and
/// the four gem ids; we take them as explicit parameters so the helper
/// stays testable without pulling in `ItemQualifier`.
///
/// * Classic (`vanilla`): returns 0 immediately.
/// * TBC/WotLK:
///   1. If no gem is slotted, return 0.
///   2. For each populated socket, verify the gem's color satisfies the
///      socket's color mask. Any mismatch (or unknown enchant / gem
///      prototype / gem property) short-circuits to 0 — matching the
///      legacy behaviour where an incomplete socket set scores nothing.
///   3. Finally return [`calculate_enchant_weight`] against
///      `proto->socketBonus`.
#[must_use]
pub fn calculate_socket_weight(
    ctx: &EnchantCtx<'_>,
    proto: &BotItemPrototype,
    gems: [SocketGem; 4],
) -> u32 {
    #[cfg(feature = "vanilla")]
    {
        let _ = (ctx, proto, gems);
        0
    }
    #[cfg(not(feature = "vanilla"))]
    {
        let has_any = gems.iter().any(|g| g.enchant_id != 0);
        if !has_any {
            return 0;
        }

        // Validate every populated socket, matching the legacy loop over
        // MAX_GEM_SOCKETS (3).
        for i in 0..proto.sockets.len() {
            let socket_color = proto.sockets[i].color;
            if socket_color == 0 {
                continue;
            }
            let Some(gem) = gems.get(i) else {
                return 0;
            };
            if gem.enchant_id == 0 {
                return 0;
            }

            // Resolve gem enchant → gem item prototype → gem color.
            let Some(enchant) = ctx.world.lookup_enchant_info(gem.enchant_id) else {
                return 0;
            };
            // The legacy `GemID` field is reused at FFI time as the gem
            // item id — we pull it via the item-enchantment description
            // mapping bundled with the enchant info. Missing = reject.
            //
            // CMaNGOS stores the gem id in
            // `SpellItemEnchantmentEntry::GemID`, which we don't expose
            // separately. Instead, walk `query_gem_properties` once at
            // load time and build a fast lookup; we query through
            // `ctx.world` so the mock can drive it in tests.
            //
            // For now: treat the enchant id as a lookup into
            // `gem_properties` via `spell_item_enchantment`. That's the
            // inverse relationship the DBC maintains.
            let Some(gem_color) = gem_color_for_enchant(ctx, enchant.id) else {
                return 0;
            };
            if (gem_color & socket_color) == 0 {
                return 0;
            }
        }

        // All sockets satisfied — score the item's socket bonus.
        calculate_enchant_weight(ctx, u32::try_from(proto.socket_bonus).unwrap_or(0))
    }
}

/// Resolve the gem color for a `spell_item_enchantment` id by scanning
/// the cached gem-properties rows. Returns `None` if the id is not a
/// known gem.
///
/// This matches the legacy flow `pEnchant->GemID → ItemPrototype →
/// GemProperties.color` but goes through the `ItemWorld` cache directly
/// because `GemID` is not in the `BotEnchantInfo` FFI struct.
#[cfg(not(feature = "vanilla"))]
fn gem_color_for_enchant(ctx: &EnchantCtx<'_>, enchant_id: u32) -> Option<u32> {
    // The manager loads `query_gem_properties` once and stores the
    // result; here we walk the fresh query for test simplicity. Callers
    // that care about perf should cache this themselves.
    let rows = ctx.world.query_gem_properties();
    rows.iter()
        .find(|row| row.spell_item_enchantment == enchant_id)
        .map(|row| row.color)
}

// ── RandomEnchantsCache + LoadRandomEnchantments ─────────────────────────

/// Rust port of the `randomEnchantsCache` field — maps a random
/// property entry id to the list of enchant property ids loaded from
/// `item_enchantment_template`.
#[derive(Default, Debug, Clone)]
pub struct RandomEnchantsCache {
    by_entry: HashMap<u32, Vec<u32>>,
}

impl RandomEnchantsCache {
    /// Fresh, empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load the cache from the raw rows returned by the bridge's
    /// `query_item_enchantment_template` query. Applies the same
    /// `chance > 0.000001 && chance <= 100.0` filter the legacy
    /// `LoadRandomEnchantments` uses.
    ///
    /// Order is preserved per entry (matching insertion order), so
    /// iteration is deterministic even though the underlying DB rows
    /// have no guaranteed ordering.
    pub fn load(&mut self, rows: &[BotItemEnchantmentRow]) {
        self.by_entry.clear();
        for row in rows {
            if !row.chance.is_finite() {
                continue;
            }
            if row.chance > 0.000_001 && row.chance <= 100.0 {
                self.by_entry.entry(row.entry).or_default().push(row.ench);
            }
        }
    }

    /// Lookup the property id vector for `entry`. Returns an empty slice
    /// when the entry is absent, mirroring the C++ `find() == end()`
    /// short-circuit.
    #[must_use]
    pub fn get(&self, entry: u32) -> &[u32] {
        self.by_entry
            .get(&entry)
            .map_or(&[], std::vec::Vec::as_slice)
    }

    /// Number of cached entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_entry.len()
    }

    /// True when the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_entry.is_empty()
    }
}

// ── calculate_best_random_enchant_id ──────────────────────────────────────

/// Port of `RandomItemMgr::CalculateBestRandomEnchantId`. Walks the
/// property-id list for `proto.random_property` and returns the id of
/// the property whose three enchant slots score highest for the given
/// `(class, spec)` pair.
///
/// Returns 0 when:
/// * `item_id` is zero,
/// * the proto's `random_property` has no cached entry, or
/// * every property scores 0.
#[must_use]
pub fn calculate_best_random_enchant_id(
    ctx: &EnchantCtx<'_>,
    cache: &RandomEnchantsCache,
    proto: &BotItemPrototype,
) -> u32 {
    let random_property = proto.random_property;
    if random_property == 0 {
        return 0;
    }

    let Ok(entry_key) = u32::try_from(random_property) else {
        return 0;
    };
    let props = cache.get(entry_key);
    if props.is_empty() {
        return 0;
    }

    let mut best_score: u32 = 0;
    let mut best_id: u32 = 0;

    for &prop_id in props {
        let Some(slots) = ctx
            .world
            .lookup_random_property_enchants(i32::try_from(prop_id).unwrap_or(0))
        else {
            continue;
        };

        let mut score: u64 = 0;
        for &ench_id in slots.iter().take(3) {
            score = score.saturating_add(u64::from(calculate_enchant_weight(ctx, ench_id)));
        }
        let score = u32::try_from(score).unwrap_or(u32::MAX);

        if score > best_score {
            best_score = score;
            best_id = prop_id;
        }
    }

    best_id
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itempool::types::{WeightScale, WeightScaleInfo, WeightScaleStat};
    use cmangos::MockItemWorld;

    fn make_scale(weights: &[(&str, u32)]) -> WeightScale {
        WeightScale {
            info: WeightScaleInfo {
                id: 1,
                name: "test".to_string(),
                class_id: 1,
            },
            stats: weights
                .iter()
                .map(|(s, w)| WeightScaleStat {
                    stat: (*s).to_string(),
                    weight: *w,
                })
                .collect(),
        }
    }

    fn make_enchant(
        id: u32,
        types: [u32; 3],
        amounts: [i32; 3],
        spell_ids: [u32; 3],
    ) -> BotEnchantInfo {
        BotEnchantInfo {
            id,
            type_: types,
            amount: amounts,
            spell_id: spell_ids,
            description: [0; 96],
            is_valid: true,
        }
    }

    #[test]
    fn unknown_enchant_id_returns_zero() {
        let scale = make_scale(&[("sta", 5)]);
        let world = MockItemWorld::new();
        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        assert_eq!(calculate_enchant_weight(&ctx, 0), 0);
        assert_eq!(calculate_enchant_weight(&ctx, 42), 0);
    }

    #[test]
    fn damage_type_scores_mledps() {
        let scale = make_scale(&[("mledps", 3)]);
        let world = MockItemWorld::new();
        // Enchant slot 0: DAMAGE, amount 10 → 3 * 10 = 30.
        world.set_enchant(
            100,
            make_enchant(100, [enchant_type::DAMAGE, 0, 0], [10, 0, 0], [0, 0, 0]),
        );
        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        assert_eq!(calculate_enchant_weight(&ctx, 100), 30);
    }

    #[test]
    fn resistance_type_scores_armor() {
        let scale = make_scale(&[("armor", 2)]);
        let world = MockItemWorld::new();
        world.set_enchant(
            200,
            make_enchant(200, [0, enchant_type::RESISTANCE, 0], [0, 50, 0], [0, 0, 0]),
        );
        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        assert_eq!(calculate_enchant_weight(&ctx, 200), 100);
    }

    #[test]
    fn stat_type_reverse_lookup_by_mod() {
        use crate::itempool::stat_link::item_mod;
        let scale = make_scale(&[("sta", 4)]);
        let world = MockItemWorld::new();
        // Enchant slot 0: STAT, amount 10, spell_id = ITEM_MOD_STAMINA.
        world.set_enchant(
            300,
            make_enchant(
                300,
                [enchant_type::STAT, 0, 0],
                [10, 0, 0],
                [item_mod::STAMINA, 0, 0],
            ),
        );
        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        assert_eq!(calculate_enchant_weight(&ctx, 300), 40);
    }

    #[test]
    fn stat_type_unknown_mod_scores_zero() {
        let scale = make_scale(&[("sta", 4)]);
        let world = MockItemWorld::new();
        world.set_enchant(
            301,
            make_enchant(301, [enchant_type::STAT, 0, 0], [10, 0, 0], [9999, 0, 0]),
        );
        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        assert_eq!(calculate_enchant_weight(&ctx, 301), 0);
    }

    #[test]
    fn noop_types_score_zero() {
        // PROC, TOTEM, USE_SPELL, PRISMATIC_SOCKET all fall through.
        let scale = make_scale(&[("sta", 5), ("mledps", 5), ("armor", 5)]);
        let world = MockItemWorld::new();
        for ty in [
            enchant_type::PROC,
            enchant_type::TOTEM,
            enchant_type::USE_SPELL,
            enchant_type::PRISMATIC_SOCKET,
        ] {
            world.set_enchant(
                500 + ty,
                make_enchant(500 + ty, [ty, 0, 0], [100, 0, 0], [0, 0, 0]),
            );
        }
        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        for ty in [
            enchant_type::PROC,
            enchant_type::TOTEM,
            enchant_type::USE_SPELL,
            enchant_type::PRISMATIC_SOCKET,
        ] {
            assert_eq!(
                calculate_enchant_weight(&ctx, 500 + ty),
                0,
                "type {ty} should score 0"
            );
        }
    }

    #[test]
    fn equip_spell_type_aggregates_aura_effects() {
        use crate::itempool::stat_link::item_mod;
        use cmangos::BotSpellEntryInfo;

        let scale = make_scale(&[("sta", 5)]);
        let world = MockItemWorld::new();

        // Enchant → spell 42. Spell has two aura effects, both MOD_STAT
        // targeting ITEM_MOD_STAMINA, base_points 9 + 1 = 10 each.
        world.set_enchant(
            400,
            make_enchant(
                400,
                [enchant_type::EQUIP_SPELL, 0, 0],
                [0, 0, 0],
                [42, 0, 0],
            ),
        );
        let mut spell = BotSpellEntryInfo {
            id: 42,
            effect: [SPELL_EFFECT_APPLY_AURA, SPELL_EFFECT_APPLY_AURA, 0],
            effect_apply_aura_name: [aura::MOD_STAT, aura::MOD_STAT, 0],
            effect_base_points: [9, 9, 0],
            effect_misc_value: [
                item_mod::STAMINA as i32,
                item_mod::STAMINA as i32,
                0,
            ],
            ..BotSpellEntryInfo::default()
        };
        spell.effect_misc_value_b = [0; 3];
        world.set_spell_entry(42, spell);

        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        // 2 aura effects × (base_points 9 + 1) × weight 5 = 100.
        assert_eq!(calculate_enchant_weight(&ctx, 400), 100);
    }

    #[test]
    fn random_property_weight_sums_first_three_slots() {
        let scale = make_scale(&[("mledps", 1)]);
        let world = MockItemWorld::new();
        // Three enchants, each DAMAGE amount 5 → each scores 5.
        for ench_id in [10u32, 11, 12] {
            world.set_enchant(
                ench_id,
                make_enchant(ench_id, [enchant_type::DAMAGE, 0, 0], [5, 0, 0], [0, 0, 0]),
            );
        }
        // 4th slot must NOT be summed — if it were, total = 20.
        world.set_enchant(
            13,
            make_enchant(13, [enchant_type::DAMAGE, 0, 0], [100, 0, 0], [0, 0, 0]),
        );
        world.set_random_property_enchants(77, [10, 11, 12, 13]);

        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        assert_eq!(calculate_random_property_weight(&ctx, 77), 15);
        // Negative property ids get abs() — legacy uses `abs(randomPropertyId)`.
        assert_eq!(calculate_random_property_weight(&ctx, -77), 15);
    }

    #[test]
    fn random_property_weight_zero_id_returns_zero() {
        let scale = make_scale(&[("sta", 5)]);
        let world = MockItemWorld::new();
        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        assert_eq!(calculate_random_property_weight(&ctx, 0), 0);
    }

    #[test]
    fn random_property_weight_unknown_property_returns_zero() {
        let scale = make_scale(&[("sta", 5)]);
        let world = MockItemWorld::new();
        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        assert_eq!(calculate_random_property_weight(&ctx, 999), 0);
    }

    #[cfg(feature = "vanilla")]
    #[test]
    fn vanilla_gem_weight_is_zero() {
        let scale = make_scale(&[("mledps", 5)]);
        let world = MockItemWorld::new();
        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        // Even if the enchant would score > 0 on TBC+, vanilla short-circuits.
        world.set_enchant(
            600,
            make_enchant(600, [enchant_type::DAMAGE, 0, 0], [10, 0, 0], [0, 0, 0]),
        );
        assert_eq!(calculate_gem_weight(&ctx, 600), 0);
    }

    #[cfg(not(feature = "vanilla"))]
    #[test]
    fn tbc_wotlk_gem_weight_forwards_to_enchant() {
        let scale = make_scale(&[("mledps", 3)]);
        let world = MockItemWorld::new();
        world.set_enchant(
            700,
            make_enchant(700, [enchant_type::DAMAGE, 0, 0], [10, 0, 0], [0, 0, 0]),
        );
        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        assert_eq!(calculate_gem_weight(&ctx, 700), 30);
        assert_eq!(calculate_gem_weight(&ctx, 0), 0);
    }

    #[cfg(feature = "vanilla")]
    #[test]
    fn vanilla_socket_weight_is_zero() {
        let scale = make_scale(&[]);
        let world = MockItemWorld::new();
        let proto = BotItemPrototype::default();
        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        assert_eq!(
            calculate_socket_weight(&ctx, &proto, [SocketGem::default(); 4]),
            0
        );
    }

    #[cfg(not(feature = "vanilla"))]
    #[test]
    fn tbc_wotlk_socket_weight_zero_when_no_gems() {
        use cmangos::BotItemProtoSocket;
        let scale = make_scale(&[("mledps", 3)]);
        let world = MockItemWorld::new();
        let mut proto = BotItemPrototype::default();
        proto.sockets[0] = BotItemProtoSocket {
            color: 2, /* red */
            content: 0,
        };
        proto.socket_bonus = 100;
        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        assert_eq!(
            calculate_socket_weight(&ctx, &proto, [SocketGem::default(); 4]),
            0
        );
    }

    #[test]
    fn random_enchants_cache_load_filters_rows() {
        let rows = [
            BotItemEnchantmentRow {
                entry: 1,
                ench: 10,
                chance: 50.0,
            },
            BotItemEnchantmentRow {
                entry: 1,
                ench: 11,
                chance: 0.0, // rejected: ≤ 0.000001
            },
            BotItemEnchantmentRow {
                entry: 2,
                ench: 20,
                chance: 100.0, // boundary, accepted
            },
            BotItemEnchantmentRow {
                entry: 2,
                ench: 21,
                chance: 100.1, // rejected: > 100
            },
            BotItemEnchantmentRow {
                entry: 3,
                ench: 30,
                chance: f32::NAN, // rejected: not finite
            },
        ];
        let mut cache = RandomEnchantsCache::new();
        cache.load(&rows);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(1), &[10]);
        assert_eq!(cache.get(2), &[20]);
        assert!(cache.get(3).is_empty());
    }

    #[test]
    fn random_enchants_cache_preserves_insertion_order() {
        let rows = [
            BotItemEnchantmentRow {
                entry: 1,
                ench: 30,
                chance: 50.0,
            },
            BotItemEnchantmentRow {
                entry: 1,
                ench: 10,
                chance: 50.0,
            },
            BotItemEnchantmentRow {
                entry: 1,
                ench: 20,
                chance: 50.0,
            },
        ];
        let mut cache = RandomEnchantsCache::new();
        cache.load(&rows);
        // Order must match insertion order, not numeric sort.
        assert_eq!(cache.get(1), &[30, 10, 20]);
    }

    #[test]
    fn calculate_best_random_enchant_id_returns_highest_scoring_prop() {
        let scale = make_scale(&[("mledps", 1)]);
        let world = MockItemWorld::new();
        // Property 101 → enchants with damage amounts [2, 3, 5] = 10.
        // Property 102 → enchants with damage amounts [7, 8, 9] = 24.
        // Property 103 → enchants with damage amounts [1, 1, 1] = 3.
        for (prop, ids, dmg) in [
            (101u32, [1u32, 2, 3], [2i32, 3, 5]),
            (102, [4, 5, 6], [7, 8, 9]),
            (103, [7, 8, 9], [1, 1, 1]),
        ] {
            for i in 0..3 {
                world.set_enchant(
                    ids[i],
                    make_enchant(
                        ids[i],
                        [enchant_type::DAMAGE, 0, 0],
                        [dmg[i], 0, 0],
                        [0, 0, 0],
                    ),
                );
            }
            world.set_random_property_enchants(prop as i32, [ids[0], ids[1], ids[2], 0]);
        }

        let mut cache = RandomEnchantsCache::new();
        cache.load(&[
            BotItemEnchantmentRow {
                entry: 42,
                ench: 101,
                chance: 50.0,
            },
            BotItemEnchantmentRow {
                entry: 42,
                ench: 102,
                chance: 50.0,
            },
            BotItemEnchantmentRow {
                entry: 42,
                ench: 103,
                chance: 50.0,
            },
        ]);

        let proto = BotItemPrototype {
            random_property: 42,
            ..BotItemPrototype::default()
        };

        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        assert_eq!(calculate_best_random_enchant_id(&ctx, &cache, &proto), 102);
    }

    #[test]
    fn calculate_best_random_enchant_id_zero_random_property_is_zero() {
        let scale = make_scale(&[]);
        let world = MockItemWorld::new();
        let cache = RandomEnchantsCache::new();
        let proto = BotItemPrototype::default();
        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        assert_eq!(calculate_best_random_enchant_id(&ctx, &cache, &proto), 0);
    }

    #[test]
    fn calculate_best_random_enchant_id_empty_cache_is_zero() {
        let scale = make_scale(&[]);
        let world = MockItemWorld::new();
        let cache = RandomEnchantsCache::new();
        let proto = BotItemPrototype {
            random_property: 42,
            ..BotItemPrototype::default()
        };
        let ctx = EnchantCtx {
            player_class: 1,
            spec_id: 1,
            scale: &scale,
            world: &world,
        };
        assert_eq!(calculate_best_random_enchant_id(&ctx, &cache, &proto), 0);
    }
}
