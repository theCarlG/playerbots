//! Class/race availability table and the `NameRaceAndGender` enum.
//!
//! Pure const data. The table is ported verbatim from
//! `playerbot/RandomPlayerbotFactory.cpp::RandomPlayerbotFactory(uint32)`,
//! which populates a `std::map<uint8, std::vector<uint8>>` at first
//! construction and then reuses it for every subsequent call. On the Rust
//! side the table is a compile-time `const` indexed by class id and race
//! id, gated by the expansion feature flag so `MANGOSBOT_TWO`-only races
//! (blood elf, draenei, death knight support) are only present when the
//! `wotlk` / `tbc` feature is active.
//!
//! The `CMaNGOS` core uses `RACE_*` values that are not contiguous (race ids
//! 1..10 inclusive; id 9 is unused / `RACE_GOBLIN` which is never allowed).
//! The table is stored as a `bool` matrix `[MAX_CLASSES][MAX_RACES]`
//! instead of a list-per-class so that `is_available_race` is an `O(1)`
//! array read.

use crate::config::typed::{MAX_CLASSES, MAX_RACES};

/// `RACE_HUMAN` (1).
pub const RACE_HUMAN: u8 = 1;
/// `RACE_ORC` (2).
pub const RACE_ORC: u8 = 2;
/// `RACE_DWARF` (3).
pub const RACE_DWARF: u8 = 3;
/// `RACE_NIGHTELF` (4).
pub const RACE_NIGHTELF: u8 = 4;
/// `RACE_UNDEAD` (5).
pub const RACE_UNDEAD: u8 = 5;
/// `RACE_TAUREN` (6).
pub const RACE_TAUREN: u8 = 6;
/// `RACE_GNOME` (7).
pub const RACE_GNOME: u8 = 7;
/// `RACE_TROLL` (8).
pub const RACE_TROLL: u8 = 8;
/// `RACE_GOBLIN` (9) — explicitly never allowed.
pub const RACE_GOBLIN: u8 = 9;
/// `RACE_BLOODELF` (10) — TBC+ only.
pub const RACE_BLOODELF: u8 = 10;
/// `RACE_DRAENEI` (11) — TBC+ only.
pub const RACE_DRAENEI: u8 = 11;

/// `CLASS_WARRIOR` (1).
pub const CLASS_WARRIOR: u8 = 1;
/// `CLASS_PALADIN` (2).
pub const CLASS_PALADIN: u8 = 2;
/// `CLASS_HUNTER` (3).
pub const CLASS_HUNTER: u8 = 3;
/// `CLASS_ROGUE` (4).
pub const CLASS_ROGUE: u8 = 4;
/// `CLASS_PRIEST` (5).
pub const CLASS_PRIEST: u8 = 5;
/// `CLASS_DEATH_KNIGHT` (6) — `WotLK` only.
pub const CLASS_DEATH_KNIGHT: u8 = 6;
/// `CLASS_SHAMAN` (7).
pub const CLASS_SHAMAN: u8 = 7;
/// `CLASS_MAGE` (8).
pub const CLASS_MAGE: u8 = 8;
/// `CLASS_WARLOCK` (9).
pub const CLASS_WARLOCK: u8 = 9;
/// `CLASS_DRUID` (11).
pub const CLASS_DRUID: u8 = 11;

/// `GENDER_MALE` (0).
pub const GENDER_MALE: u8 = 0;
/// `GENDER_FEMALE` (1).
pub const GENDER_FEMALE: u8 = 1;

