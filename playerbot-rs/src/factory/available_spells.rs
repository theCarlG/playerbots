//! Factory InitAvailableSpells — mirrors
//! `PlayerbotFactory::InitAvailableSpells`.
//!
//! Two steps:
//!
//! 1. Call `learnDefaultSpells()` + `learnClassLevelSpells(true)` on the bot
//!    via the FFI — the heavy lifting (race/class starter spells and every
//!    class spell at the current level) is done by CMaNGOS itself.
//! 2. Top up a handful of spells the C++ source hard-codes as "always
//!    present" but that the class-level spellbook does not cover: extra
//!    paladin strikes (TBC/classic), mage polymorph variants at 60,
//!    warlock inferno at 50, and the classic-only level-60 class book
//!    spells that were sold by trainers at Blackrock Mountain.
//!
//! `Player::learnSpell` is idempotent, so we skip the `HasSpell` gates that
//! the C++ source used — calling it twice is a no-op.

use crate::ffi::interface::BotInterface;

// ── Class IDs (match `BotWorldSnapshot.self.class_id`) ────────────────────
const CLASS_WARRIOR: u8 = 1;
const CLASS_PALADIN: u8 = 2;
const CLASS_HUNTER: u8 = 3;
const CLASS_ROGUE: u8 = 4;
const CLASS_PRIEST: u8 = 5;
const CLASS_SHAMAN: u8 = 7;
const CLASS_MAGE: u8 = 8;
const CLASS_WARLOCK: u8 = 9;
const CLASS_DRUID: u8 = 11;

/// Run both CMaNGOS spellbook helpers and top up the hard-coded extras
/// that `PlayerbotFactory::InitAvailableSpells` adds.
pub fn init_available_spells(iface: &dyn BotInterface, class_id: u8, level: u32) {
    // Step 1 — let CMaNGOS fill the default + class-level spellbook.
    iface.bot_learn_default_spells();
    iface.bot_learn_class_level_spells(true);

    // Step 2 — hard-coded extras. Order matches the C++ source.

    // Paladin extras are compiled out on WotLK (`#ifndef MANGOSBOT_TWO`).
    #[cfg(not(feature = "wotlk"))]
    if class_id == CLASS_PALADIN {
        iface.bot_learn_spell(20271); // Judgement
        iface.bot_learn_spell(33394); // Crusader Strike
        iface.bot_learn_spell(33395); // Hand of Reckoning
    }

    // Polymorph pig + turtle are handed out to every mage at level 60+.
    if class_id == CLASS_MAGE && level >= 60 {
        iface.bot_learn_spell(28271); // Polymorph: Pig
        iface.bot_learn_spell(28272); // Polymorph: Turtle
    }

    // Inferno for warlocks at level 50+.
    if class_id == CLASS_WARLOCK && level >= 50 {
        iface.bot_learn_spell(1122);
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
            iface.bot_learn_spell(spell_id);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::interface::BotInterface;
    use crate::ffi::types::{ItemId, SpellId};
    use std::cell::RefCell;

    #[derive(Default)]
    struct MockIface {
        default_called: RefCell<u32>,
        class_level_called: RefCell<Vec<bool>>,
        learned: RefCell<Vec<u32>>,
    }

    unsafe impl Send for MockIface {}

    impl BotInterface for MockIface {
        fn get_snapshot(&self) -> crate::ffi::BotWorldSnapshot {
            unsafe { std::mem::zeroed() }
        }
        fn get_unit_snapshot(&self, _: crate::ffi::UnitHandle) -> crate::ffi::BotUnitSnapshot {
            unsafe { std::mem::zeroed() }
        }
        fn has_aura(&self, _: crate::ffi::UnitHandle, _: SpellId) -> bool {
            false
        }
        fn get_aura(
            &self,
            _: crate::ffi::UnitHandle,
            _: SpellId,
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
        fn can_cast(&self, _: SpellId, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn spell_cooldown_ms(&self, _: SpellId) -> u32 {
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
        fn cast_spell(&self, _: SpellId, _: crate::ffi::UnitHandle) -> bool {
            false
        }
        fn cast_spell_pos(&self, _: SpellId, _: f32, _: f32, _: f32) -> bool {
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

        fn bot_learn_spell(&self, spell_id: u32) {
            self.learned.borrow_mut().push(spell_id);
        }
        fn bot_learn_default_spells(&self) {
            *self.default_called.borrow_mut() += 1;
        }
        fn bot_learn_class_level_spells(&self, include_quest_rewards: bool) {
            self.class_level_called.borrow_mut().push(include_quest_rewards);
        }
    }

    #[test]
    fn always_calls_default_and_class_level_helpers() {
        let m = MockIface::default();
        init_available_spells(&m, CLASS_WARRIOR, 10);
        assert_eq!(*m.default_called.borrow(), 1);
        assert_eq!(m.class_level_called.borrow().as_slice(), &[true]);
    }

    #[cfg(not(feature = "wotlk"))]
    #[test]
    fn paladin_gets_judgement_and_strikes() {
        let m = MockIface::default();
        init_available_spells(&m, CLASS_PALADIN, 60);
        let learned = m.learned.borrow();
        assert!(learned.contains(&20271));
        assert!(learned.contains(&33394));
        assert!(learned.contains(&33395));
    }

    #[test]
    fn mage_gets_polymorph_variants_at_60() {
        let m = MockIface::default();
        init_available_spells(&m, CLASS_MAGE, 60);
        let learned = m.learned.borrow();
        assert!(learned.contains(&28271));
        assert!(learned.contains(&28272));
    }

    #[test]
    fn mage_skips_polymorph_variants_below_60() {
        let m = MockIface::default();
        init_available_spells(&m, CLASS_MAGE, 59);
        let learned = m.learned.borrow();
        assert!(!learned.contains(&28271));
        assert!(!learned.contains(&28272));
    }

    #[test]
    fn warlock_gets_inferno_at_50() {
        let m = MockIface::default();
        init_available_spells(&m, CLASS_WARLOCK, 50);
        assert!(m.learned.borrow().contains(&1122));
    }

    #[test]
    fn warlock_skips_inferno_below_50() {
        let m = MockIface::default();
        init_available_spells(&m, CLASS_WARLOCK, 49);
        assert!(!m.learned.borrow().contains(&1122));
    }

    #[cfg(feature = "vanilla")]
    #[test]
    fn classic_level_60_warrior_gets_book_spells() {
        let m = MockIface::default();
        init_available_spells(&m, CLASS_WARRIOR, 60);
        let learned = m.learned.borrow();
        for &id in &[25289u32, 25288, 25958] {
            assert!(learned.contains(&id), "missing warrior book spell {id}");
        }
    }

    #[cfg(feature = "vanilla")]
    #[test]
    fn classic_level_59_warrior_skips_book_spells() {
        let m = MockIface::default();
        init_available_spells(&m, CLASS_WARRIOR, 59);
        let learned = m.learned.borrow();
        assert!(!learned.contains(&25289));
    }
}
