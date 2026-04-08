/// Magmadar encounter — Molten Core.
///
/// Single-phase fight:
///   - Ranged/healers: stay > 30yd (Flame Buffet stacking debuff).
///   - Tanks: taunt after Panic fear breaks.
///   - Melee DPS: normal rotation.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, IsRanged, MaintainRange, IsTank, Taunt};
use crate::ffi::SpellId;
use crate::{Sel, Seq};

pub const SPELL_LAVA_BOMB: SpellId = SpellId(19411);
pub const AURA_PANIC: SpellId = SpellId(19408);
pub const AURA_FLAME_BUFFET: SpellId = SpellId(19634);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct MagmadarFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for MagmadarFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, _time: u64) {
        match event {
            EncounterEvent::CombatStarted => self.active = true,
            EncounterEvent::UnitDied { victim: _ } => self.done = true,
            EncounterEvent::GroupWipe => self.active = false,
            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        u32::from(self.active)
    }
    fn is_active(&self) -> bool {
        self.active
    }
    fn is_done(&self) -> bool {
        self.done
    }
    fn boss_entry(&self) -> u32 {
        super::ENTRY_MAGMADAR
    }

    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Sel!(
                Seq!(IsRanged, MaintainRange(30.0)),
                Seq!(IsTank, Taunt),
            ))
        } else {
            None
        }
    }
}
