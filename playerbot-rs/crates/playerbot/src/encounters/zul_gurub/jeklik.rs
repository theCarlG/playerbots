/// High Priestess Jeklik — Zul'Gurub.
///
/// Two-stance fight. The bot-actionable part is the bat phase:
///   - Jeklik takes flight (out of melee reach) and summons **Frenzied
///     Bloodseeker Bat** (npc 14965) adds that dive the raid. While she is
///     airborne, melee can't touch her, so they clear the bats instead;
///     ranged keep shooting the boss.
///   - Charge / Sonic Burst / Bloodleech / her Great Heal are handled by the
///     server and the reactive interrupt layer (the heal is interruptible and
///     the boss is the current target).
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, IsMeleeDps};
use crate::{Sel, Seq};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct JeklikFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for JeklikFsm {
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
        super::ENTRY_JEKLIK
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            // Melee kill bats when present (boss airborne / unreachable).
            // Falls through when no bats are up so melee hit Jeklik normally.
            Some(Sel!(Seq!(
                IsMeleeDps,
                Bt::FocusNearestEntry(super::ENTRY_BLOODSEEKER_BAT),
            )))
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
    fn melee_clears_bats() {
        const BAT: u64 = 80;
        let mut fsm = JeklikFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new().with_nearby_entry(BAT, super::super::ENTRY_BLOODSEEKER_BAT);
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Rogue, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        assert!(
            iface
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::Attack(h) if *h == BAT))
        );
    }

    #[test]
    fn ranged_ignores_bats() {
        // Ranged keep DPSing the boss — the melee-only bat branch fails for them.
        const BAT: u64 = 80;
        let mut fsm = JeklikFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new().with_nearby_entry(BAT, super::super::ENTRY_BLOODSEEKER_BAT);
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Failure);
    }
}
