//! Factory `InitInventoryTrade` — mirrors `PlayerbotFactory::InitInventoryTrade`.
//!
//! Picks one random trade good appropriate for the bot's level via
//! `sRandomItemMgr.GetRandomTrade`, inspects the prototype quality, and
//! stocks the bot's bags with 1-3 stacks (normal quality) or a single
//! partial stack (uncommon quality). Rare+ items are not stocked.
//!
//! Pure policy. All side effects flow through the `World`.

use super::FactoryTransaction;
use cmangos::ItemId;

// Item quality constants (ItemPrototype::Quality). Matches classic/tbc/wotlk.
const ITEM_QUALITY_NORMAL: u32 = 1;
const ITEM_QUALITY_UNCOMMON: u32 = 2;

/// Give the bot one random trade good appropriate for its level.
///
/// `level` comes from `BotWorldSnapshot.self.level` — the dispatcher
/// snapshots it once and passes it in.
pub fn init_inventory_trade(tx: &mut FactoryTransaction<'_>, level: u32) {
    let item_id_raw = tx.factory_pick_trade_for_level(level);
    if item_id_raw == 0 {
        log_no_trade();
        return;
    }
    let item_id = ItemId(item_id_raw);

    let quality = tx.item_prototype_quality(item_id_raw);
    let max_stack = tx.item_max_stack_size(item_id);
    if max_stack == 0 {
        return;
    }

    let (count, stacks) = match quality {
        ITEM_QUALITY_NORMAL => {
            // Full stacks, randomized 1-3 of them.
            let stacks = tx.random_u32(1, 3);
            (max_stack, stacks)
        }
        ITEM_QUALITY_UNCOMMON => {
            // One partial stack of up to half the max stack size.
            let half = max_stack / 2;
            let count = if half == 0 {
                1
            } else {
                tx.random_u32(1, half)
            };
            (count, 1)
        }
        // Other qualities (poor / rare / epic / ...) are not stocked.
        _ => return,
    };

    for _ in 0..stacks {
        tx.inventory_add_item(item_id, count);
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
    use crate::factory::with_tx;
    use cmangos::{MockEvent, MockWorld};

    fn adds(world: &MockWorld) -> Vec<(u32, u32)> {
        world
            .events()
            .iter()
            .filter_map(|e| match e {
                MockEvent::InventoryAddItem { item, count } => Some((item.raw(), *count)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn noop_when_no_trade_available() {
        // No trade_pick set → factory_pick_trade returns 0 → noop.
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_inventory_trade(tx, 40));
        assert!(adds(&w).is_empty());
    }

    #[test]
    fn normal_quality_adds_full_stacks() {
        // Random returns the first sequence value (3 = max stacks).
        let mut w = MockWorld::builder()
            .trade_pick(20, 2589)
            .item_quality(2589, ITEM_QUALITY_NORMAL)
            .item_max_stack(2589, 20)
            .random_seq(vec![3])
            .build();
        with_tx(&mut w, |tx| init_inventory_trade(tx, 20));
        let a = adds(&w);
        assert_eq!(a.len(), 3);
        for (_, count) in &a {
            assert_eq!(*count, 20);
        }
    }

    #[test]
    fn uncommon_quality_single_partial_stack() {
        // Half of max_stack 20 = 10. Random returns 10.
        let mut w = MockWorld::builder()
            .trade_pick(40, 10285)
            .item_quality(10285, ITEM_QUALITY_UNCOMMON)
            .item_max_stack(10285, 20)
            .random_seq(vec![10])
            .build();
        with_tx(&mut w, |tx| init_inventory_trade(tx, 40));
        let a = adds(&w);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].1, 10);
    }

    #[test]
    fn rare_quality_is_noop() {
        let mut w = MockWorld::builder()
            .trade_pick(60, 9999)
            .item_quality(9999, 3) // rare
            .item_max_stack(9999, 20)
            .build();
        with_tx(&mut w, |tx| init_inventory_trade(tx, 60));
        assert!(adds(&w).is_empty());
    }

    #[test]
    fn uncommon_with_max_stack_one_still_adds_one() {
        let mut w = MockWorld::builder()
            .trade_pick(30, 1234)
            .item_quality(1234, ITEM_QUALITY_UNCOMMON)
            .item_max_stack(1234, 1)
            .build();
        with_tx(&mut w, |tx| init_inventory_trade(tx, 30));
        let a = adds(&w);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].1, 1);
    }
}
