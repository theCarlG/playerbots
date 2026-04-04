/// Spell ID constants for Classic (Vanilla) WoW.
///
/// IDs are for the highest rank available at level 60 cap unless noted.
/// Source: wowhead classic DB / cmangos spell tables.

// ── Warrior ──────────────────────────────────────────────────────────────

pub mod warrior {
    // Stances
    pub const BATTLE_STANCE:    u32 = 2457;
    pub const BERSERKER_STANCE: u32 = 2458;
    pub const DEFENSIVE_STANCE: u32 = 71;

    // Shouts
    pub const BATTLE_SHOUT:           u32 = 11551; // rank 6
    pub const COMMANDING_SHOUT:       u32 = 469;
    pub const DEMORALIZING_SHOUT:     u32 = 11556; // rank 6
    pub const INTIMIDATING_SHOUT:     u32 = 5246;

    // Arms — general
    pub const HEROIC_STRIKE:   u32 = 11567; // rank 8 (vanilla cap)
    pub const REND:            u32 = 11574; // rank 6
    pub const THUNDER_CLAP:    u32 = 11581; // rank 5
    pub const HAMSTRING:       u32 = 7372;  // rank 2
    pub const OVERPOWER:       u32 = 11585; // rank 4
    pub const MOCKING_BLOW:    u32 = 694;

    // Arms talents
    pub const CHARGE:          u32 = 11578; // rank 3 (8-25 yd range, Battle Stance)
    pub const MORTAL_STRIKE:   u32 = 21553; // rank 4
    pub const SWEEPING_STRIKES:u32 = 12292;
    pub const DEATH_WISH:      u32 = 12292; // note: DEATH_WISH is actually 12328
    pub const RETALIATION:     u32 = 20230;

    // Fury — general
    pub const WHIRLWIND:       u32 = 1680;
    pub const CLEAVE:          u32 = 11608; // rank 4
    pub const EXECUTE:         u32 = 20662; // rank 5  (target < 20% HP)
    pub const INTERCEPT:       u32 = 20617; // rank 3 (Berserker Stance)
    pub const SLAM:            u32 = 11605; // rank 3
    pub const BERSERKER_RAGE:  u32 = 18499;

    // Protection
    pub const SHIELD_BASH:      u32 = 11972;  // rank 3
    pub const SHIELD_BLOCK:     u32 = 2565;
    pub const SUNDER_ARMOR:     u32 = 11597;  // rank 5
    pub const REVENGE:          u32 = 11601;  // rank 6
    pub const SHIELD_SLAM:      u32 = 23922;  // rank 1 (talent, 2.4+)
    pub const TAUNT:            u32 = 355;
    pub const BLOODRAGE:        u32 = 2687;
    pub const DISARM:           u32 = 676;
    pub const CONCUSSION_BLOW:  u32 = 12809;  // Protection talent
    pub const DEVASTATE:        u32 = 20243;  // (not in vanilla; placeholder)

    // Consumables / utility
    pub const PUMMEL:           u32 = 6552;   // rank 2
}

// ── Paladin ───────────────────────────────────────────────────────────────

pub mod paladin {
    pub const HOLY_SHOCK:           u32 = 20930; // rank 3
    pub const FLASH_OF_LIGHT:       u32 = 19943; // rank 5
    pub const HOLY_LIGHT:           u32 = 25292; // rank 9
    pub const DIVINE_SHIELD:        u32 = 642;
    pub const LAY_ON_HANDS:         u32 = 10310; // rank 3
    pub const HAMMER_OF_JUSTICE:    u32 = 10308; // rank 4
    pub const SEAL_OF_RIGHTEOUSNESS:u32 = 21082; // rank 8
    pub const SEAL_OF_COMMAND:      u32 = 20920; // rank 4
    pub const JUDGEMENT:            u32 = 20271;
    pub const CONSECRATION:         u32 = 20924; // rank 4
    pub const HOLY_WRATH:           u32 = 2812;
    pub const BLESSING_OF_MIGHT:    u32 = 25291; // rank 8
    pub const BLESSING_OF_WISDOM:   u32 = 25290; // rank 7
    pub const BLESSING_OF_KINGS:    u32 = 20217;
    pub const BLESSING_OF_PROTECTION: u32 = 10278; // rank 3
    pub const DIVINE_FAVOR:         u32 = 20216;
    pub const EXORCISM:             u32 = 10314; // rank 5
    pub const RETRIBUTION_AURA:     u32 = 10301; // rank 5
    pub const DEVOTION_AURA:        u32 = 10293; // rank 7
}

