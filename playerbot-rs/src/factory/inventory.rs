//! Inventory management for the factory.
//!
//! The real work (iterating bag slots, calling `Player::DestroyItem`) lives on
//! the C++ side in `BotBridge::CB_InventoryDestroy*`. The Rust policy layer
//! just decides *which* wipe to issue and (later, for restock) *what* items to
//! add back.

use super::ClearScope;
use crate::ffi::interface::BotInterface;
use crate::ffi::types::ItemId;

/// Clear bag+equipped (and optionally bank) via the FFI.
pub fn clear(iface: &dyn BotInterface, scope: ClearScope) {
    match scope {
        ClearScope::EquippedAndBags => iface.inventory_destroy_equipped_and_bags(),
        ClearScope::All => iface.inventory_destroy_all(),
    }
}

/// How many of `item_id` the bot already carries (excluding bank).
///
/// Used by restock helpers: `InitPotions`, `InitFood`, `InitReagents` etc. will
/// first query this to decide how many to add.
pub fn count_in_bags(iface: &dyn BotInterface, item_id: ItemId) -> u32 {
    iface.item_count_in_bags(item_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::interface::BotInterface;
    use std::cell::Cell;

    /// Minimal mock — records which clear variant was called.
    #[derive(Default)]
    struct MockIface {
        equipped_and_bags: Cell<u32>,
        all: Cell<u32>,
        count_query: Cell<u32>,
        count_reply: u32,
    }

    // Safety: Cell is !Sync but we only use it single-threaded in tests.
    // BotInterface requires Send; wrap Cell by hand.
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

        fn inventory_destroy_equipped_and_bags(&self) {
            self.equipped_and_bags.set(self.equipped_and_bags.get() + 1);
        }
        fn inventory_destroy_all(&self) {
            self.all.set(self.all.get() + 1);
        }
        fn item_count_in_bags(&self, _: ItemId) -> u32 {
            self.count_query.set(self.count_query.get() + 1);
            self.count_reply
        }
    }

    #[test]
    fn clear_equipped_and_bags_calls_partial_wipe() {
        let m = MockIface::default();
        clear(&m, ClearScope::EquippedAndBags);
        assert_eq!(m.equipped_and_bags.get(), 1);
        assert_eq!(m.all.get(), 0);
    }

    #[test]
    fn clear_all_calls_full_wipe() {
        let m = MockIface::default();
        clear(&m, ClearScope::All);
        assert_eq!(m.all.get(), 1);
        assert_eq!(m.equipped_and_bags.get(), 0);
    }

    #[test]
    fn scope_decodes_from_mode() {
        assert_eq!(ClearScope::from_mode(0), ClearScope::EquippedAndBags);
        assert_eq!(ClearScope::from_mode(1), ClearScope::All);
        assert_eq!(ClearScope::from_mode(255), ClearScope::All);
    }

    #[test]
    fn count_in_bags_delegates() {
        let mut m = MockIface::default();
        m.count_reply = 7;
        let n = count_in_bags(&m, ItemId(12345));
        assert_eq!(n, 7);
        assert_eq!(m.count_query.get(), 1);
    }
}
