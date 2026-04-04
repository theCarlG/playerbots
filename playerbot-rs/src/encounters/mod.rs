/// Encounter FSMs and boss scripts.
///
/// Each sub-module implements one raid/dungeon encounter.
/// Encounters are gated by expansion feature flags.

pub mod molten_core;
pub mod onyxias_lair;

// TBC content
#[cfg(any(feature = "tbc", feature = "wotlk"))]
pub mod karazhan;

// Naxxramas is present in Vanilla (40-man) and WotLK (10/25-man).
// The implementations differ enough to be separate modules.
// Vanilla Naxxramas is gated on vanilla feature; WotLK version on wotlk.
// For now they share a stub.
pub mod naxxramas;
pub mod blackwing_lair;
pub mod aq20;
pub mod aq40;

/// Trait all encounter FSMs implement.
pub trait EncounterFsm: Send {
    /// Update the FSM from a push event and current snapshot data.
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, server_time_ms: u64);
    /// Return the current phase as a u32 (interpretation is per-encounter).
    fn phase_id(&self) -> u32;
    /// True if the encounter is considered active (boss is pulled).
    fn is_active(&self) -> bool;
}

/// Events that can trigger FSM transitions.
#[derive(Debug, Clone)]
pub enum EncounterEvent {
    /// No event (regular tick).
    None,
    /// A unit died (could be boss or an add).
    UnitDied { victim: u64 },
    /// A unit cast a spell (boss ability detection).
    SpellCast { caster: u64, spell_id: u32 },
    /// An aura was applied/removed on a unit.
    AuraChanged { unit: u64, spell_id: u32, applied: bool },
    /// The group was wiped (all players dead).
    GroupWipe,
}
