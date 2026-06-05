/// C'thun — Temple of Ahn'Qiraj final boss. The whole fight is tentacle
/// control, in both phases:
///   - **Flesh Tentacle** (npc 15802): inside the stomach (Phase 2); killing
///     both weakens C'thun — top priority for swallowed players.
///   - **Eye / Claw Tentacle** (15726 / 15725): the small adds that spawn
///     constantly and must be cleared.
///   - **Giant Eye / Giant Claw Tentacle** (15334 / 15728): the big ones.
/// Dodging the Eye Beam / Dark Glare sweep is a movement-coordination problem
/// not scripted per-bot; this just keeps DPS on the tentacles in priority order.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt;
use crate::{Sel, Seq};

#[derive(Clone, Debug, PartialEq)]
pub struct CthunFsm {
    entry: u32,
    active: bool,
    done: bool,
}

impl CthunFsm {
    pub fn new(entry: u32) -> Self {
        Self {
            entry,
            active: false,
            done: false,
        }
    }
}

impl EncounterFsm for CthunFsm {
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
        self.entry
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Sel!(
                // Stomach first (a swallowed player ending the phase), then the
                // small tentacles, then the giants. Each falls through when
                // absent, so when none are up DPS lands on C'thun / the Eye.
                Seq!(Bt::FocusNearestEntry(super::ENTRY_FLESH_TENTACLE)),
                Seq!(Bt::FocusNearestEntry(super::ENTRY_EYE_TENTACLE)),
                Seq!(Bt::FocusNearestEntry(super::ENTRY_CLAW_TENTACLE)),
                Seq!(Bt::FocusNearestEntry(super::ENTRY_GIANT_EYE_TENTACLE)),
                Seq!(Bt::FocusNearestEntry(super::ENTRY_GIANT_CLAW_TENTACLE)),
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
    fn focuses_flesh_tentacle_first() {
        const FLESH: u64 = 80;
        const EYE: u64 = 81;
        let mut fsm = CthunFsm::new(super::super::ENTRY_CTHUN);
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new()
            .with_nearby_entry(EYE, super::super::ENTRY_EYE_TENTACLE)
            .with_nearby_entry(FLESH, super::super::ENTRY_FLESH_TENTACLE);
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        assert!(
            iface
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::Attack(h) if *h == FLESH)),
            "the stomach Flesh Tentacle is the top priority"
        );
    }

    #[test]
    fn no_tentacles_returns_failure() {
        let mut fsm = CthunFsm::new(super::super::ENTRY_CTHUN);
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Failure);
    }
}
