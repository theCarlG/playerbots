//! Inventory management for the factory.
//!
//! The real work (iterating bag slots, calling `Player::DestroyItem`) lives on
//! the C++ side in `BotBridge::CB_InventoryDestroy*`. The Rust policy layer
//! just decides *which* wipe to issue and (later, for restock) *what* items to
//! add back.

use super::{ClearScope, FactoryTransaction};
use cmangos::ItemId;

/// Clear bag+equipped (and optionally bank) via the FFI.
pub fn clear(tx: &mut FactoryTransaction<'_>, scope: ClearScope) {
    match scope {
        ClearScope::EquippedAndBags => tx.inventory_destroy_equipped_and_bags(),
        ClearScope::All => tx.inventory_destroy_all(),
    }
}

/// How many of `item_id` the bot already carries (excluding bank).
///
/// Used by restock helpers: `InitPotions`, `InitFood`, `InitReagents` etc. will
/// first query this to decide how many to add.
pub fn count_in_bags(tx: &mut FactoryTransaction<'_>, item_id: ItemId) -> u32 {
    tx.item_count_in_bags(item_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::with_tx;
    use cmangos::{MockEvent, MockWorld};

    #[test]
    fn clear_equipped_and_bags_calls_partial_wipe() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| clear(tx, ClearScope::EquippedAndBags));
        assert!(matches!(
            w.last_event(),
            Some(MockEvent::DestroyEquippedAndBags)
        ));
    }

    #[test]
    fn clear_all_calls_full_wipe() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| clear(tx, ClearScope::All));
        assert!(matches!(w.last_event(), Some(MockEvent::DestroyAll)));
    }

    #[test]
    fn scope_decodes_from_mode() {
        assert_eq!(ClearScope::from_mode(0), ClearScope::EquippedAndBags);
        assert_eq!(ClearScope::from_mode(1), ClearScope::All);
        assert_eq!(ClearScope::from_mode(255), ClearScope::All);
    }

    #[test]
    fn count_in_bags_delegates() {
        let mut w = MockWorld::builder().item_in_bags(12345, 7).build();
        let n = with_tx(&mut w, |tx| count_in_bags(tx, ItemId(12345)));
        assert_eq!(n, 7);
    }
}
