/// Thaddius encounter FSM — Naxxramas, Construct Wing.
///
/// Two-phase fight with the most mechanically demanding mechanic in Naxxramas:
/// the polarity charge system.
///
/// Phase 1: Two adds — Stalagg and Feugen — must be tanked simultaneously.
///   The raid is split: half goes to Stalagg's platform (west), half to Feugen's
///   (east).  Both must die within seconds of each other or the dead one
///   resurrects Thaddius early.
///
/// Phase 2: Thaddius itself.
///   - Every ~30s: Polarity Shift (spell 28089) — each bot gets a +/- charge aura.
///     Bots with the SAME polarity must stack together.
///     Bots with OPPOSITE polarity standing within 13 yards deal massive damage.
///   - After polarity shift: bots with + charge go to the left half of the room,
///     bots with - charge go to the right half.
///   - Bots cannot stand near anyone with the opposite charge.
///   - Positive charge aura: 29659 (Positive Charge)
///   - Negative charge aura: 29660 (Negative Charge)
///
/// Bot behavior:
///   - Phase 1: tank role = tank add on assigned platform; DPS/heal = focus one add.
///   - Phase 2: check own polarity aura, immediately run to correct side after shift.

use super::super::{EncounterEvent, EncounterFsm};

pub const SPELL_POLARITY_SHIFT:    u32 = 28089;
pub const AURA_POSITIVE_CHARGE:    u32 = 29659;
pub const AURA_NEGATIVE_CHARGE:    u32 = 29660;

pub const ENTRY_STALAGG: u32 = 15929;
pub const ENTRY_FEUGEN:  u32 = 15930;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThaddiusPhase {
    Idle,
    /// Adds (Stalagg + Feugen) are alive.
    Adds,
    /// Thaddius is active, waiting for polarity shift.
    Normal,
    /// Polarity Shift just fired — bots must reposition NOW.
    PolarityReposition,
}

pub struct ThaddiusFsm {
    pub phase:              ThaddiusPhase,
    pub stalagg_dead:       bool,
    pub feugen_dead:        bool,
    /// Server time when polarity shift occurred.
    pub polarity_shift_ms:  u64,
    done: bool,
}

impl ThaddiusFsm {
    pub fn new() -> Self {
        Self {
            phase:             ThaddiusPhase::Idle,
            stalagg_dead:      false,
            feugen_dead:       false,
            polarity_shift_ms: 0,
            done:              false,
        }
    }

    pub const PHASE_IDLE:          u32 = 0;
    pub const PHASE_ADDS:          u32 = 1;
    pub const PHASE_THADDIUS:      u32 = 2;
    pub const PHASE_POLARITY:      u32 = 3;

    /// How long (ms) to stay in Polarity Reposition state before resuming DPS.
    const REPOSITION_WINDOW_MS: u64 = 8_000;
}

impl Default for ThaddiusFsm {
    fn default() -> Self { Self::new() }
}

impl EncounterFsm for ThaddiusFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, time_ms: u64) {
        if self.done { return; }

        match event {
            EncounterEvent::CombatStarted => {
                self.phase = ThaddiusPhase::Adds;
            }

            EncounterEvent::UnitDied { victim: _ } => {
                match self.phase {
                    ThaddiusPhase::Adds => {
                        // Can't distinguish which add died without NPC entry lookup.
                        // Assume sequential deaths: first = Feugen, second = Stalagg.
                        if !self.feugen_dead {
                            self.feugen_dead = true;
                        } else if !self.stalagg_dead {
                            self.stalagg_dead = true;
                        }
                        // Both adds dead → Thaddius spawns.
                        if self.stalagg_dead && self.feugen_dead {
                            self.phase = ThaddiusPhase::Normal;
                        }
                    }
                    ThaddiusPhase::Normal | ThaddiusPhase::PolarityReposition => {
                        self.done = true;
                    }
                    _ => {}
                }
            }

            EncounterEvent::SpellCast { spell_id, success: true, .. } => {
                if *spell_id == SPELL_POLARITY_SHIFT
                    && self.phase == ThaddiusPhase::Normal
                {
                    self.phase = ThaddiusPhase::PolarityReposition;
                    self.polarity_shift_ms = time_ms;
                }
            }

            EncounterEvent::None => {
                // Return to Normal after reposition window.
                if self.phase == ThaddiusPhase::PolarityReposition
                    && time_ms.saturating_sub(self.polarity_shift_ms) > Self::REPOSITION_WINDOW_MS
                {
                    self.phase = ThaddiusPhase::Normal;
                }
            }

            EncounterEvent::GroupWipe => {
                self.phase = ThaddiusPhase::Idle;
                self.stalagg_dead = false;
                self.feugen_dead  = false;
            }

            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        match self.phase {
            ThaddiusPhase::Idle               => Self::PHASE_IDLE,
            ThaddiusPhase::Adds               => Self::PHASE_ADDS,
            ThaddiusPhase::Normal             => Self::PHASE_THADDIUS,
            ThaddiusPhase::PolarityReposition => Self::PHASE_POLARITY,
        }
    }

    fn is_active(&self) -> bool { self.phase != ThaddiusPhase::Idle }
    fn is_done(&self)   -> bool { self.done }
    fn boss_entry(&self) -> u32 { super::ENTRY_THADDIUS }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encounters::EncounterEvent;

    #[test]
    fn thaddius_starts_in_adds_phase() {
        let mut fsm = ThaddiusFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        assert_eq!(fsm.phase, ThaddiusPhase::Adds);
    }

    #[test]
    fn thaddius_transitions_after_both_adds_die() {
        let mut fsm = ThaddiusFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        fsm.update(&EncounterEvent::UnitDied { victim: 100 }, 1.0, 1000); // Feugen
        assert_eq!(fsm.phase, ThaddiusPhase::Adds);
        fsm.update(&EncounterEvent::UnitDied { victim: 200 }, 1.0, 2000); // Stalagg
        assert_eq!(fsm.phase, ThaddiusPhase::Normal);
    }

    #[test]
    fn thaddius_polarity_shift_triggers_reposition() {
        let mut fsm = ThaddiusFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        // Skip adds phase.
        fsm.phase = ThaddiusPhase::Normal;

        fsm.update(&EncounterEvent::SpellCast {
            caster: 1, spell_id: SPELL_POLARITY_SHIFT, success: true
        }, 0.80, 1000);
        assert_eq!(fsm.phase, ThaddiusPhase::PolarityReposition);
    }

    #[test]
    fn thaddius_returns_to_normal_after_reposition_window() {
        let mut fsm = ThaddiusFsm::new();
        fsm.phase = ThaddiusPhase::PolarityReposition;
        fsm.polarity_shift_ms = 1000;

        fsm.update(&EncounterEvent::None, 0.70, 1000 + ThaddiusFsm::REPOSITION_WINDOW_MS + 1);
        assert_eq!(fsm.phase, ThaddiusPhase::Normal);
    }
}
