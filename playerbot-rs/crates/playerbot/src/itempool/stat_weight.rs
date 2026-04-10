//! Stat-weight scoring — port of `RandomItemMgr::CalculateStatWeight`
//! (`RandomItemMgr.cpp` lines 1486–2159) and
//! `CalculateSingleStatWeight` (lines 2499–2514).
//!
//! Pure function: every input comes through the [`StatWeightCtx`]
//! parameter. The `WeightScale` table lives in the manager; this module
//! only reads a reference to it (spec-id → scale). Spell lookups go
//! through the `ItemWorld::lookup_spell_entry` callback.
//!
//! The legacy C++ keeps a mutable `ItemSpecType&` out-parameter that
//! classifies the item (caster/attack/tank/…). We return it alongside
//! the weight as [`StatWeightResult`].

use cmangos::{BotItemPrototype, BotSpellEntryInfo, ItemWorld};

use super::equip_filter::{armor_sub, class, item_class, weapon_sub};
#[allow(unused_imports)]
use super::stat_link::{item_mod, item_mod_to_stat_name};
use super::types::{ItemSpecType, WeightScale};

// ── Constants ────────────────────────────────────────────────────────────

// `InventoryType` values referenced by the scoring logic.
const INVTYPE_WEAPON_MAINHAND: u32 = 21;
const INVTYPE_HOLDABLE: u32 = 23;
const INVTYPE_RELIC: u32 = 28;

// `ItemSpelltriggerType` values.
const SPELLTRIGGER_ON_USE: u32 = 0;
const SPELLTRIGGER_ON_EQUIP: u32 = 1;
const SPELLTRIGGER_CHANCE_ON_HIT: u32 = 2;

// `SpellEffect::SPELL_EFFECT_APPLY_AURA`.
const SPELL_EFFECT_APPLY_AURA: u32 = 6;

// `SpellSchoolMask` values.
const SPELL_SCHOOL_MASK_NORMAL: i32 = 1;
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

// `SKILL_DEFENSE`.
const SKILL_DEFENSE: i32 = 95;

// `SocketColor` values. Only used on TBC/WotLK since Classic has no sockets.
#[cfg(any(feature = "tbc", feature = "wotlk"))]
const SOCKET_COLOR_META: u32 = 1;
#[cfg(any(feature = "tbc", feature = "wotlk"))]
const SOCKET_COLOR_RED: u32 = 2;
#[cfg(any(feature = "tbc", feature = "wotlk"))]
const SOCKET_COLOR_YELLOW: u32 = 4;
#[cfg(any(feature = "tbc", feature = "wotlk"))]
const SOCKET_COLOR_BLUE: u32 = 8;

// `AuraType` values used by the scoring code.
#[allow(dead_code)]
mod aura {
    pub const MOD_DAMAGE_DONE: u32 = 13;
    pub const MOD_SKILL: u32 = 30;
    pub const MOD_PARRY_PERCENT: u32 = 47;
    pub const MOD_DODGE_PERCENT: u32 = 49;
    pub const MOD_BLOCK_PERCENT: u32 = 51;
    pub const MOD_CRIT_PERCENT: u32 = 52;
    pub const MOD_HIT_CHANCE: u32 = 54;
    pub const MOD_SPELL_HIT_CHANCE: u32 = 55;
    pub const MOD_SPELL_CRIT_CHANCE: u32 = 57;
    pub const MOD_SPELL_CRIT_CHANCE_SCHOOL: u32 = 71;
    pub const MOD_POWER_REGEN: u32 = 85;
    pub const MOD_ATTACK_POWER: u32 = 99;
    pub const MOD_TARGET_RESISTANCE: u32 = 123;
    pub const MOD_RANGED_ATTACK_POWER: u32 = 124;
    pub const MOD_HEALING_DONE: u32 = 135;
    pub const MOD_SHIELD_BLOCKVALUE: u32 = 158;
    pub const MOD_RATING: u32 = 189;
}

// ── CombatRating → stat-name (TBC+ `SPELL_AURA_MOD_RATING` decoding) ──────

/// Port of the `weightRatingLink` map used to decode
/// `SPELL_AURA_MOD_RATING` effect masks in TBC+. Returns the stat name
/// whose weight should be applied for a given `CR_*` index.
///
/// Matches the expansion-conditional blocks in `RandomItemMgr.cpp`
/// lines 105–114 (vanilla, subset) and 139–151 (TBC+ adds spell variants,
/// drops armor-pen).
#[cfg(any(feature = "tbc", feature = "wotlk"))]
#[must_use]
fn rating_to_stat_name(rating: u32) -> Option<&'static str> {
    // enum CombatRating values from Unit.h, shared across TBC+ WotLK.
    const CR_DEFENSE_SKILL: u32 = 1;
    const CR_DODGE: u32 = 2;
    const CR_PARRY: u32 = 3;
    const CR_BLOCK: u32 = 4;
    const CR_HIT_MELEE: u32 = 5;
    const CR_CRIT_MELEE: u32 = 8;
    const CR_HASTE_MELEE: u32 = 17;
    const CR_EXPERTISE: u32 = 23;

    // Spell-variant ratings only show up in the TBC weightRatingLink map;
    // WotLK's unified spell power replaces them.
    #[cfg(feature = "tbc")]
    const CR_HIT_SPELL: u32 = 7;
    #[cfg(feature = "tbc")]
    const CR_CRIT_SPELL: u32 = 10;
    #[cfg(feature = "tbc")]
    const CR_HASTE_SPELL: u32 = 19;

    // Armor-pen rating is WotLK only.
    #[cfg(feature = "wotlk")]
    const CR_ARMOR_PENETRATION: u32 = 24;

    match rating {
        CR_EXPERTISE => Some("exprtng"),
        CR_CRIT_MELEE => Some("critstrkrtng"),
        CR_HIT_MELEE => Some("hitrtng"),
        CR_HASTE_MELEE => Some("hastertng"),
        CR_DEFENSE_SKILL => Some("defrtng"),
        CR_DODGE => Some("dodgertng"),
        CR_BLOCK => Some("blockrtng"),
        CR_PARRY => Some("parryrtng"),
        #[cfg(feature = "tbc")]
        CR_CRIT_SPELL => Some("spellcritstrkrtng"),
        #[cfg(feature = "tbc")]
        CR_HASTE_SPELL => Some("spellhastertng"),
        #[cfg(feature = "tbc")]
        CR_HIT_SPELL => Some("spellhitrtng"),
        #[cfg(feature = "wotlk")]
        CR_ARMOR_PENETRATION => Some("armorpenrtng"),
        _ => None,
    }
}

