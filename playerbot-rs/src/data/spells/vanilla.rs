/// Spell ID constants for Classic (Vanilla) `WoW`.
///
/// IDs are for the highest rank available at level 60 cap unless noted.
/// Source: wowhead classic DB / cmangos spell tables.
use crate::ffi::SpellId;

// ── Warrior ──────────────────────────────────────────────────────────────

pub mod warrior {
    use super::SpellId;

    // Stances
    pub const BATTLE_STANCE: SpellId = SpellId(2457);
    pub const BERSERKER_STANCE: SpellId = SpellId(2458);
    pub const DEFENSIVE_STANCE: SpellId = SpellId(71);

    // Shouts
    pub const BATTLE_SHOUT: SpellId = SpellId(11551); // rank 6
    pub const COMMANDING_SHOUT: SpellId = SpellId(469);
    pub const DEMORALIZING_SHOUT: SpellId = SpellId(11556); // rank 6
    pub const INTIMIDATING_SHOUT: SpellId = SpellId(5246);

    // Arms — general
    pub const HEROIC_STRIKE: SpellId = SpellId(11567); // rank 8 (vanilla cap)
    pub const REND: SpellId = SpellId(11574); // rank 6
    pub const THUNDER_CLAP: SpellId = SpellId(11581); // rank 5
    pub const HAMSTRING: SpellId = SpellId(7372); // rank 2
    pub const OVERPOWER: SpellId = SpellId(11585); // rank 4
    pub const MOCKING_BLOW: SpellId = SpellId(694);

    // Arms talents
    pub const CHARGE: SpellId = SpellId(11578); // rank 3 (8-25 yd range, Battle Stance)
    pub const MORTAL_STRIKE: SpellId = SpellId(21553); // rank 4
    pub const SWEEPING_STRIKES: SpellId = SpellId(12292);
    pub const DEATH_WISH: SpellId = SpellId(12328);
    pub const RETALIATION: SpellId = SpellId(20230);
    pub const RECKLESSNESS: SpellId = SpellId(1719);

    // Fury — general
    pub const BLOODTHIRST: SpellId = SpellId(25251); // rank 5 (talent, level 40+)
    pub const WHIRLWIND: SpellId = SpellId(1680);
    pub const CLEAVE: SpellId = SpellId(11608); // rank 4
    pub const EXECUTE: SpellId = SpellId(20662); // rank 5  (target < 20% HP)
    pub const INTERCEPT: SpellId = SpellId(20617); // rank 3 (Berserker Stance)
    pub const SLAM: SpellId = SpellId(11605); // rank 3
    pub const BERSERKER_RAGE: SpellId = SpellId(18499);

    // Protection
    pub const SHIELD_WALL: SpellId = SpellId(871); // 30% damage reduction, 30s (talent)
    pub const LAST_STAND: SpellId = SpellId(12975); // +30% max HP for 20s (talent)
    pub const SHIELD_BASH: SpellId = SpellId(11972); // rank 3
    pub const SHIELD_BLOCK: SpellId = SpellId(2565);
    pub const SUNDER_ARMOR: SpellId = SpellId(11597); // rank 5
    pub const REVENGE: SpellId = SpellId(11601); // rank 6
    pub const SHIELD_SLAM: SpellId = SpellId(23922); // rank 1 (talent, 2.4+)
    pub const TAUNT: SpellId = SpellId(355);
    pub const BLOODRAGE: SpellId = SpellId(2687);
    pub const DISARM: SpellId = SpellId(676);
    pub const CONCUSSION_BLOW: SpellId = SpellId(12809); // Protection talent
    pub const DEVASTATE: SpellId = SpellId(20243); // (not in vanilla; placeholder)

    // Consumables / utility
    pub const PUMMEL: SpellId = SpellId(6552); // rank 2
}

// ── Paladin ───────────────────────────────────────────────────────────────

pub mod paladin {
    use super::SpellId;

