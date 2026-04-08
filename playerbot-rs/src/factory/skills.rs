//! Factory InitSkills — mirrors `PlayerbotFactory::InitSkills`.
//!
//! Three sub-policies run in sequence:
//!
//!   1. `bot_update_skills_for_level` — refreshes the innate level-scaled
//!      skills (languages, racials, …). Matches `Player::UpdateSkillsForLevel`.
//!   2. Riding skill — granted based on level, with thresholds that vary
//!      per expansion (classic 40/60, TBC 30/60/68/70, WotLK 20/40/60/70).
//!   3. Weapon + armor skills — a random-roll table keyed on class. Weapon
//!      skills are rolled with `SetRandomSkill`, picking a value inside
//!      `[maxValue - level, maxValue]` and only upgrading if higher than
//!      the current value.
//!
//! Pure policy: every DB/game-state read or write flows through the
//! `BotInterface`. The dispatcher snapshots `level`, `class_id` once at
//! entry and passes them in.

use crate::ffi::interface::BotInterface;

// ── Skill IDs (from mangos `SharedDefines.h`) ─────────────────────────────

const SKILL_SWORDS: u32 = 43;
const SKILL_AXES: u32 = 44;
const SKILL_BOWS: u32 = 45;
const SKILL_GUNS: u32 = 46;
const SKILL_MACES: u32 = 54;
const SKILL_2H_SWORDS: u32 = 55;
const SKILL_STAVES: u32 = 136;
const SKILL_2H_MACES: u32 = 160;
const SKILL_2H_AXES: u32 = 172;
const SKILL_DAGGERS: u32 = 173;
const SKILL_THROWN: u32 = 176;
const SKILL_CROSSBOWS: u32 = 226;
const SKILL_WANDS: u32 = 228;
const SKILL_POLEARMS: u32 = 229;
const SKILL_PLATE_MAIL: u32 = 293;
const SKILL_MAIL: u32 = 413;
const SKILL_FIST_WEAPONS: u32 = 473;
const SKILL_RIDING: u32 = 762;

// ── Class IDs ─────────────────────────────────────────────────────────────

const CLASS_WARRIOR: u8 = 1;
const CLASS_PALADIN: u8 = 2;
const CLASS_HUNTER: u8 = 3;
const CLASS_ROGUE: u8 = 4;
const CLASS_PRIEST: u8 = 5;
#[cfg(feature = "wotlk")]
const CLASS_DEATH_KNIGHT: u8 = 6;
const CLASS_SHAMAN: u8 = 7;
const CLASS_MAGE: u8 = 8;
const CLASS_WARLOCK: u8 = 9;
const CLASS_DRUID: u8 = 11;

/// Run the factory InitSkills step.
pub fn init_skills(iface: &dyn BotInterface, class_id: u8, level: u32) {
    iface.bot_update_skills_for_level();

    apply_riding(iface, level);
    apply_armor(iface, class_id, level);
    apply_weapon_table(iface, class_id, level);
}

// ── Riding skill ──────────────────────────────────────────────────────────

#[cfg(feature = "vanilla")]
fn riding_value_for_level(level: u32) -> u32 {
    if level >= 60 {
        150
    } else if level >= 40 {
        75
    } else {
        0
    }
}

#[cfg(feature = "tbc")]
fn riding_value_for_level(level: u32) -> u32 {
    if level >= 70 {
        300
    } else if level >= 68 {
        225
    } else if level >= 60 {
        150
    } else if level >= 30 {
        75
    } else {
        0
    }
}

#[cfg(feature = "wotlk")]
fn riding_value_for_level(level: u32) -> u32 {
    if level >= 70 {
        300
    } else if level >= 60 {
        225
    } else if level >= 40 {
        150
    } else if level >= 20 {
        75
    } else {
        0
    }
}

// Fallback when none of the three expansion features is enabled — still
// compiles so `cargo test` (default features) is green. Matches the vanilla
// thresholds.
#[cfg(not(any(feature = "vanilla", feature = "tbc", feature = "wotlk")))]
fn riding_value_for_level(level: u32) -> u32 {
    if level >= 60 {
        150
    } else if level >= 40 {
        75
    } else {
        0
    }
}

fn apply_riding(iface: &dyn BotInterface, level: u32) {
    let v = riding_value_for_level(level);
    iface.bot_set_skill(SKILL_RIDING, v, v);
}

// ── Armor proficiencies (plate / mail at level 40) ────────────────────────

