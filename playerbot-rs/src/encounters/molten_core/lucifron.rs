/// Lucifron encounter — Molten Core.
///
/// Single-phase fight. Key mechanics:
///   - **Impending Doom** (18093): targeted nuke, dispellable curse.
///   - **Lucifron's Curse** (19703): -100% stat curse, dispellable.
///   - **Shadow Shock** (20603): 5y PBAOE — melee spread only if stacking.
/// Dispellers (priest/mage/druid) hammer dispels on the raid.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, *};
use crate::ffi::SpellId;

pub const AURA_IMPENDING_DOOM: SpellId = SpellId(18093);
pub const AURA_LUCIFRONS_CURSE: SpellId = SpellId(19703);
pub const AURA_SHADOW_SHOCK: SpellId = SpellId(20603);

#[derive(Clone, Debug)]
pub struct LucifronFsm {
    active: bool,
    done: bool,
    bt: Bt,
}

impl PartialEq for LucifronFsm {
    fn eq(&self, other: &Self) -> bool {
        self.active == other.active && self.done == other.done
    }
}

impl LucifronFsm {
    pub fn new() -> Self {
        Self {
            active: false,
            done: false,
            bt: Self::build_bt(),
        }
    }

    fn build_bt() -> Bt {
        // Dispellers run the default reactive dispel path — the encounter
        // override does not need to re-issue dispels. Ranged stay back to
        // avoid Shadow Shock; tanks hold threat normally.
        Sel(vec![
            // Ranged keep 15y from the boss to avoid Shadow Shock splash.
            Seq(vec![IsRanged, MaintainRange(15.0)]),
        ])
    }
}

impl Default for LucifronFsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for LucifronFsm {
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
        super::ENTRY_LUCIFRON
    }
    fn phase_bt(&self) -> Option<&Bt> {
        if self.active { Some(&self.bt) } else { None }
    }
}
