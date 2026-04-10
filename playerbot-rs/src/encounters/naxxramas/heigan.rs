/// Heigan the Unclean encounter — Naxxramas, Plague Wing.
///
/// Famous for the "Heigan Dance" — requires repositioning on a strict rotation.
///
/// Phase 1 (DPS phase, ~90s):
///   Heigan is on his platform. Ranged/heals stand near him.
///   Eruption zones rotate through 4 channels in the room floor.
///
/// Phase 2 (Dance phase, ~45s):
///   Heigan teleports to the dance floor. All raid members MUST follow
///   and survive the eruption rotation by standing in the safe zone.
///   Eruption fires every 4s, cycling through zones 1→2→3→4→1...
///
/// Bot behavior per state:
///   - DPS phase: normal rotation (no override).
///   - Dance phase: move to safe zone based on eruption tracking.
use super::super::{EncounterEvent, EncounterFsm};
use crate::engine::bt::Bt::{self, MoveToSafeZone};
use crate::ffi::SpellId;

pub const SPELL_ERUPTION_ZONE1: SpellId = SpellId(29998);
pub const SPELL_ERUPTION_ZONE2: SpellId = SpellId(30004);
pub const SPELL_ERUPTION_ZONE3: SpellId = SpellId(30006);
pub const SPELL_ERUPTION_ZONE4: SpellId = SpellId(30010);
pub const SPELL_PLAGUE_CLOUD: SpellId = SpellId(29350);

