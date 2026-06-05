/// Gothik the Harvester — Naxxramas, Military Wing. Phase 1 splits the raid by
/// a gate: the live side fights **Unrelenting** adds, the dead side their
/// **Spectral** echoes. The split itself is a raid-coordination call, but the
/// per-bot job is the same either side — kill the adds near you. Death Knights
/// are the dangerous ones, so they're focused ahead of riders and trainees.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt;
use crate::{Sel, Seq};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct GothikFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for GothikFsm {
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
        super::ENTRY_GOTHIK
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            // Death Knights first, then riders, then trainees — on whichever
            // side the bot is (only that side's adds are in range).
            Some(Sel!(
                Seq!(Bt::FocusNearestEntry(super::ENTRY_UNRELENTING_DEATHKNIGHT)),
                Seq!(Bt::FocusNearestEntry(super::ENTRY_SPECTRAL_DEATHKNIGHT)),
                Seq!(Bt::FocusNearestEntry(super::ENTRY_UNRELENTING_RIDER)),
                Seq!(Bt::FocusNearestEntry(super::ENTRY_SPECTRAL_RIDER)),
                Seq!(Bt::FocusNearestEntry(super::ENTRY_UNRELENTING_TRAINEE)),
                Seq!(Bt::FocusNearestEntry(super::ENTRY_SPECTRAL_TRAINEE)),
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
    fn focuses_deathknight_first() {
        const DK: u64 = 95;
        const TRAINEE: u64 = 96;
        let mut fsm = GothikFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new()
            .with_nearby_entry(TRAINEE, super::super::ENTRY_UNRELENTING_TRAINEE)
            .with_nearby_entry(DK, super::super::ENTRY_UNRELENTING_DEATHKNIGHT);
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        assert!(
            iface
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::Attack(h) if *h == DK)),
            "the Death Knight is focused before the trainee"
        );
    }

    #[test]
    fn no_adds_returns_failure() {
        let mut fsm = GothikFsm::default();
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
