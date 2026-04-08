/// Firemaw — Blackwing Lair drake triplet (1 of 3).
///
/// Key mechanics:
///   - **Shadow Flame** frontal cone — melee stay behind, ranged stay out.
///   - **Wing Buffet** knockback — tank wall-hug (positional, not a BT).
///   - **Flame Buffet** stacking tank-swap debuff — future tank-swap hook.
///
/// The three drakes (Firemaw, Ebonroc, Flamegor) share this core
/// mechanic set; each gets its own FSM so follow-up work can attach
/// per-drake interrupts/dispels (Ebonroc Heal, Flamegor Frenzy).
use super::super::{EncounterEvent, EncounterFsm};
use crate::engine::bt::Bt::{self, *};
use crate::ffi::SpellId;
use crate::{Sel, Seq};

pub const SPELL_SHADOW_FLAME: SpellId = SpellId(22539);
pub const SPELL_WING_BUFFET: SpellId = SpellId(23339);
pub const AURA_FLAME_BUFFET: SpellId = SpellId(23341);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct FiremawFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for FiremawFsm {
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
        super::ENTRY_FIREMAW
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            // Melee stay behind the drake (out of Shadow Flame cone);
            // ranged maintain max range to stay outside both the cone and
            // Wing Buffet knockback arc.
            Some(Sel!(
                Seq!(IsMeleeDps, MoveBehind(5.0)),
                Seq!(IsRanged, MaintainRange(30.0)),
            ))
        } else {
            None
        }
    }
}