/// Set of `CombatRating` indices that classify an item as "tank".
///
/// Covers `CR_DEFENSE_SKILL` (1), `CR_DODGE` (2), `CR_PARRY` (3), and
/// `CR_BLOCK` (4).
#[cfg(any(feature = "tbc", feature = "wotlk"))]
const fn is_tank_rating(rating: u32) -> bool {
    matches!(rating, 1..=4)
}

/// Set of `CombatRating` indices that classify an item as "dps".
#[cfg(any(feature = "tbc", feature = "wotlk"))]
const fn is_dps_rating(rating: u32) -> bool {
    matches!(rating, 8 /*CR_CRIT_MELEE*/ | 9 /*CR_CRIT_RANGED*/)
}

// ── Weapon helpers ────────────────────────────────────────────────────────

#[inline]
fn is_weapon(proto: &BotItemPrototype) -> bool {
    proto.class_id == item_class::WEAPON
}

#[inline]
fn is_ranged_weapon(proto: &BotItemPrototype) -> bool {
    if !is_weapon(proto) {
        return false;
    }
    matches!(
        proto.sub_class,
        weapon_sub::BOW
            | weapon_sub::GUN
            | weapon_sub::CROSSBOW
            | weapon_sub::THROWN
            | weapon_sub::WAND
    )
}

#[inline]
fn is_relic_slot(proto: &BotItemPrototype) -> bool {
    proto.inventory_type == INVTYPE_RELIC
}

// ── Whitelist ─────────────────────────────────────────────────────────────

/// Returns `true` if an item is on the random-gear whitelist. The legacy
/// code reads `sPlayerbotAIConfig.randomGearWhitelist`; we take it as a
/// slice so the manager can pass the config-driven list without this
/// module needing to know about config storage.
#[inline]
fn is_whitelisted(item_id: u32, whitelist: &[u32]) -> bool {
    whitelist.binary_search(&item_id).is_ok() || whitelist.contains(&item_id)
}

// ── CalculateSingleStatWeight ─────────────────────────────────────────────

/// Port of `RandomItemMgr::CalculateSingleStatWeight`. Iterates the
/// weight-scale entry for `spec` looking for `stat_name` and returns
/// `weight * value`. Returns 0 if the scale does not list the stat.
#[must_use]
pub fn calculate_single_stat_weight(scale: &WeightScale, stat_name: &str, value: i64) -> u32 {
    for entry in &scale.stats {
        if entry.stat == stat_name {
            // The legacy code stores weights as `uint32`, so a signed
            // `value` can still produce a negative product when `value`
            // is negative (e.g. negative basic stat). The C++ silently
            // underflows through uint32; we mirror that via wrapping
            // multiplication so the negative-stat branch in
            // `calculate_stat_weight` sees the same bit pattern.
            let product = (entry.weight as i64).wrapping_mul(value);
            return product as u32;
        }
    }
    0
}

// ── StatWeightCtx ─────────────────────────────────────────────────────────

/// Inputs to [`calculate_stat_weight`]. Bundled into a struct so tests
/// can construct them without shepherding a dozen arguments.
#[derive(Clone, Copy)]
pub struct StatWeightCtx<'a> {
    pub player_class: u8,
    /// Weight-scale id (1..=32), not a talent tab index. Used only by
    /// the spec-specific filters (ret/tank/…).
    pub spec_id: u32,
    /// Reference to the weight-scale vector for `spec_id`.
    pub scale: &'a WeightScale,
    /// `sPlayerbotAIConfig.randomGearWhitelist`.
    pub whitelist: &'a [u32],
    /// Item-world vtable used to resolve item spells.
    pub world: &'a dyn ItemWorld,
}

/// Result of [`calculate_stat_weight`] — the total weight plus the item
/// spec classification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatWeightResult {
    pub weight: u32,
    pub item_spec: ItemSpecType,
}

// ── calculate_stat_weight ─────────────────────────────────────────────────