fn apply_armor(iface: &dyn BotInterface, class_id: u8, level: u32) {
    let value = if level < 40 { 0 } else { 1 };
    match class_id {
        CLASS_WARRIOR | CLASS_PALADIN => {
            iface.bot_set_skill(SKILL_PLATE_MAIL, value, value);
        }
        CLASS_SHAMAN | CLASS_HUNTER => {
            iface.bot_set_skill(SKILL_MAIL, value, value);
        }
        _ => {}
    }
}

// ── Weapon skill table ────────────────────────────────────────────────────

fn weapon_skills_for_class(class_id: u8) -> &'static [u32] {
    match class_id {
        CLASS_DRUID => {
            #[cfg(feature = "wotlk")]
            {
                &[
                    SKILL_MACES,
                    SKILL_STAVES,
                    SKILL_2H_MACES,
                    SKILL_DAGGERS,
                    SKILL_POLEARMS,
                    SKILL_FIST_WEAPONS,
                ]
            }
            #[cfg(not(feature = "wotlk"))]
            {
                &[
                    SKILL_MACES,
                    SKILL_STAVES,
                    SKILL_2H_MACES,
                    SKILL_DAGGERS,
                    SKILL_FIST_WEAPONS,
                ]
            }
        }
        CLASS_WARRIOR => &[
            SKILL_SWORDS,
            SKILL_AXES,
            SKILL_BOWS,
            SKILL_GUNS,
            SKILL_MACES,
            SKILL_2H_SWORDS,
            SKILL_STAVES,
            SKILL_2H_MACES,
            SKILL_2H_AXES,
            SKILL_DAGGERS,
            SKILL_CROSSBOWS,
            SKILL_POLEARMS,
            SKILL_FIST_WEAPONS,
            SKILL_THROWN,
        ],
        CLASS_PALADIN => &[
            SKILL_SWORDS,
            SKILL_AXES,
            SKILL_MACES,
            SKILL_2H_SWORDS,
            SKILL_2H_MACES,
            SKILL_2H_AXES,
            SKILL_POLEARMS,
        ],
        CLASS_PRIEST => &[SKILL_MACES, SKILL_STAVES, SKILL_DAGGERS, SKILL_WANDS],
        CLASS_SHAMAN => &[
            SKILL_AXES,
            SKILL_MACES,
            SKILL_STAVES,
            SKILL_2H_MACES,
            SKILL_2H_AXES,
            SKILL_DAGGERS,
            SKILL_FIST_WEAPONS,
        ],
        CLASS_MAGE | CLASS_WARLOCK => &[SKILL_SWORDS, SKILL_STAVES, SKILL_DAGGERS, SKILL_WANDS],
        CLASS_HUNTER => &[
            SKILL_SWORDS,
            SKILL_AXES,
            SKILL_BOWS,
            SKILL_GUNS,
            SKILL_2H_SWORDS,
            SKILL_STAVES,
            SKILL_2H_AXES,
            SKILL_DAGGERS,
            SKILL_CROSSBOWS,
            SKILL_POLEARMS,
            SKILL_FIST_WEAPONS,
            SKILL_THROWN,
        ],
        CLASS_ROGUE => {
            #[cfg(feature = "wotlk")]
            {
                &[
                    SKILL_SWORDS,
                    SKILL_BOWS,
                    SKILL_GUNS,
                    SKILL_MACES,
                    SKILL_DAGGERS,
                    SKILL_CROSSBOWS,
                    SKILL_FIST_WEAPONS,
                    SKILL_THROWN,
                    SKILL_AXES,
                ]
            }
            #[cfg(not(feature = "wotlk"))]
            {
                &[
                    SKILL_SWORDS,
                    SKILL_BOWS,
                    SKILL_GUNS,
                    SKILL_MACES,
                    SKILL_DAGGERS,
                    SKILL_CROSSBOWS,
                    SKILL_FIST_WEAPONS,
                    SKILL_THROWN,
                ]
            }
        }
        #[cfg(feature = "wotlk")]
        CLASS_DEATH_KNIGHT => &[
            SKILL_SWORDS,
            SKILL_AXES,
            SKILL_MACES,
            SKILL_2H_SWORDS,
            SKILL_2H_MACES,
            SKILL_2H_AXES,
            SKILL_POLEARMS,
        ],
        _ => &[],
    }
}

fn apply_weapon_table(iface: &dyn BotInterface, class_id: u8, level: u32) {
    for &skill in weapon_skills_for_class(class_id) {
        set_random_skill(iface, skill, level);
    }
}

