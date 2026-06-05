/// Anub'Rekhan — Naxxramas, Spider Wing.
///
/// Two bot-actionable mechanics:
///   - **Locust Swarm** (28785): a self-buff that pulses a heavy melee-range
///     `PBAoE`. While Anub has it, melee back out of range until it falls off.
///   - **Corpse Scarab** (npc 16698): scarabs that erupt from corpses (and from
///     the Crypt Guards) and swarm the raid — clear them.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, IsMeleeDps, MaintainRange};
use cmangos::SpellId;
use crate::{Sel, Seq};

pub const AURA_LOCUST_SWARM: SpellId = SpellId(28785);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct AnubRekhanFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for AnubRekhanFsm {
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
        super::ENTRY_ANUB_REKHAN
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Sel!(
                // Locust Swarm up → melee get out of the PBAoE.
                Seq!(
                    IsMeleeDps,
                    Bt::target_has(AURA_LOCUST_SWARM),
                    MaintainRange(15.0)
                ),
                // Otherwise clear the scarab swarm.
                Seq!(Bt::FocusNearestEntry(super::ENTRY_CORPSE_SCARAB)),
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
    fn focuses_corpse_scarab() {
        const SCARAB: u64 = 90;
        let mut fsm = AnubRekhanFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new().with_nearby_entry(SCARAB, super::super::ENTRY_CORPSE_SCARAB);
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        assert!(
            iface
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::Attack(h) if *h == SCARAB))
        );
    }

    #[test]
    fn no_adds_returns_failure() {
        let mut fsm = AnubRekhanFsm::default();
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