/// Port of `RandomItemMgr::CalculateStatWeight`.
///
/// The legacy function is a monolithic 670-line block that walks the
/// proto's stats, weapon damage, item spells, and socket bonuses, then
/// applies a long list of spec filters. We keep the structure 1:1 so
/// the port stays auditable.
#[must_use]
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn calculate_stat_weight(ctx: &StatWeightCtx<'_>, proto: &BotItemPrototype) -> StatWeightResult {
    let player_class = ctx.player_class;
    let spec = ctx.spec_id;

    // Flags matching the C++ locals exactly so each branch reads 1:1.
    let mut spec_type = ItemSpecType::NONE;
    let mut stat_weight: u64 = 0;
    let mut spell_power: u64 = 0;
    let mut spell_heal: u64 = 0;
    let mut attack_power: u64 = 0;
    let mut is_caster_item = false;
    let mut is_attack_item = false;
    let mut is_dps_item = false;
    let mut is_tank_item = false;
    let mut is_healing_item = false;
    let mut is_spell_damage_item = false;
    let mut has_int = false;

    // ─── Class-level "noCaster" / "hasMana" flags ────────────────────────
    //
    // WotLK adds DK; ret (spec 6), enh shaman (21), ferals (30/32) and
    // arms-warrior subspec (unused here) are also flagged non-caster.
    #[cfg(feature = "wotlk")]
    let no_caster = player_class == class::WARRIOR
        || player_class == class::ROGUE
        || player_class == class::DEATH_KNIGHT
        || player_class == class::HUNTER
        || spec == 30
        || spec == 32
        || spec == 21
        || spec == 6;
    #[cfg(not(feature = "wotlk"))]
    let no_caster = player_class == class::WARRIOR
        || player_class == class::ROGUE
        || player_class == class::HUNTER
        || spec == 30
        || spec == 32
        || spec == 21
        || spec == 6;

    #[cfg(feature = "wotlk")]
    let has_mana = !(player_class == class::WARRIOR
        || player_class == class::ROGUE
        || player_class == class::DEATH_KNIGHT);
    #[cfg(not(feature = "wotlk"))]
    let has_mana = !(player_class == class::WARRIOR || player_class == class::ROGUE);

    // ─── Early-exit: WotLK librams/idols/totems/sigils ───────────────────
    #[cfg(feature = "wotlk")]
    {
        if !is_weapon(proto)
            && (proto.sub_class == armor_sub::LIBRAM
                || proto.sub_class == armor_sub::IDOL
                || proto.sub_class == armor_sub::TOTEM
                || proto.sub_class == armor_sub::SIGIL)
        {
            return StatWeightResult {
                weight: proto.quality + proto.item_level,
                item_spec: ItemSpecType::NONE,
            };
        }
    }

    // ─── Early-exit: classic hunter thrown ───────────────────────────────
    #[cfg(not(feature = "wotlk"))]
    {
        if player_class == class::HUNTER && proto.sub_class == weapon_sub::THROWN {
            return StatWeightResult {
                weight: proto.item_level,
                item_spec: ItemSpecType::NONE,
            };
        }
    }

    // ─── Relic slot filter ───────────────────────────────────────────────
    if is_relic_slot(proto) {
        if player_class == class::PALADIN && proto.sub_class != armor_sub::LIBRAM {
            return StatWeightResult::default();
        }
        if player_class == class::DRUID && proto.sub_class != armor_sub::IDOL {
            return StatWeightResult::default();
        }
        if player_class == class::SHAMAN && proto.sub_class != armor_sub::TOTEM {
            return StatWeightResult::default();
        }
        if matches!(
            player_class,
            class::WARRIOR
                | class::HUNTER
                | class::ROGUE
                | class::PRIEST
                | class::MAGE
                | class::WARLOCK
        ) {
            return StatWeightResult::default();
        }
        return StatWeightResult {
            weight: proto.quality + proto.item_level,
            item_spec: ItemSpecType::NONE,
        };
    }

    // ─── Whitelist handling ──────────────────────────────────────────────
    let mut is_whitelist = is_whitelisted(proto.item_id, ctx.whitelist);

    // Classic: whitelist PVP items (weird stats) and the only feral OH.
    #[cfg(feature = "vanilla")]
    {
        if proto.required_honor_rank != 0 {
            is_whitelist = true;
        }
        if (spec == 30 || spec == 32) && proto.item_id == 13385 {
            is_whitelist = true;
        }
    }

    // Atiesh class-specific variants.
    if (player_class == class::MAGE && proto.item_id == 22589)
        || (player_class == class::WARLOCK && proto.item_id == 22630)
        || (player_class == class::PRIEST && proto.item_id == 22631)
        || (player_class == class::DRUID && proto.item_id == 22632)
    {
        is_whitelist = true;
    }

    // ─── Basic stats ─────────────────────────────────────────────────────
    let mut basic_stats_weight: i64 = 0;
    for j in 0..proto.stats.len() {
        let stat_type = proto.stats[j].stat_type;
        let val = proto.stats[j].stat_value;
        if val == 0 {
            continue;
        }

        let Some(weight_name) = item_mod_to_stat_name(stat_type) else {
            continue;
        };

        // TBC+ tank/dps/caster classification hooks.
        #[cfg(any(feature = "tbc", feature = "wotlk"))]
        {
            if stat_type == item_mod::DODGE_RATING
                || stat_type == item_mod::PARRY_RATING
                || stat_type == item_mod::BLOCK_RATING
                || stat_type == item_mod::DEFENSE_SKILL_RATING
            {
                is_tank_item = true;
            }
            if stat_type == item_mod::CRIT_MELEE_RATING || stat_type == item_mod::CRIT_RATING {
                is_dps_item = true;
            }
        }
        #[cfg(feature = "wotlk")]
        {
            if stat_type == item_mod::SPELL_POWER {
                is_caster_item = true;
                is_healing_item = true;
                is_spell_damage_item = true;
            }
        }

        let single = calculate_single_stat_weight(ctx.scale, weight_name, val as i64);
        basic_stats_weight += single as i64;

        // val != 0 has already been checked above, so these flags apply.
        if weight_name == "int" && !no_caster {
            is_caster_item = true;
        }
        if weight_name == "int" {
            has_int = true;
        }
        if weight_name == "splpwr" {
            is_caster_item = true;
        }
        if weight_name == "str" || weight_name == "agi" || weight_name == "atkpwr" {
            is_attack_item = true;
        }
    }

    // ─── Defensive stats (block, armor) ──────────────────────────────────
    let defense_stats = (calculate_single_stat_weight(ctx.scale, "block", proto.block as i64)
        as u64)
        + (calculate_single_stat_weight(ctx.scale, "armor", proto.armor as i64) as u64);

    // ─── Weapon DPS ──────────────────────────────────────────────────────
    if is_weapon(proto) {
        for dmg in &proto.damages {
            if dmg.damage_max == 0.0 {
                break;
            }
            let delay_sec = proto.delay as f32 / 1000.0;
            if delay_sec <= 0.0 {
                continue;
            }
            let dps = ((dmg.damage_min + dmg.damage_max) / delay_sec / 2.0) as i64;
            if dps != 0 {
                let name = if is_ranged_weapon(proto) { "rgddps" } else { "mledps" };
                stat_weight += calculate_single_stat_weight(ctx.scale, name, dps) as u64;
            }
        }
    }

    // ─── Item spell effects ──────────────────────────────────────────────
    let mut spell_damage_amt: i64 = 0;
    let mut spell_healing_amt: i64 = 0;
    let mut aura_stat_weight: u64 = 0;
    let mut aura_ap_stat_weight: u64 = 0;
    let mut aura_heal_stat_weight: u64 = 0;
    let mut aura_damage_stat_weight: u64 = 0;
    let mut is_feral = false;

    for spell_ref in &proto.spells {
        if spell_ref.spell_id <= 0 {
            continue;
        }

        // On-equip for weapons; on-use/on-hit for armor (except holdables).
        let trigger = spell_ref.spell_trigger;
        let is_valid_trigger = trigger == SPELLTRIGGER_ON_EQUIP
            || (!is_weapon(proto)
                && proto.inventory_type != INVTYPE_HOLDABLE
                && (trigger == SPELLTRIGGER_ON_USE || trigger == SPELLTRIGGER_CHANCE_ON_HIT));
        if !is_valid_trigger {
            continue;
        }

        let Some(spellproto) = ctx.world.lookup_spell_entry(spell_ref.spell_id as u32) else {
            continue;
        };

        let mut has_ap = false;
        let mut eff_stat: u64 = 0;
        let mut eff_ap: u64 = 0;
        let mut eff_heal: u64 = 0;
        let mut eff_damage: u64 = 0;

        // Three effect slots.
        for j in 0..3 {
            if spellproto.effect[j] != SPELL_EFFECT_APPLY_AURA
                || spellproto.effect_base_points[j] < 0
            {
                continue;
            }
            let aura_name = spellproto.effect_apply_aura_name[j];
            let base_val = spellproto.effect_base_points[j] + 1;
            let misc = spellproto.effect_misc_value[j];

            let decoded = decode_aura_effect(
                ctx,
                proto,
                &spellproto,
                aura_name,
                base_val,
                misc,
                &mut has_ap,
                &mut is_feral,
                &mut is_caster_item,
                &mut is_spell_damage_item,
                &mut is_healing_item,
                &mut is_attack_item,
                &mut is_tank_item,
                &mut is_dps_item,
                is_whitelist,
            );

            let Some(contribution) = decoded else {
                // A `None` means the aura-decoding triggered a
                // return-zero guard in the legacy code (e.g. non-feral
                // druid AP rejection). Early-exit the whole weight.
                return StatWeightResult::default();
            };

            eff_stat += contribution.eff_stat;
            eff_ap += contribution.eff_ap;
            eff_heal += contribution.eff_heal;
            eff_damage += contribution.eff_damage;
            if contribution.spell_damage != 0 {
                spell_damage_amt = contribution.spell_damage;
            }
            if contribution.spell_healing != 0 {
                spell_healing_amt = contribution.spell_healing;
            }
        }

        // ─── Trigger coverage factor ─────────────────────────────────────
        let coverage = trigger_coverage(trigger, spell_ref, &spellproto);

        eff_stat = ((eff_stat as f64) * (coverage as f64)) as u64;
        eff_ap = ((eff_ap as f64) * (coverage as f64)) as u64;
        eff_heal = ((eff_heal as f64) * (coverage as f64)) as u64;
        eff_damage = ((eff_damage as f64) * (coverage as f64)) as u64;

        aura_stat_weight += eff_stat;
        aura_ap_stat_weight += eff_ap;
        aura_heal_stat_weight += eff_heal;
        aura_damage_stat_weight += eff_damage;
    }

    // ─── Classic feral-druid 1h mace guard ───────────────────────────────
    #[cfg(feature = "vanilla")]
    {
        if !is_whitelist
            && !is_feral
            && player_class == class::DRUID
            && is_weapon(proto)
            && proto.sub_class == weapon_sub::MACE
            && (spec == 30 || spec == 32)
        {
            return StatWeightResult::default();
        }
    }

    stat_weight += aura_stat_weight;
    spell_heal += aura_heal_stat_weight;
    spell_power += aura_damage_stat_weight;
    attack_power += aura_ap_stat_weight;

    // ─── Sockets (TBC+) ──────────────────────────────────────────────────
    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    let mut socket_bonus: u64 = 0;
    #[cfg(not(any(feature = "tbc", feature = "wotlk")))]
    let socket_bonus: u64 = 0;

    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    {
        for socket in &proto.sockets {
            if socket.color == 0 {
                continue;
            }
            let name = match socket.color {
                SOCKET_COLOR_META => "metasocket",
                SOCKET_COLOR_YELLOW => "yellowsocket",
                SOCKET_COLOR_BLUE => "bluesocket",
                SOCKET_COLOR_RED => "redsocket",
                _ => continue,
            };
            socket_bonus += calculate_single_stat_weight(ctx.scale, name, 1) as u64;
        }
    }

    // ─── TBC: healing > damage overrides spec ─────────────────────────────
    #[cfg(all(feature = "tbc", not(feature = "wotlk")))]
    {
        if spell_damage_amt != 0
            && spell_healing_amt != 0
            && is_spell_damage_item
            && is_healing_item
            && spell_healing_amt > spell_damage_amt
        {
            is_spell_damage_item = false;
            is_healing_item = true;
        }
    }

    // ─── Spec type classification ────────────────────────────────────────
    #[cfg(feature = "wotlk")]
    {
        if spell_heal != 0 || spell_power != 0 {
            spec_type |= ItemSpecType::SPELL_HEALING;
            spec_type |= ItemSpecType::SPELL_DAMAGE;
        }
    }
    #[cfg(not(feature = "wotlk"))]
    {
        if spell_heal > spell_power || is_healing_item {
            spec_type |= ItemSpecType::SPELL_HEALING;
        }
        if spell_power >= spell_heal {
            spec_type |= ItemSpecType::SPELL_DAMAGE;
        }
    }

    if is_tank_item
        && (no_caster
            || !has_mana
            || spell_heal == 0
            || (!is_healing_item && !is_spell_damage_item))
    {
        spec_type |= ItemSpecType::TANK;
    }
    if is_attack_item {
        spec_type |= ItemSpecType::ATTACK;
    }
    if !no_caster && (is_caster_item || has_int || is_spell_damage_item) {
        spec_type |= ItemSpecType::CASTER;
    }

    // ─── TBC: ret/enh should not use spellpower, tanking filter ──────────
    #[cfg(all(feature = "tbc", not(feature = "wotlk")))]
    {
        if (spec == 6 || spec == 21)
            && (is_spell_damage_item
                || spell_damage_amt != 0
                || spell_healing_amt != 0
                || spell_heal != 0)
        {
            return StatWeightResult::default();
        }
        if proto.required_level > 60 && is_tank_item && !(spec == 30 || spec == 3 || spec == 5) {
            return StatWeightResult::default();
        }
        if proto.required_level > 60 && is_dps_item && (spec == 30 || spec == 3 || spec == 5) {
            return StatWeightResult::default();
        }
    }

    #[cfg(feature = "wotlk")]
    {
        if (spec == 6 || spec == 21)
            && (is_spell_damage_item
                || spell_damage_amt != 0
                || spell_healing_amt != 0
                || spell_heal != 0)
        {
            return StatWeightResult::default();
        }
        if proto.required_level > 60
            && is_tank_item
            && !(spec == 30 || spec == 3 || spec == 5 || spec == 18)
        {
            return StatWeightResult::default();
        }
        if proto.required_level > 60
            && is_dps_item
            && (spec == 30 || spec == 3 || spec == 5 || spec == 18)
        {
            return StatWeightResult::default();
        }
    }

    // ─── Tank-weapon speed caps ──────────────────────────────────────────
    if !is_whitelist && spec == 3 && is_weapon(proto) && proto.delay > 2300 {
        return StatWeightResult::default();
    }
    if !is_whitelist && spec == 5 && is_weapon(proto) && proto.delay > 2400 {
        return StatWeightResult::default();
    }

    // ─── Caster-item sanity filters ──────────────────────────────────────
    if is_caster_item
        || has_int
        || spell_heal != 0
        || spell_power != 0
        || is_spell_damage_item
        || is_healing_item
    {
        if !is_whitelist
            && (!has_mana || (no_caster && !(spec == 6 || spec == 30 || spec == 32 || spec == 21)))
            && (spell_heal != 0 || is_healing_item || is_spell_damage_item || spell_power != 0)
        {
            return StatWeightResult::default();
        }
        if !is_whitelist && !has_mana && has_int {
            return StatWeightResult::default();
        }
        if !is_whitelist
            && !has_mana
            && no_caster
            && (spell_power > attack_power || spell_heal > attack_power)
        {
            return StatWeightResult::default();
        }

        #[cfg(not(feature = "wotlk"))]
        {
            if !is_whitelist
                && (spec != 6 && spec != 21)
                && spell_power == 0
                && spell_heal == 0
                && is_spell_damage_item
            {
                return StatWeightResult::default();
            }
            if !is_whitelist && spell_heal == 0 && is_healing_item && !is_spell_damage_item {
                return StatWeightResult::default();
            }
        }

        if !is_whitelist
            && (spec != 6 && spec != 21)
            && !no_caster
            && is_spell_damage_item
            && spell_power == 0
            && !(spell_damage_amt != 0
                && spell_healing_amt != 0
                && is_weapon(proto)
                && proto.inventory_type == INVTYPE_WEAPON_MAINHAND)
        {
            return StatWeightResult::default();
        }

        let mut player_caster = false;
        for entry in &ctx.scale.stats {
            if matches!(
                entry.stat.as_str(),
                "splpwr"
                    | "int"
                    | "manargn"
                    | "splheal"
                    | "spellcritstrkrtng"
                    | "spellhitrtng"
            ) {
                player_caster = true;
            }
        }
        if !is_whitelist
            && (spec != 6 && spec != 21 && player_class != class::HUNTER)
            && !player_caster
        {
            return StatWeightResult::default();
        }
    }

    // ─── Attack-item sanity filter ───────────────────────────────────────
    if is_attack_item {
        if !is_whitelist
            && has_mana
            && !no_caster
            && !(has_int
                || spell_power != 0
                || spell_heal != 0
                || is_healing_item
                || is_spell_damage_item)
        {
            return StatWeightResult::default();
        }

        let mut player_attacker = false;
        for entry in &ctx.scale.stats {
            if matches!(
                entry.stat.as_str(),
                "str" | "agi" | "atkpwr" | "mledps" | "rgddps" | "hitrtng" | "critstrkrtng"
            ) {
                player_attacker = true;
            }
        }
        if !is_whitelist && !player_attacker {
            return StatWeightResult::default();
        }
    }

    stat_weight += spell_power;
    stat_weight += spell_heal;
    stat_weight += attack_power;
    stat_weight += defense_stats;

    // Items whose only contribution is socket bonuses are rejected.
    if socket_bonus != 0 && stat_weight == 0 && basic_stats_weight == 0 {
        return StatWeightResult::default();
    }
    stat_weight += socket_bonus;

    // Handle negative basic stats — if |negative| >= positive total, zero
    // the whole weight; otherwise add the signed delta.
    let final_weight: u64 = if basic_stats_weight < 0
        && (basic_stats_weight.unsigned_abs()) >= stat_weight
    {
        0
    } else if basic_stats_weight < 0 {
        stat_weight - basic_stats_weight.unsigned_abs()
    } else {
        stat_weight + basic_stats_weight as u64
    };

    StatWeightResult {
        weight: final_weight.min(u32::MAX as u64) as u32,
        item_spec: spec_type,
    }
}

