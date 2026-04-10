//! Miscellaneous factory steps — small ports that don't warrant a whole
//! module of their own.
//!
//!   * `cancel_auras` — strip every aura before re-rolling a bot
//!     (mirrors `PlayerbotFactory::CancelAuras`).
//!   * `init_skill_tool_kit` — hand out the mandatory trade-skill tools
//!     (mining pick, blacksmith hammer, etc.). Mirrors
//!     `PlayerbotFactory::InitInventorySkill`.
//!
//! The skill-tool list is a static (`skill_id`, `item_id`) table verbatim
//! from the C++ source. We dedup via `item_count_in_bags` so re-running
//! the factory on an existing bot does not double up on tools.

use super::FactoryTransaction;
use cmangos::ItemId;

// Skill IDs — mirror the handful we care about here. Full list lives in
// `factory::progression`, but duplicating these three avoids a cross-module
// dependency on a private constant.
const SKILL_MINING: u32 = 186;
const SKILL_BLACKSMITHING: u32 = 164;
const SKILL_ENGINEERING: u32 = 202;
const SKILL_ENCHANTING: u32 = 333;
const SKILL_SKINNING: u32 = 393;

// Item IDs for mandatory profession tools.
const ITEM_MINING_PICK: ItemId = ItemId(2901);
const ITEM_BLACKSMITH_HAMMER: ItemId = ItemId(5956);
const ITEM_ARCLIGHT_SPANNER: ItemId = ItemId(6219);
const ITEM_RUNED_ARCANITE_ROD: ItemId = ItemId(16207);
const ITEM_SKINNING_KNIFE: ItemId = ItemId(7005);

/// One entry in the skill-tool starter kit table.
struct ToolEntry {
    skills: &'static [u32], // any of these grants the tool
    item: ItemId,
}

// ── Public entry points ───────────────────────────────────────────────────

/// Strip every aura (buffs & debuffs) from the bot.
///
/// Mirrors `PlayerbotFactory::CancelAuras`. Delegates straight to the
/// game — there is no Rust-side policy here, but it lives inside the
/// factory module so all re-roll steps flow through one entry point.
pub fn cancel_auras(tx: &mut FactoryTransaction<'_>) {
    tx.bot_remove_all_auras();
}

/// Equip a starter bag into every empty bag slot.
///
/// Mirrors `PlayerbotFactory::InitBags`: on Classic, every empty slot of
/// `INVENTORY_SLOT_BAG_START..END` gets a Traveler's Backpack (item 4500).
/// On TBC/WotLK it would be a Netherweave Bag (23162) — wired via cfg.
#[cfg(feature = "vanilla")]
const STARTER_BAG: ItemId = ItemId(4500); // Traveler's Backpack
#[cfg(not(feature = "vanilla"))]
const STARTER_BAG: ItemId = ItemId(23162); // Netherweave Bag

pub fn init_bags(tx: &mut FactoryTransaction<'_>) {
    let empty = tx.bot_empty_bag_slot_count();
    for _ in 0..empty {
        tx.bot_store_new_in_best_slots(STARTER_BAG, 1);
    }
}

/// Give the bot the mandatory tool for each trade skill it already knows.
///
/// Mirrors `PlayerbotFactory::InitInventorySkill`. The C++ version calls
/// `StoreItem` which dedups via `HasItemCount`; we mirror that here by
/// skipping any tool already in bags.
pub fn init_skill_tool_kit(tx: &mut FactoryTransaction<'_>) {
    for entry in tool_table() {
        if !entry.skills.iter().any(|&s| tx.bot_has_skill(s)) {
            continue;
        }
        if tx.item_count_in_bags(entry.item) > 0 {
            continue;
        }
        tx.inventory_add_item(entry.item, 1);
    }
}