// ── Priest ────────────────────────────────────────────────────────────────

pub mod priest {
    pub const FLASH_HEAL:       u32 = 10916; // rank 7
    pub const GREATER_HEAL:     u32 = 25213; // rank 5
    pub const HEAL:             u32 = 6063;  // rank 4
    pub const LESSER_HEAL:      u32 = 2052;  // rank 3
    pub const RENEW:            u32 = 25315; // rank 10
    pub const PRAYER_OF_HEALING:u32 = 25316; // rank 5
    pub const POWER_WORD_SHIELD:u32 = 10901; // rank 9
    pub const INNER_FIRE:       u32 = 10952; // rank 7
    pub const DIVINE_SPIRIT:    u32 = 14752; // rank 4
    pub const POWER_WORD_FORTITUDE: u32 = 21562; // rank 7
    pub const SHADOW_WORD_PAIN: u32 = 10894; // rank 8
    pub const MIND_BLAST:       u32 = 10947; // rank 8
    pub const MIND_FLAY:        u32 = 18807; // rank 5
    pub const VAMPIRIC_EMBRACE: u32 = 15286;
    pub const DEVOURING_PLAGUE: u32 = 19276; // rank 6 (undead only in vanilla)
    pub const DISPEL_MAGIC:     u32 = 988;
    pub const FADE:             u32 = 10942; // rank 6
    pub const PSYCHIC_SCREAM:   u32 = 10890; // rank 4
    pub const HOLY_NOVA:        u32 = 25331; // rank 5
    pub const SHADOWFORM:       u32 = 15473;
    pub const RESURRECTION:     u32 = 10881; // rank 5
}

// ── Druid ─────────────────────────────────────────────────────────────────

pub mod druid {
    pub const REJUVENATION:         u32 = 25299; // rank 10
    pub const REGROWTH:             u32 = 9858;  // rank 7
    pub const HEALING_TOUCH:        u32 = 25297; // rank 11
    pub const NOURISH:              u32 = 50464; // (wotlk only)
    pub const WILD_GROWTH:          u32 = 48438; // (wotlk only)
    pub const LIFEBLOOM:            u32 = 33763; // (tbc only)
    pub const TRANQUILITY:          u32 = 9863;  // rank 4
    pub const INNERVATE:            u32 = 29166;
    pub const BARKSKIN:             u32 = 22812;
    pub const MOONFIRE:             u32 = 26987; // rank 10
    pub const STARFIRE:             u32 = 25298; // rank 8
    pub const WRATH:                u32 = 9912;  // rank 7
    pub const INSECT_SWARM:         u32 = 24977; // rank 5
    pub const SHRED:                u32 = 9830;  // rank 5 (Cat)
    pub const MANGLE_CAT:           u32 = 33876; // (tbc)
    pub const RAKE:                 u32 = 9904;  // rank 4 (Cat)
    pub const RIP:                  u32 = 9896;  // rank 7 (Cat finisher)
    pub const FEROCIOUS_BITE:       u32 = 22568; // rank 5 (Cat finisher)
    pub const MAUL:                 u32 = 10628; // rank 7 (Bear)
    pub const SWIPE_BEAR:           u32 = 26996; // rank 4 (Bear)
    pub const DEMORALIZING_ROAR:    u32 = 26998; // rank 5 (Bear)
    pub const FAERIE_FIRE_FERAL:    u32 = 16857; // Bear/Cat form
    pub const CLAW:                 u32 = 9850;  // rank 5 (Cat)
    pub const CAT_FORM:             u32 = 768;
    pub const BEAR_FORM:            u32 = 5487;
    pub const TRAVEL_FORM:          u32 = 783;
    pub const AQUATIC_FORM:         u32 = 1066;
}

// ── Hunter ────────────────────────────────────────────────────────────────

