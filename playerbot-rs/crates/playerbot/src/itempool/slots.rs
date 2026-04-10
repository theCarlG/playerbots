//! Equipment slot → viable `InventoryType` mapping.
//!
//! Mirrors the `viableSlots` table populated in the C++
//! `RandomItemMgr` constructor. Used by
//! [`super::equip_filter::can_equip_item`] to reject prototypes whose
//! `InventoryType` is incompatible with the target slot.

use core::cell::OnceCell;
use std::collections::HashMap;

/// `EquipmentSlots` enum values (the slot indices the bot equips to).
/// Mirrors `EquipmentSlots` from `CMaNGOS` `Player.h`.
#[allow(dead_code)]
pub mod slot {
    pub const HEAD: u8 = 0;
    pub const NECK: u8 = 1;
    pub const SHOULDERS: u8 = 2;
    pub const BODY: u8 = 3;
    pub const CHEST: u8 = 4;
    pub const WAIST: u8 = 5;
    pub const LEGS: u8 = 6;
    pub const FEET: u8 = 7;
    pub const WRISTS: u8 = 8;
    pub const HANDS: u8 = 9;
    pub const FINGER1: u8 = 10;
    pub const FINGER2: u8 = 11;
    pub const TRINKET1: u8 = 12;
    pub const TRINKET2: u8 = 13;
    pub const BACK: u8 = 14;
    pub const MAINHAND: u8 = 15;
    pub const OFFHAND: u8 = 16;
    pub const RANGED: u8 = 17;
    pub const TABARD: u8 = 18;
}

/// `InventoryType` enum values (what an item *is*: head, body, weapon...).
/// Mirrors `InventoryType` from `CMaNGOS` `ItemPrototype.h`.
#[allow(dead_code)]
pub mod inv {
    pub const NON_EQUIP: u8 = 0;
    pub const HEAD: u8 = 1;
    pub const NECK: u8 = 2;
    pub const SHOULDERS: u8 = 3;
    pub const BODY: u8 = 4;
    pub const CHEST: u8 = 5;
    pub const WAIST: u8 = 6;
    pub const LEGS: u8 = 7;
    pub const FEET: u8 = 8;
    pub const WRISTS: u8 = 9;
    pub const HANDS: u8 = 10;
    pub const FINGER: u8 = 11;
    pub const TRINKET: u8 = 12;
    pub const WEAPON: u8 = 13;
    pub const SHIELD: u8 = 14;
    pub const RANGED: u8 = 15;
    pub const CLOAK: u8 = 16;
    pub const TWO_HAND_WEAPON: u8 = 17;
    pub const BAG: u8 = 18;
    pub const TABARD: u8 = 19;
    pub const ROBE: u8 = 20;
    pub const WEAPON_MAIN_HAND: u8 = 21;
    pub const WEAPON_OFF_HAND: u8 = 22;
    pub const HOLDABLE: u8 = 23;
    pub const AMMO: u8 = 24;
    pub const THROWN: u8 = 25;
    pub const RANGED_RIGHT: u8 = 26;
    pub const QUIVER: u8 = 27;
    pub const RELIC: u8 = 28;
}

fn build() -> HashMap<u8, Vec<u8>> {
    let mut m: HashMap<u8, Vec<u8>> = HashMap::new();
    m.insert(slot::HEAD, vec![inv::HEAD]);
    m.insert(slot::NECK, vec![inv::NECK]);
    m.insert(slot::SHOULDERS, vec![inv::SHOULDERS]);
    m.insert(slot::BODY, vec![inv::BODY]);
    m.insert(slot::CHEST, vec![inv::CHEST, inv::ROBE]);
    m.insert(slot::WAIST, vec![inv::WAIST]);
    m.insert(slot::LEGS, vec![inv::LEGS]);
    m.insert(slot::FEET, vec![inv::FEET]);
    m.insert(slot::WRISTS, vec![inv::WRISTS]);
    m.insert(slot::HANDS, vec![inv::HANDS]);
    m.insert(slot::FINGER1, vec![inv::FINGER]);
    m.insert(slot::FINGER2, vec![inv::FINGER]);
    m.insert(slot::TRINKET1, vec![inv::TRINKET]);
    m.insert(slot::TRINKET2, vec![inv::TRINKET]);
    m.insert(
        slot::MAINHAND,
        vec![inv::WEAPON, inv::TWO_HAND_WEAPON, inv::WEAPON_MAIN_HAND],
    );
    m.insert(
        slot::OFFHAND,
        vec![
            inv::WEAPON,
            inv::TWO_HAND_WEAPON,
            inv::SHIELD,
            inv::WEAPON_OFF_HAND,
            inv::HOLDABLE,
        ],
    );
    m.insert(
        slot::RANGED,
        vec![inv::RANGED, inv::THROWN, inv::RANGED_RIGHT, inv::RELIC],
    );
    m.insert(slot::TABARD, vec![inv::TABARD]);
    m.insert(slot::BACK, vec![inv::CLOAK]);
    m
}

