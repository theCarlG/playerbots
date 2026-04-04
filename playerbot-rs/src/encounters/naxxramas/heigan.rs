/// Heigan the Unclean encounter FSM — Naxxramas, Plague Wing.
///
/// Famous for the "Heigan Dance" — one of the most mechanically demanding
/// fights for bots (requires repositioning on a strict rotation).
///
/// Phase 1 (DPS phase, ~90s):
///   Heigan is on his platform.  Ranged/heals stand near him.
///   Eruption zones rotate through 4 channels in the room floor.
///   Bots must move to the "safe channel" at the right moment.
///
/// Phase 2 (Teleport/dance phase, ~45s):
///   Heigan teleports to the dance floor.  All raid members MUST follow
///   and survive the eruption rotation by standing in the currently-safe zone.
///   Eruption fires every 4s, cycling through zones 1→2→3→4→1...
///
/// Eruption zone layout (top-down, 4 lanes North→South):
///   Zone 1 (northmost): always safe during zone 4 eruption
///   Zone 2: safe during zone 1 eruption
///   Zone 3: safe during zone 2 eruption
///   Zone 4 (southmost): safe during zone 3 eruption
///
/// Bot behavior:
///   - Track which eruption zone is currently active (fired) via AuraChanged.
///   - Move to the safe zone (current_zone + 1) % 4.
///   - During Phase 1: stay on platform with Heigan.
///   - During Phase 2: follow the eruption rotation on the dance floor.

use super::super::{EncounterEvent, EncounterFsm};
use crate::ffi::SpellId;

/// Spell IDs for Heigan's eruption in each zone.
pub const SPELL_ERUPTION_ZONE1: SpellId = SpellId(29998);
pub const SPELL_ERUPTION_ZONE2: SpellId = SpellId(30004);
pub const SPELL_ERUPTION_ZONE3: SpellId = SpellId(30006);
pub const SPELL_ERUPTION_ZONE4: SpellId = SpellId(30010);

/// Plague Cloud (damage aura on the dance floor when not dancing correctly).
pub const SPELL_PLAGUE_CLOUD:   SpellId = SpellId(29350);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeiganPhase {
    Idle,
    /// DPS phase — Heigan on platform.
    DpsPhase,
    /// Dance phase — all bots on floor following eruption rotation.
    DancePhase,
}

pub struct HeiganFsm {
    pub phase:          HeiganPhase,
    /// Currently erupting zone (1-4, 0 = unknown).
    pub erupting_zone:  u8,
    /// Server time when Phase 2 started (for timing the dance).
    pub dance_start_ms: u64,
    done: bool,
}

impl HeiganFsm {
    pub fn new() -> Self {
        Self {
            phase:          HeiganPhase::Idle,
            erupting_zone:  0,
            dance_start_ms: 0,
            done:           false,
        }
    }

    /// Which zone is safe right now (the zone after the erupting one).
    /// Returns 1-4.  Returns 1 if no eruption data yet.
    pub fn safe_zone(&self) -> u8 {
        match self.erupting_zone {
            1 => 2,
            2 => 3,
            3 => 4,
            4 => 1,
            _ => 1,
        }
    }

    pub const PHASE_IDLE:  u32 = 0;
    pub const PHASE_DPS:   u32 = 1;
    pub const PHASE_DANCE: u32 = 2;
}

impl Default for HeiganFsm {
    fn default() -> Self { Self::new() }
}

impl EncounterFsm for HeiganFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, time_ms: u64) {
        if self.done { return; }

        match event {
            EncounterEvent::CombatStarted => {
                self.phase = HeiganPhase::DpsPhase;
            }

            EncounterEvent::SpellCast { caster: _, spell_id, success: true } => {
                // Heigan teleports to the dance floor, signalling Phase 2 start.
                // Detect via his teleport spell (29166) or eruption beginning.
                match *spell_id {
                    SPELL_ERUPTION_ZONE1 => {
                        self.erupting_zone = 1;
                        if self.phase == HeiganPhase::DpsPhase {
                            self.phase = HeiganPhase::DancePhase;
                            self.dance_start_ms = time_ms;
                        }
                    }
                    SPELL_ERUPTION_ZONE2 => { self.erupting_zone = 2; }
                    SPELL_ERUPTION_ZONE3 => { self.erupting_zone = 3; }
                    SPELL_ERUPTION_ZONE4 => { self.erupting_zone = 4; }
                    _ => {}
                }
            }

            EncounterEvent::UnitDied { victim: _ } => {
                self.done = true;
            }

            EncounterEvent::GroupWipe => {
                self.phase = HeiganPhase::Idle;
            }

            EncounterEvent::None => {
                // Dance phase lasts ~45s, then Heigan teleports back.
                if self.phase == HeiganPhase::DancePhase
                    && time_ms.saturating_sub(self.dance_start_ms) > 45_000
                {
                    self.phase = HeiganPhase::DpsPhase;
                }
            }

            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        match self.phase {
            HeiganPhase::Idle      => Self::PHASE_IDLE,
            HeiganPhase::DpsPhase  => Self::PHASE_DPS,
            HeiganPhase::DancePhase => Self::PHASE_DANCE,
        }
    }

    fn is_active(&self) -> bool { self.phase != HeiganPhase::Idle }
    fn is_done(&self)   -> bool { self.done }
    fn boss_entry(&self) -> u32 { super::ENTRY_HEIGAN }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encounters::EncounterEvent;

    #[test]
    fn heigan_starts_in_dps_phase() {
        let mut fsm = HeiganFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        assert_eq!(fsm.phase, HeiganPhase::DpsPhase);
    }

    #[test]
    fn heigan_transitions_to_dance_on_first_eruption() {
        let mut fsm = HeiganFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        fsm.update(&EncounterEvent::SpellCast {
            caster: 1, spell_id: SPELL_ERUPTION_ZONE1, success: true
        }, 1.0, 5000);
        assert_eq!(fsm.phase, HeiganPhase::DancePhase);
        assert_eq!(fsm.erupting_zone, 1);
        assert_eq!(fsm.safe_zone(), 2);
    }

    #[test]
    fn heigan_eruption_zone_tracking() {
        let mut fsm = HeiganFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        // Enter dance phase.
        fsm.update(&EncounterEvent::SpellCast {
            caster: 1, spell_id: SPELL_ERUPTION_ZONE1, success: true
        }, 1.0, 1000);
        // Next eruption.
        fsm.update(&EncounterEvent::SpellCast {
            caster: 1, spell_id: SPELL_ERUPTION_ZONE2, success: true
        }, 1.0, 5000);
        assert_eq!(fsm.erupting_zone, 2);
        assert_eq!(fsm.safe_zone(), 3);
    }

    #[test]
    fn heigan_returns_to_dps_phase_after_45s() {
        let mut fsm = HeiganFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        fsm.update(&EncounterEvent::SpellCast {
            caster: 1, spell_id: SPELL_ERUPTION_ZONE1, success: true
        }, 1.0, 0);
        assert_eq!(fsm.phase, HeiganPhase::DancePhase);

        // Advance past 45s.
        fsm.update(&EncounterEvent::None, 0.8, 46_000);
        assert_eq!(fsm.phase, HeiganPhase::DpsPhase);
    }
}
