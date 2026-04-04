/// Encounter FSMs and boss mechanics.
///
/// Each sub-module implements one raid/dungeon encounter.
/// Encounters are gated by expansion feature flags where needed.
///
/// # Integration
/// `BotState::encounter` holds the active `Box<dyn EncounterFsm>`.
/// The encounter is activated via `coordinator::encounter_for_zone(zone_id)`.
/// Push events (from tick.rs) are dispatched to the FSM via `EncounterFsm::update()`.
/// The BT reads `encounter.phase_id()` to select phase-appropriate subtrees.

pub mod coordinator;
pub mod mechanics;
pub mod molten_core;
pub mod onyxias_lair;
pub mod blackwing_lair;
pub mod aq20;
pub mod aq40;
pub mod naxxramas;

// TBC content
#[cfg(any(feature = "tbc", feature = "wotlk"))]
pub mod karazhan;

/// Trait all encounter FSMs implement.
pub trait EncounterFsm: Send {
    /// Update the FSM from a push event and current boss HP.
    ///
    /// Called once per tick before the BT runs, from `process_events` in tick.rs.
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, server_time_ms: u64);

    /// Current phase ID as a `u32` (interpretation is per-encounter).
    ///
    /// Phase 0 means pre-pull / not yet active.
    fn phase_id(&self) -> u32;

    /// True once the encounter has been pulled (boss engaged).
    fn is_active(&self) -> bool;

    /// True once the encounter is finished (boss dead or reset).
    fn is_done(&self) -> bool;

    /// NPC entry ID of the primary boss tracked by this FSM.
    /// Used by the coordinator to match the right FSM to a given target.
    fn boss_entry(&self) -> u32;
}

/// Events that can drive FSM transitions.
#[derive(Debug, Clone)]
pub enum EncounterEvent {
    /// Regular tick — no specific event.
    None,
    /// A unit died (could be boss, player, or add).
    UnitDied { victim: u64 },
    /// A unit started or completed a spell cast.
    SpellCast { caster: u64, spell_id: u32, success: bool },
    /// An aura was applied to or removed from a unit.
    AuraChanged { unit: u64, spell_id: u32, applied: bool },
    /// This bot was pulled into combat.
    CombatStarted,
    /// The group wiped (all players dead / boss reset).
    GroupWipe,
}

/// A trivial single-phase FSM for bosses with no phase transitions.
/// Used as the default for mechanically simple encounters.
pub struct SimpleFsm {
    entry:     u32,
    active:    bool,
    done:      bool,
}

impl SimpleFsm {
    pub fn new(entry: u32) -> Self {
        Self { entry, active: false, done: false }
    }
}

impl EncounterFsm for SimpleFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, _time: u64) {
        match event {
            EncounterEvent::CombatStarted => self.active = true,
            EncounterEvent::UnitDied { victim: _ } => {
                // If the boss died, mark done.
                // (Proper detection requires NPC entry matching — done in coordinator.)
            }
            EncounterEvent::GroupWipe => { self.active = false; self.done = true; }
            _ => {}
        }
    }
    fn phase_id(&self) -> u32  { if self.active { 1 } else { 0 } }
    fn is_active(&self) -> bool { self.active }
    fn is_done(&self) -> bool   { self.done }
    fn boss_entry(&self) -> u32 { self.entry }
}
