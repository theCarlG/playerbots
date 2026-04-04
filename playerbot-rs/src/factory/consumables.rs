//! Consumable initialization (potions, food, reagents).
//!
//! Ports `PlayerbotFactory::InitPotions` / `InitFood` / `InitReagents`:
//!
//!   * **Potions/Food** — item selection is delegated to C++ (wraps
//!     `sRandomItemMgr.GetRandomPotion` / `GetFood` which reads level-scaled
//!     DB tables). Rust owns the restock policy (dedup check, stack-size
//!     rollout, bags-full handling).
//!   * **Reagents** — fully Rust, since the class/level tables are hard-coded
//!     constants. The spell-derived totem loop (iterate `SpellMap` and pick
//!     up totems from `SpellEntry::Totem[]`) is **not** ported here — it
//!     needs a `SpellEntry` FFI surface which does not yet exist. That loop
//!     will be re-added when we port `InitAvailableSpells`.
//!
//! Unit tests use a deterministic `MockIface` that records every call so
//! we can assert on item IDs, counts, and dedup behavior without touching
//! real `CMaNGOS` DB tables.

use crate::ffi::interface::BotInterface;
use crate::ffi::types::ItemId;

// ── Game constants ────────────────────────────────────────────────────────

// WoW class IDs (matching Player::getClass() return values).
const CLASS_WARRIOR: u8 = 1;
const CLASS_PALADIN: u8 = 2;
const CLASS_HUNTER: u8 = 3;
const CLASS_ROGUE: u8 = 4;
const CLASS_PRIEST: u8 = 5;
#[allow(dead_code)] // referenced for completeness; has no reagents of its own.
const CLASS_DEATHKNIGHT: u8 = 6;
const CLASS_SHAMAN: u8 = 7;
const CLASS_MAGE: u8 = 8;
const CLASS_WARLOCK: u8 = 9;
const CLASS_DRUID: u8 = 11;

// Spell effect IDs (subset — only what the factory queries).
const SPELL_EFFECT_HEAL: u32 = 10;
const SPELL_EFFECT_ENERGIZE: u32 = 30;

// Food categories used by RandomItemMgr::GetFood.
const FOOD_CATEGORY_FOOD: u32 = 11;
const FOOD_CATEGORY_DRINK: u32 = 59;

// ── Public entry points ───────────────────────────────────────────────────

/// Give the bot healing potions (and mana potions for mana users) appropriate
/// for its level. Skips a category if the bot already carries that exact item.
pub fn init_potions(iface: &dyn BotInterface, level: u32, has_mana: bool) {
    // Healing always, energize only for mana users.
    restock_picked(iface, level, SPELL_EFFECT_HEAL, PickKind::Potion);
    if has_mana {
        restock_picked(iface, level, SPELL_EFFECT_ENERGIZE, PickKind::Potion);
    }
}

/// Give the bot food (always) and drink (mana users only) appropriate for its
/// level.
pub fn init_food(iface: &dyn BotInterface, level: u32, has_mana: bool) {
    restock_picked(iface, level, FOOD_CATEGORY_FOOD, PickKind::Food);
    if has_mana {
        restock_picked(iface, level, FOOD_CATEGORY_DRINK, PickKind::Food);
    }
}

/// Give the bot class-specific reagents appropriate for its level (mage runes,
/// druid catalysts, warlock soul shards, etc.).
pub fn init_reagents(iface: &dyn BotInterface, class: u8, level: u32) {
    let plan = reagent_plan_for(class, level);
    for item_id in plan.items {
        let max_stack = iface.item_max_stack_size(item_id).max(1);
        let carried = iface.item_count_in_bags(item_id);
        if carried > max_stack {
            continue;
        }
        // Factory rolls [max/2, max*regCount]. Cap at a single stack to avoid
        // overflow into multiple slots (the old code relied on StoreNewItem
        // splitting across slots; we stay simple and just add one stack).
        let lo = max_stack / 2;
        let hi = max_stack.saturating_mul(plan.regions_count).max(lo + 1);
        let count = iface.random_u32(lo, hi).min(max_stack);
        if count == 0 {
            continue;
        }
        let _added = iface.inventory_add_item(item_id, count);
    }
}

