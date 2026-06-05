/// Baron Geddon encounter — Molten Core.
///
/// Single-phase fight with two "get out" mechanics. For BOTH, the only correct
/// play is to move away — a class immunity does NOT substitute:
///   - Ice Block / Divine Shield don't stop the Living Bomb explosion from
///     hitting the *rest* of the raid, and Ice Block actually roots the mage in
///     place so it can't clear out at all.
///   - Fire Ward only chips one Inferno tick while you still must leave the
///     escalating radius.
/// So the immunity special-casing is gone; every affected bot relocates.
///
/// 1. **Living Bomb** (aura 20475): the bombed bot detonates after ~8s, hitting
///    everyone nearby and knocking them up. The debuffed bot runs out of the
///    raid (40yd). Bots WITHOUT the debuff are unaffected — `self_has` checks
///    this bot only.
///
/// 2. **Inferno** (aura 19695 on Geddon): he channels an escalating `PBAoE`
///    ring. Everyone flees out of the radius (30yd).
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, FleeToSafe, MoveAwayFromRaid};
use cmangos::SpellId;
use crate::{Sel, Seq};

pub const AURA_LIVING_BOMB: SpellId = SpellId(20475);
pub const AURA_INFERNO: SpellId = SpellId(19695);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct BaronGeddonFsm {
    active: bool,
    done: bool,
}

impl BaronGeddonFsm {
    fn build_bt() -> Bt {
        Sel!(Self::living_bomb(), Self::inferno())
    }

    /// Living Bomb: the bombed bot runs clear of the raid so its detonation
    /// doesn't catch anyone else.
    fn living_bomb() -> Bt {
        Seq!(Bt::self_has(AURA_LIVING_BOMB), MoveAwayFromRaid(40.0))
    }

    /// Inferno: everyone flees out of the boss's escalating `PBAoE`.
    fn inferno() -> Bt {
        Seq!(Bt::target_has(AURA_INFERNO), FleeToSafe(30.0))
    }
}

impl EncounterFsm for BaronGeddonFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, _time: u64) {
        match event {
            EncounterEvent::CombatStarted => self.active = true,
            EncounterEvent::UnitDied { victim: _ } => self.done = true,
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
        super::ENTRY_BARON_GEDDON
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
    use crate::bot::state::PlayerClass;
    use crate::engine::bt_nodes::{BtNode, BtResult};
    use crate::engine::context::tests::{TestCtxOwned, make_encounter_ctx};
    use cmangos::MockWorld;
    use cmangos::BotRole;

    #[test]
    fn living_bomb_bombed_mage_moves_out() {
        // A bombed mage must clear the raid like everyone else — it does NOT
        // Ice Block (that would root it in place and still blow up the raid).
        let mut fsm = BaronGeddonFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new()
            .with_aura(AURA_LIVING_BOMB)
            .with_safe_pos();
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert!(matches!(bt.tick(&mut ctx), BtResult::Running));
    }

    #[test]
    fn inferno_mage_flees_not_fire_wards() {
        // Inferno: even a mage flees the radius rather than standing in it
        // behind a Fire Ward.
        let mut fsm = BaronGeddonFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new().with_aura(AURA_INFERNO).with_safe_pos();
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 100; // Baron Geddon
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert!(matches!(bt.tick(&mut ctx), BtResult::Running));
    }

    #[test]
    fn living_bomb_warrior_flees() {
        let mut fsm = BaronGeddonFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new()
            .with_aura(AURA_LIVING_BOMB)
            .with_safe_pos();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert!(matches!(bt.tick(&mut ctx), BtResult::Running));
    }

    #[test]
    fn no_mechanic_returns_failure() {
        let mut fsm = BaronGeddonFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn no_bt_when_idle() {
        assert!(
            BaronGeddonFsm::default()
                .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
                .is_none()
        );
    }
}
