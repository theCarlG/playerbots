/// Spell ID constants introduced in Wrath of the Lich King (patch 3.x).
///
/// As with `tbc.rs`, older ranks remain addressable via `spells::vanilla`.
/// This module lists only abilities first available in WotLK — most
/// notably the entire Death Knight class, plus new utility/cooldowns that
/// rotation trees branch on.
use crate::ffi::SpellId;

// ── Death Knight (new class in WotLK) ───────────────────────────────────
// These are the rotation-critical DK IDs the class trees reference. The
// full table including talent-specific strikes lives inline in each class
// file under `cfg(feature = "wotlk")`.
pub mod deathknight {
    use crate::ffi::SpellId;

    // Diseases
    pub const ICY_TOUCH:      SpellId = SpellId(45477);
    pub const PLAGUE_STRIKE:  SpellId = SpellId(45462);
    pub const FROST_FEVER:    SpellId = SpellId(55095);
    pub const BLOOD_PLAGUE:   SpellId = SpellId(55078);

    // Strikes
    pub const DEATH_STRIKE:   SpellId = SpellId(49998);
    pub const OBLITERATE:     SpellId = SpellId(49020);
    pub const SCOURGE_STRIKE: SpellId = SpellId(55271);
    pub const HEART_STRIKE:   SpellId = SpellId(55050);
    pub const FROST_STRIKE:   SpellId = SpellId(49143);

    // Runic power / AoE
    pub const DEATH_COIL:      SpellId = SpellId(47541);
    pub const DEATH_AND_DECAY: SpellId = SpellId(43265);
    pub const BLOOD_BOIL:      SpellId = SpellId(48721);
    pub const HOWLING_BLAST:   SpellId = SpellId(49184);

    // Utility / cooldowns
    pub const DEATH_GRIP:      SpellId = SpellId(49576);
    pub const BONE_SHIELD:     SpellId = SpellId(49222);
    pub const ICEBOUND_FORTITUDE: SpellId = SpellId(48792);
    pub const ANTI_MAGIC_SHELL:   SpellId = SpellId(48707);
    pub const MIND_FREEZE:     SpellId = SpellId(47528);
    pub const DARK_COMMAND:    SpellId = SpellId(56222);
}

// ── Shaman ──────────────────────────────────────────────────────────────
pub const HEX:             SpellId = SpellId(51514);
pub const RIPTIDE:         SpellId = SpellId(61295);
pub const THUNDERSTORM:    SpellId = SpellId(51490);

// ── Paladin ─────────────────────────────────────────────────────────────
pub const HAMMER_OF_THE_RIGHTEOUS: SpellId = SpellId(53595);
pub const HAND_OF_RECKONING:       SpellId = SpellId(62124);
pub const DIVINE_PLEA:             SpellId = SpellId(54428);

// ── Druid ───────────────────────────────────────────────────────────────
pub const WILD_GROWTH:     SpellId = SpellId(48438);
pub const NOURISH:         SpellId = SpellId(50464);
pub const BERSERK_DRUID:   SpellId = SpellId(50334);

// ── Priest ──────────────────────────────────────────────────────────────
pub const PENANCE:         SpellId = SpellId(47540);
pub const DIVINE_HYMN:     SpellId = SpellId(64843);
pub const GUARDIAN_SPIRIT: SpellId = SpellId(47788);

// ── Mage ────────────────────────────────────────────────────────────────
pub const DEEP_FREEZE:     SpellId = SpellId(44572);
pub const LIVING_BOMB:     SpellId = SpellId(44457);
pub const FROSTFIRE_BOLT:  SpellId = SpellId(44614);

// ── Warlock ─────────────────────────────────────────────────────────────
pub const HAUNT:           SpellId = SpellId(48181);
pub const CHAOS_BOLT:      SpellId = SpellId(50796);
pub const DEMONIC_CIRCLE:  SpellId = SpellId(48018);

// ── Hunter ──────────────────────────────────────────────────────────────
pub const EXPLOSIVE_SHOT:  SpellId = SpellId(53301);
pub const BLACK_ARROW:     SpellId = SpellId(3674);
pub const CHIMERA_SHOT:    SpellId = SpellId(53209);

// ── Rogue ───────────────────────────────────────────────────────────────
pub const FAN_OF_KNIVES:   SpellId = SpellId(51723);
pub const MUTILATE:        SpellId = SpellId(1329);
pub const SHADOWSTEP:      SpellId = SpellId(36554);

// ── Warrior ─────────────────────────────────────────────────────────────
pub const SHATTERING_THROW: SpellId = SpellId(64382);
pub const BLADESTORM:       SpellId = SpellId(46924);
