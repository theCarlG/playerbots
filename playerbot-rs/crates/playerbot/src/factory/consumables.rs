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

use super::FactoryTransaction;
use cmangos::ItemId;

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
const SPELL_EFFECT_LEARN_SPELL: u32 = 36;

// Food categories used by RandomItemMgr::GetFood.
const FOOD_CATEGORY_FOOD: u32 = 11;
const FOOD_CATEGORY_DRINK: u32 = 59;

/// Fixed stack size used by `PlayerbotFactory::AddConsumables` — the C++
/// version always calls `StoreItem(id, 5)`.
const ADDITIONAL_STACK: u32 = 5;

// ── AddConsumables item IDs (ported verbatim from `PlayerbotFactory.h`) ──
//
// Weightstones / sharpening stones (warrior / paladin / hunter).
// Note: ROUGH and COARSE weightstones share item id 3239 in the C++ header —
// preserved here for bug-for-bug parity with the old code.
const ROUGH_WEIGHTSTONE: ItemId = ItemId(3239);
const COARSE_WEIGHTSTONE: ItemId = ItemId(3239);
const HEAVY_WEIGHTSTONE: ItemId = ItemId(3241);
const SOLID_WEIGHTSTONE: ItemId = ItemId(7965);
const DENSE_WEIGHTSTONE: ItemId = ItemId(12643);
// Fel / Adamantite tiers are TBC+ only; defined for all builds so the plan
// table stays readable.
#[cfg_attr(feature = "vanilla", allow(dead_code))]
const FEL_WEIGHTSTONE: ItemId = ItemId(28420);
#[cfg_attr(feature = "vanilla", allow(dead_code))]
const ADAMANTITE_WEIGHTSTONE: ItemId = ItemId(28421);
const ROUGH_SHARPENING_STONE: ItemId = ItemId(2862);
const COARSE_SHARPENING_STONE: ItemId = ItemId(2863);
const HEAVY_SHARPENING_STONE: ItemId = ItemId(2871);
const SOL_SHARPENING_STONE: ItemId = ItemId(7964);
const DENSE_SHARPENING_STONE: ItemId = ItemId(12404);
#[cfg_attr(feature = "vanilla", allow(dead_code))]
const FEL_SHARPENING_STONE: ItemId = ItemId(23528);
#[cfg_attr(feature = "vanilla", allow(dead_code))]
const ADAMANTITE_SHARPENING_STONE: ItemId = ItemId(23529);

// Mana / wizard oils (priest / mage / warlock).
const MINOR_MANA_OIL: ItemId = ItemId(20745);
const LESSER_MANA_OIL: ItemId = ItemId(20747);
const BRILLIANT_MANA_OIL: ItemId = ItemId(20748);
// Superior oils are TBC+ only.
#[cfg_attr(feature = "vanilla", allow(dead_code))]
const SUPERIOR_MANA_OIL: ItemId = ItemId(22521);
const MINOR_WIZARD_OIL: ItemId = ItemId(20744);
const LESSER_WIZARD_OIL: ItemId = ItemId(20746);
const WIZARD_OIL: ItemId = ItemId(20750);
const BRILLIANT_WIZARD_OIL: ItemId = ItemId(20749);
#[cfg_attr(feature = "vanilla", allow(dead_code))]
const SUPERIOR_WIZARD_OIL: ItemId = ItemId(22522);

// Rogue poisons.
const INSTANT_POISON: ItemId = ItemId(6947);
const INSTANT_POISON_II: ItemId = ItemId(6949);
const INSTANT_POISON_III: ItemId = ItemId(6950);
const INSTANT_POISON_IV: ItemId = ItemId(8926);
const INSTANT_POISON_V: ItemId = ItemId(8927);
const INSTANT_POISON_VI: ItemId = ItemId(8928);
const INSTANT_POISON_VII: ItemId = ItemId(21927);
const INSTANT_POISON_VIII: ItemId = ItemId(43230);
const INSTANT_POISON_IX: ItemId = ItemId(43231);
const DEADLY_POISON: ItemId = ItemId(2892);
const DEADLY_POISON_II: ItemId = ItemId(2893);
const DEADLY_POISON_III: ItemId = ItemId(8984);
const DEADLY_POISON_IV: ItemId = ItemId(8985);
const DEADLY_POISON_V: ItemId = ItemId(20844);
const DEADLY_POISON_VI: ItemId = ItemId(22053);
const DEADLY_POISON_VII: ItemId = ItemId(22054);
const DEADLY_POISON_VIII: ItemId = ItemId(43232);
const DEADLY_POISON_IX: ItemId = ItemId(43233);
const CRIPPLING_POISON: ItemId = ItemId(3775);
const CRIPPLING_POISON_II: ItemId = ItemId(3776);
const MIND_POISON: ItemId = ItemId(5237);
const MIND_POISON_II: ItemId = ItemId(6951);
const MIND_POISON_III: ItemId = ItemId(9186);