// ── Internal ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickKind {
    Potion,
    Food,
}

/// Shared restock logic for potions and food: pick an item from the DB via
/// the FFI, skip if the bot already has some, otherwise add a half-to-full
/// stack.
fn restock_picked(iface: &dyn BotInterface, level: u32, selector: u32, kind: PickKind) {
    let item_id = match kind {
        PickKind::Potion => iface.factory_pick_potion_for_level(level, selector),
        PickKind::Food => iface.factory_pick_food_for_level(level, selector),
    };
    if item_id == ItemId::NONE {
        // RandomItemMgr has no entry for this level+selector. Skip silently —
        // matches old C++ behavior (logged `outDetail` then `continue`).
        return;
    }
    if iface.item_count_in_bags(item_id) > 0 {
        return;
    }
    let max_stack = iface.item_max_stack_size(item_id).max(1);
    let lo = max_stack / 2;
    let hi = max_stack;
    let count = iface.random_u32(lo.max(1), hi);
    iface.inventory_add_item(item_id, count);
}

/// Per-class reagent plan keyed by level. Ported verbatim from
/// `PlayerbotFactory::InitReagents`. Expansion-specific entries (e.g. wotlk
/// druid Flintweed Seed 22147, Wild Quillvine 22148) are included for all
/// builds — `CMaNGOS` will silently skip unknown item IDs on classic/tbc.
#[derive(Debug, Default)]
struct ReagentPlan {
    items: Vec<ItemId>,
    /// `regCount` from old code — multiplier on the upper bound of the
    /// per-stack roll. Paladins get 3× symbols, Warlocks get 10× shards, etc.
    regions_count: u32,
}

