//! Factory `InitInventoryTrade` — mirrors `PlayerbotFactory::InitInventoryTrade`.
//!
//! Picks one random trade good appropriate for the bot's level via
//! `sRandomItemMgr.GetRandomTrade`, inspects the prototype quality, and
//! stocks the bot's bags with 1-3 stacks (normal quality) or a single
//! partial stack (uncommon quality). Rare+ items are not stocked.
//!
//! Pure policy. All side effects flow through the `BotInterface`.

use crate::ffi::interface::BotInterface;
use crate::ffi::types::ItemId;

// Item quality constants (ItemPrototype::Quality). Matches classic/tbc/wotlk.
const ITEM_QUALITY_NORMAL: u32 = 1;
const ITEM_QUALITY_UNCOMMON: u32 = 2;

/// Give the bot one random trade good appropriate for its level.
///
/// `level` comes from `BotWorldSnapshot.self.level` — the dispatcher
/// snapshots it once and passes it in.
pub fn init_inventory_trade(iface: &dyn BotInterface, level: u32) {
    let item_id_raw = iface.factory_pick_trade_for_level(level);
    if item_id_raw == 0 {
        log_no_trade();
        return;
    }
    let item_id = ItemId(item_id_raw);

    let quality = iface.item_prototype_quality(item_id_raw);
    let max_stack = iface.item_max_stack_size(item_id);
    if max_stack == 0 {
        return;
    }

    let (count, stacks) = match quality {
        ITEM_QUALITY_NORMAL => {
            // Full stacks, randomized 1-3 of them.
            let stacks = iface.random_u32(1, 3);
            (max_stack, stacks)
        }
        ITEM_QUALITY_UNCOMMON => {
            // One partial stack of up to half the max stack size.
            let half = max_stack / 2;
            let count = if half == 0 {
                1
            } else {
                iface.random_u32(1, half)
            };
            (count, 1)
        }
        // Other qualities (poor / rare / epic / ...) are not stocked.
        _ => return,
    };

    for _ in 0..stacks {
        iface.inventory_add_item(item_id, count);
    }
}

/// Log a warning when `factory_pick_trade_for_level` returns 0. Extracted so
/// the policy function itself stays straight-line.
#[inline]
fn log_no_trade() {
    crate::log_warn!("factory: no trade items available for bot level");
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::interface::BotInterface;
    use crate::ffi::types::ItemId;
    use std::cell::RefCell;

    struct MockIface {
        trade_id: u32,
        quality: u32,
        max_stack: u32,
        random_fn: Box<dyn Fn(u32, u32) -> u32>,
        adds: RefCell<Vec<(u32, u32)>>,
    }

    unsafe impl Send for MockIface {}

    impl Default for MockIface {
        fn default() -> Self {
            Self {
                trade_id: 0,
                quality: 0,
                max_stack: 0,
                random_fn: Box::new(|min, _| min),
                adds: RefCell::new(Vec::new()),
            }
        }
    }

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

        fn factory_pick_trade_for_level(&self, _: u32) -> u32 {
            self.trade_id
        }
        fn item_prototype_quality(&self, _: u32) -> u32 {
            self.quality
        }
        fn item_max_stack_size(&self, _: ItemId) -> u32 {
            self.max_stack
        }
        fn random_u32(&self, min: u32, max: u32) -> u32 {
            (self.random_fn)(min, max)
        }
        fn inventory_add_item(&self, item_id: ItemId, count: u32) -> u32 {
            self.adds.borrow_mut().push((item_id.raw(), count));
            count
        }
    }

    #[test]
    fn noop_when_no_trade_available() {
        let m = MockIface {
            trade_id: 0,
            ..Default::default()
        };
        init_inventory_trade(&m, 40);
        assert!(m.adds.borrow().is_empty());
    }

    #[test]
    fn normal_quality_adds_full_stacks() {
        let m = MockIface {
            trade_id: 2589, // linen cloth, illustrative
            quality: ITEM_QUALITY_NORMAL,
            max_stack: 20,
            random_fn: Box::new(|_min, max| max), // always max → 3 stacks
            ..Default::default()
        };
        init_inventory_trade(&m, 20);
        assert_eq!(m.adds.borrow().len(), 3);
        for &(_, count) in m.adds.borrow().iter() {
            assert_eq!(count, 20);
        }
    }

    #[test]
    fn uncommon_quality_single_partial_stack() {
        let m = MockIface {
            trade_id: 10285,
            quality: ITEM_QUALITY_UNCOMMON,
            max_stack: 20,
            random_fn: Box::new(|_, max| max), // urand(1, 10) → 10
            ..Default::default()
        };
        init_inventory_trade(&m, 40);
        assert_eq!(m.adds.borrow().len(), 1);
        assert_eq!(m.adds.borrow()[0].1, 10);
    }

    #[test]
    fn rare_quality_is_noop() {
        let m = MockIface {
            trade_id: 9999,
            quality: 3, // rare
            max_stack: 20,
            ..Default::default()
        };
        init_inventory_trade(&m, 60);
        assert!(m.adds.borrow().is_empty());
    }

    #[test]
    fn uncommon_with_max_stack_one_still_adds_one() {
        let m = MockIface {
            trade_id: 1234,
            quality: ITEM_QUALITY_UNCOMMON,
            max_stack: 1,
            ..Default::default()
        };
        init_inventory_trade(&m, 30);
        assert_eq!(m.adds.borrow().len(), 1);
        assert_eq!(m.adds.borrow()[0].1, 1);
    }
}
