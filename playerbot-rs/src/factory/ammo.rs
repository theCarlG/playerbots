//! Factory ammo initialization — mirrors `PlayerbotFactory::InitAmmo`.
//!
//! Only warriors, rogues, and hunters are considered (the three classes that
//! can equip a ranged weapon that consumes ammo). The bot's ranged slot is
//! inspected: gun → bullet, bow/crossbow → arrow, thrown → thrown (except for
//! hunters, whose "thrown" category is handled by the weapon itself and does
//! not need ammo). The correct ammo stack count is `5 + level/10` stacks of
//! 200, topped up from whatever the bot currently carries.
//!
//! Pure policy. All state lives on the C++ side; this module reads the
//! current ranged weapon / ammo id / inventory count over the FFI and calls
//! `bot_set_ammo` + `inventory_add_item` to top up.
//!
//! Constants come straight from `Entities/ItemPrototype.h`.

use crate::ffi::interface::BotInterface;

// ── Class gate (matches `CLASS_*` from SharedDefines.h) ───────────────────
const CLASS_WARRIOR: u8 = 1;
const CLASS_HUNTER: u8 = 3;
const CLASS_ROGUE: u8 = 4;

// ── Weapon subclasses we care about ──────────────────────────────────────
const WEAPON_BOW: u32 = 2;
const WEAPON_GUN: u32 = 3;
const WEAPON_THROWN: u32 = 16;
const WEAPON_CROSSBOW: u32 = 18;

// ── Ammo subclasses (ITEM_CLASS_PROJECTILE / ITEM_CLASS_WEAPON thrown) ────
const AMMO_ARROW: u32 = 2;
const AMMO_BULLET: u32 = 3;
const AMMO_THROWN: u32 = 4;

// Ammo is stored in stacks of 200 (hard-coded in the C++ source). Stack size
// and the min-refill threshold are both in units of 200.
const AMMO_STACK: u32 = 200;
const LOW_STACKS: u32 = 2;

/// Decide the ammo subclass for the given ranged weapon subclass, or
/// `None` when no ammo is required.
fn ammo_for_weapon(weapon_subclass: u32, class_id: u8) -> Option<u32> {
    match weapon_subclass {
        WEAPON_GUN => Some(AMMO_BULLET),
        WEAPON_BOW | WEAPON_CROSSBOW => Some(AMMO_ARROW),
        WEAPON_THROWN if class_id != CLASS_HUNTER => Some(AMMO_THROWN),
        _ => None,
    }
}

