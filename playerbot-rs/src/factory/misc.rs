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

use crate::ffi::interface::BotInterface;
use crate::ffi::types::ItemId;

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
pub fn cancel_auras(iface: &dyn BotInterface) {
    iface.bot_remove_all_auras();
}

/// Give the bot the mandatory tool for each trade skill it already knows.
///
/// Mirrors `PlayerbotFactory::InitInventorySkill`. The C++ version calls
/// `StoreItem` which dedups via `HasItemCount`; we mirror that here by
/// skipping any tool already in bags.
pub fn init_skill_tool_kit(iface: &dyn BotInterface) {
    for entry in tool_table() {
        if !entry.skills.iter().any(|&s| iface.bot_has_skill(s)) {
            continue;
        }
        if iface.item_count_in_bags(entry.item) > 0 {
            continue;
        }
        iface.inventory_add_item(entry.item, 1);
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
    use crate::ffi::interface::BotInterface;
    use std::cell::RefCell;
    use std::collections::HashSet;

    #[derive(Default)]
    struct Calls {
        remove_auras: u32,
        added: Vec<(ItemId, u32)>,
    }

    #[derive(Default)]
    struct MockIface {
        skills: HashSet<u32>,
        carried: std::collections::HashMap<u32, u32>,
        calls: RefCell<Calls>,
    }

    // Safety: RefCell is !Sync but only touched single-threaded in tests.
    unsafe impl Send for MockIface {}

    impl MockIface {
        fn with_skills(skills: &[u32]) -> Self {
            Self {
                skills: skills.iter().copied().collect(),
                ..Default::default()
            }
        }
        fn add_carried(mut self, item: ItemId, count: u32) -> Self {
            self.carried.insert(item.raw(), count);
            self
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

        fn bot_remove_all_auras(&self) {
            self.calls.borrow_mut().remove_auras += 1;
        }
        fn bot_has_skill(&self, skill_id: u32) -> bool {
            self.skills.contains(&skill_id)
        }
        fn item_count_in_bags(&self, item: ItemId) -> u32 {
            self.carried.get(&item.raw()).copied().unwrap_or(0)
        }
        fn inventory_add_item(&self, item: ItemId, count: u32) -> u32 {
            self.calls.borrow_mut().added.push((item, count));
            count
        }
    }

    #[test]
    fn cancel_auras_delegates_once() {
        let m = MockIface::default();
        cancel_auras(&m);
        assert_eq!(m.calls.borrow().remove_auras, 1);
    }

    #[test]
    fn skill_tool_kit_adds_mining_pick_for_miner() {
        let m = MockIface::with_skills(&[SKILL_MINING]);
        init_skill_tool_kit(&m);
        let added = &m.calls.borrow().added;
        assert_eq!(added.len(), 1);
        assert_eq!(added[0], (ITEM_MINING_PICK, 1));
    }

    #[test]
    fn skill_tool_kit_noop_for_bot_with_no_trade_skills() {
        let m = MockIface::default();
        init_skill_tool_kit(&m);
        assert!(m.calls.borrow().added.is_empty());
    }

    #[test]
    fn skill_tool_kit_skips_tool_already_in_bags() {
        let m = MockIface::with_skills(&[SKILL_MINING]).add_carried(ITEM_MINING_PICK, 1);
        init_skill_tool_kit(&m);
        assert!(m.calls.borrow().added.is_empty());
    }

    #[test]
    fn engineer_gets_both_hammer_and_spanner() {
        let m = MockIface::with_skills(&[SKILL_ENGINEERING]);
        init_skill_tool_kit(&m);
        let added = &m.calls.borrow().added;
        assert_eq!(added.len(), 2);
        let items: HashSet<ItemId> = added.iter().map(|(i, _)| *i).collect();
        assert!(items.contains(&ITEM_BLACKSMITH_HAMMER));
        assert!(items.contains(&ITEM_ARCLIGHT_SPANNER));
    }

    #[test]
    fn blacksmith_gets_hammer_but_not_spanner() {
        let m = MockIface::with_skills(&[SKILL_BLACKSMITHING]);
        init_skill_tool_kit(&m);
        let added = &m.calls.borrow().added;
        assert_eq!(added.len(), 1);
        assert_eq!(added[0], (ITEM_BLACKSMITH_HAMMER, 1));
    }

    #[test]
    fn enchanter_gets_rod() {
        let m = MockIface::with_skills(&[SKILL_ENCHANTING]);
        init_skill_tool_kit(&m);
        let added = &m.calls.borrow().added;
        assert_eq!(added.len(), 1);
        assert_eq!(added[0], (ITEM_RUNED_ARCANITE_ROD, 1));
    }

    #[test]
    fn skinner_gets_knife() {
        let m = MockIface::with_skills(&[SKILL_SKINNING]);
        init_skill_tool_kit(&m);
        let added = &m.calls.borrow().added;
        assert_eq!(added.len(), 1);
        assert_eq!(added[0], (ITEM_SKINNING_KNIFE, 1));
    }
}
