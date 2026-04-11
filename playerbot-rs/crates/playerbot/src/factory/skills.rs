//! Factory `InitSkills` — mirrors `PlayerbotFactory::InitSkills`.
//!
//! Three sub-policies run in sequence:
//!
//!   1. `bot_update_skills_for_level` — refreshes the innate level-scaled
//!      skills (languages, racials, …). Matches `Player::UpdateSkillsForLevel`.
//!   2. Riding skill — granted based on level, with thresholds that vary
//!      per expansion (classic 40/60, TBC 30/60/68/70, `WotLK` 20/40/60/70).
//!   3. Weapon + armor skills — a random-roll table keyed on class. Weapon
//!      skills are rolled with `SetRandomSkill`, picking a value inside
//!      `[maxValue - level, maxValue]` and only upgrading if higher than
//!      the current value.
//!
//! Pure policy: every DB/game-state read or write flows through the
//! `World`. The dispatcher snapshots `level`, `class_id` once at
//! entry and passes them in.

use super::FactoryTransaction;

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

/// Run the factory `InitSkills` step.
pub fn init_skills(tx: &mut FactoryTransaction<'_>, class_id: u8, level: u32) {
    tx.bot_update_skills_for_level();

    apply_riding(tx, level);
    apply_armor(tx, class_id, level);
    apply_weapon_table(tx, class_id, level);
}

// ── Riding skill ──────────────────────────────────────────────────────────

// Feature priority: wotlk > tbc > vanilla. With `--all-features` the
// strictest expansion wins so the function is not multiply-defined.
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

#[cfg(all(feature = "tbc", not(feature = "wotlk")))]
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

// Vanilla/no-feature fallback: classic thresholds.
#[cfg(not(any(feature = "tbc", feature = "wotlk")))]
fn riding_value_for_level(level: u32) -> u32 {
    if level >= 60 {
        150
    } else if level >= 40 {
        75
    } else {
        0
    }
}

fn apply_riding(tx: &mut FactoryTransaction<'_>, level: u32) {
    let v = riding_value_for_level(level);
    tx.bot_set_skill(SKILL_RIDING, v, v);
}

// ── Armor proficiencies (plate / mail at level 40) ────────────────────────