fn reagent_plan_for(class: u8, level: u32) -> ReagentPlan {
    let mut plan = ReagentPlan {
        regions_count: 1,
        ..Default::default()
    };
    match class {
        CLASS_MAGE => {
            plan.regions_count = 2;
            if level > 11 {
                plan.items = vec![ItemId(17056)];
            }
            if level > 19 {
                plan.items = vec![ItemId(17056), ItemId(17031)];
            }
            if level > 35 {
                plan.items = vec![ItemId(17056), ItemId(17031), ItemId(17032)];
            }
            if level > 55 {
                plan.items = vec![ItemId(17056), ItemId(17031), ItemId(17032), ItemId(17020)];
            }
        }
        CLASS_DRUID => {
            plan.regions_count = 2;
            if level > 19 {
                plan.items = vec![ItemId(17034)];
            }
            if level > 29 {
                plan.items = vec![ItemId(17035)];
            }
            if level > 39 {
                plan.items = vec![ItemId(17036)];
            }
            if level > 49 {
                plan.items = vec![ItemId(17037), ItemId(17021)];
            }
            if level > 59 {
                plan.items = vec![ItemId(17038), ItemId(17026)];
            }
            if level > 69 {
                plan.items = vec![ItemId(22147), ItemId(22148)];
            }
        }
        CLASS_PALADIN => {
            plan.regions_count = 3;
            if level > 50 {
                plan.items = vec![ItemId(21177)];
            }
        }
        CLASS_SHAMAN => {
            plan.regions_count = 1;
            if level > 22 {
                plan.items = vec![ItemId(17057)];
            }
            if level > 28 {
                plan.items = vec![ItemId(17057), ItemId(17058)];
            }
            if level > 29 {
                plan.items = vec![ItemId(17057), ItemId(17058), ItemId(17030)];
            }
        }
        CLASS_WARLOCK => {
            plan.regions_count = 10;
            if level > 9 {
                plan.items = vec![ItemId(6265)];
            }
            if level > 49 {
                plan.items = vec![ItemId(6265), ItemId(5565)];
            }
        }
        CLASS_PRIEST => {
            plan.regions_count = 3;
            if level > 48 {
                plan.items = vec![ItemId(17028)];
            }
            if level > 55 {
                plan.items = vec![ItemId(17028), ItemId(17029)];
            }
        }
        CLASS_ROGUE => {
            plan.regions_count = 1;
            if level > 21 {
                plan.items = vec![ItemId(5140)];
            }
            if level > 33 {
                plan.items = vec![ItemId(5140), ItemId(5530)];
            }
        }
        CLASS_WARRIOR | CLASS_HUNTER => {
            // No static reagents.
        }
        _ => {} // DK and unknown: no static reagents.
    }
    plan
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::interface::BotInterface;
    use std::cell::RefCell;

    #[derive(Default)]
    struct AddedItem {
        id: ItemId,
        #[allow(dead_code)] // retained for debugging of failing tests
        count: u32,
    }

    struct MockIface {
        carried: RefCell<std::collections::HashMap<u32, u32>>,
        added: RefCell<Vec<AddedItem>>,
        // Per-itemId stack size override, default 20.
        stack_sizes: RefCell<std::collections::HashMap<u32, u32>>,
        // Canned selections for pick_*.
        potion_pick: RefCell<std::collections::HashMap<(u32, u32), u32>>,
        food_pick: RefCell<std::collections::HashMap<(u32, u32), u32>>,
        // RNG returns the midpoint of [lo, hi] for determinism.
    }

    impl MockIface {
        fn new() -> Self {
            Self {
                carried: RefCell::default(),
                added: RefCell::default(),
                stack_sizes: RefCell::default(),
                potion_pick: RefCell::default(),
                food_pick: RefCell::default(),
            }
        }
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

        fn item_count_in_bags(&self, item_id: ItemId) -> u32 {
            *self.carried.borrow().get(&item_id.raw()).unwrap_or(&0)
        }
        fn inventory_add_item(&self, item_id: ItemId, count: u32) -> u32 {
            self.added
                .borrow_mut()
                .push(AddedItem { id: item_id, count });
            *self.carried.borrow_mut().entry(item_id.raw()).or_insert(0) += count;
            count
        }
        fn item_max_stack_size(&self, item_id: ItemId) -> u32 {
            *self.stack_sizes.borrow().get(&item_id.raw()).unwrap_or(&20)
        }
        fn factory_pick_potion_for_level(&self, level: u32, effect: u32) -> ItemId {
            ItemId(
                *self
                    .potion_pick
                    .borrow()
                    .get(&(level, effect))
                    .unwrap_or(&0),
            )
        }
        fn factory_pick_food_for_level(&self, level: u32, category: u32) -> ItemId {
            ItemId(
                *self
                    .food_pick
                    .borrow()
                    .get(&(level, category))
                    .unwrap_or(&0),
            )
        }
        fn random_u32(&self, min: u32, max: u32) -> u32 {
            // Deterministic midpoint.
            min + (max - min) / 2
        }
    }

    // ── Potions ─────────────────────────────────────────────────────────

    #[test]
    fn potions_add_heal_and_energize_for_mana_user() {
        let m = MockIface::new();
        m.potion_pick
            .borrow_mut()
            .insert((30, SPELL_EFFECT_HEAL), 929);
        m.potion_pick
            .borrow_mut()
            .insert((30, SPELL_EFFECT_ENERGIZE), 3827);
        init_potions(&m, 30, /*has_mana*/ true);
        let added = m.added.borrow();
        assert_eq!(added.len(), 2);
        assert_eq!(added[0].id, ItemId(929));
        assert_eq!(added[1].id, ItemId(3827));
    }

    #[test]
    fn potions_skip_energize_for_non_mana_users() {
        let m = MockIface::new();
        m.potion_pick
            .borrow_mut()
            .insert((60, SPELL_EFFECT_HEAL), 13446);
        m.potion_pick
            .borrow_mut()
            .insert((60, SPELL_EFFECT_ENERGIZE), 13444);
        init_potions(&m, 60, /*has_mana*/ false);
        let added = m.added.borrow();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].id, ItemId(13446));
    }

    #[test]
    fn potions_skip_if_bot_already_carries_them() {
        let m = MockIface::new();
        m.potion_pick
            .borrow_mut()
            .insert((40, SPELL_EFFECT_HEAL), 1710);
        m.carried.borrow_mut().insert(1710, 3);
        init_potions(&m, 40, /*has_mana*/ false);
        assert!(m.added.borrow().is_empty());
    }

    #[test]
    fn potions_skip_if_selector_returns_zero() {
        let m = MockIface::new(); // no pick registered → returns 0
        init_potions(&m, 5, /*has_mana*/ false);
        assert!(m.added.borrow().is_empty());
    }

    // ── Food ────────────────────────────────────────────────────────────

    #[test]
    fn food_adds_food_and_drink_for_mana_user() {
        let m = MockIface::new();
        m.food_pick
            .borrow_mut()
            .insert((45, FOOD_CATEGORY_FOOD), 4544);
        m.food_pick
            .borrow_mut()
            .insert((45, FOOD_CATEGORY_DRINK), 1645);
        init_food(&m, 45, true);
        let ids: Vec<_> = m.added.borrow().iter().map(|a| a.id).collect();
        assert_eq!(ids, vec![ItemId(4544), ItemId(1645)]);
    }

    #[test]
    fn food_skips_drink_for_non_mana_user() {
        let m = MockIface::new();
        m.food_pick
            .borrow_mut()
            .insert((45, FOOD_CATEGORY_FOOD), 4544);
        m.food_pick
            .borrow_mut()
            .insert((45, FOOD_CATEGORY_DRINK), 1645);
        init_food(&m, 45, false);
        let ids: Vec<_> = m.added.borrow().iter().map(|a| a.id).collect();
        assert_eq!(ids, vec![ItemId(4544)]);
    }

    // ── Reagents ────────────────────────────────────────────────────────

    #[test]
    fn mage_reagents_scale_by_level() {
        let p10 = reagent_plan_for(CLASS_MAGE, 10);
        assert!(p10.items.is_empty());

        let p20 = reagent_plan_for(CLASS_MAGE, 20);
        assert_eq!(p20.items, vec![ItemId(17056), ItemId(17031)]);

        let p60 = reagent_plan_for(CLASS_MAGE, 60);
        assert_eq!(
            p60.items,
            vec![ItemId(17056), ItemId(17031), ItemId(17032), ItemId(17020)]
        );
    }

    #[test]
    fn warlock_reagents_high_regions_count() {
        let plan = reagent_plan_for(CLASS_WARLOCK, 50);
        assert_eq!(plan.items, vec![ItemId(6265), ItemId(5565)]);
        assert_eq!(plan.regions_count, 10);
    }

    #[test]
    fn warrior_has_no_static_reagents() {
        assert!(reagent_plan_for(CLASS_WARRIOR, 80).items.is_empty());
    }

    #[test]
    fn init_reagents_adds_items_for_mage() {
        let m = MockIface::new();
        init_reagents(&m, CLASS_MAGE, 20);
        let ids: Vec<_> = m.added.borrow().iter().map(|a| a.id).collect();
        assert_eq!(ids, vec![ItemId(17056), ItemId(17031)]);
    }

    #[test]
    fn init_reagents_skips_already_full_stack() {
        let m = MockIface::new();
        // Pretend bot already carries > max_stack of item 17056 (stack size 20 default).
        m.carried.borrow_mut().insert(17056, 100);
        init_reagents(&m, CLASS_MAGE, 20);
        // 17031 should still be added, 17056 skipped.
        let ids: Vec<_> = m.added.borrow().iter().map(|a| a.id).collect();
        assert_eq!(ids, vec![ItemId(17031)]);
    }

    #[test]
    fn init_reagents_noop_for_warrior() {
        let m = MockIface::new();
        init_reagents(&m, CLASS_WARRIOR, 80);
        assert!(m.added.borrow().is_empty());
    }
}