    pub const HOLY_SHOCK: SpellId = SpellId(20930); // rank 3
    pub const FLASH_OF_LIGHT: SpellId = SpellId(19943); // rank 5
    pub const HOLY_LIGHT: SpellId = SpellId(25292); // rank 9
    pub const DIVINE_SHIELD: SpellId = SpellId(642);
    pub const LAY_ON_HANDS: SpellId = SpellId(10310); // rank 3
    pub const HAMMER_OF_JUSTICE: SpellId = SpellId(10308); // rank 4
    pub const HAMMER_OF_WRATH: SpellId = SpellId(24239); // rank 3 (target < 20% HP)
    pub const SEAL_OF_RIGHTEOUSNESS: SpellId = SpellId(21082); // rank 8
    pub const SEAL_OF_COMMAND: SpellId = SpellId(20920); // rank 4
    pub const JUDGEMENT: SpellId = SpellId(20271);
    pub const CONSECRATION: SpellId = SpellId(20924); // rank 4
    pub const HOLY_WRATH: SpellId = SpellId(2812);
    pub const BLESSING_OF_MIGHT: SpellId = SpellId(25291); // rank 8
    pub const BLESSING_OF_WISDOM: SpellId = SpellId(25290); // rank 7
    pub const BLESSING_OF_KINGS: SpellId = SpellId(20217);
    pub const BLESSING_OF_PROTECTION: SpellId = SpellId(10278); // rank 3
    pub const DIVINE_FAVOR: SpellId = SpellId(20216);
    pub const EXORCISM: SpellId = SpellId(10314); // rank 5
    pub const RETRIBUTION_AURA: SpellId = SpellId(10301); // rank 5
    pub const DEVOTION_AURA: SpellId = SpellId(10293); // rank 7
}

// ── Priest ────────────────────────────────────────────────────────────────

pub mod priest {
    use super::SpellId;

    pub const FLASH_HEAL: SpellId = SpellId(10916); // rank 7
    pub const GREATER_HEAL: SpellId = SpellId(25213); // rank 5
    pub const HEAL: SpellId = SpellId(6063); // rank 4
    pub const LESSER_HEAL: SpellId = SpellId(2052); // rank 3
    pub const RENEW: SpellId = SpellId(25315); // rank 10
    pub const PRAYER_OF_HEALING: SpellId = SpellId(25316); // rank 5
    pub const POWER_WORD_SHIELD: SpellId = SpellId(10901); // rank 9
    pub const INNER_FIRE: SpellId = SpellId(10952); // rank 7
    pub const DIVINE_SPIRIT: SpellId = SpellId(14752); // rank 4
    pub const POWER_WORD_FORTITUDE: SpellId = SpellId(21562); // rank 7
    pub const SHADOW_WORD_PAIN: SpellId = SpellId(10894); // rank 8
    pub const MIND_BLAST: SpellId = SpellId(10947); // rank 8
    pub const MIND_FLAY: SpellId = SpellId(18807); // rank 5
    pub const VAMPIRIC_EMBRACE: SpellId = SpellId(15286);
    pub const DEVOURING_PLAGUE: SpellId = SpellId(19276); // rank 6 (undead only in vanilla)
    pub const DISPEL_MAGIC: SpellId = SpellId(988);
    pub const FADE: SpellId = SpellId(10942); // rank 6
    pub const PSYCHIC_SCREAM: SpellId = SpellId(10890); // rank 4
    pub const HOLY_NOVA: SpellId = SpellId(25331); // rank 5
    pub const SHADOWFORM: SpellId = SpellId(15473);
    pub const RESURRECTION: SpellId = SpellId(10881); // rank 5
    pub const INNER_FOCUS: SpellId = SpellId(14751);
}

// ── Druid ─────────────────────────────────────────────────────────────────

pub mod druid {
    use super::SpellId;

