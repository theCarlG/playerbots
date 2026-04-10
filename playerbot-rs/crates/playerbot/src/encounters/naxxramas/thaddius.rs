/// Thaddius encounter — Naxxramas, Construct Wing.
///
/// Phase 1: Two adds (Stalagg + Feugen) must be tanked and killed simultaneously.
///
/// Phase 2: Thaddius himself.
///   - Every ~30s: Polarity Shift — each bot gets a +/- charge aura.
///     Same polarity must stack. Opposite polarity within 13yd = massive damage.
///   - Positive charge aura: 29659. Negative charge aura: 29660.
///   - After shift: positive → left side, negative → right side.
///
/// Bot behavior per state:
///   - Adds: attack nearest hostile (kill Stalagg/Feugen).
///   - Polarity: check own charge, move to correct side of room.
///   - Normal (Thaddius, no shift): normal rotation.
use super::super::{EncounterEvent, EncounterFsm};
use crate::engine::bt::Bt::{self, AttackNearest, MoveToSafeZone};
use cmangos::SpellId;
use crate::{Sel, Seq};

pub const SPELL_POLARITY_SHIFT: SpellId = SpellId(28089);
pub const AURA_POSITIVE_CHARGE: SpellId = SpellId(29659);
pub const AURA_NEGATIVE_CHARGE: SpellId = SpellId(29660);

pub const ENTRY_STALAGG: u32 = 15929;
pub const ENTRY_FEUGEN: u32 = 15930;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThaddiusPhase {
    #[default]
    Idle,
    Adds,
    Normal,
    PolarityReposition,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ThaddiusFsm {
    pub phase: ThaddiusPhase,
    pub stalagg_dead: bool,
    pub feugen_dead: bool,
    pub polarity_shift_ms: u64,
    done: bool,
}

impl ThaddiusFsm {
    pub const PHASE_IDLE: u32 = 0;
    pub const PHASE_ADDS: u32 = 1;
    pub const PHASE_THADDIUS: u32 = 2;
    pub const PHASE_POLARITY: u32 = 3;

    const REPOSITION_WINDOW_MS: u64 = 8_000;
}

impl EncounterFsm for ThaddiusFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, time_ms: u64) {
        if self.done {
            return;
        }

        match event {
            EncounterEvent::CombatStarted => {
                self.phase = ThaddiusPhase::Adds;
            }

            EncounterEvent::UnitDied { victim: _ } => match self.phase {
                ThaddiusPhase::Adds => {
                    if !self.feugen_dead {
                        self.feugen_dead = true;
                    } else if !self.stalagg_dead {
                        self.stalagg_dead = true;
                    }
                    if self.stalagg_dead && self.feugen_dead {
                        self.phase = ThaddiusPhase::Normal;
                    }
                }
                ThaddiusPhase::Normal | ThaddiusPhase::PolarityReposition => {
                    self.done = true;
                }
                ThaddiusPhase::Idle => {}
            },

            EncounterEvent::SpellCast {
                spell_id,
                success: true,
                ..
            } => {
                if *spell_id == SPELL_POLARITY_SHIFT && self.phase == ThaddiusPhase::Normal {
                    self.phase = ThaddiusPhase::PolarityReposition;
                    self.polarity_shift_ms = time_ms;
                }
            }

            EncounterEvent::None => {
                if self.phase == ThaddiusPhase::PolarityReposition
                    && time_ms.saturating_sub(self.polarity_shift_ms) > Self::REPOSITION_WINDOW_MS
                {
                    self.phase = ThaddiusPhase::Normal;
                }
            }

            EncounterEvent::GroupWipe => {
                self.phase = ThaddiusPhase::Idle;
                self.stalagg_dead = false;
                self.feugen_dead = false;
            }

            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        match self.phase {
            ThaddiusPhase::Idle => Self::PHASE_IDLE,
            ThaddiusPhase::Adds => Self::PHASE_ADDS,
            ThaddiusPhase::Normal => Self::PHASE_THADDIUS,
            ThaddiusPhase::PolarityReposition => Self::PHASE_POLARITY,
        }
    }

    fn is_active(&self) -> bool {
        self.phase != ThaddiusPhase::Idle
    }
    fn is_done(&self) -> bool {
        self.done
    }
    fn boss_entry(&self) -> u32 {
        super::ENTRY_THADDIUS
    }

    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        match self.phase {
            ThaddiusPhase::Adds => Some(AttackNearest),
            ThaddiusPhase::PolarityReposition => Some(Sel!(
                // Positive charge → move to safe zone (left side)
                Seq!(Bt::self_has(AURA_POSITIVE_CHARGE), MoveToSafeZone),
                // Negative charge → move to safe zone (right side)
                Seq!(Bt::self_has(AURA_NEGATIVE_CHARGE), MoveToSafeZone),
            )),
            _ => None, // Normal/Idle: normal rotation
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encounters::EncounterEvent;

    // ── FSM tests ──────────────────────────────────────────────────────

    #[test]
    fn starts_in_adds_phase() {
        let mut fsm = ThaddiusFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        assert_eq!(fsm.phase, ThaddiusPhase::Adds);
    }

    #[test]
    fn transitions_after_both_adds_die() {
        let mut fsm = ThaddiusFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        fsm.update(&EncounterEvent::UnitDied { victim: 100 }, 1.0, 1000);
        assert_eq!(fsm.phase, ThaddiusPhase::Adds);
        fsm.update(&EncounterEvent::UnitDied { victim: 200 }, 1.0, 2000);
        assert_eq!(fsm.phase, ThaddiusPhase::Normal);
    }

    #[test]
    fn polarity_shift_triggers_reposition() {
        let mut fsm = ThaddiusFsm::default();
        fsm.phase = ThaddiusPhase::Normal;

        fsm.update(
            &EncounterEvent::SpellCast {
                caster: 1,
                spell_id: SPELL_POLARITY_SHIFT,
                success: true,
            },
            0.80,
            1000,
        );
        assert_eq!(fsm.phase, ThaddiusPhase::PolarityReposition);
    }

    #[test]
    fn returns_to_normal_after_reposition_window() {
        let mut fsm = ThaddiusFsm::default();
        fsm.phase = ThaddiusPhase::PolarityReposition;
        fsm.polarity_shift_ms = 1000;

        fsm.update(
            &EncounterEvent::None,
            0.70,
            1000 + ThaddiusFsm::REPOSITION_WINDOW_MS + 1,
        );
        assert_eq!(fsm.phase, ThaddiusPhase::Normal);
    }

    // ── BT tests ───────────────────────────────────────────────────────

    #[test]
    fn adds_phase_has_bt() {
        let mut fsm = ThaddiusFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        assert!(
            fsm.phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
                .is_some()
        );
    }

    #[test]
    fn polarity_phase_has_bt() {
        let mut fsm = ThaddiusFsm::default();
        fsm.phase = ThaddiusPhase::PolarityReposition;
        assert!(
            fsm.phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
                .is_some()
        );
    }

    #[test]
    fn normal_phase_no_bt() {
        let mut fsm = ThaddiusFsm::default();
        fsm.phase = ThaddiusPhase::Normal;
        assert!(
            fsm.phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
                .is_none()
        );
    }
}
