// Spell ID tables are always compiled — they are just numeric constants.
// Feature flags gate behavior modules (encounters, class builds), not spell IDs.
pub mod tbc;
pub mod vanilla;
pub mod wotlk;

use crate::ffi::SpellId;

/// Case-insensitive spell-name → id lookup for the `cast <name>` chat
/// command. Only covers spells users actually type in raid chat
/// (taunts, interrupts, panic buttons, utility) — not exhaustive.
///
/// Input is expected to already be lowercased and trimmed. Underscores
/// and spaces are both accepted (`cast Mortal Strike`, `cast mortal_strike`).
pub fn lookup_spell_by_name(name: &str) -> Option<SpellId> {
    // Normalize: lowercase, collapse separators to spaces.
    let normalized: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c == '_' || c == '-' { ' ' } else { c })
        .collect();
    let key = normalized.split_whitespace().collect::<Vec<_>>().join(" ");

    let id: u32 = match key.as_str() {
        // ── RTSC ─────────────────────────────────────────────────
        "aedm" => 30758, // RTSC movement spell

        // ── Warrior ──────────────────────────────────────────────
        "battle stance" => 2457,
        "berserker stance" => 2458,
        "defensive stance" => 71,
        "battle shout" => 11551,
        "commanding shout" => 469,
        "demoralizing shout" => 11556,
        "intimidating shout" => 5246,
        "challenging shout" => 1161,
        "heroic strike" => 11567,
        "rend" => 11574,
        "thunder clap" => 11581,
        "hamstring" => 7372,
        "overpower" => 11585,
        "mocking blow" => 694,
        "charge" => 11578,
        "mortal strike" => 21553,
        "sweeping strikes" => 12292,
        "death wish" => 12328,
        "retaliation" => 20230,
        "recklessness" => 1719,
        "bloodthirst" => 25251,
        "whirlwind" => 1680,
        "cleave" => 11608,
        "execute" => 20662,
        "intercept" => 20617,
        "slam" => 11605,
        "berserker rage" => 18499,
        "shield wall" => 871,
        "last stand" => 12975,
        "shield bash" => 11972,
        "shield block" => 2565,
        "sunder armor" => 11597,
        "revenge" => 11601,
        "shield slam" => 23922,
        "taunt" => 355,
        "bloodrage" => 2687,
        "disarm" => 676,
        "concussion blow" => 12809,
        "pummel" => 6552,

        // ── Paladin ──────────────────────────────────────────────
        "holy shock" => 20930,
        "flash of light" => 19943,
        "holy light" => 25292,
        "divine shield" => 642,
        "bubble" => 642,
        "divine protection" => 498,
        "lay on hands" => 10310,
        "hammer of justice" => 10308,
        "hammer of wrath" => 24239,
        "seal of righteousness" => 21082,
        "seal of command" => 20920,
        "judgement" => 20271,
        "consecration" => 20924,
        "holy wrath" => 2812,
        "blessing of might" => 25291,
        "blessing of wisdom" => 25290,
        "blessing of kings" => 20217,
        "blessing of protection" => 10278,
        "divine favor" => 20216,
        "exorcism" => 10314,
        "retribution aura" => 10301,
        "devotion aura" => 10293,
        "righteous defense" => 31789,
        "hand of reckoning" => 62124, // wotlk

        // ── Priest ───────────────────────────────────────────────
        "flash heal" => 10916,
        "greater heal" => 25213,
        "heal" => 6063,
        "lesser heal" => 2052,
        "renew" => 25315,
        "prayer of healing" => 25316,
        "power word shield" => 10901,
        "inner fire" => 10952,
        "divine spirit" => 14752,
        "power word fortitude" => 21562,
        "shadow word pain" => 10894,
        "mind blast" => 10947,
        "mind flay" => 18807,
        "vampiric embrace" => 15286,
        "devouring plague" => 19276,
        "dispel magic" => 988,
        "fade" => 10942,
        "psychic scream" => 10890,
        "holy nova" => 25331,
        "shadowform" => 15473,
        "resurrection" => 10881,
        "inner focus" => 14751,
        "silence" => 15487,

        // ── Druid ────────────────────────────────────────────────
        "rejuvenation" => 25299,
        "regrowth" => 9858,
        "healing touch" => 25297,
        "tranquility" => 9863,
        "innervate" => 29166,
        "barkskin" => 22812,
        "moonfire" => 26987,
        "starfire" => 25298,
        "wrath" => 9912,
        "insect swarm" => 24977,
        "shred" => 9830,
        "rake" => 9904,
        "rip" => 9896,
        "ferocious bite" => 22568,
        "maul" => 10628,
        "swipe" => 26996,
        "demoralizing roar" => 26998,
        "faerie fire" => 16857,
        "faerie fire feral" => 16857,
        "claw" => 9850,
        "cat form" => 768,
        "bear form" => 5487,
        "travel form" => 783,
        "aquatic form" => 1066,
        "nature swiftness" => 17116,
        "tigers fury" => 9846,
        "growl" => 6795,
        "challenging roar" => 5209,
        "entangling roots" => 339,
        "hibernate" => 2637,
        "remove curse" => 475,

        // ── Hunter ───────────────────────────────────────────────
        "auto shot" => 75,
        "arcane shot" => 14287,
        "serpent sting" => 13555,
        "aimed shot" => 20904,
        "multi shot" | "multishot" => 14290,
        "volley" => 14295,
        "hunters mark" | "hunter's mark" => 14325,
        "raptor strike" => 14266,
        "disengage" => 781,
        "feign death" => 5384,
        "aspect of the hawk" => 25296,
        "aspect of the monkey" => 8078,
        "aspect of the cheetah" => 5118,
        "explosive trap" => 14316,
        "freezing trap" => 14311,
        "immolation trap" => 14305,
        "wing clip" => 14268,
        "scatter shot" => 19503,
        "bestial wrath" => 19574,
        "rapid fire" => 3045,
        "tranquilizing shot" => 19801,
        "scare beast" => 1513,

        // ── Mage ─────────────────────────────────────────────────
        "fireball" => 25306,
        "fire blast" => 10199,
        "scorch" => 10205,
        "flamestrike" => 10216,
        "blast wave" => 11113,
        "pyroblast" => 18809,
        "frostbolt" => 25304,
        "frost nova" => 10230,
        "ice block" => 11958,
        "cold snap" => 12472,
        "arcane missiles" => 25345,
        "arcane explosion" => 10202,
        "blizzard" => 10187,
        "cone of cold" => 10161,
        "counterspell" => 2139,
        "polymorph" => 12826,
        "blink" => 1953,
        "evocation" => 12051,
        "arcane intellect" => 10157,
        "arcane brilliance" => 23028,
        "combustion" => 11129,
        "arcane power" => 12042,
        "presence of mind" => 12043,

        // ── Rogue ────────────────────────────────────────────────
        "sinister strike" => 11293,
        "backstab" => 25300,
        "hemorrhage" => 17347,
        "eviscerate" => 26865,
        "slice and dice" => 6774,
        "rupture" => 11275,
        "expose armor" => 8647,
        "garrote" => 11289,
        "ambush" => 11269,
        "stealth" => 1787,
        "sprint" => 11305,
        "evasion" => 26669,
        "blind" => 2094,
        "kidney shot" => 8643,
        "gouge" => 11286,
        "vanish" => 11327,
        "kick" => 1769,
        "cold blood" => 14177,
        "adrenaline rush" => 13750,
        "blade flurry" => 13877,
        "sap" => 6770,
        "cloak of shadows" => 31224,

        // ── Shaman ───────────────────────────────────────────────
        "lightning bolt" => 25448,
        "chain lightning" => 25442,
        "lightning shield" => 10432,
        "flame shock" => 10448,
        "earth shock" => 10414,
        "frost shock" => 10473,
        "healing wave" => 25357,
        "lesser healing wave" => 10468,
        "chain heal" => 25423,
        "purge" => 8012,
        "stormstrike" => 17364,
        "elemental mastery" => 16166,
        "grace of air totem" => 25359,
        "strength of earth totem" => 25361,
        "windfury totem" => 25587,
        "mana spring totem" => 10497,
        "fire resistance totem" => 8184,
        "stoneskin totem" => 10408,
        "earthbind totem" => 2484,
        "flametongue totem" => 16387,

        // ── Warlock ──────────────────────────────────────────────
        "shadow bolt" => 25307,
        "immolate" => 11672,
        "corruption" => 25311,
        "curse of agony" => 11722,
        "curse of elements" => 17937,
        "curse of weakness" => 11707,
        "curse of tongues" => 11719,
        "drain life" => 11699,
        "drain soul" => 11676,
        "life tap" => 11689,
        "dark pact" => 18220,
        "fear" => 6215,
        "howl of terror" => 17925,
        "death coil" => 6789,
        "conflagrate" => 11772,
        "shadowburn" => 18876,
        "rain of fire" => 11688,
        "hellfire" => 11683,
        "soul fire" => 6353,
        "banish" => 18647,
        "health funnel" => 11700,
        "spell lock" => 19244,
        "seduction" => 6358,

        // ── Utility / misc ───────────────────────────────────────
        "cleanse" => 4987,
        "purify" => 1152,
        "abolish disease" => 552,
        "dispersion" => 47585,

        _ => return None,
    };
    Some(SpellId(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_common_names() {
        assert_eq!(lookup_spell_by_name("taunt"), Some(SpellId(355)));
        assert_eq!(lookup_spell_by_name("Taunt"), Some(SpellId(355)));
        assert_eq!(lookup_spell_by_name("MORTAL STRIKE"), Some(SpellId(21553)));
        assert_eq!(lookup_spell_by_name("mortal_strike"), Some(SpellId(21553)));
        assert_eq!(lookup_spell_by_name("bubble"), Some(SpellId(642)));
        // RTSC spell
        assert_eq!(lookup_spell_by_name("aedm"), Some(SpellId(30758)));
        // RaidControl commonly-used spells
        assert_eq!(lookup_spell_by_name("Sunder Armor"), Some(SpellId(11597)));
        assert_eq!(lookup_spell_by_name("Blizzard"), Some(SpellId(10187)));
        assert_eq!(lookup_spell_by_name("Volley"), Some(SpellId(14295)));
        assert_eq!(lookup_spell_by_name("Multishot"), Some(SpellId(14290)));
        assert_eq!(lookup_spell_by_name("Multi Shot"), Some(SpellId(14290)));
        assert_eq!(
            lookup_spell_by_name("Tranquilizing Shot"),
            Some(SpellId(19801))
        );
        assert_eq!(
            lookup_spell_by_name("Challenging Shout"),
            Some(SpellId(1161))
        );
        // Falls through to CastByName at runtime (FFI resolution)
        assert_eq!(lookup_spell_by_name("nonsense"), None);
    }
}
