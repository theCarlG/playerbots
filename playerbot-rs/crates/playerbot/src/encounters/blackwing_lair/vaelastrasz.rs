/// Vaelastrasz the Corrupt — Blackwing Lair.
///
/// Single-phase burn with a hard 5-minute enrage (essence of the red).
/// Key mechanics:
///   - **Essence of the Red** (23513): +250 rage/energy, +50mp/s. Bots must
///     blow full cooldowns from start — this is *the* burst race boss.
///   - **Burning Adrenaline** (18173): 20s debuff that causes the victim
///     to explode on expiry, dealing 10k fire damage to everything in 8y.
///     Affected bots MUST run out of the raid before the debuff expires.
///   - **Flame Breath** (23461): frontal cone — tank faces boss away.
///
/// Strategy: everyone stacks the tank until debuffed, then affected bots
/// run to the corner. Burning Adrenaline on the tank = tank swap (handled
/// by reactive layer picking up a new MT target).
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, MoveAwayFromRaid};
use cmangos::SpellId;
use crate::{Sel, Seq};

pub const AURA_BURNING_ADRENALINE: SpellId = SpellId(18173);
pub const AURA_ESSENCE_OF_THE_RED: SpellId = SpellId(23513);
pub const SPELL_FLAME_BREATH: SpellId = SpellId(23461);

#[derive(Clone, Debug)]
pub struct VaelastraszFsm {
    active: bool,
    done: bool,
}

impl PartialEq for VaelastraszFsm {
    fn eq(&self, other: &Self) -> bool {
        self.active == other.active && self.done == other.done
    }
}

impl VaelastraszFsm {
    pub fn new() -> Self {
        Self {
            active: false,
            done: false,
        }
    }

    fn build_bt() -> Bt {
        // Burning Adrenaline victims must immediately MoveAwayFromRaid.
        // Non-debuffed bots stay on the boss (burn race).
        Sel!(Seq!(
            Bt::self_has(AURA_BURNING_ADRENALINE),
            MoveAwayFromRaid(25.0),
        ))
    }
}

impl Default for VaelastraszFsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for VaelastraszFsm {
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
        super::ENTRY_VAELASTRASZ
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Self::build_bt())
        } else {
            None
        }
    }
}
