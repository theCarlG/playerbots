//! Progression wipe — clears trade skills, the spellbook, and the quest log
//! so the factory can re-roll a bot's character progression from scratch.
//!
//! Corresponds to the C++ `PlayerbotFactory::ClearSkills`, `ClearSpells`,
//! and `ResetQuests` methods. The policy here is the fixed list of trade
//! skills to wipe; the actual game-state mutations are delegated back
//! through the `BotInterface` callbacks.

use crate::ffi::interface::BotInterface;

// ── Skill IDs ─────────────────────────────────────────────────────────────
//
// Values mirror `SharedDefines.h` in the CMaNGOS core. Kept here as plain
// constants so the policy layer stays self-contained and testable.

const SKILL_FIRST_AID: u32 = 129;
const SKILL_BLACKSMITHING: u32 = 164;
const SKILL_LEATHERWORKING: u32 = 165;
const SKILL_ALCHEMY: u32 = 171;
const SKILL_HERBALISM: u32 = 182;
const SKILL_COOKING: u32 = 185;
const SKILL_MINING: u32 = 186;
const SKILL_TAILORING: u32 = 197;
const SKILL_ENGINEERING: u32 = 202;
const SKILL_ENCHANTING: u32 = 333;
const SKILL_FISHING: u32 = 356;
const SKILL_SKINNING: u32 = 393;
#[cfg(not(feature = "vanilla"))]
const SKILL_JEWELCRAFTING: u32 = 755;

/// Trade skills cleared by the factory before re-rolling professions.
///
/// Kept as a `fn` rather than a `const` so the `cfg`-gated TBC/WotLK
/// jewelcrafting entry can be appended without a `#[cfg]` inside a
/// const array literal.
pub fn trade_skills() -> &'static [u32] {
    #[cfg(feature = "vanilla")]
    {
        &[
            SKILL_ALCHEMY,
            SKILL_ENCHANTING,
            SKILL_SKINNING,
            SKILL_TAILORING,
            SKILL_LEATHERWORKING,
            SKILL_ENGINEERING,
            SKILL_HERBALISM,
            SKILL_MINING,
            SKILL_BLACKSMITHING,
            SKILL_COOKING,
            SKILL_FIRST_AID,
            SKILL_FISHING,
        ]
    }
    #[cfg(not(feature = "vanilla"))]
    {
        &[
            SKILL_ALCHEMY,
            SKILL_ENCHANTING,
            SKILL_SKINNING,
            SKILL_TAILORING,
            SKILL_LEATHERWORKING,
            SKILL_ENGINEERING,
            SKILL_HERBALISM,
            SKILL_MINING,
            SKILL_BLACKSMITHING,
            SKILL_COOKING,
            SKILL_FIRST_AID,
            SKILL_FISHING,
            SKILL_JEWELCRAFTING,
        ]
    }
}

/// Clear every trade skill the factory knows about.
///
/// Mirrors `PlayerbotFactory::ClearSkills`. The two `PLAYER_SKILL_INDEX(0/1)`
/// slot resets from the original are intentionally omitted — the original
/// call site is commented out upstream, and those writes are a WoW internal
/// detail that does not belong on the Rust side of the FFI.
pub fn clear_trade_skills(iface: &dyn BotInterface) {
    for &skill_id in trade_skills() {
        iface.bot_clear_skill(skill_id);
    }
}

/// Reset the bot's spellbook to class defaults.
///
/// Mirrors `PlayerbotFactory::ClearSpells`. Delegates straight to CMaNGOS'
/// `Player::resetSpells()` via the callback — there is no Rust-side policy.
pub fn clear_spells(iface: &dyn BotInterface) {
    iface.bot_reset_spells();
}

/// Drop every quest in the bot's log and wipe the rewarded-quest state.
///
/// Mirrors `PlayerbotFactory::ResetQuests`. Again a pure delegation: the
/// iteration over `sObjectMgr.GetQuestTemplates()` and the DB delete happen
/// on the C++ side because that data is not available to Rust.
pub fn reset_all_quests(iface: &dyn BotInterface) {
    iface.bot_reset_all_quests();
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::interface::BotInterface;
    use crate::ffi::types::ItemId;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Calls {
        cleared_skills: Vec<u32>,
        reset_spells_count: u32,
        reset_quests_count: u32,
    }

    #[derive(Default)]
    struct MockIface {
        calls: RefCell<Calls>,
    }

    // Safety: RefCell is !Sync but tests are single-threaded; BotInterface
    // requires Send, so we wrap by hand the same way the other factory mocks do.
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

        fn bot_clear_skill(&self, skill_id: u32) {
            self.calls.borrow_mut().cleared_skills.push(skill_id);
        }
        fn bot_reset_spells(&self) {
            self.calls.borrow_mut().reset_spells_count += 1;
        }
        fn bot_reset_all_quests(&self) {
            self.calls.borrow_mut().reset_quests_count += 1;
        }
    }

    #[test]
    fn clear_trade_skills_calls_every_skill_exactly_once() {
        let mock = MockIface::default();
        clear_trade_skills(&mock);

        let calls = mock.calls.borrow();
        assert_eq!(calls.cleared_skills.len(), trade_skills().len());
        for &skill in trade_skills() {
            assert!(
                calls.cleared_skills.contains(&skill),
                "missing clear for skill {skill}",
            );
        }
    }

    #[test]
    fn trade_skills_list_is_unique() {
        let list = trade_skills();
        let mut sorted: Vec<u32> = list.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            list.len(),
            "duplicate skill id in trade_skills()"
        );
    }

    #[test]
    fn trade_skills_includes_core_professions() {
        let list = trade_skills();
        assert!(list.contains(&SKILL_ALCHEMY));
        assert!(list.contains(&SKILL_ENCHANTING));
        assert!(list.contains(&SKILL_FIRST_AID));
        assert!(list.contains(&SKILL_FISHING));
    }

    #[test]
    fn clear_spells_delegates_once() {
        let mock = MockIface::default();
        clear_spells(&mock);
        clear_spells(&mock);
        assert_eq!(mock.calls.borrow().reset_spells_count, 2);
    }

    #[test]
    fn reset_all_quests_delegates_once() {
        let mock = MockIface::default();
        reset_all_quests(&mock);
        assert_eq!(mock.calls.borrow().reset_quests_count, 1);
    }
}
