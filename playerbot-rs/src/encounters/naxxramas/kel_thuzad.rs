/// Kel'Thuzad encounter FSM — Naxxramas final boss.
///
/// Phase 1 (~3min 30s): Add waves.
///   Skeletons (Soldier of the Frozen Wastes), Banshees (Soul Weaver), and
///   Abominations (Unstoppable Abomination) pour in from the room portals.
///   Raid must survive until Phase 2 begins — no boss to target.
///
/// Phase 2: Kel'Thuzad himself emerges from the sarcophagus.
///   - Frost Blast (aura 27808): frozen target takes 26k damage — healers must
///     top up the target immediately.
///   - Chains of Kel'Thuzad (28410): 1 player is chained and mind-controlled
///     for 20s, used to attack the raid.  Others must kill their MC'd ally.
///   - Shadow Fissure (27810): ground AoE — move out immediately.
///   - Glacial Blast (29258): massive frost damage from a chain of bolts.
///   - Frost Nova on Phase 2 start.
///
/// Phase 3 (< 45% HP): Kel'Thuzad calls for aid — four Crypt Fiends + two
///   Abominations from the portals.  Continue DPS on KT.

use super::super::{EncounterEvent, EncounterFsm};

pub const AURA_FROST_BLAST:     u32 = 27808;
pub const SPELL_CHAINS_OF_KT:   u32 = 28410;
pub const SPELL_SHADOW_FISSURE: u32 = 27810;
pub const SPELL_GLACIAL_BLAST:  u32 = 29258;

/// Entry ID for Kel'Thuzad himself (emerges from sarcophagus in Phase 2).
pub const ENTRY_SARCOPHAGUS_KT: u32 = 15990;

/// Duration of Phase 1 (approximately 3 minutes 30 seconds).
const PHASE1_DURATION_MS: u64 = 210_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KtPhase {
    Idle,
    /// Phase 1: add waves before KT appears.
    AddWaves,
    /// Phase 2: KT is active, normal DPS.
    KtActive,
    /// Phase 3: adds from portals + KT.
    AddsSummoned,
}

pub struct KelThuzadFsm {
    pub phase:       KtPhase,
    pub pull_time_ms: u64,
    done: bool,
}

impl KelThuzadFsm {
    pub fn new() -> Self {
        Self { phase: KtPhase::Idle, pull_time_ms: 0, done: false }
    }

    pub const PHASE_IDLE:         u32 = 0;
    pub const PHASE_ADD_WAVES:    u32 = 1;
    pub const PHASE_KT_ACTIVE:    u32 = 2;
    pub const PHASE_ADDS_PORTAL:  u32 = 3;
}

impl Default for KelThuzadFsm {
    fn default() -> Self { Self::new() }
}

impl EncounterFsm for KelThuzadFsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, time_ms: u64) {
        if self.done { return; }

        match event {
            EncounterEvent::CombatStarted => {
                self.phase = KtPhase::AddWaves;
                self.pull_time_ms = time_ms;
            }

            EncounterEvent::UnitDied { victim: _ } => {
                if self.phase == KtPhase::KtActive || self.phase == KtPhase::AddsSummoned {
                    self.done = true;
                }
            }

            EncounterEvent::GroupWipe => {
                self.phase = KtPhase::Idle;
            }

            EncounterEvent::None => {
                match self.phase {
                    KtPhase::AddWaves => {
                        // KT appears after ~3min 30s.
                        if time_ms.saturating_sub(self.pull_time_ms) >= PHASE1_DURATION_MS {
                            self.phase = KtPhase::KtActive;
                        }
                    }
                    KtPhase::KtActive => {
                        if boss_hp_pct < 0.45 {
                            self.phase = KtPhase::AddsSummoned;
                        }
                    }
                    _ => {}
                }
            }

            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        match self.phase {
            KtPhase::Idle         => Self::PHASE_IDLE,
            KtPhase::AddWaves     => Self::PHASE_ADD_WAVES,
            KtPhase::KtActive     => Self::PHASE_KT_ACTIVE,
            KtPhase::AddsSummoned => Self::PHASE_ADDS_PORTAL,
        }
    }

    fn is_active(&self) -> bool { self.phase != KtPhase::Idle }
    fn is_done(&self)   -> bool { self.done }
    fn boss_entry(&self) -> u32 { super::ENTRY_KEL_THUZAD }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encounters::EncounterEvent;

    #[test]
    fn kt_phase1_times_out_into_phase2() {
        let mut fsm = KelThuzadFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        assert_eq!(fsm.phase, KtPhase::AddWaves);

        fsm.update(&EncounterEvent::None, 1.0, PHASE1_DURATION_MS);
        assert_eq!(fsm.phase, KtPhase::KtActive);
    }

    #[test]
    fn kt_phase3_at_45pct() {
        let mut fsm = KelThuzadFsm::new();
        fsm.phase = KtPhase::KtActive;
        fsm.update(&EncounterEvent::None, 0.44, 0);
        assert_eq!(fsm.phase, KtPhase::AddsSummoned);
    }
}
