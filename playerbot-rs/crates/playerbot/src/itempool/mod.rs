//! Item pool subsystem — Rust port of `playerbot/RandomItemMgr.{h,cpp}`.
//!
//! # Layout
//!
//! * [`types`] — core structs (`ItemInfoEntry`, `BotEquipKey`,
//!   `WeightScale`, `ItemSpecType`, `RandomItemType`).
//! * [`slots`] — `EquipmentSlots` → viable `InventoryType` table.
//! * [`stat_link`] — stat name ↔ `ITEM_MOD_*` mapping tables.
//! * [`equip_filter`] — `CanEquipItem`, `CanEquipArmor`, `CanEquipWeapon`,
//!   `ShouldEquipArmorForSpec`, `ShouldEquipWeaponForSpec`.
//! * [`stat_weight`] — `CalculateStatWeight` and its single-stat primitive.
//!
//! The top-level manager, consumable lookup tables, rarity cache, FFI
//! surface, and test fixtures live in sibling modules added during the
//! later steps of the Phase F port.

pub mod consumables;
pub mod enchant;
pub mod equip_cache;
pub mod equip_filter;
#[allow(unsafe_code)]
pub mod ffi;
pub mod item_info;
pub mod manager;
pub mod player_spec;
pub mod random_cache;
pub mod rarity;
pub mod slots;
pub mod stat_link;
pub mod stat_weight;
pub mod types;
pub mod unavailable_ids;