thread_local! {
    /// Process-local cache so the table isn't rebuilt on every
    /// `CanEquipItem` call. Populated lazily on first access.
    static VIABLE_SLOTS: OnceCell<HashMap<u8, Vec<u8>>> = const { OnceCell::new() };
}

/// True if `inventory_type` is viable for the target `equip_slot`.
///
/// Matches the C++ `viableSlots[slot].find(inventory_type) != end()`
/// predicate.
#[must_use]
pub fn slot_accepts(equip_slot: u8, inventory_type: u8) -> bool {
    VIABLE_SLOTS.with(|cell| {
        let map = cell.get_or_init(build);
        map.get(&equip_slot)
            .is_some_and(|slots| slots.contains(&inventory_type))
    })
}

/// True if `inventory_type` fits in *any* equip slot — used by
/// `CanEquipItemNew` which doesn't care which slot, only that the item
/// is equippable at all.
#[must_use]
pub fn any_slot_accepts(inventory_type: u8) -> bool {
    VIABLE_SLOTS.with(|cell| {
        let map = cell.get_or_init(build);
        map.values().any(|slots| slots.contains(&inventory_type))
    })
}

/// Iterate every (slot, `inventory_type`) pair in the legacy
/// `viableSlots` table. Used by tests and by the item-info cache
/// builder when it needs to scan the full slot × inv-type space.
pub fn for_each_slot_inv<F: FnMut(u8, u8)>(mut f: F) {
    VIABLE_SLOTS.with(|cell| {
        let map = cell.get_or_init(build);
        let mut slots: Vec<u8> = map.keys().copied().collect();
        slots.sort_unstable();
        for s in slots {
            if let Some(inv_types) = map.get(&s) {
                for &iv in inv_types {
                    f(s, iv);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mainhand_accepts_weapon_and_two_hand() {
        assert!(slot_accepts(slot::MAINHAND, inv::WEAPON));
        assert!(slot_accepts(slot::MAINHAND, inv::TWO_HAND_WEAPON));
        assert!(slot_accepts(slot::MAINHAND, inv::WEAPON_MAIN_HAND));
        assert!(!slot_accepts(slot::MAINHAND, inv::SHIELD));
    }

    #[test]
    fn offhand_accepts_shield_and_holdable() {
        assert!(slot_accepts(slot::OFFHAND, inv::SHIELD));
        assert!(slot_accepts(slot::OFFHAND, inv::HOLDABLE));
        assert!(slot_accepts(slot::OFFHAND, inv::WEAPON_OFF_HAND));
        assert!(!slot_accepts(slot::OFFHAND, inv::RANGED));
    }

    #[test]
    fn chest_accepts_robe() {
        assert!(slot_accepts(slot::CHEST, inv::CHEST));
        assert!(slot_accepts(slot::CHEST, inv::ROBE));
    }

    #[test]
    fn ranged_accepts_all_four_types() {
        assert!(slot_accepts(slot::RANGED, inv::RANGED));
        assert!(slot_accepts(slot::RANGED, inv::THROWN));
        assert!(slot_accepts(slot::RANGED, inv::RANGED_RIGHT));
        assert!(slot_accepts(slot::RANGED, inv::RELIC));
    }

    #[test]
    fn any_slot_accepts_known_types() {
        assert!(any_slot_accepts(inv::WEAPON));
        assert!(any_slot_accepts(inv::CLOAK));
        // Bag is not a valid equip target.
        assert!(!any_slot_accepts(inv::BAG));
    }
}
