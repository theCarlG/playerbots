//! Safe Rust wrappers around the `cmangos-sys` C ABI.
//!
//! This crate is the single point of contact for the rest of the Rust
//! workspace. It exposes:
//!
//! * Re-exports of every POD type from `cmangos-sys`.
//! * Type-safe ID newtypes (`SpellId`, `ItemId`, `SkillId`, `TalentId`).
//! * The `World` trait (formerly `BotInterface`) — the abstract façade over
//!   game-world queries and commands.
//! * `VtableWorld` — the production implementation that calls through the
//!   `BotCallbacks` vtable handed in by the C++ side.
//! * `OwnedList<T, F>` — RAII guard around C-allocated arrays + the 13 type
//!   aliases (`AuraList`, `ThreatList`, …).
//! * `MockWorld` (under the `mock` feature) — full in-memory `World` impl
//!   used by tests.

#![deny(unsafe_code)]

pub mod ids;
#[allow(unsafe_code)]
pub mod item_world;
#[allow(unsafe_code)]
pub mod login_world;
#[cfg(any(test, feature = "mock"))]
pub mod mock;
#[allow(unsafe_code)]
pub mod owned;
#[allow(unsafe_code)]
pub mod real;
pub mod snapshot;
pub mod unit;
pub mod world;

// Re-export every POD type from cmangos-sys at the crate root.
pub use cmangos_sys::{
    BotAuraInfo, BotCallbacks, BotCandidateInfo, BotDispelTarget, BotEnchantInfo,
    BotGemPropertiesRow, BotHandle, BotInventoryItem, BotItemEnchantmentRow, BotItemProtoDamage,
    BotItemProtoSocket, BotItemProtoSpellRef, BotItemProtoStat, BotItemPrototype,
    BotItemRarityRow, BotMailSummary, BotPlayerItemCtx, BotPosition, BotQuestInfo,
    BotQuestItemRow, BotRealPlayerInfo, BotReputationEntry, BotSafePosition, BotSkillEntry,
    BotSpellEntryInfo, BotSpellInfo, BotTalentEntry, BotTaxiNode, BotThreatEntry, BotTravelDest,
    BotUnitSnapshot, BotVendorItemRow, BotWeightScaleRow, BotWeightScaleStatRow,
    BotWorldSnapshot, ItemCallbacks, LoginCallbacks, UnitHandle,
};

pub use ids::{BotRole, ItemId, SkillId, SpellId, TalentId};
pub use item_world::{ItemWorld, QuestStatus, TalentTabInfo, VtableItemWorld};
#[cfg(any(test, feature = "mock"))]
pub use item_world::{MockItemEvent, MockItemWorld};
pub use login_world::{HolderState, LoginWorld, VtableLoginWorld};
#[cfg(any(test, feature = "mock"))]
pub use login_world::{MockLoginEvent, MockLoginWorld};
pub use owned::{
    AuraList, BotSpellList, GatherableList, InventoryList, OwnedCString, OwnedList, QuestLog,
    ReputationList, SkillList, TalentList, TaxiNodeList, ThreatList, TravelDestList, UnitList,
};
#[cfg(any(test, feature = "mock"))]
pub use mock::{MockEvent, MockState, MockWorld, MockWorldBuilder};
pub use real::VtableWorld;
pub use snapshot::{UnitSnapshotExt, WorldSnapshotExt};
pub use unit::UnitRef;
pub use world::World;
