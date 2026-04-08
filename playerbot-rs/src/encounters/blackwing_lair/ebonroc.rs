/// Ebonroc — Blackwing Lair drake triplet (2 of 3).
///
/// Same core mechanics as Firemaw (Shadow Flame cone, Wing Buffet,
/// Flame Buffet). **Distinct**: Ebonroc casts `Heal` (interruptible);
/// the raid must kick it on every attempt.
use super::super::{EncounterEvent, EncounterFsm};
use crate::engine::bt::Bt::{self, *};
use crate::ffi::SpellId;
use crate::{Sel, Seq};

pub const SPELL_SHADOW_FLAME: SpellId = SpellId(22539);
pub const SPELL_WING_BUFFET: SpellId = SpellId(23339);
pub const AURA_FLAME_BUFFET: SpellId = SpellId(23341);
pub const SPELL_HEAL: SpellId = SpellId(23954);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct EbonrocFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for EbonrocFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, _time: u64) {
        match event {
            EncounterEvent::CombatStarted => self.active = true,
            EncounterEvent::UnitDied { .. } if self.active => self.done = true,
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
        super::ENTRY_EBONROC
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Sel!(
                Seq!(IsMeleeDps, MoveBehind(5.0)),
                Seq!(IsRanged, MaintainRange(30.0)),
            ))
        } else {
            None
        }
    }
}