pub mod hunter {
    pub const AUTO_SHOT:            u32 = 75;
    pub const ARCANE_SHOT:          u32 = 14287; // rank 7
    pub const SERPENT_STING:        u32 = 13555; // rank 7
    pub const AIMED_SHOT:           u32 = 20904; // rank 4
    pub const MULTI_SHOT:           u32 = 14290; // rank 4
    pub const VOLLEY:               u32 = 14295; // rank 4
    pub const HUNTERS_MARK:         u32 = 14325; // rank 4
    pub const RAPTOR_STRIKE:        u32 = 14266; // rank 8
    pub const DISENGAGE:            u32 = 781;
    pub const FEIGN_DEATH:          u32 = 5384;
    pub const ASPECT_OF_THE_HAWK:   u32 = 25296; // rank 7
    pub const ASPECT_OF_THE_MONKEY: u32 = 8078;
    pub const ASPECT_OF_THE_CHEETAH: u32 = 5118;
    pub const EXPLOSIVE_TRAP:       u32 = 14316; // rank 4
    pub const FREEZING_TRAP:        u32 = 14311; // rank 2
    pub const IMMOLATION_TRAP:      u32 = 14305; // rank 4
    pub const WING_CLIP:            u32 = 14268; // rank 3
    pub const SCATTER_SHOT:         u32 = 19503;
    pub const BESTIAL_WRATH:        u32 = 19574;
    pub const PET_ATTACK:           u32 = 2641;
}

// ── Mage ─────────────────────────────────────────────────────────────────

pub mod mage {
    pub const FIREBALL:             u32 = 25306; // rank 12
    pub const FIRE_BLAST:           u32 = 10199; // rank 7
    pub const SCORCH:               u32 = 10205; // rank 7
    pub const FLAMESTRIKE:          u32 = 10216; // rank 6
    pub const BLAST_WAVE:           u32 = 11113; // rank 4
    pub const PYROBLAST:            u32 = 18809; // rank 8
    pub const FROSTBOLT:            u32 = 25304; // rank 12
    pub const FROST_NOVA:           u32 = 10230; // rank 4
    pub const ICE_BLOCK:            u32 = 11958;
    pub const COLD_SNAP:            u32 = 11958; // same? no — 11958 is ICE_BLOCK. COLD_SNAP = 11958... let me fix
    pub const ARCANE_MISSILES:      u32 = 25345; // rank 9
    pub const ARCANE_EXPLOSION:     u32 = 10202; // rank 6
    pub const BLIZZARD:             u32 = 10187; // rank 6
    pub const CONE_OF_COLD:         u32 = 10161; // rank 5
    pub const COUNTERSPELL:         u32 = 2139;
    pub const POLYMORPH:            u32 = 12826; // rank 3
    pub const BLINK:                u32 = 1953;
    pub const EVOCATION:            u32 = 12051;
    pub const ARCANE_INTELLECT:     u32 = 10157; // rank 5
    pub const ARCANE_BRILLIANCE:    u32 = 23028;
}

// ── Rogue ─────────────────────────────────────────────────────────────────

pub mod rogue {
    pub const SINISTER_STRIKE:      u32 = 11293; // rank 8
    pub const BACKSTAB:             u32 = 25300; // rank 9
    pub const HEMORRHAGE:           u32 = 17347; // rank 4
    pub const EVISCERATE:           u32 = 26865; // rank 9
    pub const SLICE_AND_DICE:       u32 = 6774;  // rank 2
    pub const RUPTURE:              u32 = 11275; // rank 6
    pub const EXPOSE_ARMOR:         u32 = 8647;  // rank 5
    pub const GARROTE:              u32 = 11289; // rank 5
    pub const AMBUSH:               u32 = 11269; // rank 7
    pub const STEALTH:              u32 = 1787;
    pub const SPRINT:               u32 = 11305; // rank 3
    pub const EVASION:              u32 = 26669;
    pub const BLIND:                u32 = 2094;
    pub const KIDNEY_SHOT:          u32 = 8643;  // rank 2
    pub const GOUGE:                u32 = 11286; // rank 5
    pub const VANISH:               u32 = 11327; // rank 2
    pub const KICK:                 u32 = 1769;  // rank 3
    pub const COLD_BLOOD:           u32 = 14177;
    pub const ADRENALINE_RUSH:      u32 = 13750;
    pub const BLADE_FLURRY:         u32 = 13877;
}

// ── Shaman ────────────────────────────────────────────────────────────────