/// Race/gender combination used as the key into the `ai_playerbot_names`
/// table on the `CMaNGOS` side. Mirrors the `NameRaceAndGender` enum in
/// `playerbot/RandomPlayerbotFactory.h` with identical ordering (Generic
/// covers human + undead as the C++ version does).
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum NameRaceAndGender {
    GenericMale = 0,
    GenericFemale = 1,
    GnomeMale = 2,
    GnomeFemale = 3,
    DwarfMale = 4,
    DwarfFemale = 5,
    NightelfMale = 6,
    NightelfFemale = 7,
    DraeneiMale = 8,
    DraeneiFemale = 9,
    OrcMale = 10,
    OrcFemale = 11,
    TrollMale = 12,
    TrollFemale = 13,
    TaurenMale = 14,
    TaurenFemale = 15,
    BloodelfMale = 16,
    BloodelfFemale = 17,
}

impl NameRaceAndGender {
    /// Full list of variants, in declaration order. Used by the name-pool
    /// expansion pass in `selection::expand_name_pool`.
    pub const ALL: [Self; 18] = [
        Self::GenericMale,
        Self::GenericFemale,
        Self::GnomeMale,
        Self::GnomeFemale,
        Self::DwarfMale,
        Self::DwarfFemale,
        Self::NightelfMale,
        Self::NightelfFemale,
        Self::DraeneiMale,
        Self::DraeneiFemale,
        Self::OrcMale,
        Self::OrcFemale,
        Self::TrollMale,
        Self::TrollFemale,
        Self::TaurenMale,
        Self::TaurenFemale,
        Self::BloodelfMale,
        Self::BloodelfFemale,
    ];

    /// Index into the `NameRaceAndGender::ALL` array, matches the raw
    /// `uint8` stored in the `ai_playerbot_names.gender` column on the
    /// C++ side.
    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }

    /// Decode a raw value from the `ai_playerbot_names.gender` column.
    /// Out-of-range values fall back to `GenericMale` so malformed rows
    /// never panic the name-pool loader.
    #[must_use]
    pub const fn from_raw(raw: u8) -> Self {
        match raw {
            0 => Self::GenericMale,
            1 => Self::GenericFemale,
            2 => Self::GnomeMale,
            3 => Self::GnomeFemale,
            4 => Self::DwarfMale,
            5 => Self::DwarfFemale,
            6 => Self::NightelfMale,
            7 => Self::NightelfFemale,
            8 => Self::DraeneiMale,
            9 => Self::DraeneiFemale,
            10 => Self::OrcMale,
            11 => Self::OrcFemale,
            12 => Self::TrollMale,
            13 => Self::TrollFemale,
            14 => Self::TaurenMale,
            15 => Self::TaurenFemale,
            16 => Self::BloodelfMale,
            17 => Self::BloodelfFemale,
            _ => Self::GenericMale,
        }
    }
}

/// Map a `(gender, race)` pair to its `NameRaceAndGender` slot, mirroring
/// the C++ `RandomPlayerbotFactory::CombineRaceAndGender` switch.
///
/// Gender is `0` for male, `1` for female. Unknown races fall back to the
/// generic (human/undead) slot — same behaviour as the original code path,
/// including its `sLog.outError` on unexpected race ids (we surface that
/// via the returned `Option` so callers can log it).
#[must_use]
pub fn combine_race_and_gender(gender: u8, race: u8) -> NameRaceAndGender {
    // Clamp gender to the male/female range; the C++ code treats any
    // non-zero gender as female.
    let gender = (gender != 0) as u8;

    let base = match race {
        RACE_HUMAN | RACE_UNDEAD => NameRaceAndGender::GenericMale,
        RACE_ORC => NameRaceAndGender::OrcMale,
        RACE_DWARF => NameRaceAndGender::DwarfMale,
        RACE_NIGHTELF => NameRaceAndGender::NightelfMale,
        RACE_TAUREN => NameRaceAndGender::TaurenMale,
        RACE_GNOME => NameRaceAndGender::GnomeMale,
        RACE_TROLL => NameRaceAndGender::TrollMale,
        #[cfg(any(feature = "tbc", feature = "wotlk"))]
        RACE_DRAENEI => NameRaceAndGender::DraeneiMale,
        #[cfg(any(feature = "tbc", feature = "wotlk"))]
        RACE_BLOODELF => NameRaceAndGender::BloodelfMale,
        _ => NameRaceAndGender::GenericMale,
    };

    NameRaceAndGender::from_raw(base.as_index() as u8 + gender)
}

