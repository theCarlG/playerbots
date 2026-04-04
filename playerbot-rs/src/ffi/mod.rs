// FFI layer — generated bindings from botffi.h and safe Rust wrappers.
//
// Nothing outside this module should ever see raw BotCallbacks function pointers.
// All access goes through the BotInterface trait defined in interface.rs.

mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub mod interface;

// Re-export the plain-data types from generated bindings.
// These are safe to use anywhere — they are just data with no invariants.
pub use bindings::{
    BotAuraInfo, BotCallbacks, BotHandle, BotPosition, BotSafePosition, BotThreatEntry,
    BotUnitSnapshot, BotWorldSnapshot, UnitHandle,
};
