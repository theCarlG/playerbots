/// Jin'do the Hexxer — Zul'Gurub.
///
/// Single-phase fight whose difficulty is entirely about add/totem control:
///   - **Powerful Healing Ward** (npc 14987): a totem that heals Jin'do for a
///     huge amount. If it lives the fight never ends — kill it first, above all.
///   - **Brain Wash Totem** (npc 15112): pulses fear/confusion on the raid —
///     kill it next.
///   - **Shade of Jin'do** (npc 14986): spirit adds that chain-grip a raid
///     member down to ~1 HP; killing the shade frees them.
///   - Powerful Hex turns its victim into a frog (can't act) — nothing for the
///     bot to do but wait it out, so it isn't scripted.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt;
use crate::{Sel, Seq};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct JindoFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for JindoFsm {
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
        super::ENTRY_JINDO
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            // Priority: Healing Ward (heals the boss) → Brain Wash Totem →
            // Shades. Each falls through (Failure) when none are present, so
            // when all adds are dead the bot resumes hitting Jin'do.
            Some(Sel!(
                Seq!(Bt::FocusNearestEntry(super::ENTRY_POWERFUL_HEALING_WARD)),
                Seq!(Bt::FocusNearestEntry(super::ENTRY_BRAIN_WASH_TOTEM)),
                Seq!(Bt::FocusNearestEntry(super::ENTRY_SHADE_OF_JINDO)),
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
    fn focuses_healing_ward_first() {
        const WARD: u64 = 50;
        let mut fsm = JindoFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface =
            MockWorld::new().with_nearby_entry(WARD, super::super::ENTRY_POWERFUL_HEALING_WARD);
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        assert!(
            iface
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::Attack(h) if *h == WARD)),
            "bot attacks the Healing Ward totem"
        );
    }

    #[test]
    fn no_adds_returns_failure() {
        let mut fsm = JindoFsm::default();
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