// ── Aura-effect decoder ───────────────────────────────────────────────────

#[derive(Default)]
struct AuraContribution {
    eff_stat: u64,
    eff_ap: u64,
    eff_heal: u64,
    eff_damage: u64,
    spell_damage: i64,
    spell_healing: i64,
}

/// Decodes a single `SPELL_EFFECT_APPLY_AURA` effect. Returns `None` if
/// the aura triggers one of the legacy code's `return 0` guards
/// (e.g. non-druid feral AP spell, non-ranged class with ranged AP).
#[allow(clippy::too_many_arguments)]
fn decode_aura_effect(
    ctx: &StatWeightCtx<'_>,
    proto: &BotItemPrototype,
    spellproto: &BotSpellEntryInfo,
    aura_name: u32,
    base_val: i32,
    misc: i32,
    has_ap: &mut bool,
    is_feral: &mut bool,
    is_caster_item: &mut bool,
    is_spell_damage_item: &mut bool,
    is_healing_item: &mut bool,
    is_attack_item: &mut bool,
    is_tank_item: &mut bool,
    #[cfg_attr(not(any(feature = "tbc", feature = "wotlk")), allow(unused_variables))]
    is_dps_item: &mut bool,
    is_whitelist: bool,
) -> Option<AuraContribution> {
    let mut out = AuraContribution::default();

    // WotLK: SPELL_AURA_MOD_DAMAGE_DONE with school mask classifies as
    // both spell damage AND healing (unified spell power).
    #[cfg(feature = "wotlk")]
    {
        if aura_name == aura::MOD_DAMAGE_DONE {
            *is_spell_damage_item = true;
            *is_caster_item = true;
            out.spell_damage = (base_val) as i64;

            if misc == SPELL_SCHOOL_MASK_MAGIC {
                *is_healing_item = true;
                out.eff_damage +=
                    calculate_single_stat_weight(ctx.scale, "splpwr", out.spell_damage) as u64;
            } else {
                let mut special: u64 = 0;
                if (misc & SPELL_SCHOOL_MASK_ARCANE) != 0 {
                    special += calculate_single_stat_weight(ctx.scale, "arcsplpwr", out.spell_damage)
                        as u64;
                }
                if (misc & SPELL_SCHOOL_MASK_FROST) != 0 {
                    special += calculate_single_stat_weight(ctx.scale, "frosplpwr", out.spell_damage)
                        as u64;
                }
                if (misc & SPELL_SCHOOL_MASK_FIRE) != 0 {
                    special += calculate_single_stat_weight(ctx.scale, "firsplpwr", out.spell_damage)
                        as u64;
                }
                if (misc & SPELL_SCHOOL_MASK_SHADOW) != 0 {
                    special += calculate_single_stat_weight(ctx.scale, "shasplpwr", out.spell_damage)
                        as u64;
                }
                if (misc & SPELL_SCHOOL_MASK_NATURE) != 0 {
                    special += calculate_single_stat_weight(ctx.scale, "natsplpwr", out.spell_damage)
                        as u64;
                }
                if !is_whitelist && special == 0 && *is_spell_damage_item {
                    return None;
                }
                out.eff_damage += special;
            }
        }
    }

    // Classic/TBC: separate damage and healing auras.
    #[cfg(not(feature = "wotlk"))]
    {
        if aura_name == aura::MOD_DAMAGE_DONE {
            out.spell_damage = base_val as i64;
            *is_spell_damage_item = true;

            if misc == SPELL_SCHOOL_MASK_MAGIC {
                out.eff_damage +=
                    calculate_single_stat_weight(ctx.scale, "splpwr", out.spell_damage) as u64;
            } else {
                let mut special: u64 = 0;
                if (misc & SPELL_SCHOOL_MASK_ARCANE) != 0 {
                    special += calculate_single_stat_weight(ctx.scale, "arcsplpwr", out.spell_damage)
                        as u64;
                }
                if (misc & SPELL_SCHOOL_MASK_FROST) != 0 {
                    special += calculate_single_stat_weight(ctx.scale, "frosplpwr", out.spell_damage)
                        as u64;
                }
                if (misc & SPELL_SCHOOL_MASK_FIRE) != 0 {
                    special += calculate_single_stat_weight(ctx.scale, "firsplpwr", out.spell_damage)
                        as u64;
                }
                if (misc & SPELL_SCHOOL_MASK_SHADOW) != 0 {
                    special += calculate_single_stat_weight(ctx.scale, "shasplpwr", out.spell_damage)
                        as u64;
                }
                if (misc & SPELL_SCHOOL_MASK_NATURE) != 0 {
                    special += calculate_single_stat_weight(ctx.scale, "natsplpwr", out.spell_damage)
                        as u64;
                }
                out.eff_damage += special;
            }
        }

        if aura_name == aura::MOD_HEALING_DONE {
            *is_healing_item = true;
            out.spell_healing = base_val as i64;
            out.eff_heal +=
                calculate_single_stat_weight(ctx.scale, "splheal", out.spell_healing) as u64;
        }
    }

    // Spell hit rating (pre-TBC bucket; still decoded on TBC+).
    if aura_name == aura::MOD_SPELL_HIT_CHANCE {
        *is_caster_item = true;
        out.eff_stat +=
            calculate_single_stat_weight(ctx.scale, "spellhitrtng", base_val as i64) as u64;
    }

    // Spell crit rating (pre-TBC; still decoded on TBC+).
    if aura_name == aura::MOD_SPELL_CRIT_CHANCE || aura_name == aura::MOD_SPELL_CRIT_CHANCE_SCHOOL {
        *is_caster_item = true;
        out.eff_stat +=
            calculate_single_stat_weight(ctx.scale, "spellcritstrkrtng", base_val as i64) as u64;
    }

    // Spell penetration — MOD_TARGET_RESISTANCE with school mask SPELL.
    if aura_name == aura::MOD_TARGET_RESISTANCE && misc == SPELL_SCHOOL_MASK_SPELL {
        out.eff_stat += calculate_single_stat_weight(
            ctx.scale,
            "spellpenrtng",
            (base_val as i64).unsigned_abs() as i64,
        ) as u64;
    }

    // Attack power (MOD_ATTACK_POWER). "Attack Power - Feral" marks the
    // feral variant; non-druid weapons with it are rejected.
    if !*has_ap && aura_name == aura::MOD_ATTACK_POWER {
        *has_ap = true;
        *is_attack_item = true;

        // Decode the C string name to catch "Attack Power - Feral".
        let spell_name = spellproto_name(spellproto);
        if spell_name.contains("Attack Power - Feral") {
            *is_feral = true;
        }

        #[cfg(feature = "vanilla")]
        {
            if !is_whitelist
                && *is_feral
                && (ctx.player_class != class::DRUID
                    && ctx.player_class != class::WARRIOR
                    && ctx.player_class != class::PALADIN
                    && is_weapon(proto))
            {
                return None;
            }
        }
        #[cfg(not(feature = "vanilla"))]
        {
            if !is_whitelist && *is_feral && ctx.player_class != class::DRUID {
                return None;
            }
        }

        let name = if *is_feral { "feratkpwr" } else { "atkpwr" };
        out.eff_ap += calculate_single_stat_weight(ctx.scale, name, base_val as i64) as u64;
    }

    // Ranged AP — only hunters + ranged weapons allowed.
    if !*has_ap && aura_name == aura::MOD_RANGED_ATTACK_POWER {
        if ctx.player_class == class::SHAMAN
            || (!is_ranged_weapon(proto) && ctx.player_class != class::HUNTER)
        {
            return None;
        }
        *has_ap = true;
        *is_attack_item = true;
        out.eff_ap += calculate_single_stat_weight(ctx.scale, "atkpwr", base_val as i64) as u64;
    }

    if aura_name == aura::MOD_SHIELD_BLOCKVALUE {
        *is_tank_item = true;
        out.eff_stat += calculate_single_stat_weight(ctx.scale, "block", base_val as i64) as u64;
    }
    if aura_name == aura::MOD_PARRY_PERCENT {
        *is_tank_item = true;
        out.eff_stat += calculate_single_stat_weight(ctx.scale, "parryrtng", base_val as i64) as u64;
    }
    if aura_name == aura::MOD_DODGE_PERCENT {
        *is_tank_item = true;
        out.eff_stat += calculate_single_stat_weight(ctx.scale, "dodgertng", base_val as i64) as u64;
    }
    if aura_name == aura::MOD_BLOCK_PERCENT {
        *is_tank_item = true;
        out.eff_stat += calculate_single_stat_weight(ctx.scale, "blockrtng", base_val as i64) as u64;
    }

    // Armor pen — MOD_TARGET_RESISTANCE with NORMAL school.
    if aura_name == aura::MOD_TARGET_RESISTANCE && misc == SPELL_SCHOOL_MASK_NORMAL {
        out.eff_stat += calculate_single_stat_weight(
            ctx.scale,
            "armorpenrtng",
            (base_val as i64).unsigned_abs() as i64,
        ) as u64;
    }

    if aura_name == aura::MOD_HIT_CHANCE {
        *is_attack_item = true;
        out.eff_stat += calculate_single_stat_weight(ctx.scale, "hitrtng", base_val as i64) as u64;
    }
    if aura_name == aura::MOD_CRIT_PERCENT {
        *is_attack_item = true;
        out.eff_stat +=
            calculate_single_stat_weight(ctx.scale, "critstrkrtng", base_val as i64) as u64;
    }

    // Defense skill bonus.
    if aura_name == aura::MOD_SKILL && misc == SKILL_DEFENSE {
        *is_tank_item = true;
        out.eff_stat += calculate_single_stat_weight(ctx.scale, "defrtng", base_val as i64) as u64;
    }

    // MOD_RATING — TBC+ only, decodes the effect mask into multiple ratings.
    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    {
        if aura_name == aura::MOD_RATING {
            for rating in 0..32 {
                if (misc & (1 << rating)) == 0 {
                    continue;
                }
                let Some(weight_name) = rating_to_stat_name(rating as u32) else {
                    continue;
                };
                if is_tank_rating(rating as u32) {
                    *is_tank_item = true;
                }
                if is_dps_rating(rating as u32) {
                    *is_dps_item = true;
                }
                out.eff_stat +=
                    calculate_single_stat_weight(ctx.scale, weight_name, base_val as i64) as u64;
            }
        }
    }

    if aura_name == aura::MOD_POWER_REGEN {
        *is_caster_item = true;
        out.eff_stat += calculate_single_stat_weight(ctx.scale, "manargn", base_val as i64) as u64;
    }

    Some(out)
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn trigger_coverage(
    trigger: u32,
    spell_ref: &cmangos::BotItemProtoSpellRef,
    spellproto: &BotSpellEntryInfo,
) -> f32 {
    if trigger == SPELLTRIGGER_ON_USE {
        if spell_ref.spell_cooldown != 0 {
            let duration = spellproto.duration_ms as f32;
            duration / spell_ref.spell_cooldown as f32
        } else {
            // Most trinkets: 20s buff / 120s cd ≈ 17%.
            0.17
        }
    } else if trigger == SPELLTRIGGER_CHANCE_ON_HIT {
        #[cfg(feature = "vanilla")]
        let average_item_delay: f32 = 2.43;
        #[cfg(all(feature = "tbc", not(feature = "wotlk")))]
        let average_item_delay: f32 = 2.09;
        #[cfg(feature = "wotlk")]
        let average_item_delay: f32 = 2.39;
        #[cfg(not(any(feature = "vanilla", feature = "tbc", feature = "wotlk")))]
        let average_item_delay: f32 = 2.43;

        let proc_chance = spellproto.proc_chance as f32;
        let duration = spellproto.duration_ms as f32;
        let cov = (proc_chance / 100.0) * (duration / average_item_delay);
        cov.min(0.9)
    } else {
        1.0
    }
}

/// Decode a `BotSpellEntryInfo::name` (nul-terminated C char buffer) into
/// a borrowed `&str`. Invalid UTF-8 sequences are replaced conservatively
/// so the legacy feral-AP substring search still works on ASCII names.
fn spellproto_name(spellproto: &BotSpellEntryInfo) -> &str {
    let bytes: &[u8] = {
        // Safety note: the C buffer is exactly `name.len()` bytes and
        // nul-terminated. We avoid unsafe by searching the signed-char
        // buffer for the first nul and slicing on the byte view.
        let raw: &[core::ffi::c_char] = &spellproto.name;
        // SAFETY: we use a pure-safe read by transmuting through the
        // bytemuck-equivalent cast. Since this crate denies unsafe, we
        // work with the signed view and reinterpret char-by-char.
        //
        // The deny(unsafe_code) prevents a direct transmute, so we do a
        // manual `&[i8]` → `&[u8]` via a per-element cast into a static
        // buffer would allocate. Instead we search the `&[i8]` for a
        // zero and slice the original bytes via `core::slice::from_raw_parts`.
        //
        // Actually: because `c_char` may be i8 or u8 depending on target,
        // and we can't `as_bytes` it under deny(unsafe_code), we use a
        // helper-scoped `#[allow(unsafe_code)]` in `c_str_to_str` below.
        c_char_slice_as_bytes(raw)
    };
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..end]).unwrap_or("")
}

