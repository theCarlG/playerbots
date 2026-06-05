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

use super::FactoryTransaction;

// ── Class gate (matches `CLASS_*` from SharedDefines.h) ───────────────────
const CLASS_WARRIOR: u8 = 1;
const CLASS_HUNTER: u8 = 3;
const CLASS_ROGUE: u8 = 4;

/// The three classes that can equip an ammo-consuming ranged weapon. Shared
/// with the runtime ammo-restock leaf.
pub(crate) fn is_ammo_class(class_id: u8) -> bool {
    matches!(class_id, CLASS_WARRIOR | CLASS_HUNTER | CLASS_ROGUE)
}

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
pub(crate) const AMMO_STACK: u32 = 200;
pub(crate) const LOW_STACKS: u32 = 2;

/// Decide the ammo subclass for the given ranged weapon subclass, or
/// `None` when no ammo is required.
pub(crate) fn ammo_for_weapon(weapon_subclass: u32, class_id: u8) -> Option<u32> {
    match weapon_subclass {
        WEAPON_GUN => Some(AMMO_BULLET),
        WEAPON_BOW | WEAPON_CROSSBOW => Some(AMMO_ARROW),
        WEAPON_THROWN if class_id != CLASS_HUNTER => Some(AMMO_THROWN),
        _ => None,
    }
}