/// `[class][race]` availability matrix. `availability()[cls][race]` is
/// `true` iff the C++ `RandomPlayerbotFactory::availableRaces[cls]` would
/// contain `race`. Rows for classes 0, 10 (an unused slot), and the DK
/// row on non-wotlk builds are all zero.
#[must_use]
pub const fn availability() -> &'static [[bool; MAX_RACES]; MAX_CLASSES] {
    &AVAILABLE
}

/// Same data as [`availability`], expressed as a `bool` slice for
/// callers that only need a flat view.
///
/// Build-time feature flags (`vanilla`/`tbc`/`wotlk`) select which rows
/// include the TBC+ races and the `WotLK` death-knight row.
const AVAILABLE: [[bool; MAX_RACES]; MAX_CLASSES] = build_matrix();

const fn build_matrix() -> [[bool; MAX_RACES]; MAX_CLASSES] {
    let mut m = [[false; MAX_RACES]; MAX_CLASSES];

    // Warrior
    m[CLASS_WARRIOR as usize][RACE_HUMAN as usize] = true;
    m[CLASS_WARRIOR as usize][RACE_NIGHTELF as usize] = true;
    m[CLASS_WARRIOR as usize][RACE_GNOME as usize] = true;
    m[CLASS_WARRIOR as usize][RACE_DWARF as usize] = true;
    m[CLASS_WARRIOR as usize][RACE_ORC as usize] = true;
    m[CLASS_WARRIOR as usize][RACE_UNDEAD as usize] = true;
    m[CLASS_WARRIOR as usize][RACE_TAUREN as usize] = true;
    m[CLASS_WARRIOR as usize][RACE_TROLL as usize] = true;
    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    {
        m[CLASS_WARRIOR as usize][RACE_DRAENEI as usize] = true;
    }

    // Paladin
    m[CLASS_PALADIN as usize][RACE_HUMAN as usize] = true;
    m[CLASS_PALADIN as usize][RACE_DWARF as usize] = true;
    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    {
        m[CLASS_PALADIN as usize][RACE_DRAENEI as usize] = true;
        m[CLASS_PALADIN as usize][RACE_BLOODELF as usize] = true;
    }

    // Rogue
    m[CLASS_ROGUE as usize][RACE_HUMAN as usize] = true;
    m[CLASS_ROGUE as usize][RACE_DWARF as usize] = true;
    m[CLASS_ROGUE as usize][RACE_NIGHTELF as usize] = true;
    m[CLASS_ROGUE as usize][RACE_GNOME as usize] = true;
    m[CLASS_ROGUE as usize][RACE_ORC as usize] = true;
    m[CLASS_ROGUE as usize][RACE_UNDEAD as usize] = true;
    m[CLASS_ROGUE as usize][RACE_TROLL as usize] = true;
    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    {
        m[CLASS_ROGUE as usize][RACE_BLOODELF as usize] = true;
    }

    // Priest
    m[CLASS_PRIEST as usize][RACE_HUMAN as usize] = true;
    m[CLASS_PRIEST as usize][RACE_DWARF as usize] = true;
    m[CLASS_PRIEST as usize][RACE_NIGHTELF as usize] = true;
    m[CLASS_PRIEST as usize][RACE_TROLL as usize] = true;
    m[CLASS_PRIEST as usize][RACE_UNDEAD as usize] = true;
    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    {
        m[CLASS_PRIEST as usize][RACE_DRAENEI as usize] = true;
        m[CLASS_PRIEST as usize][RACE_BLOODELF as usize] = true;
    }

    // Mage
    m[CLASS_MAGE as usize][RACE_HUMAN as usize] = true;
    m[CLASS_MAGE as usize][RACE_GNOME as usize] = true;
    m[CLASS_MAGE as usize][RACE_UNDEAD as usize] = true;
    m[CLASS_MAGE as usize][RACE_TROLL as usize] = true;
    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    {
        m[CLASS_MAGE as usize][RACE_DRAENEI as usize] = true;
        m[CLASS_MAGE as usize][RACE_BLOODELF as usize] = true;
    }

    // Warlock
    m[CLASS_WARLOCK as usize][RACE_HUMAN as usize] = true;
    m[CLASS_WARLOCK as usize][RACE_GNOME as usize] = true;
    m[CLASS_WARLOCK as usize][RACE_UNDEAD as usize] = true;
    m[CLASS_WARLOCK as usize][RACE_ORC as usize] = true;
    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    {
        m[CLASS_WARLOCK as usize][RACE_BLOODELF as usize] = true;
    }

    // Shaman
    m[CLASS_SHAMAN as usize][RACE_ORC as usize] = true;
    m[CLASS_SHAMAN as usize][RACE_TAUREN as usize] = true;
    m[CLASS_SHAMAN as usize][RACE_TROLL as usize] = true;
    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    {
        m[CLASS_SHAMAN as usize][RACE_DRAENEI as usize] = true;
    }

    // Hunter
    m[CLASS_HUNTER as usize][RACE_DWARF as usize] = true;
    m[CLASS_HUNTER as usize][RACE_NIGHTELF as usize] = true;
    m[CLASS_HUNTER as usize][RACE_ORC as usize] = true;
    m[CLASS_HUNTER as usize][RACE_TAUREN as usize] = true;
    m[CLASS_HUNTER as usize][RACE_TROLL as usize] = true;
    #[cfg(any(feature = "tbc", feature = "wotlk"))]
    {
        m[CLASS_HUNTER as usize][RACE_DRAENEI as usize] = true;
        m[CLASS_HUNTER as usize][RACE_BLOODELF as usize] = true;
    }

    // Druid
    m[CLASS_DRUID as usize][RACE_NIGHTELF as usize] = true;
    m[CLASS_DRUID as usize][RACE_TAUREN as usize] = true;

    // Death knight — every playable race, but only in WotLK.
    #[cfg(feature = "wotlk")]
    {
        m[CLASS_DEATH_KNIGHT as usize][RACE_NIGHTELF as usize] = true;
        m[CLASS_DEATH_KNIGHT as usize][RACE_TAUREN as usize] = true;
        m[CLASS_DEATH_KNIGHT as usize][RACE_HUMAN as usize] = true;
        m[CLASS_DEATH_KNIGHT as usize][RACE_ORC as usize] = true;
        m[CLASS_DEATH_KNIGHT as usize][RACE_UNDEAD as usize] = true;
        m[CLASS_DEATH_KNIGHT as usize][RACE_TROLL as usize] = true;
        m[CLASS_DEATH_KNIGHT as usize][RACE_BLOODELF as usize] = true;
        m[CLASS_DEATH_KNIGHT as usize][RACE_DRAENEI as usize] = true;
        m[CLASS_DEATH_KNIGHT as usize][RACE_GNOME as usize] = true;
        m[CLASS_DEATH_KNIGHT as usize][RACE_DWARF as usize] = true;
    }

    m
}

