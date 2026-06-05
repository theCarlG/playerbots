/// High Priestess Mar'li — Zul'Gurub.
///
/// Transforms into spider form around 50%. The bot-actionable mechanic is the
/// adds:
///   - **Spawn of Mar'li** (npc 15041): hatch from eggs around the room and
///     swarm the raid; they hit hard for their size and must be cleared fast.
///   - Poison Volley / Drain Life / Web are server-driven (Web roots the
///     victim — nothing to script while held).
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt;
use crate::{Sel, Seq};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct MarliFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for MarliFsm {
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
        super::ENTRY_MARLI
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            // Clear the spawns first; fall through to the boss when none remain.
            Some(Sel!(Seq!(Bt::FocusNearestEntry(super::ENTRY_SPAWN_OF_MARLI))))
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
    fn focuses_spawn() {
        const SPAWN: u64 = 60;
        let mut fsm = MarliFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new().with_nearby_entry(SPAWN, super::super::ENTRY_SPAWN_OF_MARLI);
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        assert!(
            iface
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::Attack(h) if *h == SPAWN))
        );
    }

    #[test]
    fn no_spawn_returns_failure() {
        let mut fsm = MarliFsm::default();
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