/// Mirror of `PlayerbotFactory::SetRandomSkill`: roll a value in
/// `[maxValue - level, maxValue]` where `maxValue` is the expansion-
/// appropriate skill cap, and only upgrade if higher than the current.
fn set_random_skill(iface: &dyn BotInterface, skill_id: u32, level: u32) {
    let max_value = skill_cap_for_level(level);
    let lo = max_value.saturating_sub(level);
    let value = iface.random_u32(lo, max_value);

    let cur = iface.bot_get_skill_value(skill_id);
    if cur == 0 || value > cur {
        iface.bot_set_skill(skill_id, value, max_value);
    }
}

/// Maximum weapon skill value for the given character level. Classic caps
/// at `level * 5` across the board; TBC/WotLK extend the cap above level 60.
fn skill_cap_for_level(level: u32) -> u32 {
    #[cfg(feature = "vanilla")]
    {
        level * 5
    }
    #[cfg(feature = "tbc")]
    {
        if level > 60 {
            (level + 5) * 5
        } else {
            level * 5
        }
    }
    #[cfg(feature = "wotlk")]
    {
        if level > 60 {
            (level + 10) * 5
        } else {
            level * 5
        }
    }
    #[cfg(not(any(feature = "vanilla", feature = "tbc", feature = "wotlk")))]
    {
        level * 5
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::interface::BotInterface;
    use crate::ffi::types::ItemId;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MockIface {
        update_calls: RefCell<u32>,
        skills: RefCell<HashMap<u32, (u32, u32)>>, // id -> (value, max)
                                                   // Deterministic RNG: always return `min`.
    }

    unsafe impl Send for MockIface {}

    impl BotInterface for MockIface {
        fn get_snapshot(&self) -> crate::ffi::BotWorldSnapshot {
            unsafe { std::mem::zeroed() }
        }
        fn get_unit_snapshot(&self, _: crate::ffi::UnitHandle) -> crate::ffi::BotUnitSnapshot {
            unsafe { std::mem::zeroed() }
        }
        fn has_aura(&self, _: crate::ffi::UnitHandle, _: crate::ffi::SpellId) -> bool {
            false
        }
        fn get_aura(
            &self,
            _: crate::ffi::UnitHandle,
            _: crate::ffi::SpellId,
        ) -> Option<crate::ffi::BotAuraInfo> {
            None
        }
        fn get_auras(&self, _: crate::ffi::UnitHandle) -> Vec<crate::ffi::BotAuraInfo> {
            vec![]
        }
        fn get_threat_list(&self, _: crate::ffi::UnitHandle) -> Vec<crate::ffi::BotThreatEntry> {
            vec![]
        }
        fn get_unit_threat(&self, _: crate::ffi::UnitHandle, _: crate::ffi::UnitHandle) -> f32 {
            0.0
        }
        fn unit_distance(&self, _: crate::ffi::UnitHandle) -> f32 {
            0.0
        }
        fn can_cast(&self, _: crate::ffi::SpellId, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn spell_cooldown_ms(&self, _: crate::ffi::SpellId) -> u32 {
            0
        }
        fn has_los(&self, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn get_nearby_units(&self, _: f32, _: bool) -> Vec<crate::ffi::UnitHandle> {
            vec![]
        }
        fn get_behind_position(
            &self,
            _: crate::ffi::UnitHandle,
            _: f32,
        ) -> crate::ffi::BotPosition {
            unsafe { std::mem::zeroed() }
        }
        fn get_safe_position(&self, _: f32) -> Option<crate::ffi::BotPosition> {
            None
        }
        fn get_spread_position(
            &self,
            _: crate::ffi::UnitHandle,
            _: f32,
            _: u8,
            _: u8,
        ) -> crate::ffi::BotPosition {
            unsafe { std::mem::zeroed() }
        }
        fn can_reach(&self, _: f32, _: f32, _: f32) -> bool {
            false
        }
        fn cast_spell(&self, _: crate::ffi::SpellId, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn cast_spell_pos(&self, _: crate::ffi::SpellId, _: f32, _: f32, _: f32) -> bool {
            false
        }
        fn move_to(&self, _: f32, _: f32, _: f32) -> bool {
            false
        }
        fn follow(&self, _: crate::ffi::UnitHandle, _: f32, _: f32) -> bool {
            false
        }
        fn stop_moving(&self) -> bool {
            false
        }
        fn attack(&self, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn auto_attack(&self, _: bool) -> bool {
            false
        }
        fn say(&self, _: &str, _: u32) -> bool {
            false
        }
        fn use_item(&self, _: ItemId, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn taunt(&self, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn group_get_tank(&self) -> Option<crate::ffi::UnitHandle> {
            None
        }
        fn group_get_healer(&self) -> Option<crate::ffi::UnitHandle> {
            None
        }
        fn group_get_role(&self, _: crate::ffi::UnitHandle) -> crate::ffi::BotRole {
            crate::ffi::BotRole::NONE
        }

        fn bot_update_skills_for_level(&self) {
            *self.update_calls.borrow_mut() += 1;
        }
        fn bot_get_skill_value(&self, skill_id: u32) -> u32 {
            self.skills
                .borrow()
                .get(&skill_id)
                .map(|(v, _)| *v)
                .unwrap_or(0)
        }
        fn bot_set_skill(&self, skill_id: u32, value: u32, max: u32) {
            self.skills.borrow_mut().insert(skill_id, (value, max));
        }
        fn random_u32(&self, min: u32, _max: u32) -> u32 {
            min
        }
    }

    #[test]
    fn calls_update_skills_for_level() {
        let m = MockIface::default();
        init_skills(&m, CLASS_WARRIOR, 60);
        assert_eq!(*m.update_calls.borrow(), 1);
    }

    #[test]
    fn warrior_at_60_gets_plate_mail() {
        let m = MockIface::default();
        init_skills(&m, CLASS_WARRIOR, 60);
        let s = m.skills.borrow();
        assert_eq!(s.get(&SKILL_PLATE_MAIL), Some(&(1, 1)));
        assert!(!s.contains_key(&SKILL_MAIL));
    }

    #[test]
    fn hunter_at_60_gets_mail() {
        let m = MockIface::default();
        init_skills(&m, CLASS_HUNTER, 60);
        let s = m.skills.borrow();
        assert_eq!(s.get(&SKILL_MAIL), Some(&(1, 1)));
    }

    #[test]
    fn armor_zero_below_40() {
        let m = MockIface::default();
        init_skills(&m, CLASS_WARRIOR, 30);
        let s = m.skills.borrow();
        assert_eq!(s.get(&SKILL_PLATE_MAIL), Some(&(0, 0)));
    }

    #[test]
    fn riding_follows_level_thresholds() {
        let m = MockIface::default();
        init_skills(&m, CLASS_WARRIOR, 20);
        assert_eq!(m.skills.borrow().get(&SKILL_RIDING), Some(&(0, 0)));

        let m = MockIface::default();
        init_skills(&m, CLASS_WARRIOR, 40);
        assert_eq!(m.skills.borrow().get(&SKILL_RIDING), Some(&(75, 75)));

        let m = MockIface::default();
        init_skills(&m, CLASS_WARRIOR, 60);
        assert_eq!(m.skills.borrow().get(&SKILL_RIDING), Some(&(150, 150)));
    }

    #[test]
    fn mage_gets_wands_but_not_polearms() {
        let m = MockIface::default();
        init_skills(&m, CLASS_MAGE, 60);
        let s = m.skills.borrow();
        assert!(s.contains_key(&SKILL_WANDS));
        assert!(s.contains_key(&SKILL_STAVES));
        assert!(s.contains_key(&SKILL_SWORDS));
        assert!(s.contains_key(&SKILL_DAGGERS));
        assert!(!s.contains_key(&SKILL_POLEARMS));
        assert!(!s.contains_key(&SKILL_2H_AXES));
    }

    #[test]
    fn weapon_cap_at_level_60_is_300() {
        let m = MockIface::default();
        init_skills(&m, CLASS_WARRIOR, 60);
        let s = m.skills.borrow();
        // `random_u32` always returns `min` = max - level = 300 - 60 = 240;
        // cap = 300.
        let (value, max) = s.get(&SKILL_SWORDS).copied().unwrap();
        assert_eq!(max, 300);
        assert_eq!(value, 240);
    }

    #[test]
    fn only_upgrades_weapon_skill_when_higher() {
        let m = MockIface::default();
        // Pre-seed a very high current value.
        m.skills.borrow_mut().insert(SKILL_SWORDS, (299, 300));
        init_skills(&m, CLASS_WARRIOR, 60);
        // Rolled value is 240, which is not > 299, so untouched.
        let (value, _) = m.skills.borrow().get(&SKILL_SWORDS).copied().unwrap();
        assert_eq!(value, 299);
    }
}
