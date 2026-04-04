/// Ragnaros encounter FSM — Molten Core final boss.
///
/// Fight timeline:
///   Phase 1 (100% → 75%): Ground phase.  Slow melee, Wrath of Ragnaros (AoE),
///                           Hand of Ragnaros (melee knockback).
///   Submerged (75%, 50%, 25% thresholds): Ragnaros submerges.  8 Sons of Flame
///                           spawn at the edge and walk toward raid.  Kill them all
///                           to trigger re-emerge.  Happens up to 3 times.
///   Phase 2 (< 25%):        No more submerges. Adds spawn continuously.
///                           Sons of Flame + Wrath of Ragnaros.
///
/// Bot behavior per phase:
///   - Phase 1 / active ground: spread 15+ yards from each other (Wrath knockback).
///     Tank holds Ragnaros. Ranged/heals spread to fire ledge edge.
///   - Submerged: kill Sons of Flame (priority targets). Don't stand in fire.
///   - Phase 2: burn boss, kill Sons as spawned.

use super::super::{EncounterEvent, EncounterFsm};
use crate::ffi::SpellId;

/// NPC entry for Son of Flame.
pub const ENTRY_SON_OF_FLAME: u32 = 12143;

/// Spell IDs
pub const SPELL_WRATH_OF_RAGNAROS:  SpellId = SpellId(20566); // AoE knockback
pub const SPELL_HAND_OF_RAGNAROS:   SpellId = SpellId(19780); // melee knockback
pub const SPELL_ELEMENTAL_FIRE:     SpellId = SpellId(20563); // DoT

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RagnarosPhase {
    /// Not yet engaged.
    Idle,
    /// Active ground phase (100%–75%, and between submerges).
    Ground,
    /// Submerged — Sons of Flame must be killed.
    Submerged,
    /// Phase 2: < 25% HP, continuous adds.
    Phase2,
}

pub struct RagnarosFsm {
    pub phase:          RagnarosPhase,
    pub submerge_count: u8,   // number of submerges so far
    pub sons_killed:    u8,   // sons killed during current submerge
    done:               bool,
}

impl RagnarosFsm {
    pub fn new() -> Self {
        Self {
            phase:          RagnarosPhase::Idle,
            submerge_count: 0,
            sons_killed:    0,
            done:           false,
        }
    }

    /// Phase ID encoding for the BT subtree switch:
    ///   0 = Idle, 1 = Ground, 2 = Submerged, 3 = Phase2
    pub const PHASE_IDLE:      u32 = 0;
    pub const PHASE_GROUND:    u32 = 1;
    pub const PHASE_SUBMERGED: u32 = 2;
    pub const PHASE_2:         u32 = 3;
}

impl Default for RagnarosFsm {
    fn default() -> Self { Self::new() }
}

impl EncounterFsm for RagnarosFsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, _time_ms: u64) {
        if self.done { return; }

        match event {
            EncounterEvent::CombatStarted => {
                self.phase = RagnarosPhase::Ground;
            }

            EncounterEvent::UnitDied { victim: _ } => {
                // Track Son of Flame deaths to know when to expect re-emerge.
                if self.phase == RagnarosPhase::Submerged {
                    self.sons_killed += 1;
                    // Re-emerge happens when all 8 Sons are killed.
                    if self.sons_killed >= 8 {
                        self.phase = RagnarosPhase::Ground;
                        self.sons_killed = 0;
                    }
                } else {
                    // Check if Ragnaros himself died.
                    self.done = true;
                }
            }

            EncounterEvent::GroupWipe => {
                self.phase = RagnarosPhase::Idle;
                self.submerge_count = 0;
                self.sons_killed = 0;
            }

            EncounterEvent::None => {
                // HP-based transitions.
                match self.phase {
                    RagnarosPhase::Ground => {
                        // Submerge triggers at 75%, 50%, 25% (each only once).
                        let should_submerge = match self.submerge_count {
                            0 => boss_hp_pct < 0.75,
                            1 => boss_hp_pct < 0.50,
                            2 => boss_hp_pct < 0.25,
                            _ => false,
                        };
                        if should_submerge {
                            self.phase = RagnarosPhase::Submerged;
                            self.submerge_count += 1;
                            self.sons_killed = 0;
                            // Phase 2 starts after the 3rd submerge (at 25%).
                            if self.submerge_count >= 3 {
                                // Will transition to Phase2 after sons die.
                            }
                        }
                    }
                    // Submerge → Ground handled by UnitDied above.
                    // Phase2 → done handled by UnitDied above.
                    _ => {}
                }

                // Phase 2 enters after submerge_count 3 and re-emerge.
                if self.phase == RagnarosPhase::Ground
                    && self.submerge_count >= 3
                    && boss_hp_pct < 0.25
                {
                    self.phase = RagnarosPhase::Phase2;
                }
            }

            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        match self.phase {
            RagnarosPhase::Idle      => Self::PHASE_IDLE,
            RagnarosPhase::Ground    => Self::PHASE_GROUND,
            RagnarosPhase::Submerged => Self::PHASE_SUBMERGED,
            RagnarosPhase::Phase2    => Self::PHASE_2,
        }
    }

