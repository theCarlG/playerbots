/// Shazzrah encounter — Molten Core.
///
/// Single-phase caster fight. Key mechanics:
///   - **Shazzrah's Curse** (19714): reduced magic resists on raid — cleanse.
///   - **Arcane Explosion** (19712): ~10y PBAOE — melee stay away when channel.
///   - **Counterspell**: Shazzrah silences casters — rotate casts.
///   - **Gate of Shazzrah** (23138): teleports to random raid member.
///     Everyone MUST spread out so a teleport doesn't chain-kill stacked bots.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, *};
use crate::ffi::SpellId;
use crate::{Sel, Seq};

pub const AURA_SHAZZRAH_CURSE: SpellId = SpellId(19714);
pub const SPELL_ARCANE_EXPLOSION: SpellId = SpellId(19712);
pub const SPELL_GATE_OF_SHAZZRAH: SpellId = SpellId(23138);

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ShazzrahFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for ShazzrahFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, _time: u64) {
        match event {
            EncounterEvent::CombatStarted => self.active = true,
            EncounterEvent::UnitDied { .. } => self.done = true,
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
        super::ENTRY_SHAZZRAH
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Sel!(
                Seq!(IsRanged, MaintainRange(30.0)),
                Seq!(IsMeleeDps, MaintainRange(5.0)),
            ))
        } else {
            None
        }
    }
}