/// Run the factory InitAmmo step for a bot.
///
/// `class_id`, `level` come from `BotWorldSnapshot.self` — the dispatcher
/// snapshots these once and passes them in, avoiding a repeated FFI call.
pub fn init_ammo(iface: &dyn BotInterface, class_id: u8, level: u32) {
    // Only the three ammo-using classes.
    if class_id != CLASS_WARRIOR && class_id != CLASS_HUNTER && class_id != CLASS_ROGUE {
        return;
    }

    let weapon_sub = iface.bot_equipped_ranged_subclass();
    if weapon_sub == u32::MAX {
        return; // No ranged weapon equipped.
    }
    let Some(ammo_sub) = ammo_for_weapon(weapon_sub, class_id) else {
        return;
    };

    let max_count = 5 + level / 10;

    let mut entry = iface.bot_current_ammo_id();
    let mut count = if entry == 0 {
        0
    } else {
        iface.item_count_in_bags(crate::ffi::types::ItemId(entry)) / AMMO_STACK
    };

    // Pick fresh ammo if none equipped or stack count is critically low.
    if entry == 0 || count <= LOW_STACKS {
        entry = iface.factory_pick_ammo_for_level(level, ammo_sub);
        if entry == 0 {
            return;
        }
        count = iface.item_count_in_bags(crate::ffi::types::ItemId(entry)) / AMMO_STACK;
    }

    // Top up to max_count stacks.
    if count < max_count {
        let missing = max_count - count;
        for _ in 0..missing {
            iface.inventory_add_item(crate::ffi::types::ItemId(entry), AMMO_STACK);
        }
    }

    // Equip as the active ammo if it isn't already.
    if iface.bot_current_ammo_id() != entry {
        iface.bot_set_ammo(entry);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::interface::BotInterface;
    use crate::ffi::types::ItemId;
    use std::cell::RefCell;

    #[derive(Default)]
    struct MockIface {
        ranged_subclass: u32,
        current_ammo: RefCell<u32>,
        pick_returns: u32,
        inventory: RefCell<std::collections::HashMap<u32, u32>>,
        adds: RefCell<Vec<(u32, u32)>>,
        set_ammo_calls: RefCell<Vec<u32>>,
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

        fn bot_equipped_ranged_subclass(&self) -> u32 {
            self.ranged_subclass
        }
        fn bot_current_ammo_id(&self) -> u32 {
            *self.current_ammo.borrow()
        }
        fn factory_pick_ammo_for_level(&self, _: u32, _: u32) -> u32 {
            self.pick_returns
        }
        fn bot_set_ammo(&self, item_id: u32) {
            self.set_ammo_calls.borrow_mut().push(item_id);
            *self.current_ammo.borrow_mut() = item_id;
        }
        fn item_count_in_bags(&self, item_id: ItemId) -> u32 {
            self.inventory.borrow().get(&item_id.raw()).copied().unwrap_or(0)
        }
        fn inventory_add_item(&self, item_id: ItemId, count: u32) -> u32 {
            self.adds.borrow_mut().push((item_id.raw(), count));
            *self.inventory.borrow_mut().entry(item_id.raw()).or_insert(0) += count;
            count
        }
    }

    #[test]
    fn noop_for_non_ammo_class() {
        let m = MockIface { ranged_subclass: WEAPON_BOW, pick_returns: 2512, ..Default::default() };
        init_ammo(&m, 8 /* mage */, 60);
        assert!(m.adds.borrow().is_empty());
        assert!(m.set_ammo_calls.borrow().is_empty());
    }

    #[test]
    fn noop_when_no_ranged_equipped() {
        let m = MockIface { ranged_subclass: u32::MAX, pick_returns: 2512, ..Default::default() };
        init_ammo(&m, CLASS_HUNTER, 60);
        assert!(m.adds.borrow().is_empty());
    }

    #[test]
    fn hunter_with_thrown_weapon_is_noop() {
        let m = MockIface { ranged_subclass: WEAPON_THROWN, pick_returns: 999, ..Default::default() };
        init_ammo(&m, CLASS_HUNTER, 60);
        assert!(m.adds.borrow().is_empty());
    }

    #[test]
    fn rogue_with_thrown_weapon_gets_thrown_ammo() {
        let m = MockIface { ranged_subclass: WEAPON_THROWN, pick_returns: 2512, ..Default::default() };
        init_ammo(&m, CLASS_ROGUE, 60);
        assert_eq!(*m.current_ammo.borrow(), 2512);
        // 5 + 60/10 = 11 stacks of 200
        assert_eq!(m.adds.borrow().len(), 11);
        for &(id, n) in m.adds.borrow().iter() {
            assert_eq!(id, 2512);
            assert_eq!(n, AMMO_STACK);
        }
    }

    #[test]
    fn bow_gets_arrows_level_scales_stack_count() {
        let m = MockIface { ranged_subclass: WEAPON_BOW, pick_returns: 2516, ..Default::default() };
        init_ammo(&m, CLASS_HUNTER, 40);
        // 5 + 40/10 = 9 stacks
        assert_eq!(m.adds.borrow().len(), 9);
    }

    #[test]
    fn gun_gets_bullets() {
        let m = MockIface { ranged_subclass: WEAPON_GUN, pick_returns: 2519, ..Default::default() };
        init_ammo(&m, CLASS_WARRIOR, 20);
        // 5 + 20/10 = 7 stacks
        assert_eq!(m.adds.borrow().len(), 7);
        assert_eq!(*m.current_ammo.borrow(), 2519);
    }

    #[test]
    fn keeps_current_ammo_when_stocked() {
        let m = MockIface {
            ranged_subclass: WEAPON_BOW,
            pick_returns: 2516,
            current_ammo: RefCell::new(2512),
            ..Default::default()
        };
        // Pre-seed inventory with 11 stacks of the already-equipped ammo (>LOW).
        m.inventory.borrow_mut().insert(2512, AMMO_STACK * 20);
        init_ammo(&m, CLASS_HUNTER, 60);
        // No new stacks needed and no set_ammo call (still 2512).
        assert!(m.adds.borrow().is_empty());
        assert!(m.set_ammo_calls.borrow().is_empty());
    }

    #[test]
    fn replaces_low_stock_ammo() {
        let m = MockIface {
            ranged_subclass: WEAPON_BOW,
            pick_returns: 2516,
            current_ammo: RefCell::new(2512),
            ..Default::default()
        };
        // Only 2 stacks of the old ammo → ≤ LOW_STACKS triggers replacement.
        m.inventory.borrow_mut().insert(2512, AMMO_STACK * 2);
        init_ammo(&m, CLASS_HUNTER, 60);
        assert_eq!(*m.current_ammo.borrow(), 2516);
        assert_eq!(m.set_ammo_calls.borrow().as_slice(), &[2516]);
        // 11 stacks of the new ammo added.
        assert_eq!(m.adds.borrow().len(), 11);
    }
}
