/// High Priestess Arlokk — Zul'Gurub.
///
/// Periodically Vanishes (untargetable) and summons panthers. The bot-
/// actionable mechanic is clearing the adds:
///   - **Zulian Prowler** (npc 15101): panthers that spawn from the cages and
///     maul the raid; kill them, especially while Arlokk is vanished and there
///     is nothing else to hit.
///   - Gouge / Whirlwind / Mark of Arlokk are server-driven.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt;
use crate::{Sel, Seq};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ArlokkFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for ArlokkFsm {
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
        super::ENTRY_ARLOKK
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Sel!(Seq!(Bt::FocusNearestEntry(
                super::ENTRY_ZULIAN_PROWLER
            ))))
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
    fn focuses_prowler() {
        const CAT: u64 = 70;
        let mut fsm = ArlokkFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new().with_nearby_entry(CAT, super::super::ENTRY_ZULIAN_PROWLER);
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        assert!(
            iface
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::Attack(h) if *h == CAT))
        );
    }

    #[test]
    fn no_adds_returns_failure() {
        let mut fsm = ArlokkFsm::default();
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