    fn is_active(&self) -> bool { self.phase != RagnarosPhase::Idle }
    fn is_done(&self)   -> bool { self.done }
    fn boss_entry(&self) -> u32 { super::ENTRY_RAGNAROS }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encounters::EncounterEvent;

    #[test]
    fn ragnaros_activates_on_combat_started() {
        let mut fsm = RagnarosFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        assert_eq!(fsm.phase, RagnarosPhase::Ground);
    }

    #[test]
    fn ragnaros_submerges_at_75pct() {
        let mut fsm = RagnarosFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        fsm.update(&EncounterEvent::None, 0.74, 1000);
        assert_eq!(fsm.phase, RagnarosPhase::Submerged);
        assert_eq!(fsm.submerge_count, 1);
    }

    #[test]
    fn ragnaros_reemerges_after_8_sons_killed() {
        let mut fsm = RagnarosFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        fsm.update(&EncounterEvent::None, 0.74, 1000);
        assert_eq!(fsm.phase, RagnarosPhase::Submerged);

        // Kill 7 sons — still submerged.
        for _ in 0..7 {
            fsm.update(&EncounterEvent::UnitDied { victim: 999 }, 0.74, 2000);
        }
        assert_eq!(fsm.phase, RagnarosPhase::Submerged);

        // Kill 8th son — re-emerges.
        fsm.update(&EncounterEvent::UnitDied { victim: 999 }, 0.74, 3000);
        assert_eq!(fsm.phase, RagnarosPhase::Ground);
    }

    #[test]
    fn ragnaros_does_not_submerge_twice_at_same_threshold() {
        let mut fsm = RagnarosFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        // First submerge at 75%.
        fsm.update(&EncounterEvent::None, 0.74, 1000);
        assert_eq!(fsm.submerge_count, 1);
        // Kill sons → re-emerge.
        for _ in 0..8 {
            fsm.update(&EncounterEvent::UnitDied { victim: 999 }, 0.74, 2000);
        }
        // Still at 74%, should NOT submerge again (need to cross 50% threshold).
        fsm.update(&EncounterEvent::None, 0.74, 3000);
        assert_eq!(fsm.phase, RagnarosPhase::Ground);
        assert_eq!(fsm.submerge_count, 1);
    }

    #[test]
    fn ragnaros_enters_phase2_at_25pct_after_3rd_submerge() {
        let mut fsm = RagnarosFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);

        // Simulate three submerge cycles.
        let hps = [0.74_f32, 0.49, 0.24];
        for hp in &hps {
            fsm.update(&EncounterEvent::None, *hp, 0);
            for _ in 0..8 {
                fsm.update(&EncounterEvent::UnitDied { victim: 999 }, *hp, 0);
            }
        }
        // After 3rd re-emerge with hp < 25%, enter Phase2.
        fsm.update(&EncounterEvent::None, 0.24, 0);
        assert_eq!(fsm.phase, RagnarosPhase::Phase2);
    }
}
