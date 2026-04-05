/// Garr encounter — Molten Core.
///
/// Garr + 8 Firesworn adds. Key mechanics:
///   - **Antimagic Pulse** (19492): Garr dispels a magic buff from raid.
///   - **Magma Shackles** (19496): slow debuff on melee (keep casting/healing).
///   - **Eruption** (19497): each Firesworn explodes at ~10% HP — 25y fire burst.
///     Melee must back off when a Firesworn is low or get one-shot.
///   - Firesworn frenzy (19516) on ally death — +60% haste, purge it.
///
/// Strategy: everyone stays spread (Eruption `AoE`). Melee pull out when a
/// Firesworn crosses ~15% to avoid Eruption. Mages/shamans purge frenzy.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, *};
use crate::{Seq, Sel};
use crate::ffi::SpellId;

pub const AURA_ANTIMAGIC_PULSE: SpellId = SpellId(19492);
pub const AURA_MAGMA_SHACKLES: SpellId = SpellId(19496);
pub const SPELL_ERUPTION: SpellId = SpellId(19497);
pub const AURA_FIRESWORN_FRENZY: SpellId = SpellId(19516);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct GarrFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for GarrFsm {
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
        super::ENTRY_GARR
    }
    fn phase_bt(&self) -> Option<Bt> {
        if self.active {
            Some(
                // Ranged stay back at 20y from the boss (out of most Eruption chains).
                // Melee rely on the reactive flee layer to dodge Eruption bursts.
                Sel!(Seq!(IsRanged, MaintainRange(20.0))),
            )
        } else {
            None
        }
    }
}
