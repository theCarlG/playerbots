/// Golemagg the Incinerator encounter — Molten Core.
///
/// Single-phase fight. Key mechanics:
///   - **2 Core Rager adds** — must NOT be killed; they respawn at full
///     HP. OTs hold them away from raid. DPS focuses Golemagg.
///   - **Magma Splash** (13880): stacking debuff on melee — tanks need
///     FR gear and cooldowns.
///   - **Pyroblast** (20228): targeted, unavoidable.
///   - At 10% HP Golemagg enrages — burn phase.
/// Ranged DPS can safely sit at max range. Melee stay on Golemagg.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, *};
use crate::ffi::SpellId;
use crate::{Sel, Seq};

pub const SPELL_MAGMA_SPLASH: SpellId = SpellId(13880);
pub const SPELL_PYROBLAST: SpellId = SpellId(20228);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct GolemaggFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for GolemaggFsm {
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
        super::ENTRY_GOLEMAGG
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            // Ranged stay at max range to avoid Magma Splash stacking.
            Some(Sel!(Seq!(IsRanged, MaintainRange(30.0))))
        } else {
            None
        }
    }
}
