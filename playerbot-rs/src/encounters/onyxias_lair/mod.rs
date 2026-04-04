/// Onyxia's Lair — single boss, 3-phase fight.
///
/// Zone ID: 2159.  40-player raid (Vanilla) / 10/25-player (WotLK).
///
/// Phase 1 (100% → 65%): Onyxia is on the ground.
///   - Cleave, Tail Sweep: melee MUST attack from behind or side.
///   - Wing Buffet: knockback — ranged spread and angle away.
///   - Flame Breath: frontal cone — tank faces boss away from raid.
///
/// Phase 2 (65% → 40%): Onyxia takes flight.
///   - She's immune to melee (ranged only).
///   - Deep Breath: dive-bombs a position (circle indicator) — MOVE OUT.
///   - Fireball Volley: AoE, spread apart (8+ yards).
///   - Whelp eggs hatch — adds must be killed (offtank/cleave DPS handles them).
///
/// Phase 3 (< 40%): Onyxia lands again (same as Phase 1 but adds keep spawning).
///   - Continuously spawns Onyxian Whelps.
///   - Deep Breath from Phase 2 may still occur briefly at landing.

use super::{EncounterEvent, EncounterFsm};
use crate::ffi::SpellId;

pub const ENTRY_ONYXIA: u32 = 10184;

/// Spell / aura IDs
pub const SPELL_DEEP_BREATH:    SpellId = SpellId(22267); // Phase 2 dive bomb
pub const SPELL_FLAME_BREATH:   SpellId = SpellId(18435); // Frontal cone
pub const SPELL_FIREBALL_VOLLEY:SpellId = SpellId(18392); // Phase 2 AoE fireball
pub const SPELL_WING_BUFFET:    SpellId = SpellId(18500); // Knockback

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnyxiaPhase {
    Idle,
    Phase1,   // Ground — melee attack from behind
    Phase2,   // Air — ranged only, dodge Deep Breath
    Phase3,   // Ground again — adds spawning
}

pub struct OnyxiaFsm {
    pub phase: OnyxiaPhase,
    done: bool,
}

impl OnyxiaFsm {
    pub fn new() -> Self {
        Self { phase: OnyxiaPhase::Idle, done: false }
    }

    pub const PHASE_IDLE:   u32 = 0;
    pub const PHASE_GROUND: u32 = 1; // Phase 1 and 3 both use ground mechanics
    pub const PHASE_AIR:    u32 = 2;
}

impl Default for OnyxiaFsm {
    fn default() -> Self { Self::new() }
}

impl EncounterFsm for OnyxiaFsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, _time: u64) {
        if self.done { return; }

        match event {
            EncounterEvent::CombatStarted => {
                self.phase = OnyxiaPhase::Phase1;
            }
            EncounterEvent::UnitDied { victim: _ } => {
                if self.phase != OnyxiaPhase::Idle {
                    self.done = true;
                }
            }
            EncounterEvent::GroupWipe => {
                self.phase = OnyxiaPhase::Idle;
            }
            EncounterEvent::None => {
                match self.phase {
                    OnyxiaPhase::Phase1 if boss_hp_pct < 0.65 => {
                        self.phase = OnyxiaPhase::Phase2;
                    }
                    OnyxiaPhase::Phase2 if boss_hp_pct < 0.40 => {
                        self.phase = OnyxiaPhase::Phase3;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        match self.phase {
            OnyxiaPhase::Idle           => Self::PHASE_IDLE,
            OnyxiaPhase::Phase1
            | OnyxiaPhase::Phase3       => Self::PHASE_GROUND,
            OnyxiaPhase::Phase2         => Self::PHASE_AIR,
        }
    }

    fn is_active(&self) -> bool { self.phase != OnyxiaPhase::Idle }
    fn is_done(&self)   -> bool { self.done }
    fn boss_entry(&self) -> u32 { ENTRY_ONYXIA }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encounters::EncounterEvent;

    #[test]
    fn onyxia_transitions_phases() {
        let mut fsm = OnyxiaFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        assert_eq!(fsm.phase, OnyxiaPhase::Phase1);

        fsm.update(&EncounterEvent::None, 0.64, 1000);
        assert_eq!(fsm.phase, OnyxiaPhase::Phase2);

        fsm.update(&EncounterEvent::None, 0.39, 2000);
        assert_eq!(fsm.phase, OnyxiaPhase::Phase3);
    }

    #[test]
    fn onyxia_phase1_and_3_both_report_ground() {
        let mut fsm = OnyxiaFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        assert_eq!(fsm.phase_id(), OnyxiaFsm::PHASE_GROUND);

        fsm.phase = OnyxiaPhase::Phase3;
        assert_eq!(fsm.phase_id(), OnyxiaFsm::PHASE_GROUND);
    }
}