/// Centers of the four eruption stripes on Heigan's dance floor.
///
/// The stripes run parallel to each other; zones 1..4 are the four stripes
/// closest-to-farthest from Heigan's platform. The room is oriented along
/// the X axis so the zone X centers differ; Y/Z are roughly constant.
///
/// Values are taken from widely-documented Heigan guides (the core
/// eruption trigger GameObjects aren't addressable from the bot side).
/// Bots pathfind to the closest point in the stripe via `move_to`, so
/// each entry is the *stripe center*, not a precise safe spot.
const HEIGAN_ZONE_CENTERS: [(f32, f32, f32); 4] = [
    (2793.0, -3710.0, 273.5), // Zone 1 (closest to platform)
    (2786.0, -3710.0, 273.5), // Zone 2
    (2779.0, -3710.0, 273.5), // Zone 3
    (2772.0, -3710.0, 273.5), // Zone 4 (farthest)
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeiganPhase {
    #[default]
    Idle,
    DpsPhase,
    DancePhase,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct HeiganFsm {
    pub phase: HeiganPhase,
    pub erupting_zone: u8,
    pub dance_start_ms: u64,
    done: bool,
}

impl HeiganFsm {
    /// Which zone is safe right now (the zone after the erupting one).
    /// Returns 1-4. Returns 1 if no eruption data yet.
    pub fn safe_zone(&self) -> u8 {
        match self.erupting_zone {
            1 => 2,
            2 => 3,
            3 => 4,
            4 => 1,
            _ => 1,
        }
    }

    pub const PHASE_IDLE: u32 = 0;
    pub const PHASE_DPS: u32 = 1;
    pub const PHASE_DANCE: u32 = 2;
}

impl EncounterFsm for HeiganFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, time_ms: u64) {
        if self.done {
            return;
        }

        match event {
            EncounterEvent::CombatStarted => {
                self.phase = HeiganPhase::DpsPhase;
            }

            EncounterEvent::SpellCast {
                caster: _,
                spell_id,
                success: true,
            } => match *spell_id {
                SPELL_ERUPTION_ZONE1 => {
                    self.erupting_zone = 1;
                    if self.phase == HeiganPhase::DpsPhase {
                        self.phase = HeiganPhase::DancePhase;
                        self.dance_start_ms = time_ms;
                    }
                }
                SPELL_ERUPTION_ZONE2 => {
                    self.erupting_zone = 2;
                }
                SPELL_ERUPTION_ZONE3 => {
                    self.erupting_zone = 3;
                }
                SPELL_ERUPTION_ZONE4 => {
                    self.erupting_zone = 4;
                }
                _ => {}
            },

            EncounterEvent::UnitDied { victim: _ } => {
                self.done = true;
            }

            EncounterEvent::GroupWipe => {
                self.phase = HeiganPhase::Idle;
            }

            EncounterEvent::None => {
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
            HeiganPhase::Idle => Self::PHASE_IDLE,
            HeiganPhase::DpsPhase => Self::PHASE_DPS,
            HeiganPhase::DancePhase => Self::PHASE_DANCE,
        }
    }

    fn is_active(&self) -> bool {
        self.phase != HeiganPhase::Idle
    }
    fn is_done(&self) -> bool {
        self.done
    }
    fn boss_entry(&self) -> u32 {
        super::ENTRY_HEIGAN
    }

    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        match self.phase {
            HeiganPhase::DancePhase => Some(MoveToSafeZone),
            _ => None, // DPS phase: normal rotation
        }
    }

    fn safe_zone_hint(&self) -> u8 {
        self.safe_zone()
    }

    fn safe_zone_position(&self) -> Option<(f32, f32, f32)> {
        // Only meaningful during the dance phase; in DPS phase we
        // shouldn't be moving to a stripe.
        if self.phase != HeiganPhase::DancePhase {
            return None;
        }
        let idx = self.safe_zone() as usize;
        if idx == 0 || idx > HEIGAN_ZONE_CENTERS.len() {
            return None;
        }
        Some(HEIGAN_ZONE_CENTERS[idx - 1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encounters::EncounterEvent;

    // ── FSM tests ──────────────────────────────────────────────────────

    #[test]
    fn starts_in_dps_phase() {
        let mut fsm = HeiganFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        assert_eq!(fsm.phase, HeiganPhase::DpsPhase);
    }

    #[test]
    fn transitions_to_dance_on_first_eruption() {
        let mut fsm = HeiganFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        fsm.update(
            &EncounterEvent::SpellCast {
                caster: 1,
                spell_id: SPELL_ERUPTION_ZONE1,
                success: true,
            },
            1.0,
            5000,
        );
        assert_eq!(fsm.phase, HeiganPhase::DancePhase);
        assert_eq!(fsm.erupting_zone, 1);
        assert_eq!(fsm.safe_zone(), 2);
    }

    #[test]
    fn eruption_zone_tracking() {
        let mut fsm = HeiganFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        fsm.update(
            &EncounterEvent::SpellCast {
                caster: 1,
                spell_id: SPELL_ERUPTION_ZONE1,
                success: true,
            },
            1.0,
            1000,
        );
        fsm.update(
            &EncounterEvent::SpellCast {
                caster: 1,
                spell_id: SPELL_ERUPTION_ZONE2,
                success: true,
            },
            1.0,
            5000,
        );
        assert_eq!(fsm.erupting_zone, 2);
        assert_eq!(fsm.safe_zone(), 3);
    }

    #[test]
    fn returns_to_dps_phase_after_45s() {
        let mut fsm = HeiganFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        fsm.update(
            &EncounterEvent::SpellCast {
                caster: 1,
                spell_id: SPELL_ERUPTION_ZONE1,
                success: true,
            },
            1.0,
            0,
        );
        assert_eq!(fsm.phase, HeiganPhase::DancePhase);

        fsm.update(&EncounterEvent::None, 0.8, 46_000);
        assert_eq!(fsm.phase, HeiganPhase::DpsPhase);
    }

    // ── BT tests ───────────────────────────────────────────────────────

    #[test]
    fn dance_phase_has_bt() {
        let mut fsm = HeiganFsm::default();
        fsm.phase = HeiganPhase::DancePhase;
        assert!(
            fsm.phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
                .is_some()
        );
    }

    #[test]
    fn dps_phase_no_bt() {
        let mut fsm = HeiganFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        assert!(
            fsm.phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
                .is_none()
        );
    }
}