fn apply_armor(tx: &mut FactoryTransaction<'_>, class_id: u8, level: u32) {
    let value = if level < 40 { 0 } else { 1 };
    match class_id {
        CLASS_WARRIOR | CLASS_PALADIN => {
            tx.bot_set_skill(SKILL_PLATE_MAIL, value, value);
        }
        CLASS_SHAMAN | CLASS_HUNTER => {
            tx.bot_set_skill(SKILL_MAIL, value, value);
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

fn apply_weapon_table(tx: &mut FactoryTransaction<'_>, class_id: u8, level: u32) {
    for &skill in weapon_skills_for_class(class_id) {
        set_random_skill(tx, skill, level);
    }
}

/// Mirror of `PlayerbotFactory::SetRandomSkill`: roll a value in
/// `[maxValue - level, maxValue]` where `maxValue` is the expansion-
/// appropriate skill cap, and only upgrade if higher than the current.
///
/// `pub(super)` so `factory::init_trade_skills` can reuse the exact
/// same cap/upgrade semantics when assigning the 5 profession skills
/// (first aid, fishing, cooking, first profession, second profession).
pub(super) fn set_random_skill(tx: &mut FactoryTransaction<'_>, skill_id: u32, level: u32) {
    let max_value = skill_cap_for_level(level);
    let lo = max_value.saturating_sub(level);
    let value = tx.random_u32(lo, max_value);

    let cur = tx.bot_get_skill_value(skill_id);
    if cur == 0 || value > cur {
        tx.bot_set_skill(skill_id, value, max_value);
    }
}

/// Maximum weapon skill value for the given character level. Classic caps
/// at `level * 5` across the board; TBC/WotLK extend the cap above level 60.
///
/// Feature priority is `wotlk` > `tbc` > `vanilla` so `--all-features` builds
/// pick the highest expansion deterministically.
fn skill_cap_for_level(level: u32) -> u32 {
    #[cfg(feature = "wotlk")]
    {
        if level > 60 {
            (level + 10) * 5
        } else {
            level * 5
        }
    }
    #[cfg(all(feature = "tbc", not(feature = "wotlk")))]
    {
        if level > 60 {
            (level + 5) * 5
        } else {
            level * 5
        }
    }
    #[cfg(not(any(feature = "tbc", feature = "wotlk")))]
    {
        level * 5
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::with_tx;
    use cmangos::{MockEvent, MockWorld};
    use std::collections::HashMap;

    /// Final (value, max) per skill from the SetSkill event log. Last write wins.
    fn skill_state(w: &MockWorld) -> HashMap<u32, (u32, u32)> {
        let mut out = HashMap::new();
        for e in w.events() {
            if let MockEvent::SetSkill { skill, value, max } = e {
                out.insert(skill, (value, max));
            }
        }
        out
    }

    fn update_call_count(w: &MockWorld) -> u32 {
        w.events()
            .iter()
            .filter(|e| matches!(e, MockEvent::UpdateSkillsForLevel))
            .count() as u32
    }

    #[test]
    fn calls_update_skills_for_level() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_skills(tx, CLASS_WARRIOR, 60));
        assert_eq!(update_call_count(&w), 1);
    }

    #[test]
    fn warrior_at_60_gets_plate_mail() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_skills(tx, CLASS_WARRIOR, 60));
        let s = skill_state(&w);
        assert_eq!(s.get(&SKILL_PLATE_MAIL), Some(&(1, 1)));
        assert!(!s.contains_key(&SKILL_MAIL));
    }

    #[test]
    fn hunter_at_60_gets_mail() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_skills(tx, CLASS_HUNTER, 60));
        let s = skill_state(&w);
        assert_eq!(s.get(&SKILL_MAIL), Some(&(1, 1)));
    }

    #[test]
    fn armor_zero_below_40() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_skills(tx, CLASS_WARRIOR, 30));
        let s = skill_state(&w);
        assert_eq!(s.get(&SKILL_PLATE_MAIL), Some(&(0, 0)));
    }

    #[test]
    fn riding_follows_level_thresholds() {
        // Each expansion lowered the riding-skill thresholds; check the
        // values appropriate for the active feature.
        let levels: &[(u32, u32)] = {
            #[cfg(feature = "wotlk")]
            {
                &[(10, 0), (20, 75), (40, 150), (60, 225), (70, 300)]
            }
            #[cfg(all(feature = "tbc", not(feature = "wotlk")))]
            {
                &[(20, 0), (30, 75), (60, 150), (68, 225), (70, 300)]
            }
            #[cfg(all(
                not(feature = "tbc"),
                not(feature = "wotlk"),
            ))]
            {
                &[(20, 0), (40, 75), (60, 150)]
            }
        };
        for &(lvl, expected) in levels {
            let mut w = MockWorld::default();
            with_tx(&mut w, |tx| init_skills(tx, CLASS_WARRIOR, lvl));
            assert_eq!(
                skill_state(&w).get(&SKILL_RIDING),
                Some(&(expected, expected)),
                "level {lvl}",
            );
        }
    }

    #[test]
    fn mage_gets_wands_but_not_polearms() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_skills(tx, CLASS_MAGE, 60));
        let s = skill_state(&w);
        assert!(s.contains_key(&SKILL_WANDS));
        assert!(s.contains_key(&SKILL_STAVES));
        assert!(s.contains_key(&SKILL_SWORDS));
        assert!(s.contains_key(&SKILL_DAGGERS));
        assert!(!s.contains_key(&SKILL_POLEARMS));
        assert!(!s.contains_key(&SKILL_2H_AXES));
    }

    #[test]
    fn weapon_cap_at_level_60_is_300() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_skills(tx, CLASS_WARRIOR, 60));
        let s = skill_state(&w);
        // `random_u32` defaults to `min` = max - level = 300 - 60 = 240; cap = 300.
        let (value, max) = s.get(&SKILL_SWORDS).copied().unwrap();
        assert_eq!(max, 300);
        assert_eq!(value, 240);
    }

    #[test]
    fn only_upgrades_weapon_skill_when_higher() {
        // Pre-seed a very high current value via the builder.
        let mut w = MockWorld::builder().skill(SKILL_SWORDS, 299, 300).build();
        with_tx(&mut w, |tx| init_skills(tx, CLASS_WARRIOR, 60));
        // Rolled value is 240, which is not > 299, so should NOT have been
        // overwritten. The state record for SKILL_SWORDS should still be 299.
        // No SetSkill event for SKILL_SWORDS should have fired during the run.
        let no_swords_set = w
            .events()
            .iter()
            .all(|e| !matches!(e, MockEvent::SetSkill { skill, .. } if *skill == SKILL_SWORDS));
        assert!(no_swords_set, "SKILL_SWORDS should not have been overwritten");
    }
}
