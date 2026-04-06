/// Gehennas encounter — Molten Core.
///
/// Single-phase fight. Key mechanics:
///   - **Gehennas' Curse** (19716): -75% healing curse, must be decursed.
///   - **Rain of Fire** (19717): 15y AoE — ranged stay spread.
///   - **Shadow Bolt** (19728): random target, unavoidable.
/// Two Flamewaker adds should be killed first (handled by kill-order/RTI).
/// Dispellers (mage/druid) priority-decurse the raid.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, *};
use crate::ffi::SpellId;
use crate::{Sel, Seq};

pub const SPELL_GEHENNAS_CURSE: SpellId = SpellId(19716);
pub const SPELL_RAIN_OF_FIRE: SpellId = SpellId(19717);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct GehennasFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for GehennasFsm {
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
        super::ENTRY_GEHENNAS
    }
    fn phase_bt(&self) -> Option<Bt> {
        if self.active {
            // Ranged spread to avoid Rain of Fire splash; melee normal.
            Some(Sel!(Seq!(IsRanged, MaintainRange(15.0))))
        } else {
            None
        }
    }
}
