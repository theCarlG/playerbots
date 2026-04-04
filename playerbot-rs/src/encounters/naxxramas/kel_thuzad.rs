/// Kel'Thuzad encounter — Naxxramas final boss.
///
/// Phase 1 (~3min 30s): Add waves.
///   Skeletons, Banshees, and Abominations pour in from portals.
///   Raid must survive — no boss to target.
///
/// Phase 2: Kel'Thuzad emerges from sarcophagus.
///   - Shadow Fissure (27810): ground `AoE` — move out immediately.
///   - Frost Blast (27808): frozen target needs immediate healing.
///   - Chains of Kel'Thuzad (28410): mind control.
///
/// Phase 3 (< 45% HP): KT calls for aid — adds from portals + continue boss DPS.
///
/// Bot behavior per state:
///   - Add waves: attack nearest hostile.
///   - KT active: dodge Shadow Fissure (flee if debuffed).
///   - Adds + portal: non-tanks switch to adds, tanks stay on KT.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, AttackNearest, Seq, HasDebuff, FleeToSafe, Sel, IsTank};
use crate::ffi::SpellId;

pub const AURA_FROST_BLAST: SpellId = SpellId(27808);
pub const SPELL_CHAINS_OF_KT: SpellId = SpellId(28410);
pub const SPELL_SHADOW_FISSURE: SpellId = SpellId(27810);
pub const SPELL_GLACIAL_BLAST: SpellId = SpellId(29258);

pub const ENTRY_SARCOPHAGUS_KT: u32 = 15990;

const PHASE1_DURATION_MS: u64 = 210_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KtPhase {
    Idle,
    AddWaves,
    KtActive,
    AddsSummoned,
}

#[derive(Clone, Debug)]
pub struct KelThuzadFsm {
    pub phase: KtPhase,
    pub pull_time_ms: u64,
    done: bool,
    add_waves_bt: Bt,
    kt_active_bt: Bt,
    adds_portal_bt: Bt,
}

impl PartialEq for KelThuzadFsm {
    fn eq(&self, other: &Self) -> bool {
        self.phase == other.phase
            && self.pull_time_ms == other.pull_time_ms
            && self.done == other.done
    }
}

impl KelThuzadFsm {
    pub fn new() -> Self {
        Self {
            phase: KtPhase::Idle,
            pull_time_ms: 0,
            done: false,
            // Phase 1: kill adds
            add_waves_bt: AttackNearest,
            // Phase 2: dodge Shadow Fissure
            kt_active_bt: Seq(vec![HasDebuff(SPELL_SHADOW_FISSURE), FleeToSafe(15.0)]),
            // Phase 3: dodge fissure (priority), non-tanks switch to adds
            adds_portal_bt: Sel(vec![
                Seq(vec![HasDebuff(SPELL_SHADOW_FISSURE), FleeToSafe(15.0)]),
                Seq(vec![IsTank.not(), AttackNearest]),
            ]),
        }
    }

    pub const PHASE_IDLE: u32 = 0;
    pub const PHASE_ADD_WAVES: u32 = 1;
    pub const PHASE_KT_ACTIVE: u32 = 2;
    pub const PHASE_ADDS_PORTAL: u32 = 3;
}

impl Default for KelThuzadFsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for KelThuzadFsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, time_ms: u64) {
        if self.done {
            return;
        }

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

            EncounterEvent::None => match self.phase {
                KtPhase::AddWaves => {
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
            },

            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        match self.phase {
            KtPhase::Idle => Self::PHASE_IDLE,
            KtPhase::AddWaves => Self::PHASE_ADD_WAVES,
            KtPhase::KtActive => Self::PHASE_KT_ACTIVE,
            KtPhase::AddsSummoned => Self::PHASE_ADDS_PORTAL,
        }
    }

    fn is_active(&self) -> bool {
        self.phase != KtPhase::Idle
    }
    fn is_done(&self) -> bool {
        self.done
    }
    fn boss_entry(&self) -> u32 {
        super::ENTRY_KEL_THUZAD
    }

    fn phase_bt(&self) -> Option<&Bt> {
        match self.phase {
            KtPhase::Idle => None,
            KtPhase::AddWaves => Some(&self.add_waves_bt),
            KtPhase::KtActive => Some(&self.kt_active_bt),
            KtPhase::AddsSummoned => Some(&self.adds_portal_bt),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::state::PlayerClass;
    use crate::encounters::EncounterEvent;
    use crate::engine::bt_nodes::{BtNode, BtResult};
    use crate::engine::context::tests::{TestCtxOwned, TestInterface, make_encounter_ctx};
    use crate::ffi::BotRole;

    // ── FSM tests ──────────────────────────────────────────────────────

    #[test]
    fn phase1_times_out_into_phase2() {
        let mut fsm = KelThuzadFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        assert_eq!(fsm.phase, KtPhase::AddWaves);

        fsm.update(&EncounterEvent::None, 1.0, PHASE1_DURATION_MS);
        assert_eq!(fsm.phase, KtPhase::KtActive);
    }

    #[test]
    fn phase3_at_45pct() {
        let mut fsm = KelThuzadFsm::new();
        fsm.phase = KtPhase::KtActive;
        fsm.update(&EncounterEvent::None, 0.44, 0);
        assert_eq!(fsm.phase, KtPhase::AddsSummoned);
    }

    // ── BT tests ───────────────────────────────────────────────────────

    #[test]
    fn add_waves_attacks_adds() {
        let mut fsm = KelThuzadFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);

        let bt = fsm.phase_bt().expect("add waves should have BT");
        let iface = TestInterface::new();
        let mut owned = TestCtxOwned::new();
        owned.attackers = vec![77]; // an add
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn kt_active_no_fissure_returns_failure() {
        let mut fsm = KelThuzadFsm::new();
        fsm.phase = KtPhase::KtActive;

        let bt = fsm.phase_bt().expect("KT active should have BT");
        let iface = TestInterface::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Failure);
    }
}