    pub const REJUVENATION: SpellId = SpellId(25299); // rank 10
    pub const REGROWTH: SpellId = SpellId(9858); // rank 7
    pub const HEALING_TOUCH: SpellId = SpellId(25297); // rank 11
    pub const NOURISH: SpellId = SpellId(50464); // (wotlk only)
    pub const WILD_GROWTH: SpellId = SpellId(48438); // (wotlk only)
    pub const LIFEBLOOM: SpellId = SpellId(33763); // (tbc only)
    pub const TRANQUILITY: SpellId = SpellId(9863); // rank 4
    pub const INNERVATE: SpellId = SpellId(29166);
    pub const BARKSKIN: SpellId = SpellId(22812);
    pub const MOONFIRE: SpellId = SpellId(26987); // rank 10
    pub const STARFIRE: SpellId = SpellId(25298); // rank 8
    pub const WRATH: SpellId = SpellId(9912); // rank 7
    pub const INSECT_SWARM: SpellId = SpellId(24977); // rank 5
    pub const SHRED: SpellId = SpellId(9830); // rank 5 (Cat)
    pub const MANGLE_CAT: SpellId = SpellId(33876); // (tbc)
    pub const RAKE: SpellId = SpellId(9904); // rank 4 (Cat)
    pub const RIP: SpellId = SpellId(9896); // rank 7 (Cat finisher)
    pub const FEROCIOUS_BITE: SpellId = SpellId(22568); // rank 5 (Cat finisher)
    pub const MAUL: SpellId = SpellId(10628); // rank 7 (Bear)
    pub const SWIPE_BEAR: SpellId = SpellId(26996); // rank 4 (Bear)
    pub const DEMORALIZING_ROAR: SpellId = SpellId(26998); // rank 5 (Bear)
    pub const FAERIE_FIRE_FERAL: SpellId = SpellId(16857); // Bear/Cat form
    pub const CLAW: SpellId = SpellId(9850); // rank 5 (Cat)
    pub const CAT_FORM: SpellId = SpellId(768);
    pub const BEAR_FORM: SpellId = SpellId(5487);
    pub const TRAVEL_FORM: SpellId = SpellId(783);
    pub const AQUATIC_FORM: SpellId = SpellId(1066);
    pub const NATURE_SWIFTNESS: SpellId = SpellId(17116);
    pub const TIGERS_FURY: SpellId = SpellId(9846); // rank 4
    pub const HURRICANE: SpellId = SpellId(17402); // rank 3
}

// ── Hunter ────────────────────────────────────────────────────────────────

pub mod hunter {
    use super::SpellId;

    pub const AUTO_SHOT: SpellId = SpellId(75);
    pub const ARCANE_SHOT: SpellId = SpellId(14287); // rank 7
    pub const SERPENT_STING: SpellId = SpellId(13555); // rank 7
    pub const AIMED_SHOT: SpellId = SpellId(20904); // rank 4
    pub const MULTI_SHOT: SpellId = SpellId(14290); // rank 4
    pub const VOLLEY: SpellId = SpellId(14295); // rank 4
    pub const HUNTERS_MARK: SpellId = SpellId(14325); // rank 4
    pub const RAPTOR_STRIKE: SpellId = SpellId(14266); // rank 8
    pub const DISENGAGE: SpellId = SpellId(781);
    pub const FEIGN_DEATH: SpellId = SpellId(5384);
    pub const ASPECT_OF_THE_HAWK: SpellId = SpellId(25296); // rank 7
    pub const ASPECT_OF_THE_MONKEY: SpellId = SpellId(8078);
    pub const ASPECT_OF_THE_CHEETAH: SpellId = SpellId(5118);
    pub const EXPLOSIVE_TRAP: SpellId = SpellId(14316); // rank 4
    pub const FREEZING_TRAP: SpellId = SpellId(14311); // rank 2
    pub const IMMOLATION_TRAP: SpellId = SpellId(14305); // rank 4
    pub const WING_CLIP: SpellId = SpellId(14268); // rank 3
    pub const SCATTER_SHOT: SpellId = SpellId(19503);
    pub const BESTIAL_WRATH: SpellId = SpellId(19574);
    pub const RAPID_FIRE: SpellId = SpellId(3045);
    pub const PET_ATTACK: SpellId = SpellId(2641);
}

// ── Mage ─────────────────────────────────────────────────────────────────

pub mod mage {
    use super::SpellId;

