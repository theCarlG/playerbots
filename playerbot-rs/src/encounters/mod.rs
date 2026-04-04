pub mod aq20;
pub mod aq40;
pub mod blackwing_lair;
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
pub mod bt;
pub mod bt_overrides;
pub mod coordinator;
pub mod molten_core;
pub mod naxxramas;
pub mod onyxias_lair;

// TBC content
#[cfg(any(feature = "tbc", feature = "wotlk"))]
pub mod karazhan;

use crate::engine::bt::Bt;
use crate::ffi::SpellId;

/// Reserved phase IDs shared by all encounters.
///
/// Encounters are free to use any `u32` for their internal phases, but these
/// two values are conventional slots that every FSM gets for free:
///
/// - `PHASE_PREPULL`: the FSM is constructed but `CombatStarted` has not fired.
/// - `PHASE_VICTORY`: `is_done()` is true (boss dead). The FSM may still
///   return a `phase_bt()` for a brief victory routine before the encounter
///   is torn down.
pub const PHASE_PREPULL: u32 = 0;
pub const PHASE_VICTORY: u32 = u32::MAX;

/// Trait all encounter FSMs implement.
///
/// Each boss is a state machine. Each state can have an associated BT that
/// overrides the normal combat rotation (via `phase_bt()`). When `phase_bt()`
/// returns `None`, the normal rotation runs.
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

    /// Hint for Heigan-style safe zone positioning (1-4). 0 = not applicable.
    fn safe_zone_hint(&self) -> u8 {
        0
    }

    /// Returns the BT override for the current phase, if any.
    ///
    /// When `Some`, this BT runs instead of the normal combat rotation.
    /// When `None`, the normal rotation runs unmodified.
    ///
    /// BTs are built once at FSM construction and stored in the boss struct.
    /// Returns a reference to the pre-built tree — zero allocation per tick,
    /// no vtable dispatch (concrete `Bt` data type).
    ///
    /// Transitions (pull, phase change, victory) are expressed as dedicated
    /// phase states inside each FSM's phase enum; there is no separate edge
    /// hook. The FSM's `update()` flips into the transition phase and
    /// `phase_bt()` returns the matching tree.
    fn phase_bt(&self) -> Option<&Bt> {
        None
    }
}

/// Events that can drive FSM transitions.
#[derive(Debug, Clone)]
pub enum EncounterEvent {
    /// Regular tick — no specific event.
    None,
    /// A unit died (could be boss, player, or add).
    UnitDied { victim: u64 },
    /// A unit started or completed a spell cast.
    SpellCast {
        caster: u64,
        spell_id: SpellId,
        success: bool,
    },
    /// An aura was applied to or removed from a unit.
    AuraChanged {
        unit: u64,
        spell_id: SpellId,
        applied: bool,
    },
    /// This bot was pulled into combat.
    CombatStarted,
    /// The group wiped (all players dead / boss reset).
    GroupWipe,
}

/// A trivial single-phase FSM for bosses with no phase transitions.
/// Used as the default for mechanically simple encounters.
pub struct SimpleFsm {
    entry: u32,
    active: bool,
    done: bool,
}

impl SimpleFsm {
    pub fn new(entry: u32) -> Self {
        Self {
            entry,
            active: false,
            done: false,
        }
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
            EncounterEvent::GroupWipe => {
                self.active = false;
                self.done = true;
            }
            _ => {}
        }
    }
    fn phase_id(&self) -> u32 {
        if self.active { 1 } else { 0 }
    }
    fn is_active(&self) -> bool {
        self.active
    }
    fn is_done(&self) -> bool {
        self.done
    }
    fn boss_entry(&self) -> u32 {
        self.entry
    }
}