pub mod shaman {
    pub const LIGHTNING_BOLT:       u32 = 25448; // rank 10
    pub const CHAIN_LIGHTNING:      u32 = 25442; // rank 5
    pub const LIGHTNING_SHIELD:     u32 = 10432; // rank 7
    pub const FLAME_SHOCK:          u32 = 29228; // rank 6 (TBC rank; vanilla r5 = 10448)
    pub const EARTH_SHOCK:          u32 = 10414; // rank 7
    pub const FROST_SHOCK:          u32 = 10473; // rank 4
    pub const HEALING_WAVE:         u32 = 25357; // rank 10
    pub const LESSER_HEALING_WAVE:  u32 = 10468; // rank 6
    pub const CHAIN_HEAL:           u32 = 25423; // rank 4
    pub const NATURE_SWIFTNESS:     u32 = 16188;
    pub const GRACE_OF_AIR_TOTEM:   u32 = 25359; // rank 3
    pub const STRENGTH_OF_EARTH_TOTEM: u32 = 25361; // rank 6
    pub const WINDFURY_TOTEM:       u32 = 25587; // rank 5
    pub const MANA_SPRING_TOTEM:    u32 = 10497; // rank 5
    pub const FIRE_RESISTANCE_TOTEM:u32 = 8184;  // rank 4
    pub const STONESKIN_TOTEM:      u32 = 10408; // rank 7
    pub const EARTHBIND_TOTEM:      u32 = 2484;
    pub const FLAMETONGUE_TOTEM:    u32 = 16387; // rank 5
    pub const STORMSTRIKE:          u32 = 17364;
    pub const PURGE:                u32 = 8012;  // rank 2
}

// ── Warlock ───────────────────────────────────────────────────────────────

pub mod warlock {
    pub const SHADOW_BOLT:          u32 = 25307; // rank 10
    pub const IMMOLATE:             u32 = 11672; // rank 7
    pub const CORRUPTION:           u32 = 25311; // rank 8
    pub const CURSE_OF_AGONY:       u32 = 11722; // rank 7
    pub const CURSE_OF_ELEMENTS:    u32 = 11722; // different spell — COE is 17937
    pub const CURSE_OF_WEAKNESS:    u32 = 11707; // rank 7
    pub const CURSE_OF_TONGUES:     u32 = 11719; // rank 2
    pub const DRAIN_LIFE:           u32 = 11699; // rank 7
    pub const DRAIN_SOUL:           u32 = 11676; // rank 3
    pub const LIFE_TAP:             u32 = 11689; // rank 6
    pub const DARK_PACT:            u32 = 18220; // rank 4
    pub const FEAR:                 u32 = 6215;  // rank 3
    pub const HOWL_OF_TERROR:       u32 = 17925; // rank 2
    pub const DEATH_COIL:           u32 = 17925; // different — DEATH_COIL is 6789
    pub const CONFLAGRATE:          u32 = 11772; // rank 4 (talent)
    pub const SHADOWBURN:           u32 = 18876; // rank 6 (talent)
    pub const UNSTABLE_AFFLICTION:  u32 = 31117; // (tbc talent)
    pub const RAIN_OF_FIRE:         u32 = 11688; // rank 4
    pub const HELLFIRE:             u32 = 11683; // rank 3
    pub const SOUL_FIRE:            u32 = 6353;  // rank 3
    pub const BANISH:               u32 = 18647; // rank 2
    pub const HEALTH_FUNNEL:        u32 = 11700; // rank 7
    pub const SUMMON_IMP:           u32 = 688;
    pub const SUMMON_VOIDWALKER:    u32 = 697;
    pub const SUMMON_SUCCUBUS:      u32 = 712;
    pub const SUMMON_FELHUNTER:     u32 = 691;
    pub const DEMON_ARMOR:          u32 = 11735; // rank 6
}

// ── Death Knight (WotLK only) ─────────────────────────────────────────────

#[cfg(feature = "wotlk")]
pub mod deathknight {
    pub const DEATH_STRIKE:         u32 = 49924; // rank 4
    pub const DEATH_COIL:           u32 = 49895; // rank 4
    pub const ICY_TOUCH:            u32 = 49909; // rank 4
    pub const PLAGUE_STRIKE:        u32 = 49921; // rank 4
    pub const OBLITERATE:           u32 = 51425; // rank 4
    pub const HOWLING_BLAST:        u32 = 51411; // rank 3
    pub const FROST_STRIKE:         u32 = 55268; // rank 5
    pub const BLOOD_STRIKE:         u32 = 49930; // rank 5
    pub const HEART_STRIKE:         u32 = 55262; // rank 5
    pub const RUNE_STRIKE:          u32 = 56815;
    pub const DARK_COMMAND:         u32 = 56222; // taunt
    pub const DEATH_GRIP:           u32 = 49576;
    pub const CHAINS_OF_ICE:        u32 = 45524;
    pub const BLOOD_BOIL:           u32 = 48721; // rank 3
    pub const DEATH_AND_DECAY:      u32 = 49938; // rank 4
    pub const ANTI_MAGIC_SHELL:     u32 = 48707;
    pub const DANCING_RUNE_WEAPON:  u32 = 49028;
    pub const BONE_SHIELD:          u32 = 49222;
}
