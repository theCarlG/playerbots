/// Majordomo Executus encounter — Molten Core.
///
/// Add fight — Majordomo himself is not attacked. Kill his 8 minions:
///   - 4 Flamewaker Healers (entry 11663) — kill FIRST, interrupt heals.
///   - 4 Flamewaker Elites (entry 11664) — kill after healers.
/// Majordomo submits when all 8 adds die.
///
/// Key mechanics:
///   - **Teleport** (20618): random teleport to Majordomo, threat reset.
///   - **Magic Reflection** (20619): 10s shield on 2 adds — stop casting.
///   - **Damage Reflection** (21075): 10s shield on 2 adds — stop melee.
///   - Adds should be tanked spread to avoid `AoE` chaining.
/// Interrupt priority on healers; DPS assists RTI marks.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, TargetCastingInterruptible, Interrupt, IsRanged, MaintainRange};
use crate::ffi::SpellId;
use crate::{Sel, Seq};

pub const SPELL_MAGIC_REFLECTION: SpellId = SpellId(20619);
pub const SPELL_DAMAGE_REFLECTION: SpellId = SpellId(21075);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct MajordomoFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for MajordomoFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, _time: u64) {
        match event {
            EncounterEvent::CombatStarted => self.active = true,
            EncounterEvent::UnitDied { .. } if self.active => {
                // Majordomo submits after all 8 adds die. The coordinator
                // tracks add deaths separately; we treat any UnitDied as
                // potential completion (the coordinator will confirm).
            }
            EncounterEvent::GroupWipe => {
                self.active = false;
                self.done = true;
            }
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
        super::ENTRY_MAJORDOMO
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            // Interrupt healer adds; spread to avoid AoE chaining.
            Some(Sel!(
                Bt::throttle(500, Seq!(TargetCastingInterruptible, Interrupt)),
                Seq!(IsRanged, MaintainRange(15.0)),
            ))
        } else {
            None
        }
    }
}
