/// Sulfuron Harbinger encounter — Molten Core.
///
/// Single-phase fight. Key mechanics:
///   - **4 Flamewaker Priest adds** — must be killed first; they heal
///     Sulfuron via Dark Mending (19775). Interrupt priority.
///   - **Inspire** (19779): +25% phys dmg buff on adds — kill adds fast.
///   - **Hand of Ragnaros** (19780): knockdown on melee, stay spread.
/// The encounter is largely add-management. Once adds die, Sulfuron
/// is straightforward tank-and-spank.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, TargetCastingInterruptible, Interrupt, IsRanged, MaintainRange};
use cmangos::SpellId;
use crate::{Sel, Seq};

pub const SPELL_DARK_MENDING: SpellId = SpellId(19775);
pub const SPELL_HAND_OF_RAGNAROS: SpellId = SpellId(19780);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct SulfuronFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for SulfuronFsm {
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
        super::ENTRY_SULFURON
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            // Interrupt Dark Mending on adds; ranged stay at range.
            Some(Sel!(
                Bt::throttle(500, Seq!(TargetCastingInterruptible, Interrupt)),
                Seq!(IsRanged, MaintainRange(10.0)),
            ))
        } else {
            None
        }
    }
}
