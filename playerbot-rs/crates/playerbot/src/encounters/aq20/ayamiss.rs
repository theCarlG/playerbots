/// Ayamiss the Hunter — Ruins of Ahn'Qiraj.
///
/// Phase 1 Ayamiss is airborne (out of melee reach) raining Stinger Spray and
/// Paralyze while adds stream in; at ~70% she lands for a normal melee phase.
/// The bot-actionable mechanic is the adds:
///   - **Hive'Zara Larva** (15555): crawls to the altar to sacrifice a captured
///     raid member — killing it frees them, so it is the top priority.
///   - **Hive'Zara Swarmer** (15546): the swarm; melee (who can't reach the
///     airborne boss) clear these.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, IsMeleeDps};
use crate::{Sel, Seq};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct AyamissFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for AyamissFsm {
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
        super::ENTRY_AYAMISS
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Sel!(
                // Larva sacrifices a captured player — everyone kills it first.
                Seq!(Bt::FocusNearestEntry(super::ENTRY_HIVE_ZARA_LARVA)),
                // Melee can't reach the airborne boss — clear swarmers.
                Seq!(
                    IsMeleeDps,
                    Bt::FocusNearestEntry(super::ENTRY_HIVE_ZARA_SWARMER),
                ),
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
    fn larva_focused_by_everyone() {
        const LARVA: u64 = 60;
        let mut fsm = AyamissFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        // A ranged caster still drops the larva (it isn't melee-gated).
        let iface = MockWorld::new().with_nearby_entry(LARVA, super::super::ENTRY_HIVE_ZARA_LARVA);
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        assert!(
            iface
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::Attack(h) if *h == LARVA))
        );
    }

    #[test]
    fn ranged_ignores_swarmers() {
        // Only swarmers up + a ranged bot → the melee-gated branch fails, so
        // ranged stay on the boss.
        const SWARMER: u64 = 61;
        let mut fsm = AyamissFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface =
            MockWorld::new().with_nearby_entry(SWARMER, super::super::ENTRY_HIVE_ZARA_SWARMER);
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Failure);
    }
}