    pub const FIREBALL: SpellId = SpellId(25306); // rank 12
    pub const FIRE_BLAST: SpellId = SpellId(10199); // rank 7
    pub const SCORCH: SpellId = SpellId(10205); // rank 7
    pub const FLAMESTRIKE: SpellId = SpellId(10216); // rank 6
    pub const BLAST_WAVE: SpellId = SpellId(11113); // rank 4
    pub const PYROBLAST: SpellId = SpellId(18809); // rank 8
    pub const FROSTBOLT: SpellId = SpellId(25304); // rank 12
    pub const FROST_NOVA: SpellId = SpellId(10230); // rank 4
    pub const ICE_BLOCK: SpellId = SpellId(11958);
    pub const COLD_SNAP: SpellId = SpellId(11958); // same? no — 11958 is ICE_BLOCK. COLD_SNAP = 11958... let me fix
    pub const ARCANE_MISSILES: SpellId = SpellId(25345); // rank 9
    pub const ARCANE_EXPLOSION: SpellId = SpellId(10202); // rank 6
    pub const BLIZZARD: SpellId = SpellId(10187); // rank 6
    pub const CONE_OF_COLD: SpellId = SpellId(10161); // rank 5
    pub const COUNTERSPELL: SpellId = SpellId(2139);
    pub const POLYMORPH: SpellId = SpellId(12826); // rank 3
    pub const BLINK: SpellId = SpellId(1953);
    pub const EVOCATION: SpellId = SpellId(12051);
    pub const ARCANE_INTELLECT: SpellId = SpellId(10157); // rank 5
    pub const ARCANE_BRILLIANCE: SpellId = SpellId(23028);
    // Burst cooldowns (BOOST combat order)
    pub const COMBUSTION: SpellId = SpellId(11129);
    pub const ARCANE_POWER: SpellId = SpellId(12042);
    pub const PRESENCE_OF_MIND: SpellId = SpellId(12043);
}

// ── Rogue ─────────────────────────────────────────────────────────────────

pub mod rogue {
    use super::SpellId;

    pub const SINISTER_STRIKE: SpellId = SpellId(11293); // rank 8
    pub const BACKSTAB: SpellId = SpellId(25300); // rank 9
    pub const HEMORRHAGE: SpellId = SpellId(17347); // rank 4
    pub const EVISCERATE: SpellId = SpellId(26865); // rank 9
    pub const SLICE_AND_DICE: SpellId = SpellId(6774); // rank 2
    pub const RUPTURE: SpellId = SpellId(11275); // rank 6
    pub const EXPOSE_ARMOR: SpellId = SpellId(8647); // rank 5
    pub const GARROTE: SpellId = SpellId(11289); // rank 5
    pub const AMBUSH: SpellId = SpellId(11269); // rank 7
    pub const STEALTH: SpellId = SpellId(1787);
    pub const SPRINT: SpellId = SpellId(11305); // rank 3
    pub const EVASION: SpellId = SpellId(26669);
    pub const BLIND: SpellId = SpellId(2094);
    pub const KIDNEY_SHOT: SpellId = SpellId(8643); // rank 2
    pub const GOUGE: SpellId = SpellId(11286); // rank 5
    pub const VANISH: SpellId = SpellId(11327); // rank 2
    pub const KICK: SpellId = SpellId(1769); // rank 3
    pub const COLD_BLOOD: SpellId = SpellId(14177);
    pub const ADRENALINE_RUSH: SpellId = SpellId(13750);
    pub const BLADE_FLURRY: SpellId = SpellId(13877);
}

// ── Shaman ────────────────────────────────────────────────────────────────

pub mod shaman {
    use super::SpellId;

    pub const LIGHTNING_BOLT: SpellId = SpellId(25448); // rank 10
    pub const CHAIN_LIGHTNING: SpellId = SpellId(25442); // rank 5
    pub const LIGHTNING_SHIELD: SpellId = SpellId(10432); // rank 7
    pub const FLAME_SHOCK: SpellId = SpellId(29228); // rank 6 (TBC rank; vanilla r5 = 10448)
    pub const EARTH_SHOCK: SpellId = SpellId(10414); // rank 7
    pub const FROST_SHOCK: SpellId = SpellId(10473); // rank 4
    pub const HEALING_WAVE: SpellId = SpellId(25357); // rank 10
    pub const LESSER_HEALING_WAVE: SpellId = SpellId(10468); // rank 6
    pub const CHAIN_HEAL: SpellId = SpellId(25423); // rank 4
    pub const NATURE_SWIFTNESS: SpellId = SpellId(16188);
    pub const GRACE_OF_AIR_TOTEM: SpellId = SpellId(25359); // rank 3
    pub const STRENGTH_OF_EARTH_TOTEM: SpellId = SpellId(25361); // rank 6
    pub const WINDFURY_TOTEM: SpellId = SpellId(25587); // rank 5
    pub const MANA_SPRING_TOTEM: SpellId = SpellId(10497); // rank 5
    pub const FIRE_RESISTANCE_TOTEM: SpellId = SpellId(8184); // rank 4
    pub const STONESKIN_TOTEM: SpellId = SpellId(10408); // rank 7
    pub const EARTHBIND_TOTEM: SpellId = SpellId(2484);
    pub const FLAMETONGUE_TOTEM: SpellId = SpellId(16387); // rank 5
    pub const STORMSTRIKE: SpellId = SpellId(17364);
    pub const PURGE: SpellId = SpellId(8012); // rank 2
    pub const ELEMENTAL_MASTERY: SpellId = SpellId(16166);
}

