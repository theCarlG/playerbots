//! Stat name → `ITEM_MOD_*` / `CR_*` mapping tables.
//!
//! Ports the `weightStatLink`, `weightRatingLink`, and `ItemStatLink`
//! maps initialized in the C++ `RandomItemMgr` constructor. The
//! contents are expansion-conditional, matching the `#ifdef
//! MANGOSBOT_ZERO/ONE/TWO` blocks in the legacy code.

/// `ITEM_MOD_*` enum (`ItemPrototype.h`). Only the values we actually
/// consult are listed; the rest are omitted to avoid dead-code noise.
#[allow(dead_code)]
pub mod item_mod {
    pub const MANA: u32 = 0;
    pub const HEALTH: u32 = 1;
    pub const AGILITY: u32 = 2;
    pub const STRENGTH: u32 = 3;
    pub const INTELLECT: u32 = 4;
    pub const SPIRIT: u32 = 5;
    pub const STAMINA: u32 = 6;
    pub const DEFENSE_SKILL_RATING: u32 = 12;
    pub const DODGE_RATING: u32 = 13;
    pub const PARRY_RATING: u32 = 14;
    pub const BLOCK_RATING: u32 = 15;
    pub const HIT_MELEE_RATING: u32 = 16;
    pub const HIT_RANGED_RATING: u32 = 17;
    pub const HIT_SPELL_RATING: u32 = 18;
    pub const CRIT_MELEE_RATING: u32 = 19;
    pub const CRIT_RANGED_RATING: u32 = 20;
    pub const CRIT_SPELL_RATING: u32 = 21;
    pub const HIT_TAKEN_MELEE_RATING: u32 = 22;
    pub const HIT_TAKEN_RANGED_RATING: u32 = 23;
    pub const HIT_TAKEN_SPELL_RATING: u32 = 24;
    pub const CRIT_TAKEN_MELEE_RATING: u32 = 25;
    pub const CRIT_TAKEN_RANGED_RATING: u32 = 26;
    pub const CRIT_TAKEN_SPELL_RATING: u32 = 27;
    pub const HASTE_MELEE_RATING: u32 = 28;
    pub const HASTE_RANGED_RATING: u32 = 29;
    pub const HASTE_SPELL_RATING: u32 = 30;
    pub const HIT_RATING: u32 = 31;
    pub const CRIT_RATING: u32 = 32;
    pub const HIT_TAKEN_RATING: u32 = 33;
    pub const CRIT_TAKEN_RATING: u32 = 34;
    pub const RESILIENCE_RATING: u32 = 35;
    pub const HASTE_RATING: u32 = 36;
    pub const EXPERTISE_RATING: u32 = 37;
    pub const ATTACK_POWER: u32 = 38;
    pub const RANGED_ATTACK_POWER: u32 = 39;
    pub const FERAL_ATTACK_POWER: u32 = 40;
    pub const SPELL_HEALING_DONE: u32 = 41;
    pub const SPELL_DAMAGE_DONE: u32 = 42;
    pub const MANA_REGENERATION: u32 = 43;
    pub const ARMOR_PENETRATION_RATING: u32 = 44;
    pub const SPELL_POWER: u32 = 45;
    pub const HEALTH_REGEN: u32 = 46;
    pub const SPELL_PENETRATION: u32 = 47;
    pub const BLOCK_VALUE: u32 = 48;
}

/// Return the `ITEM_MOD_*` value corresponding to a weight-scale stat
/// name, or `None` if the name is not recognised for the current
/// expansion.
///
/// # Expansion gating
///
/// * Classic (`vanilla`): only the five primary stats — sta/str/agi/int/spi.
/// * TBC (`tbc`): adds the rating set, excluding spell power, attack
///   power, feral attack power, armor-penetration rating, block
///   value, and mana regen. Adds spell-specific crit/hit/haste/hit rating.
/// * `WotLK` (`wotlk`): the full rating set including spell power, AP, feral
///   AP, armor-pen rating, block value, and mana regen.
#[must_use]
pub fn stat_to_item_mod(name: &str) -> Option<u32> {
    // Shared across all expansions.
    match name {
        "sta" => return Some(item_mod::STAMINA),
        "str" => return Some(item_mod::STRENGTH),
        "agi" => return Some(item_mod::AGILITY),
        "int" => return Some(item_mod::INTELLECT),
        "spi" => return Some(item_mod::SPIRIT),
        _ => {}
    }

    #[cfg(feature = "wotlk")]
    {
        match name {
            "splpwr" => return Some(item_mod::SPELL_POWER),
            "atkpwr" => return Some(item_mod::ATTACK_POWER),
            "feratkpwr" => return Some(item_mod::FERAL_ATTACK_POWER),
            "exprtng" => return Some(item_mod::EXPERTISE_RATING),
            "critstrkrtng" => return Some(item_mod::CRIT_RATING),
            "hitrtng" => return Some(item_mod::HIT_RATING),
            "hastertng" => return Some(item_mod::HASTE_RATING),
            "armorpenrtng" => return Some(item_mod::ARMOR_PENETRATION_RATING),
            "defrtng" => return Some(item_mod::DEFENSE_SKILL_RATING),
            "dodgertng" => return Some(item_mod::DODGE_RATING),
            "block" => return Some(item_mod::BLOCK_VALUE),
            "blockrtng" => return Some(item_mod::BLOCK_RATING),
            "parryrtng" => return Some(item_mod::PARRY_RATING),
            "manargn" => return Some(item_mod::MANA_REGENERATION),
            _ => {}
        }
    }

    #[cfg(feature = "tbc")]
    {
        match name {
            "exprtng" => return Some(item_mod::EXPERTISE_RATING),
            "critstrkrtng" => return Some(item_mod::CRIT_RATING),
            "spellcritstrkrtng" => return Some(item_mod::CRIT_SPELL_RATING),
            "hitrtng" => return Some(item_mod::HIT_RATING),
            "spellhitrtng" => return Some(item_mod::HIT_SPELL_RATING),
            "hastertng" => return Some(item_mod::HASTE_RATING),
            "spellhastertng" => return Some(item_mod::HASTE_SPELL_RATING),
            "defrtng" => return Some(item_mod::DEFENSE_SKILL_RATING),
            "dodgertng" => return Some(item_mod::DODGE_RATING),
            "blockrtng" => return Some(item_mod::BLOCK_RATING),
            "parryrtng" => return Some(item_mod::PARRY_RATING),
            _ => {}
        }
    }

    None
}

