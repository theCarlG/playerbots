// FFI layer — generated bindings from botffi.h and safe Rust wrappers.
//
// Nothing outside this module should ever see raw BotCallbacks function pointers.
// All access goes through the BotInterface trait defined in interface.rs.

mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub mod interface;
pub mod types;

// Re-export the plain-data types from generated bindings.
// These are safe to use anywhere — they are just data with no invariants.
pub use bindings::{
    BotAuraInfo, BotCallbacks, BotDispelTarget, BotHandle, BotMailSummary, BotPosition,
    BotQuestInfo, BotReputationEntry, BotSafePosition, BotSkillEntry, BotSpellInfo, BotTalentEntry,
    BotTaxiNode, BotThreatEntry, BotUnitSnapshot, BotWorldSnapshot, UnitHandle,
};

// Re-export newtypes for use throughout the crate.
pub use types::{BotRole, ItemId, SpellId};