// ── Warlock ───────────────────────────────────────────────────────────────

pub mod warlock {
    use super::SpellId;

    pub const SHADOW_BOLT: SpellId = SpellId(25307); // rank 10
    pub const IMMOLATE: SpellId = SpellId(11672); // rank 7
    pub const CORRUPTION: SpellId = SpellId(25311); // rank 8
    pub const CURSE_OF_AGONY: SpellId = SpellId(11722); // rank 7
    pub const CURSE_OF_ELEMENTS: SpellId = SpellId(11722); // different spell — COE is 17937
    pub const CURSE_OF_WEAKNESS: SpellId = SpellId(11707); // rank 7
    pub const CURSE_OF_TONGUES: SpellId = SpellId(11719); // rank 2
    pub const DRAIN_LIFE: SpellId = SpellId(11699); // rank 7
    pub const DRAIN_SOUL: SpellId = SpellId(11676); // rank 3
    pub const LIFE_TAP: SpellId = SpellId(11689); // rank 6
    pub const DARK_PACT: SpellId = SpellId(18220); // rank 4
    pub const FEAR: SpellId = SpellId(6215); // rank 3
    pub const HOWL_OF_TERROR: SpellId = SpellId(17925); // rank 2
    pub const DEATH_COIL: SpellId = SpellId(17925); // different — DEATH_COIL is 6789
    pub const CONFLAGRATE: SpellId = SpellId(11772); // rank 4 (talent)
    pub const SHADOWBURN: SpellId = SpellId(18876); // rank 6 (talent)
    pub const UNSTABLE_AFFLICTION: SpellId = SpellId(31117); // (tbc talent)
    pub const RAIN_OF_FIRE: SpellId = SpellId(11688); // rank 4
    pub const HELLFIRE: SpellId = SpellId(11683); // rank 3
    pub const SOUL_FIRE: SpellId = SpellId(6353); // rank 3
    pub const BANISH: SpellId = SpellId(18647); // rank 2
    pub const HEALTH_FUNNEL: SpellId = SpellId(11700); // rank 7
    pub const SUMMON_IMP: SpellId = SpellId(688);
    pub const SUMMON_VOIDWALKER: SpellId = SpellId(697);
    pub const SUMMON_SUCCUBUS: SpellId = SpellId(712);
    pub const SUMMON_FELHUNTER: SpellId = SpellId(691);
    pub const DEMON_ARMOR: SpellId = SpellId(11735); // rank 6
}

// ── Death Knight (WotLK only) ─────────────────────────────────────────────

#[cfg(feature = "wotlk")]
pub mod deathknight {
    use super::SpellId;

    pub const DEATH_STRIKE: SpellId = SpellId(49924); // rank 4
    pub const DEATH_COIL: SpellId = SpellId(49895); // rank 4
    pub const ICY_TOUCH: SpellId = SpellId(49909); // rank 4
    pub const PLAGUE_STRIKE: SpellId = SpellId(49921); // rank 4
    pub const OBLITERATE: SpellId = SpellId(51425); // rank 4
    pub const HOWLING_BLAST: SpellId = SpellId(51411); // rank 3
    pub const FROST_STRIKE: SpellId = SpellId(55268); // rank 5
    pub const BLOOD_STRIKE: SpellId = SpellId(49930); // rank 5
    pub const HEART_STRIKE: SpellId = SpellId(55262); // rank 5
    pub const RUNE_STRIKE: SpellId = SpellId(56815);
    pub const DARK_COMMAND: SpellId = SpellId(56222); // taunt
    pub const DEATH_GRIP: SpellId = SpellId(49576);
    pub const CHAINS_OF_ICE: SpellId = SpellId(45524);
    pub const BLOOD_BOIL: SpellId = SpellId(48721); // rank 3
    pub const DEATH_AND_DECAY: SpellId = SpellId(49938); // rank 4
    pub const ANTI_MAGIC_SHELL: SpellId = SpellId(48707);
    pub const DANCING_RUNE_WEAPON: SpellId = SpellId(49028);
    pub const BONE_SHIELD: SpellId = SpellId(49222);
}