/// Mirrors `RandomPlayerbotFactory::isAvailableRace(cls, race)` — returns
/// `true` iff the given class/race pair is permitted for random bot
/// creation on the current expansion.
///
/// Rules, ported verbatim:
/// 1. `RACE_GOBLIN` is never allowed.
/// 2. Class id `10` (legacy "unused" slot) is never allowed; on non-WotLK
///    builds class id `6` (`CLASS_DEATH_KNIGHT`) is also rejected.
/// 3. Otherwise, look up the `[cls][race]` bit in the availability matrix.
#[must_use]
pub fn is_available_race(cls: u8, race: u8) -> bool {
    if race == RACE_GOBLIN {
        return false;
    }
    if cls == 10 {
        return false;
    }
    #[cfg(not(feature = "wotlk"))]
    {
        if cls == CLASS_DEATH_KNIGHT {
            return false;
        }
    }

    let cls_idx = cls as usize;
    let race_idx = race as usize;
    if cls_idx >= MAX_CLASSES || race_idx >= MAX_RACES {
        return false;
    }
    AVAILABLE[cls_idx][race_idx]
}

/// First race id that is available for `cls`, or `RACE_HUMAN` as a
/// last-resort fallback. Mirrors the `availableRaces[cls].front()` call
/// in `RandomPlayerbotFactory::GetRandomRace` when the probability table
/// is all zeroes — which means it returns the *C++ insertion order*
/// first race, not the first race by numeric id.
///
/// The two differ notably for [`CLASS_HUNTER`] (insertion puts
/// [`RACE_DWARF`] first; numeric-ascending would hit [`RACE_ORC`]) and
/// the `WotLK` [`CLASS_DEATH_KNIGHT`] row (insertion puts
/// [`RACE_NIGHTELF`] first).
#[must_use]
pub const fn first_available_race(cls: u8) -> u8 {
    match cls {
        CLASS_WARRIOR | CLASS_PALADIN | CLASS_ROGUE | CLASS_PRIEST | CLASS_MAGE
        | CLASS_WARLOCK => RACE_HUMAN,
        CLASS_SHAMAN => RACE_ORC,
        CLASS_HUNTER => RACE_DWARF,
        CLASS_DRUID => RACE_NIGHTELF,
        #[cfg(feature = "wotlk")]
        CLASS_DEATH_KNIGHT => RACE_NIGHTELF,
        _ => RACE_HUMAN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goblin_is_never_available() {
        for cls in 1..MAX_CLASSES as u8 {
            assert!(
                !is_available_race(cls, RACE_GOBLIN),
                "class {cls} should not allow goblin"
            );
        }
    }

    #[test]
    fn class_ten_is_never_available() {
        for race in 1..MAX_RACES as u8 {
            assert!(!is_available_race(10, race));
        }
    }

    #[test]
    fn warrior_allows_classic_races() {
        for r in [
            RACE_HUMAN,
            RACE_NIGHTELF,
            RACE_GNOME,
            RACE_DWARF,
            RACE_ORC,
            RACE_UNDEAD,
            RACE_TAUREN,
            RACE_TROLL,
        ] {
            assert!(
                is_available_race(CLASS_WARRIOR, r),
                "warrior should allow race {r}"
            );
        }
    }

    #[test]
    fn druid_only_allows_nightelf_and_tauren() {
        assert!(is_available_race(CLASS_DRUID, RACE_NIGHTELF));
        assert!(is_available_race(CLASS_DRUID, RACE_TAUREN));
        assert!(!is_available_race(CLASS_DRUID, RACE_HUMAN));
        assert!(!is_available_race(CLASS_DRUID, RACE_GNOME));
    }

    #[test]
    fn shaman_is_horde_plus_draenei_on_tbc_wotlk() {
        assert!(is_available_race(CLASS_SHAMAN, RACE_ORC));
        assert!(is_available_race(CLASS_SHAMAN, RACE_TAUREN));
        assert!(is_available_race(CLASS_SHAMAN, RACE_TROLL));
        assert!(!is_available_race(CLASS_SHAMAN, RACE_HUMAN));
        #[cfg(any(feature = "tbc", feature = "wotlk"))]
        assert!(is_available_race(CLASS_SHAMAN, RACE_DRAENEI));
        #[cfg(not(any(feature = "tbc", feature = "wotlk")))]
        assert!(!is_available_race(CLASS_SHAMAN, RACE_DRAENEI));
    }

    #[test]
    fn death_knight_is_wotlk_only() {
        #[cfg(feature = "wotlk")]
        {
            for r in [
                RACE_HUMAN,
                RACE_NIGHTELF,
                RACE_GNOME,
                RACE_DWARF,
                RACE_ORC,
                RACE_UNDEAD,
                RACE_TAUREN,
                RACE_TROLL,
                RACE_BLOODELF,
                RACE_DRAENEI,
            ] {
                assert!(is_available_race(CLASS_DEATH_KNIGHT, r));
            }
        }
        #[cfg(not(feature = "wotlk"))]
        {
            for r in 1..MAX_RACES as u8 {
                assert!(!is_available_race(CLASS_DEATH_KNIGHT, r));
            }
        }
    }

    #[test]
    fn combine_race_and_gender_human_maps_to_generic() {
        assert_eq!(
            combine_race_and_gender(GENDER_MALE, RACE_HUMAN),
            NameRaceAndGender::GenericMale
        );
        assert_eq!(
            combine_race_and_gender(GENDER_FEMALE, RACE_HUMAN),
            NameRaceAndGender::GenericFemale
        );
    }

    #[test]
    fn combine_race_and_gender_undead_also_generic() {
        assert_eq!(
            combine_race_and_gender(GENDER_MALE, RACE_UNDEAD),
            NameRaceAndGender::GenericMale
        );
        assert_eq!(
            combine_race_and_gender(GENDER_FEMALE, RACE_UNDEAD),
            NameRaceAndGender::GenericFemale
        );
    }

    #[test]
    fn combine_race_and_gender_tauren() {
        assert_eq!(
            combine_race_and_gender(GENDER_MALE, RACE_TAUREN),
            NameRaceAndGender::TaurenMale
        );
        assert_eq!(
            combine_race_and_gender(GENDER_FEMALE, RACE_TAUREN),
            NameRaceAndGender::TaurenFemale
        );
    }

    #[test]
    fn combine_race_and_gender_unknown_race_falls_back_to_generic() {
        assert_eq!(
            combine_race_and_gender(GENDER_MALE, 99),
            NameRaceAndGender::GenericMale
        );
    }

    #[test]
    fn name_race_and_gender_roundtrips_raw() {
        for v in NameRaceAndGender::ALL {
            assert_eq!(NameRaceAndGender::from_raw(v.as_index() as u8), v);
        }
    }

    #[test]
    fn first_available_race_matches_cpp_insertion_order() {
        // Classes whose C++ insertion order starts with HUMAN.
        assert_eq!(first_available_race(CLASS_WARRIOR), RACE_HUMAN);
        assert_eq!(first_available_race(CLASS_PALADIN), RACE_HUMAN);
        assert_eq!(first_available_race(CLASS_ROGUE), RACE_HUMAN);
        assert_eq!(first_available_race(CLASS_PRIEST), RACE_HUMAN);
        assert_eq!(first_available_race(CLASS_MAGE), RACE_HUMAN);
        assert_eq!(first_available_race(CLASS_WARLOCK), RACE_HUMAN);
        // Druid: NIGHTELF is inserted first (and HUMAN isn't a valid choice).
        assert_eq!(first_available_race(CLASS_DRUID), RACE_NIGHTELF);
        // Hunter: DWARF is the first entry the C++ vector inserts — this
        // differs from a numeric-ascending walk which would return ORC(2).
        assert_eq!(first_available_race(CLASS_HUNTER), RACE_DWARF);
        // Shaman: ORC is first in the C++ vector.
        assert_eq!(first_available_race(CLASS_SHAMAN), RACE_ORC);
        // Death knight: NIGHTELF is first on WotLK.
        #[cfg(feature = "wotlk")]
        assert_eq!(first_available_race(CLASS_DEATH_KNIGHT), RACE_NIGHTELF);
    }

    #[test]
    fn first_available_race_falls_back_to_human_for_unknown_class() {
        assert_eq!(first_available_race(0), RACE_HUMAN);
        assert_eq!(first_available_race(99), RACE_HUMAN);
    }
}