/// Reverse lookup used by `CalculateStatWeight` when walking
/// `ItemPrototype::ItemStat[]`. Returns the weight-scale stat name for
/// a primary stat constant (`STAT_*`), or `None` for non-primary stats.
///
/// The C++ `ItemStatLink` map covers the five primary stats only.
#[must_use]
pub fn primary_stat_name(stat: u32) -> Option<&'static str> {
    match stat {
        // `Stats` enum from CMaNGOS `SharedDefines.h`:
        // STRENGTH = 0, AGILITY = 1, STAMINA = 2, INTELLECT = 3, SPIRIT = 4
        0 => Some("str"),
        1 => Some("agi"),
        2 => Some("sta"),
        3 => Some("int"),
        4 => Some("spi"),
        _ => None,
    }
}

/// Reverse-from-`ITEM_MOD` lookup used by the stat-weight calculation
/// when decoding item stats. Returns the weight-scale name that maps
/// to a given mod, or `None` if the mod is not in the scale table for
/// the current expansion.
#[must_use]
pub fn item_mod_to_stat_name(mod_id: u32) -> Option<&'static str> {
    // Primary stats — shared.
    match mod_id {
        x if x == item_mod::STAMINA => return Some("sta"),
        x if x == item_mod::STRENGTH => return Some("str"),
        x if x == item_mod::AGILITY => return Some("agi"),
        x if x == item_mod::INTELLECT => return Some("int"),
        x if x == item_mod::SPIRIT => return Some("spi"),
        _ => {}
    }

    #[cfg(feature = "wotlk")]
    {
        match mod_id {
            x if x == item_mod::SPELL_POWER => return Some("splpwr"),
            x if x == item_mod::ATTACK_POWER => return Some("atkpwr"),
            x if x == item_mod::FERAL_ATTACK_POWER => return Some("feratkpwr"),
            x if x == item_mod::EXPERTISE_RATING => return Some("exprtng"),
            x if x == item_mod::CRIT_RATING => return Some("critstrkrtng"),
            x if x == item_mod::HIT_RATING => return Some("hitrtng"),
            x if x == item_mod::HASTE_RATING => return Some("hastertng"),
            x if x == item_mod::ARMOR_PENETRATION_RATING => return Some("armorpenrtng"),
            x if x == item_mod::DEFENSE_SKILL_RATING => return Some("defrtng"),
            x if x == item_mod::DODGE_RATING => return Some("dodgertng"),
            x if x == item_mod::BLOCK_VALUE => return Some("block"),
            x if x == item_mod::BLOCK_RATING => return Some("blockrtng"),
            x if x == item_mod::PARRY_RATING => return Some("parryrtng"),
            x if x == item_mod::MANA_REGENERATION => return Some("manargn"),
            _ => {}
        }
    }

    #[cfg(feature = "tbc")]
    {
        match mod_id {
            x if x == item_mod::EXPERTISE_RATING => return Some("exprtng"),
            x if x == item_mod::CRIT_RATING => return Some("critstrkrtng"),
            x if x == item_mod::CRIT_SPELL_RATING => return Some("spellcritstrkrtng"),
            x if x == item_mod::HIT_RATING => return Some("hitrtng"),
            x if x == item_mod::HIT_SPELL_RATING => return Some("spellhitrtng"),
            x if x == item_mod::HASTE_RATING => return Some("hastertng"),
            x if x == item_mod::HASTE_SPELL_RATING => return Some("spellhastertng"),
            x if x == item_mod::DEFENSE_SKILL_RATING => return Some("defrtng"),
            x if x == item_mod::DODGE_RATING => return Some("dodgertng"),
            x if x == item_mod::BLOCK_RATING => return Some("blockrtng"),
            x if x == item_mod::PARRY_RATING => return Some("parryrtng"),
            _ => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_stats_always_mapped() {
        assert_eq!(stat_to_item_mod("sta"), Some(item_mod::STAMINA));
        assert_eq!(stat_to_item_mod("str"), Some(item_mod::STRENGTH));
        assert_eq!(stat_to_item_mod("agi"), Some(item_mod::AGILITY));
        assert_eq!(stat_to_item_mod("int"), Some(item_mod::INTELLECT));
        assert_eq!(stat_to_item_mod("spi"), Some(item_mod::SPIRIT));
    }

    #[test]
    fn primary_stat_name_by_stat_enum() {
        assert_eq!(primary_stat_name(0), Some("str"));
        assert_eq!(primary_stat_name(1), Some("agi"));
        assert_eq!(primary_stat_name(2), Some("sta"));
        assert_eq!(primary_stat_name(3), Some("int"));
        assert_eq!(primary_stat_name(4), Some("spi"));
        assert!(primary_stat_name(5).is_none());
    }

    #[test]
    fn unknown_stats_return_none() {
        assert!(stat_to_item_mod("bogus").is_none());
    }
}