/// View a `[c_char; N]` as `&[u8]`. `c_char` aliases `i8` or `u8`
/// depending on the platform; both share the same size/alignment so the
/// reinterpret is always sound for a POD input.
#[allow(unsafe_code)]
fn c_char_slice_as_bytes(buf: &[core::ffi::c_char]) -> &[u8] {
    // Safety: `c_char` is either i8 or u8 — both 1-byte, aligned to 1.
    // The lifetime is preserved.
    unsafe { core::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), buf.len()) }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itempool::types::{WeightScale, WeightScaleInfo, WeightScaleStat};
    use cmangos::MockItemWorld;

    fn scale(name: &str, class_id: u8, entries: &[(&str, u32)]) -> WeightScale {
        WeightScale {
            info: WeightScaleInfo {
                id: 1,
                name: name.to_string(),
                class_id,
            },
            stats: entries
                .iter()
                .map(|(s, w)| WeightScaleStat {
                    stat: (*s).to_string(),
                    weight: *w,
                })
                .collect(),
        }
    }

    fn ctx<'a>(
        scale: &'a WeightScale,
        player_class: u8,
        spec_id: u32,
        world: &'a MockItemWorld,
    ) -> StatWeightCtx<'a> {
        StatWeightCtx {
            player_class,
            spec_id,
            scale,
            whitelist: &[],
            world,
        }
    }

    fn base_proto() -> BotItemPrototype {
        let mut p = BotItemPrototype::default();
        p.class_id = item_class::ARMOR;
        p.quality = 2;
        p.item_level = 10;
        p.required_level = 10;
        p
    }

    #[test]
    fn single_stat_weight_primary() {
        let s = scale("test", 1, &[("sta", 5), ("str", 7)]);
        assert_eq!(calculate_single_stat_weight(&s, "sta", 10), 50);
        assert_eq!(calculate_single_stat_weight(&s, "str", 3), 21);
        assert_eq!(calculate_single_stat_weight(&s, "int", 10), 0);
    }

    #[test]
    fn hunter_thrown_returns_item_level_on_classic() {
        #[cfg(feature = "vanilla")]
        {
            let s = scale("surv", class::HUNTER, &[("agi", 5)]);
            let world = MockItemWorld::new();
            let mut p = base_proto();
            p.class_id = item_class::WEAPON;
            p.sub_class = weapon_sub::THROWN;
            p.item_level = 42;
            let r = calculate_stat_weight(&ctx(&s, class::HUNTER, 1, &world), &p);
            assert_eq!(r.weight, 42);
        }
    }

    // On WotLK, the libram/idol/totem/sigil early-exit at the top of
    // `calculate_stat_weight` fires before the relic-slot filter and
    // returns `quality + item_level`. This matches the legacy C++
    // semantics (the relic-slot branch is effectively dead code on
    // WotLK because relic items ARE libram/idol/totem/sigil
    // subclasses), so these tests only apply to Classic/TBC.
    #[cfg(not(feature = "wotlk"))]
    #[test]
    fn paladin_relic_non_libram_zero() {
        let s = scale("holy", class::PALADIN, &[("int", 5)]);
        let world = MockItemWorld::new();
        let mut p = base_proto();
        p.inventory_type = INVTYPE_RELIC;
        p.sub_class = armor_sub::IDOL;
        let r = calculate_stat_weight(&ctx(&s, class::PALADIN, 1, &world), &p);
        assert_eq!(r.weight, 0);
    }

    #[cfg(not(feature = "wotlk"))]
    #[test]
    fn warrior_relic_is_rejected() {
        let s = scale("arms", class::WARRIOR, &[("str", 5)]);
        let world = MockItemWorld::new();
        let mut p = base_proto();
        p.inventory_type = INVTYPE_RELIC;
        p.sub_class = armor_sub::LIBRAM;
        let r = calculate_stat_weight(&ctx(&s, class::WARRIOR, 1, &world), &p);
        assert_eq!(r.weight, 0);
    }

    #[test]
    fn basic_stats_accumulate() {
        // The scale has `str` and `agi` (both `player_attacker` markers),
        // so the attack filter accepts the item.
        let s = scale(
            "arms",
            class::WARRIOR,
            &[("str", 2), ("sta", 1), ("agi", 1)],
        );
        let world = MockItemWorld::new();
        let mut p = base_proto();
        p.stats[0].stat_type = item_mod::STRENGTH;
        p.stats[0].stat_value = 10;
        p.stats[1].stat_type = item_mod::STAMINA;
        p.stats[1].stat_value = 20;
        p.stats[2].stat_type = item_mod::AGILITY;
        p.stats[2].stat_value = 5;
        p.stats_count = 3;

        let r = calculate_stat_weight(&ctx(&s, class::WARRIOR, 1, &world), &p);
        // Expected basic: str 10*2 + sta 20*1 + agi 5*1 = 45
        assert_eq!(r.weight, 45);
        assert!(r.item_spec.contains(ItemSpecType::ATTACK));
    }

    #[test]
    fn basic_stats_rejected_without_attacker_marker() {
        // Scale has no attack markers (no str/agi/atkpwr/mledps/…), so
        // even though the item has stats, the attack filter rejects it.
        let s = scale("heal", class::WARRIOR, &[("sta", 1)]);
        let world = MockItemWorld::new();
        let mut p = base_proto();
        p.stats[0].stat_type = item_mod::STRENGTH;
        p.stats[0].stat_value = 10;
        p.stats_count = 1;

        let r = calculate_stat_weight(&ctx(&s, class::WARRIOR, 1, &world), &p);
        assert_eq!(r.weight, 0);
    }

    #[test]
    fn weapon_dps_applied() {
        let s = scale(
            "arms",
            class::WARRIOR,
            &[("mledps", 3), ("atkpwr", 0)],
        );
        let world = MockItemWorld::new();
        let mut p = base_proto();
        p.class_id = item_class::WEAPON;
        p.sub_class = weapon_sub::SWORD2;
        p.delay = 2000;
        p.damages[0].damage_min = 80.0;
        p.damages[0].damage_max = 120.0;
        // Need a str/agi/atkpwr stat to flag it attack_item.
        p.stats[0].stat_type = item_mod::STRENGTH;
        p.stats[0].stat_value = 1;

        let s2 = scale(
            "arms",
            class::WARRIOR,
            &[("str", 0), ("mledps", 3), ("atkpwr", 0)],
        );
        let r = calculate_stat_weight(&ctx(&s2, class::WARRIOR, 1, &world), &p);
        // dps = (80+120)/(2000/1000)/2 = 50 → 50*3 = 150
        assert_eq!(r.weight, 150);
    }

    #[test]
    fn tank_weapon_speed_cap_rejects_slow() {
        let s = scale(
            "prot",
            class::WARRIOR,
            &[("mledps", 3), ("atkpwr", 0), ("sta", 1)],
        );
        let world = MockItemWorld::new();
        let mut p = base_proto();
        p.class_id = item_class::WEAPON;
        p.sub_class = weapon_sub::SWORD;
        p.delay = 2400;
        p.damages[0].damage_min = 80.0;
        p.damages[0].damage_max = 120.0;
        let r = calculate_stat_weight(&ctx(&s, class::WARRIOR, 3, &world), &p);
        assert_eq!(r.weight, 0);
    }
}