// LESSER_WIZARD_OIL and LESSER_MANA_OIL are defined in the C++ header but
// never referenced by `AddConsumables`. They were enum entries only; we keep
// the constants here so future additions don't have to rediscover the IDs.
#[allow(dead_code)]
const _UNREFERENCED: [ItemId; 2] = [LESSER_MANA_OIL, LESSER_WIZARD_OIL];

// ── Public entry points ───────────────────────────────────────────────────

/// Give the bot healing potions (and mana potions for mana users) appropriate
/// for its level. Skips a category if the bot already carries that exact item.
pub fn init_potions(tx: &mut FactoryTransaction<'_>, level: u32, has_mana: bool) {
    // Healing always, energize only for mana users.
    restock_picked(tx, level, SPELL_EFFECT_HEAL, PickKind::Potion);
    if has_mana {
        restock_picked(tx, level, SPELL_EFFECT_ENERGIZE, PickKind::Potion);
    }
}

/// Give the bot food (always) and drink (mana users only) appropriate for its
/// level.
pub fn init_food(tx: &mut FactoryTransaction<'_>, level: u32, has_mana: bool) {
    restock_picked(tx, level, FOOD_CATEGORY_FOOD, PickKind::Food);
    if has_mana {
        restock_picked(tx, level, FOOD_CATEGORY_DRINK, PickKind::Food);
    }
}

/// Give the bot class-specific *use-up* consumables appropriate for its level:
///
///   * **Priest / Mage / Warlock** — mana oils + wizard oils applied to the
///     weapon (spell power buff).
///   * **Warrior / Paladin / Hunter** — sharpening stones + weightstones
///     applied to the weapon (AP/DPS buff).
///   * **Rogue** — instant / deadly / crippling / mind poisons applied to
///     the weapon (damage-over-time + debuff).
///
/// Skips any item the bot already carries at least 5 of (the C++ `StoreItem`
/// helper's dedup check with `count == 5`). Each add is a fixed stack of 5,
/// matching the old `PlayerbotFactory::AddConsumables` behaviour.
pub fn init_additional_consumables(tx: &mut FactoryTransaction<'_>, class: u8, level: u32) {
    for &item_id in additional_consumables_plan(class, level).iter() {
        if item_id == ItemId::NONE {
            continue;
        }
        if tx.item_count_in_bags(item_id) >= ADDITIONAL_STACK {
            continue;
        }
        tx.inventory_add_item(item_id, ADDITIONAL_STACK);
    }
}

