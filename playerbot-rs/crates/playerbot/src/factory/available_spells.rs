//! Factory `InitAvailableSpells` — mirrors
//! `PlayerbotFactory::InitAvailableSpells`.
//!
//! Two steps:
//!
//! 1. Call `learnDefaultSpells()` + `learnClassLevelSpells(true)` on the bot
//!    via the FFI — the heavy lifting (race/class starter spells and every
//!    class spell at the current level) is done by `CMaNGOS` itself.
//! 2. Top up a handful of spells the C++ source hard-codes as "always
//!    present" but that the class-level spellbook does not cover: extra
//!    paladin strikes (TBC/classic), mage polymorph variants at 60,
//!    warlock inferno at 50, and the classic-only level-60 class book
//!    spells that were sold by trainers at Blackrock Mountain.
//!
//! `Player::learnSpell` is idempotent, so we skip the `HasSpell` gates that
//! the C++ source used — calling it twice is a no-op.

use super::FactoryTransaction;

// ── Class IDs (match `BotWorldSnapshot.self.class_id`) ────────────────────
// Several constants are only consulted on certain expansions; allow dead_code.
#[allow(dead_code)]
const CLASS_WARRIOR: u8 = 1;
#[allow(dead_code)]
const CLASS_PALADIN: u8 = 2;
#[allow(dead_code)]
const CLASS_HUNTER: u8 = 3;
#[allow(dead_code)]
const CLASS_ROGUE: u8 = 4;
#[allow(dead_code)]
const CLASS_PRIEST: u8 = 5;
#[allow(dead_code)]
const CLASS_SHAMAN: u8 = 7;
const CLASS_MAGE: u8 = 8;
const CLASS_WARLOCK: u8 = 9;
#[allow(dead_code)]
const CLASS_DRUID: u8 = 11;

/// Run both `CMaNGOS` spellbook helpers and top up the hard-coded extras
/// that `PlayerbotFactory::InitAvailableSpells` adds.
pub fn init_available_spells(tx: &mut FactoryTransaction<'_>, class_id: u8, level: u32) {
    // Step 1 — let CMaNGOS fill the default + class-level spellbook.
    tx.bot_learn_default_spells();
    tx.bot_learn_class_level_spells(true);

    // Step 2 — hard-coded extras. Order matches the C++ source.

    // Paladin extras are compiled out on WotLK (`#ifndef MANGOSBOT_TWO`).
    #[cfg(not(feature = "wotlk"))]
    if class_id == CLASS_PALADIN {
        tx.bot_learn_spell(20271); // Judgement
        tx.bot_learn_spell(33394); // Crusader Strike
        tx.bot_learn_spell(33395); // Hand of Reckoning
    }

    // Polymorph pig + turtle are handed out to every mage at level 60+.
    if class_id == CLASS_MAGE && level >= 60 {
        tx.bot_learn_spell(28271); // Polymorph: Pig
        tx.bot_learn_spell(28272); // Polymorph: Turtle
    }

    // Inferno for warlocks at level 50+.
    if class_id == CLASS_WARLOCK && level >= 50 {
        tx.bot_learn_spell(1122);
    }

    // Classic-only: level-60 class book spells from MC/BWL era trainers.
    #[cfg(feature = "vanilla")]
    if level == 60 {
        let book_spells: &[u32] = match class_id {
            CLASS_WARRIOR => &[25289, 25288, 25958],
            CLASS_PALADIN => &[25291, 25290, 25292],
            CLASS_HUNTER => &[25296, 25294, 25295],
            CLASS_MAGE => &[23028, 25345, 25306, 3723, 28612],
            CLASS_ROGUE => &[25300, 25302, 31016],
            CLASS_PRIEST => &[25314, 25315, 25316, 21564, 27683],
            CLASS_SHAMAN => &[29228, 25359, 25357, 25361],
            CLASS_WARLOCK => &[25311, 25309, 25307, 28610],
            CLASS_DRUID => &[31018, 25297, 25299, 25298, 21850],
            _ => &[],
        };
        for &spell_id in book_spells {
            tx.bot_learn_spell(spell_id);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::with_tx;
    use cmangos::{MockEvent, MockWorld};

    fn learned(world: &MockWorld) -> Vec<u32> {
        world
            .events()
            .iter()
            .filter_map(|e| match e {
                MockEvent::LearnSpell(id) => Some(*id),
                _ => None,
            })
            .collect()
    }

    fn default_called(world: &MockWorld) -> u32 {
        world
            .events()
            .iter()
            .filter(|e| matches!(e, MockEvent::LearnDefaultSpells))
            .count() as u32
    }

    fn class_level_called(world: &MockWorld) -> Vec<bool> {
        world
            .events()
            .iter()
            .filter_map(|e| match e {
                MockEvent::LearnClassLevelSpells { include_quest_rewards } => {
                    Some(*include_quest_rewards)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn always_calls_default_and_class_level_helpers() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_available_spells(tx, CLASS_WARRIOR, 10));
        assert_eq!(default_called(&w), 1);
        assert_eq!(class_level_called(&w), vec![true]);
    }

    #[cfg(not(feature = "wotlk"))]
    #[test]
    fn paladin_gets_judgement_and_strikes() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_available_spells(tx, CLASS_PALADIN, 60));
        let l = learned(&w);
        assert!(l.contains(&20271));
        assert!(l.contains(&33394));
        assert!(l.contains(&33395));
    }

    #[test]
    fn mage_gets_polymorph_variants_at_60() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_available_spells(tx, CLASS_MAGE, 60));
        let l = learned(&w);
        assert!(l.contains(&28271));
        assert!(l.contains(&28272));
    }

    #[test]
    fn mage_skips_polymorph_variants_below_60() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_available_spells(tx, CLASS_MAGE, 59));
        let l = learned(&w);
        assert!(!l.contains(&28271));
        assert!(!l.contains(&28272));
    }

    #[test]
    fn warlock_gets_inferno_at_50() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_available_spells(tx, CLASS_WARLOCK, 50));
        assert!(learned(&w).contains(&1122));
    }

    #[test]
    fn warlock_skips_inferno_below_50() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_available_spells(tx, CLASS_WARLOCK, 49));
        assert!(!learned(&w).contains(&1122));
    }

    #[cfg(feature = "vanilla")]
    #[test]
    fn classic_level_60_warrior_gets_book_spells() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_available_spells(tx, CLASS_WARRIOR, 60));
        let l = learned(&w);
        for &id in &[25289u32, 25288, 25958] {
            assert!(l.contains(&id), "missing warrior book spell {id}");
        }
    }

    #[cfg(feature = "vanilla")]
    #[test]
    fn classic_level_59_warrior_skips_book_spells() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_available_spells(tx, CLASS_WARRIOR, 59));
        assert!(!learned(&w).contains(&25289));
    }
}