/// Static skill→tool mapping. Order matches the C++ source for diffability.
fn tool_table() -> &'static [ToolEntry] {
    &[
        ToolEntry {
            skills: &[SKILL_MINING],
            item: ITEM_MINING_PICK,
        },
        ToolEntry {
            skills: &[SKILL_BLACKSMITHING, SKILL_ENGINEERING],
            item: ITEM_BLACKSMITH_HAMMER,
        },
        ToolEntry {
            skills: &[SKILL_ENGINEERING],
            item: ITEM_ARCLIGHT_SPANNER,
        },
        ToolEntry {
            skills: &[SKILL_ENCHANTING],
            item: ITEM_RUNED_ARCANITE_ROD,
        },
        ToolEntry {
            skills: &[SKILL_SKINNING],
            item: ITEM_SKINNING_KNIFE,
        },
    ]
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::with_tx;
    use cmangos::{MockEvent, MockWorld};
    use std::collections::HashSet;

    fn added(world: &MockWorld) -> Vec<(ItemId, u32)> {
        world
            .events()
            .iter()
            .filter_map(|e| match e {
                MockEvent::InventoryAddItem { item, count } => Some((*item, *count)),
                _ => None,
            })
            .collect()
    }

    fn bags_added(world: &MockWorld) -> Vec<(ItemId, u32)> {
        world
            .events()
            .iter()
            .filter_map(|e| match e {
                MockEvent::StoreInBestSlots { item, count } => Some((*item, *count)),
                _ => None,
            })
            .collect()
    }

    fn world_with_skills(skills: &[u32]) -> MockWorld {
        let mut b = MockWorld::builder();
        for &s in skills {
            b = b.skill(s, 1, 1);
        }
        b.build()
    }

    #[test]
    fn cancel_auras_delegates_once() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| cancel_auras(tx));
        let count = w
            .events()
            .iter()
            .filter(|e| matches!(e, MockEvent::RemoveAllAuras))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn skill_tool_kit_adds_mining_pick_for_miner() {
        let mut w = world_with_skills(&[SKILL_MINING]);
        with_tx(&mut w, |tx| init_skill_tool_kit(tx));
        let a = added(&w);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0], (ITEM_MINING_PICK, 1));
    }

    #[test]
    fn skill_tool_kit_noop_for_bot_with_no_trade_skills() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_skill_tool_kit(tx));
        assert!(added(&w).is_empty());
    }

    #[test]
    fn skill_tool_kit_skips_tool_already_in_bags() {
        let mut w = MockWorld::builder()
            .skill(SKILL_MINING, 1, 1)
            .item_in_bags(ITEM_MINING_PICK.raw(), 1)
            .build();
        with_tx(&mut w, |tx| init_skill_tool_kit(tx));
        assert!(added(&w).is_empty());
    }

    #[test]
    fn engineer_gets_both_hammer_and_spanner() {
        let mut w = world_with_skills(&[SKILL_ENGINEERING]);
        with_tx(&mut w, |tx| init_skill_tool_kit(tx));
        let a = added(&w);
        assert_eq!(a.len(), 2);
        let items: HashSet<ItemId> = a.iter().map(|(i, _)| *i).collect();
        assert!(items.contains(&ITEM_BLACKSMITH_HAMMER));
        assert!(items.contains(&ITEM_ARCLIGHT_SPANNER));
    }

    #[test]
    fn blacksmith_gets_hammer_but_not_spanner() {
        let mut w = world_with_skills(&[SKILL_BLACKSMITHING]);
        with_tx(&mut w, |tx| init_skill_tool_kit(tx));
        let a = added(&w);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0], (ITEM_BLACKSMITH_HAMMER, 1));
    }

    #[test]
    fn enchanter_gets_rod() {
        let mut w = world_with_skills(&[SKILL_ENCHANTING]);
        with_tx(&mut w, |tx| init_skill_tool_kit(tx));
        let a = added(&w);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0], (ITEM_RUNED_ARCANITE_ROD, 1));
    }

    #[test]
    fn init_bags_is_noop_when_no_empty_slots() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_bags(tx));
        assert!(bags_added(&w).is_empty());
    }

    #[test]
    fn init_bags_equips_one_starter_bag_per_empty_slot() {
        let mut w = MockWorld::builder().empty_bag_slots(3).build();
        with_tx(&mut w, |tx| init_bags(tx));
        let a = bags_added(&w);
        assert_eq!(a.len(), 3);
        for entry in &a {
            assert_eq!(*entry, (STARTER_BAG, 1));
        }
    }

    #[test]
    fn init_bags_fills_all_four_slots() {
        let mut w = MockWorld::builder().empty_bag_slots(4).build();
        with_tx(&mut w, |tx| init_bags(tx));
        assert_eq!(bags_added(&w).len(), 4);
    }

    #[test]
    fn skinner_gets_knife() {
        let mut w = world_with_skills(&[SKILL_SKINNING]);
        with_tx(&mut w, |tx| init_skill_tool_kit(tx));
        let a = added(&w);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0], (ITEM_SKINNING_KNIFE, 1));
    }
}