/// Give the bot class-specific reagents appropriate for its level (mage runes,
/// druid catalysts, warlock soul shards, etc.) *and* add any totem items
/// required by spells in the bot's spellbook (shaman totems, etc.).
pub fn init_reagents(tx: &mut FactoryTransaction<'_>, class: u8, level: u32) {
    // Static class/level reagent plan.
    let plan = reagent_plan_for(class, level);
    for item_id in plan.items {
        let max_stack = tx.item_max_stack_size(item_id).max(1);
        let carried = tx.item_count_in_bags(item_id);
        if carried > max_stack {
            continue;
        }
        // Factory rolls [max/2, max*regCount]. Cap at a single stack to avoid
        // overflow into multiple slots (the old code relied on StoreNewItem
        // splitting across slots; we stay simple and just add one stack).
        let lo = max_stack / 2;
        let hi = max_stack.saturating_mul(plan.regions_count).max(lo + 1);
        let count = tx.random_u32(lo, hi).min(max_stack);
        if count == 0 {
            continue;
        }
        let _added = tx.inventory_add_item(item_id, count);
    }

    // Spell-derived totems: iterate the bot's spellbook, pull SpellEntry for
    // each known spell, and give the bot any `Totem[]` items it doesn't
    // already carry. Mirrors the old C++ loop in `PlayerbotFactory::InitReagents`.
    let spells = tx.get_bot_spells();
    for &spell_id in &spells {
        let Some(info) = tx.get_spell_info(spell_id) else {
            continue;
        };
        if info.is_passive {
            continue;
        }
        // SPELL_EFFECT_LEARN_SPELL = 36 — skip "teach this spell" helpers
        // that carry unrelated totem slots.
        if info.effect[0] == SPELL_EFFECT_LEARN_SPELL {
            continue;
        }
        for &totem in &info.totem {
            if totem == 0 {
                continue;
            }
            let item = ItemId(totem);
            if tx.item_count_in_bags(item) > 0 {
                continue;
            }
            tx.inventory_add_item(item, 1);
        }
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
fn restock_picked(tx: &mut FactoryTransaction<'_>, level: u32, selector: u32, kind: PickKind) {
    let item_id = match kind {
        PickKind::Potion => tx.factory_pick_potion_for_level(level, selector),
        PickKind::Food => tx.factory_pick_food_for_level(level, selector),
    };
    if item_id == ItemId::NONE {
        // RandomItemMgr has no entry for this level+selector. Skip silently —
        // matches old C++ behavior (logged `outDetail` then `continue`).
        return;
    }
    if tx.item_count_in_bags(item_id) > 0 {
        return;
    }
    let max_stack = tx.item_max_stack_size(item_id).max(1);
    let lo = max_stack / 2;
    let hi = max_stack;
    let count = tx.random_u32(lo.max(1), hi);
    tx.inventory_add_item(item_id, count);
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

// ── AddConsumables plan tables ────────────────────────────────────────────
//
// Each plan is returned as a `smallvec`-style fixed-capacity array; the
// C++ version adds at most 4 items (rogue at level 30-59), so a 4-element
// array covers every branch and avoids a per-call heap allocation.

type AdditionalPlan = [ItemId; 4];
const EMPTY_PLAN: AdditionalPlan = [ItemId::NONE; 4];

fn additional_consumables_plan(class: u8, level: u32) -> AdditionalPlan {
    match class {
        CLASS_PRIEST | CLASS_MAGE | CLASS_WARLOCK => caster_oil_plan(level),
        CLASS_PALADIN | CLASS_WARRIOR | CLASS_HUNTER => melee_stone_plan(level),
        CLASS_ROGUE => rogue_poison_plan(level),
        _ => EMPTY_PLAN,
    }
}

/// Priest / Mage / Warlock mana + wizard oils.
fn caster_oil_plan(level: u32) -> AdditionalPlan {
    if (5..20).contains(&level) {
        return [MINOR_WIZARD_OIL, ItemId::NONE, ItemId::NONE, ItemId::NONE];
    }
    if (20..40).contains(&level) {
        return [MINOR_MANA_OIL, MINOR_WIZARD_OIL, ItemId::NONE, ItemId::NONE];
    }
    if (40..45).contains(&level) {
        return [MINOR_MANA_OIL, WIZARD_OIL, ItemId::NONE, ItemId::NONE];
    }
    #[cfg(feature = "vanilla")]
    {
        if level >= 45 {
            return [
                BRILLIANT_MANA_OIL,
                BRILLIANT_WIZARD_OIL,
                ItemId::NONE,
                ItemId::NONE,
            ];
        }
    }
    #[cfg(not(feature = "vanilla"))]
    {
        if (45..52).contains(&level) {
            return [
                BRILLIANT_MANA_OIL,
                BRILLIANT_WIZARD_OIL,
                ItemId::NONE,
                ItemId::NONE,
            ];
        }
        if (52..58).contains(&level) {
            return [
                SUPERIOR_MANA_OIL,
                BRILLIANT_WIZARD_OIL,
                ItemId::NONE,
                ItemId::NONE,
            ];
        }
        if (58..72).contains(&level) {
            return [
                SUPERIOR_MANA_OIL,
                SUPERIOR_WIZARD_OIL,
                ItemId::NONE,
                ItemId::NONE,
            ];
        }
    }
    EMPTY_PLAN
}

/// Warrior / Paladin / Hunter sharpening stones + weightstones.
fn melee_stone_plan(level: u32) -> AdditionalPlan {
    if (1..5).contains(&level) {
        return [
            ROUGH_SHARPENING_STONE,
            ROUGH_WEIGHTSTONE,
            ItemId::NONE,
            ItemId::NONE,
        ];
    }
    if (5..15).contains(&level) {
        return [
            COARSE_WEIGHTSTONE,
            COARSE_SHARPENING_STONE,
            ItemId::NONE,
            ItemId::NONE,
        ];
    }
    if (15..25).contains(&level) {
        return [
            HEAVY_WEIGHTSTONE,
            HEAVY_SHARPENING_STONE,
            ItemId::NONE,
            ItemId::NONE,
        ];
    }
    if (25..35).contains(&level) {
        return [
            SOL_SHARPENING_STONE,
            SOLID_WEIGHTSTONE,
            ItemId::NONE,
            ItemId::NONE,
        ];
    }
    #[cfg(feature = "vanilla")]
    {
        if level >= 35 {
            return [
                DENSE_WEIGHTSTONE,
                DENSE_SHARPENING_STONE,
                ItemId::NONE,
                ItemId::NONE,
            ];
        }
    }
    #[cfg(not(feature = "vanilla"))]
    {
        if (35..50).contains(&level) {
            return [
                DENSE_WEIGHTSTONE,
                DENSE_SHARPENING_STONE,
                ItemId::NONE,
                ItemId::NONE,
            ];
        }
        if (50..60).contains(&level) {
            return [
                FEL_SHARPENING_STONE,
                FEL_WEIGHTSTONE,
                ItemId::NONE,
                ItemId::NONE,
            ];
        }
        if level >= 60 {
            return [
                ADAMANTITE_WEIGHTSTONE,
                ADAMANTITE_SHARPENING_STONE,
                ItemId::NONE,
                ItemId::NONE,
            ];
        }
    }
    EMPTY_PLAN
}

/// Rogue instant / deadly / crippling / mind poisons — the busiest plan
/// because the old C++ method gates on narrow level windows.
fn rogue_poison_plan(level: u32) -> AdditionalPlan {
    match level {
        20..=27 => [
            INSTANT_POISON,
            CRIPPLING_POISON,
            ItemId::NONE,
            ItemId::NONE,
        ],
        28..=29 => [
            INSTANT_POISON_II,
            CRIPPLING_POISON,
            MIND_POISON,
            ItemId::NONE,
        ],
        30..=35 => [
            DEADLY_POISON,
            INSTANT_POISON_II,
            CRIPPLING_POISON,
            MIND_POISON,
        ],
        36..=37 => [
            DEADLY_POISON,
            INSTANT_POISON_III,
            CRIPPLING_POISON,
            MIND_POISON,
        ],
        38..=43 => [
            DEADLY_POISON_II,
            INSTANT_POISON_III,
            CRIPPLING_POISON,
            MIND_POISON_II,
        ],
        44..=45 => [
            DEADLY_POISON_II,
            INSTANT_POISON_IV,
            CRIPPLING_POISON,
            MIND_POISON_II,
        ],
        46..=51 => [
            DEADLY_POISON_III,
            INSTANT_POISON_IV,
            CRIPPLING_POISON,
            MIND_POISON_II,
        ],
        52..=53 => [
            DEADLY_POISON_III,
            INSTANT_POISON_V,
            CRIPPLING_POISON_II,
            MIND_POISON_III,
        ],
        54..=59 => [
            DEADLY_POISON_IV,
            INSTANT_POISON_V,
            CRIPPLING_POISON_II,
            MIND_POISON_III,
        ],
        60..=61 => [
            DEADLY_POISON_V,
            INSTANT_POISON_VI,
            CRIPPLING_POISON_II,
            MIND_POISON_III,
        ],
        62..=67 => [
            DEADLY_POISON_VI,
            INSTANT_POISON_VI,
            ItemId::NONE,
            ItemId::NONE,
        ],
        68..=69 => [
            DEADLY_POISON_VI,
            INSTANT_POISON_VII,
            ItemId::NONE,
            ItemId::NONE,
        ],
        70..=72 => [
            DEADLY_POISON_VII,
            INSTANT_POISON_VII,
            ItemId::NONE,
            ItemId::NONE,
        ],
        73..=75 => [
            DEADLY_POISON_VII,
            INSTANT_POISON_VIII,
            ItemId::NONE,
            ItemId::NONE,
        ],
        76..=78 => [
            DEADLY_POISON_VIII,
            INSTANT_POISON_VIII,
            ItemId::NONE,
            ItemId::NONE,
        ],
        79 => [
            DEADLY_POISON_VIII,
            INSTANT_POISON_IX,
            ItemId::NONE,
            ItemId::NONE,
        ],
        80 => [
            DEADLY_POISON_IX,
            INSTANT_POISON_IX,
            ItemId::NONE,
            ItemId::NONE,
        ],
        _ => EMPTY_PLAN,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::{with_tx, ConsumableKind};
    use cmangos::{BotSpellInfo, MockEvent, MockWorld};

    fn make_spell(id: u32) -> BotSpellInfo {
        let mut info = BotSpellInfo::default();
        info.id = id;
        info.is_valid = true;
        info
    }

    fn added_ids(w: &MockWorld) -> Vec<ItemId> {
        w.events()
            .iter()
            .filter_map(|e| match e {
                MockEvent::InventoryAddItem { item, .. } => Some(*item),
                _ => None,
            })
            .collect()
    }

    // ── Potions ─────────────────────────────────────────────────────────

    #[test]
    fn potions_add_heal_and_energize_for_mana_user() {
        let mut w = MockWorld::builder()
            .potion_pick(30, SPELL_EFFECT_HEAL, ItemId(929))
            .potion_pick(30, SPELL_EFFECT_ENERGIZE, ItemId(3827))
            .build();
        with_tx(&mut w, |tx| init_potions(tx, 30, /*has_mana*/ true));
        let ids = added_ids(&w);
        assert_eq!(ids, vec![ItemId(929), ItemId(3827)]);
    }

    #[test]
    fn potions_skip_energize_for_non_mana_users() {
        let mut w = MockWorld::builder()
            .potion_pick(60, SPELL_EFFECT_HEAL, ItemId(13446))
            .potion_pick(60, SPELL_EFFECT_ENERGIZE, ItemId(13444))
            .build();
        with_tx(&mut w, |tx| init_potions(tx, 60, /*has_mana*/ false));
        let ids = added_ids(&w);
        assert_eq!(ids, vec![ItemId(13446)]);
    }

    #[test]
    fn potions_skip_if_bot_already_carries_them() {
        let mut w = MockWorld::builder()
            .potion_pick(40, SPELL_EFFECT_HEAL, ItemId(1710))
            .item_in_bags(1710, 3)
            .build();
        with_tx(&mut w, |tx| init_potions(tx, 40, /*has_mana*/ false));
        assert!(added_ids(&w).is_empty());
    }

    #[test]
    fn potions_skip_if_selector_returns_zero() {
        // No pick registered → returns ItemId::NONE.
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_potions(tx, 5, /*has_mana*/ false));
        assert!(added_ids(&w).is_empty());
    }

    // ── Food ────────────────────────────────────────────────────────────

    #[test]
    fn food_adds_food_and_drink_for_mana_user() {
        let mut w = MockWorld::builder()
            .food_pick(45, FOOD_CATEGORY_FOOD, ItemId(4544))
            .food_pick(45, FOOD_CATEGORY_DRINK, ItemId(1645))
            .build();
        with_tx(&mut w, |tx| init_food(tx, 45, true));
        assert_eq!(added_ids(&w), vec![ItemId(4544), ItemId(1645)]);
    }

    #[test]
    fn food_skips_drink_for_non_mana_user() {
        let mut w = MockWorld::builder()
            .food_pick(45, FOOD_CATEGORY_FOOD, ItemId(4544))
            .food_pick(45, FOOD_CATEGORY_DRINK, ItemId(1645))
            .build();
        with_tx(&mut w, |tx| init_food(tx, 45, false));
        assert_eq!(added_ids(&w), vec![ItemId(4544)]);
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
        let mut w = MockWorld::builder()
            .item_max_stack(17056, 20)
            .item_max_stack(17031, 20)
            .build();
        with_tx(&mut w, |tx| init_reagents(tx, CLASS_MAGE, 20));
        assert_eq!(added_ids(&w), vec![ItemId(17056), ItemId(17031)]);
    }

    #[test]
    fn init_reagents_skips_already_full_stack() {
        // Pretend bot already carries > max_stack of item 17056 (stack 20).
        let mut w = MockWorld::builder()
            .item_max_stack(17056, 20)
            .item_max_stack(17031, 20)
            .item_in_bags(17056, 100)
            .build();
        with_tx(&mut w, |tx| init_reagents(tx, CLASS_MAGE, 20));
        // 17031 should still be added, 17056 skipped.
        assert_eq!(added_ids(&w), vec![ItemId(17031)]);
    }

    #[test]
    fn init_reagents_noop_for_warrior() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_reagents(tx, CLASS_WARRIOR, 80));
        assert!(added_ids(&w).is_empty());
    }

    // ── Spell-derived totem scan ────────────────────────────────────────

    #[test]
    fn totem_scan_adds_required_totem_item() {
        let mut info = make_spell(3599);
        info.totem[0] = 5176; // fire totem item
        let mut w = MockWorld::builder()
            .bot_spells(vec![3599])
            .spell_info(3599, info)
            .build();
        with_tx(&mut w, |tx| init_reagents(tx, CLASS_SHAMAN, 10)); // below shaman reagent plan level
        assert_eq!(added_ids(&w), vec![ItemId(5176)]);
    }

    #[test]
    fn totem_scan_skips_item_already_in_bags() {
        let mut info = make_spell(3599);
        info.totem[0] = 5176;
        let mut w = MockWorld::builder()
            .bot_spells(vec![3599])
            .spell_info(3599, info)
            .item_in_bags(5176, 1)
            .build();
        with_tx(&mut w, |tx| init_reagents(tx, CLASS_SHAMAN, 10));
        assert!(added_ids(&w).is_empty());
    }

    #[test]
    fn totem_scan_skips_passive_spell() {
        let mut info = make_spell(1234);
        info.is_passive = true;
        info.totem[0] = 5176;
        let mut w = MockWorld::builder()
            .bot_spells(vec![1234])
            .spell_info(1234, info)
            .build();
        with_tx(&mut w, |tx| init_reagents(tx, CLASS_SHAMAN, 10));
        assert!(added_ids(&w).is_empty());
    }

    #[test]
    fn totem_scan_skips_learn_spell_effect() {
        // Training-spell wrappers carry unrelated Totem[] slots that should
        // not be treated as real reagents.
        let mut info = make_spell(9999);
        info.effect[0] = SPELL_EFFECT_LEARN_SPELL;
        info.totem[0] = 5176;
        let mut w = MockWorld::builder()
            .bot_spells(vec![9999])
            .spell_info(9999, info)
            .build();
        with_tx(&mut w, |tx| init_reagents(tx, CLASS_SHAMAN, 10));
        assert!(added_ids(&w).is_empty());
    }

    #[test]
    fn totem_scan_handles_both_totem_slots() {
        let mut info = make_spell(3599);
        info.totem[0] = 5176;
        info.totem[1] = 5175;
        let mut w = MockWorld::builder()
            .bot_spells(vec![3599])
            .spell_info(3599, info)
            .build();
        with_tx(&mut w, |tx| init_reagents(tx, CLASS_SHAMAN, 10));
        assert_eq!(added_ids(&w), vec![ItemId(5176), ItemId(5175)]);
    }

    #[test]
    fn totem_scan_is_noop_when_bot_has_no_spells() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_reagents(tx, CLASS_WARRIOR, 80));
        assert!(added_ids(&w).is_empty());
    }

    // ── AddConsumables (caster oils / melee stones / rogue poisons) ─────

    #[test]
    fn additional_noop_for_unknown_class() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| {
            init_additional_consumables(tx, CLASS_DEATHKNIGHT, 60);
        });
        assert!(added_ids(&w).is_empty());
    }

    #[test]
    fn additional_noop_for_druid() {
        // Druid is a caster but `AddConsumables` deliberately excludes it.
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_additional_consumables(tx, CLASS_DRUID, 60));
        assert!(added_ids(&w).is_empty());
    }

    #[test]
    fn additional_warrior_level_1_rough_stones() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| {
            init_additional_consumables(tx, CLASS_WARRIOR, 1);
        });
        assert_eq!(
            added_ids(&w),
            vec![ROUGH_SHARPENING_STONE, ROUGH_WEIGHTSTONE]
        );
    }

    #[test]
    fn additional_paladin_level_25_solid() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| {
            init_additional_consumables(tx, CLASS_PALADIN, 25);
        });
        assert_eq!(added_ids(&w), vec![SOL_SHARPENING_STONE, SOLID_WEIGHTSTONE]);
    }

    #[test]
    #[cfg(feature = "vanilla")]
    fn additional_hunter_level_60_dense_on_vanilla() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| {
            init_additional_consumables(tx, CLASS_HUNTER, 60);
        });
        assert_eq!(
            added_ids(&w),
            vec![DENSE_WEIGHTSTONE, DENSE_SHARPENING_STONE]
        );
    }

    #[test]
    #[cfg(not(feature = "vanilla"))]
    fn additional_hunter_level_60_adamantite_on_tbc_wotlk() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| {
            init_additional_consumables(tx, CLASS_HUNTER, 60);
        });
        assert_eq!(
            added_ids(&w),
            vec![ADAMANTITE_WEIGHTSTONE, ADAMANTITE_SHARPENING_STONE]
        );
    }

    #[test]
    fn additional_mage_level_10_minor_wizard_oil() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_additional_consumables(tx, CLASS_MAGE, 10));
        assert_eq!(added_ids(&w), vec![MINOR_WIZARD_OIL]);
    }

    #[test]
    fn additional_priest_level_30_minor_mana_and_wizard() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| {
            init_additional_consumables(tx, CLASS_PRIEST, 30);
        });
        assert_eq!(added_ids(&w), vec![MINOR_MANA_OIL, MINOR_WIZARD_OIL]);
    }

    #[test]
    #[cfg(feature = "vanilla")]
    fn additional_warlock_level_60_brilliant_oils_on_vanilla() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| {
            init_additional_consumables(tx, CLASS_WARLOCK, 60);
        });
        assert_eq!(
            added_ids(&w),
            vec![BRILLIANT_MANA_OIL, BRILLIANT_WIZARD_OIL]
        );
    }

    #[test]
    #[cfg(not(feature = "vanilla"))]
    fn additional_warlock_level_60_superior_oils_on_tbc_wotlk() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| {
            init_additional_consumables(tx, CLASS_WARLOCK, 60);
        });
        assert_eq!(
            added_ids(&w),
            vec![SUPERIOR_MANA_OIL, SUPERIOR_WIZARD_OIL]
        );
    }

    #[test]
    fn additional_rogue_level_30_four_poisons() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_additional_consumables(tx, CLASS_ROGUE, 30));
        assert_eq!(
            added_ids(&w),
            vec![DEADLY_POISON, INSTANT_POISON_II, CRIPPLING_POISON, MIND_POISON]
        );
    }

    #[test]
    fn additional_rogue_level_60_four_poisons_with_crippling_ii() {
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_additional_consumables(tx, CLASS_ROGUE, 60));
        assert_eq!(
            added_ids(&w),
            vec![
                DEADLY_POISON_V,
                INSTANT_POISON_VI,
                CRIPPLING_POISON_II,
                MIND_POISON_III,
            ]
        );
    }

    #[test]
    fn additional_rogue_level_19_empty() {
        // Below the rogue poison window — no items.
        let mut w = MockWorld::default();
        with_tx(&mut w, |tx| init_additional_consumables(tx, CLASS_ROGUE, 19));
        assert!(added_ids(&w).is_empty());
    }

    #[test]
    fn additional_dedupes_items_already_stacked_to_five() {
        // Warrior at level 1 normally adds ROUGH_WEIGHTSTONE + ROUGH_SHARPENING_STONE.
        // Seed the bot's bags with 5 of each → both should be skipped.
        let mut w = MockWorld::builder()
            .item_in_bags(ROUGH_WEIGHTSTONE.0, 5)
            .item_in_bags(ROUGH_SHARPENING_STONE.0, 5)
            .build();
        with_tx(&mut w, |tx| {
            init_additional_consumables(tx, CLASS_WARRIOR, 1);
        });
        assert!(added_ids(&w).is_empty());
    }

    #[test]
    fn additional_stocks_when_bot_has_less_than_five() {
        // Bot has 4 of ROUGH_WEIGHTSTONE (< 5 threshold), so it should still
        // be re-stocked. ROUGH_SHARPENING_STONE has no count, so it too.
        let mut w = MockWorld::builder()
            .item_in_bags(ROUGH_WEIGHTSTONE.0, 4)
            .build();
        with_tx(&mut w, |tx| {
            init_additional_consumables(tx, CLASS_WARRIOR, 1);
        });
        assert_eq!(
            added_ids(&w),
            vec![ROUGH_SHARPENING_STONE, ROUGH_WEIGHTSTONE]
        );
    }

    #[test]
    fn additional_via_dispatcher_uses_kind_three() {
        assert_eq!(
            ConsumableKind::from_kind(3),
            Some(ConsumableKind::AdditionalConsumables)
        );
    }
}
