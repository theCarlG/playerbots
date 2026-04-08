//! Factory InitSpecialSpells — mirrors `PlayerbotFactory::InitSpecialSpells`.
//!
//! Iterates the config-driven `randomBotSpellIds` list (default entry on
//! TBC/WotLK: `54197` Cold Weather Flying) and teaches every valid entry
//! to the bot. Entries that do not resolve in the spell store are skipped
//! — matching the C++ guard against `!spellInfo`.

use crate::ffi::interface::BotInterface;

/// Teach the bot every spell in `sPlayerbotAIConfig.randomBotSpellIds` that
/// resolves to a valid spell entry.
pub fn init_special_spells(iface: &dyn BotInterface) {
    let ids = iface.get_random_bot_spell_ids();
    for spell_id in ids {
        if iface.get_spell_info(spell_id).is_none() {
            continue;
        }
        iface.bot_learn_spell(spell_id);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::BotSpellInfo;
    use crate::ffi::interface::BotInterface;
    use crate::ffi::types::{ItemId, SpellId};
    use std::cell::RefCell;
    use std::collections::HashSet;

    struct MockIface {
        config_ids: Vec<u32>,
        valid_ids: HashSet<u32>,
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

        fn get_random_bot_spell_ids(&self) -> Vec<u32> {
            self.config_ids.clone()
        }
        fn get_spell_info(&self, spell_id: u32) -> Option<BotSpellInfo> {
            if !self.valid_ids.contains(&spell_id) {
                return None;
            }
            let mut info: BotSpellInfo = unsafe { std::mem::zeroed() };
            info.id = spell_id;
            info.is_valid = true;
            Some(info)
        }
        fn bot_learn_spell(&self, spell_id: u32) {
            self.learned.borrow_mut().push(spell_id);
        }
    }

    #[test]
    fn empty_config_is_noop() {
        let m = MockIface {
            config_ids: vec![],
            valid_ids: HashSet::new(),
            learned: RefCell::new(Vec::new()),
        };
        init_special_spells(&m);
        assert!(m.learned.borrow().is_empty());
    }

    #[test]
    fn learns_only_valid_entries() {
        let m = MockIface {
            config_ids: vec![1, 2, 3],
            valid_ids: [1u32, 3].into_iter().collect(),
            learned: RefCell::new(Vec::new()),
        };
        init_special_spells(&m);
        let learned = m.learned.borrow().clone();
        assert_eq!(learned, vec![1, 3]);
    }

    #[test]
    fn single_entry_config() {
        let m = MockIface {
            config_ids: vec![54197],
            valid_ids: [54197u32].into_iter().collect(),
            learned: RefCell::new(Vec::new()),
        };
        init_special_spells(&m);
        assert_eq!(m.learned.borrow().as_slice(), &[54197]);
    }
}