/// Run the factory `InitAmmo` step for a bot.
///
/// `class_id`, `level` come from `BotWorldSnapshot.self` — the dispatcher
/// snapshots these once and passes them in, avoiding a repeated FFI call.
pub fn init_ammo(tx: &mut FactoryTransaction<'_>, class_id: u8, level: u32) {
    // Only the three ammo-using classes.
    if class_id != CLASS_WARRIOR && class_id != CLASS_HUNTER && class_id != CLASS_ROGUE {
        return;
    }

    let weapon_sub = tx.bot_equipped_ranged_subclass();
    if weapon_sub == u32::MAX {
        return; // No ranged weapon equipped.
    }
    let Some(ammo_sub) = ammo_for_weapon(weapon_sub, class_id) else {
        return;
    };

    let max_count = 5 + level / 10;

    let mut entry = tx.bot_current_ammo_id();
    let mut count = if entry == 0 {
        0
    } else {
        tx.item_count_in_bags(cmangos::ItemId(entry)) / AMMO_STACK
    };

    // Pick fresh ammo if none equipped or stack count is critically low.
    if entry == 0 || count <= LOW_STACKS {
        entry = tx.factory_pick_ammo_for_level(level, ammo_sub);
        if entry == 0 {
            return;
        }
        count = tx.item_count_in_bags(cmangos::ItemId(entry)) / AMMO_STACK;
    }

    // Top up to max_count stacks.
    if count < max_count {
        let missing = max_count - count;
        for _ in 0..missing {
            tx.inventory_add_item(cmangos::ItemId(entry), AMMO_STACK);
        }
    }

    // Equip as the active ammo if it isn't already.
    if tx.bot_current_ammo_id() != entry {
        tx.bot_set_ammo(entry);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::with_tx;
    use cmangos::{ItemId, MockEvent, MockWorld, World};

    fn ammo_world(ranged_subclass: u32, pick_returns: u32) -> MockWorld {
        MockWorld::builder()
            .equipped_ranged_subclass(ranged_subclass)
            .ammo_pick(60, AMMO_BULLET, pick_returns)
            .ammo_pick(60, AMMO_ARROW, pick_returns)
            .ammo_pick(60, AMMO_THROWN, pick_returns)
            .ammo_pick(40, AMMO_BULLET, pick_returns)
            .ammo_pick(40, AMMO_ARROW, pick_returns)
            .ammo_pick(40, AMMO_THROWN, pick_returns)
            .ammo_pick(20, AMMO_BULLET, pick_returns)
            .ammo_pick(20, AMMO_ARROW, pick_returns)
            .ammo_pick(20, AMMO_THROWN, pick_returns)
            .build()
    }

    fn add_count(world: &MockWorld) -> usize {
        world
            .events()
            .iter()
            .filter(|e| matches!(e, MockEvent::InventoryAddItem { .. }))
            .count()
    }

    fn set_ammo_calls(world: &MockWorld) -> Vec<u32> {
        world
            .events()
            .iter()
            .filter_map(|e| match e {
                MockEvent::SetAmmo(id) => Some(*id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn noop_for_non_ammo_class() {
        let mut w = ammo_world(WEAPON_BOW, 2512);
        with_tx(&mut w, |tx| init_ammo(tx, 8 /* mage */, 60));
        assert_eq!(add_count(&w), 0);
        assert!(set_ammo_calls(&w).is_empty());
    }

    #[test]
    fn noop_when_no_ranged_equipped() {
        let mut w = ammo_world(u32::MAX, 2512);
        with_tx(&mut w, |tx| init_ammo(tx, CLASS_HUNTER, 60));
        assert_eq!(add_count(&w), 0);
    }

    #[test]
    fn hunter_with_thrown_weapon_is_noop() {
        let mut w = ammo_world(WEAPON_THROWN, 999);
        with_tx(&mut w, |tx| init_ammo(tx, CLASS_HUNTER, 60));
        assert_eq!(add_count(&w), 0);
    }

    #[test]
    fn rogue_with_thrown_weapon_gets_thrown_ammo() {
        let mut w = ammo_world(WEAPON_THROWN, 2512);
        with_tx(&mut w, |tx| init_ammo(tx, CLASS_ROGUE, 60));
        assert_eq!(w.bot_current_ammo_id(), 2512);
        // 5 + 60/10 = 11 stacks of 200
        assert_eq!(add_count(&w), 11);
        for e in w.events() {
            if let MockEvent::InventoryAddItem { item, count } = e {
                assert_eq!(item.0, 2512);
                assert_eq!(count, AMMO_STACK);
            }
        }
    }

    #[test]
    fn bow_gets_arrows_level_scales_stack_count() {
        let mut w = ammo_world(WEAPON_BOW, 2516);
        with_tx(&mut w, |tx| init_ammo(tx, CLASS_HUNTER, 40));
        // 5 + 40/10 = 9 stacks
        assert_eq!(add_count(&w), 9);
    }

    #[test]
    fn gun_gets_bullets() {
        let mut w = ammo_world(WEAPON_GUN, 2519);
        with_tx(&mut w, |tx| init_ammo(tx, CLASS_WARRIOR, 20));
        // 5 + 20/10 = 7 stacks
        assert_eq!(add_count(&w), 7);
        assert_eq!(w.bot_current_ammo_id(), 2519);
    }

    #[test]
    fn keeps_current_ammo_when_stocked() {
        let mut w = MockWorld::builder()
            .equipped_ranged_subclass(WEAPON_BOW)
            .current_ammo_id(2512)
            .ammo_pick(60, AMMO_ARROW, 2516)
            // Pre-seed inventory with 20 stacks of the already-equipped ammo (>LOW).
            .item_in_bags(2512, AMMO_STACK * 20)
            .build();
        with_tx(&mut w, |tx| init_ammo(tx, CLASS_HUNTER, 60));
        assert_eq!(add_count(&w), 0);
        assert!(set_ammo_calls(&w).is_empty());
    }

    #[test]
    fn replaces_low_stock_ammo() {
        let mut w = MockWorld::builder()
            .equipped_ranged_subclass(WEAPON_BOW)
            .current_ammo_id(2512)
            .ammo_pick(60, AMMO_ARROW, 2516)
            // Only 2 stacks of the old ammo → ≤ LOW_STACKS triggers replacement.
            .item_in_bags(2512, AMMO_STACK * 2)
            .build();
        with_tx(&mut w, |tx| init_ammo(tx, CLASS_HUNTER, 60));
        assert_eq!(w.bot_current_ammo_id(), 2516);
        assert_eq!(set_ammo_calls(&w), vec![2516]);
        // 11 stacks of the new ammo added.
        assert_eq!(add_count(&w), 11);
    }
}

