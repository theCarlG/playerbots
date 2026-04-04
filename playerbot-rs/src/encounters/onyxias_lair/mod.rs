/// Onyxia's Lair — 3-phase fight.
///
/// States:
///   Phase 1 (100%→65%): Ground. Melee position behind boss.
///   Phase 2 (65%→40%): Air. Melee hold, dodge Deep Breath, ranged normal.
///   Phase 3 (<40%): Ground again + whelp spawns.
use super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, Sel, Seq, TargetHasAura, FleeToSafe, IsMeleeDps, HoldPosition, MoveBehind};
use crate::ffi::SpellId;

pub const ENTRY_ONYXIA: u32 = 10184;

pub const SPELL_DEEP_BREATH: SpellId = SpellId(22267);
pub const SPELL_FLAME_BREATH: SpellId = SpellId(18435);
pub const SPELL_FIREBALL_VOLLEY: SpellId = SpellId(18392);
pub const SPELL_WING_BUFFET: SpellId = SpellId(18500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnyxiaPhase {
    Idle,
    Phase1,
    Phase2,
    Phase3,
}

pub struct OnyxiaFsm {
    pub phase: OnyxiaPhase,
    done: bool,
    air_bt: Bt,
    ground_bt: Bt,
}

impl OnyxiaFsm {
    pub fn new() -> Self {
        Self {
            phase: OnyxiaPhase::Idle,
            done: false,
            air_bt: Sel(vec![
                Seq(vec![TargetHasAura(SPELL_DEEP_BREATH), FleeToSafe(40.0)]),
                Seq(vec![IsMeleeDps, HoldPosition]),
            ]),
            ground_bt: Seq(vec![IsMeleeDps, MoveBehind(5.0)]),
        }
    }

    pub const PHASE_IDLE: u32 = 0;
    pub const PHASE_GROUND: u32 = 1;
    pub const PHASE_AIR: u32 = 2;
}

impl Default for OnyxiaFsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for OnyxiaFsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, _time: u64) {
        if self.done {
            return;
        }
        match event {
            EncounterEvent::CombatStarted => self.phase = OnyxiaPhase::Phase1,
            EncounterEvent::UnitDied { .. } if self.phase != OnyxiaPhase::Idle => {
                self.done = true;
            }
            EncounterEvent::GroupWipe => self.phase = OnyxiaPhase::Idle,
            EncounterEvent::None => match self.phase {
                OnyxiaPhase::Phase1 if boss_hp_pct < 0.65 => self.phase = OnyxiaPhase::Phase2,
                OnyxiaPhase::Phase2 if boss_hp_pct < 0.40 => self.phase = OnyxiaPhase::Phase3,
                _ => {}
            },
            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        match self.phase {
            OnyxiaPhase::Idle => Self::PHASE_IDLE,
            OnyxiaPhase::Phase1 | OnyxiaPhase::Phase3 => Self::PHASE_GROUND,
            OnyxiaPhase::Phase2 => Self::PHASE_AIR,
        }
    }

    fn is_active(&self) -> bool {
        self.phase != OnyxiaPhase::Idle
    }
    fn is_done(&self) -> bool {
        self.done
    }
    fn boss_entry(&self) -> u32 {
        ENTRY_ONYXIA
    }

    fn phase_bt(&self) -> Option<&Bt> {
        match self.phase {
            OnyxiaPhase::Idle => None,
            OnyxiaPhase::Phase2 => Some(&self.air_bt),
            _ => Some(&self.ground_bt),
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

    #[test]
    fn transitions_phases() {
        let mut fsm = OnyxiaFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        assert_eq!(fsm.phase, OnyxiaPhase::Phase1);
        fsm.update(&EncounterEvent::None, 0.64, 0);
        assert_eq!(fsm.phase, OnyxiaPhase::Phase2);
        fsm.update(&EncounterEvent::None, 0.39, 0);
        assert_eq!(fsm.phase, OnyxiaPhase::Phase3);
    }

    #[test]
    fn air_melee_holds_position() {
        let mut fsm = OnyxiaFsm::new();
        fsm.phase = OnyxiaPhase::Phase2;
        let bt = fsm.phase_bt().unwrap();
        let iface = TestInterface::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Rogue, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn no_bt_when_idle() {
        assert!(OnyxiaFsm::new().phase_bt().is_none());
    }
}
