/// Chromaggus — Blackwing Lair.
///
/// Single-phase fight, but with a *random* pair of breaths per pull.
/// Key mechanics:
///   - **Brood Affliction** (23170-23174): five rotating curses — dispellers
///     keep the raid clean or Chromaggus transforms into Chromatic on stacks.
///   - **Incinerate / Time Lapse / Corrosive Acid / Ignite Flesh / Frost Burn**
///     (23308, 23310, 23313, 23315, 23316): two of the five breaths per pull.
///     All are frontal cones — tank faces boss away from raid.
///   - **Frenzy** (28371): haste buff on low HP — purge.
///
/// Strategy: everyone stays behind the boss; ranged maintain max range to
/// stay out of breath cones entirely in case of tank position slips.
use super::super::{EncounterEvent, EncounterFsm};
use crate::bot::state::PlayerClass;
use crate::encounters::bt::Bt::{self, CastOnTarget, IsClass, IsRanged, MaintainRange};
use cmangos::SpellId;
use crate::{Sel, Seq};

pub const AURA_BROOD_AFFLICTION_BLUE: SpellId = SpellId(23170);
pub const AURA_BROOD_AFFLICTION_BLACK: SpellId = SpellId(23171);
pub const AURA_BROOD_AFFLICTION_RED: SpellId = SpellId(23172);
pub const AURA_BROOD_AFFLICTION_BRONZE: SpellId = SpellId(23173);
pub const AURA_BROOD_AFFLICTION_GREEN: SpellId = SpellId(23174);
pub const AURA_FRENZY: SpellId = SpellId(28371);

const TRANQUILIZING_SHOT: SpellId = SpellId(19801);

#[derive(Clone, Debug)]
pub struct ChromaggusFsm {
    active: bool,
    done: bool,
}

impl PartialEq for ChromaggusFsm {
    fn eq(&self, other: &Self) -> bool {
        self.active == other.active && self.done == other.done
    }
}

impl ChromaggusFsm {
    pub fn new() -> Self {
        Self {
            active: false,
            done: false,
        }
    }

    fn build_bt() -> Bt {
        Sel!(
            // Hunters Tranquilizing Shot the Frenzy haste-enrage off the boss
            // (otherwise its melee output spikes and the tank can't hold).
            Seq!(
                IsClass(PlayerClass::Hunter),
                Bt::target_has(AURA_FRENZY),
                CastOnTarget(TRANQUILIZING_SHOT),
            ),
            // Ranged stay at 30y to stay out of all breath cones. Melee hug
            // the boss from behind — reactive facing handled by targeting.
            Seq!(IsRanged, MaintainRange(30.0)),
        )
    }
}

impl Default for ChromaggusFsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for ChromaggusFsm {
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
        super::ENTRY_CHROMAGGUS
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Self::build_bt())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encounters::EncounterEvent;
    use crate::engine::bt_nodes::{BtNode, BtResult};
    use crate::engine::context::tests::{TestCtxOwned, make_encounter_ctx};
    use cmangos::BotRole;
    use cmangos::MockEvent;
    use cmangos::MockWorld;

    #[test]
    fn hunter_tranq_shots_frenzy() {
        let mut fsm = ChromaggusFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new().with_aura(AURA_FRENZY);
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 100; // Chromaggus
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Hunter, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        assert!(
            iface.events().iter().any(|e| matches!(
                e,
                MockEvent::CastSpell { spell, .. } if *spell == TRANQUILIZING_SHOT
            )),
            "hunter Tranquilizing Shots the boss's Frenzy"
        );
    }

    #[test]
    fn no_bt_when_idle() {
        assert!(
            ChromaggusFsm::default()
                .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
                .is_none()
        );
    }
}
