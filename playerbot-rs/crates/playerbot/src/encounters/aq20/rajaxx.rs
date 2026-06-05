/// General Rajaxx — Ruins of Ahn'Qiraj.
///
/// The fight is seven waves of Qiraji adds (each led by a Captain) before
/// Rajaxx himself engages. The bot-actionable mechanic is the adds: the raid
/// has to chew through each wave, and the caster adds in particular need
/// killing fast.
///   - **Swarmguard Needler** (15344): ranged casters — top priority.
///   - **Qiraji Warrior** (15387) / **Qiraji Lasher** (15249): melee swarm.
/// When no adds remain (between waves / on Rajaxx himself) the focus falls
/// through to the bot's normal target. The wave Captains and Rajaxx's
/// Thunderclap / Disarm are left to normal targeting and the server.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt;
use crate::{Sel, Seq};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct RajaxxFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for RajaxxFsm {
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
        super::ENTRY_RAJAXX
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Sel!(
                Seq!(Bt::FocusNearestEntry(super::ENTRY_SWARMGUARD_NEEDLER)),
                Seq!(Bt::FocusNearestEntry(super::ENTRY_QIRAJI_WARRIOR)),
                Seq!(Bt::FocusNearestEntry(super::ENTRY_QIRAJI_LASHER)),
            ))
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
    use cmangos::BotRole;
    use cmangos::MockEvent;
    use cmangos::MockWorld;

    #[test]
    fn focuses_needler_over_warrior() {
        const NEEDLER: u64 = 30;
        const WARRIOR: u64 = 31;
        let mut fsm = RajaxxFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new()
            .with_nearby_entry(WARRIOR, super::super::ENTRY_QIRAJI_WARRIOR)
            .with_nearby_entry(NEEDLER, super::super::ENTRY_SWARMGUARD_NEEDLER);
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        assert!(
            iface
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::Attack(h) if *h == NEEDLER)),
            "the caster Needler is focused before the melee Warrior"
        );
    }

    #[test]
    fn no_adds_returns_failure() {
        let mut fsm = RajaxxFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Failure);
    }
}
