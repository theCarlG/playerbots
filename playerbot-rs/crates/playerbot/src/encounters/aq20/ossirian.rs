/// Ossirian the Unscarred — Ruins of Ahn'Qiraj.
///
/// Ossirian is immune to most damage until a **weakness crystal** is activated:
/// clicking a crystal applies a "Weakness" debuff that drops his armor/immunity
/// for a while, and he periodically re-shields ("Supreme Mode") until the next
/// crystal is used. The crystals (`GameObject` 180619) spawn around the room.
///
/// Bot behavior: whoever is standing near a crystal clicks it (10y, so the
/// whole raid doesn't abandon the boss to chase one), keeping the weakness up.
/// This mirrors the BWL Suppression-Device disarm pattern. The roaming Sand
/// Storm tornadoes are a ground hazard with no per-bot signal — left to the
/// server / human awareness.
use super::super::{EncounterEvent, EncounterFsm};
use crate::engine::bt::{BehaviorLeaf, Bt};
use crate::engine::bt_nodes::BtResult;
use crate::engine::context::TickContext;
use crate::{Sel, Seq};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct OssirianFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for OssirianFsm {
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
        super::ENTRY_OSSIRIAN
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Sel!(Seq!(
                Bt::InCombat,
                Bt::throttle(2_000, Bt::Custom(ACTIVATE_CRYSTAL)),
            )))
        } else {
            None
        }
    }
}

/// Click the nearest weakness crystal if one is within reach (10y). Falls
/// through (Failure) when none is close, so bots away from a crystal keep
/// fighting rather than running across the room.
const ACTIVATE_CRYSTAL: BehaviorLeaf = BehaviorLeaf {
    label: "ossirian_activate_crystal",
    handler: |ctx: &mut TickContext<'_>| -> BtResult {
        if ctx.timers.gcd_active(ctx.server_time_ms) {
            return BtResult::Failure;
        }
        match ctx
            .interface
            .nearby_gameobject_by_entry(super::GO_OSSIRIAN_CRYSTAL, 10.0)
        {
            Some(h) if ctx.interface.use_gameobject(h) => BtResult::Success,
            _ => BtResult::Failure,
        }
    },
    display_text: Some("Activating weakness crystal"),
};

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
    fn clicks_nearby_crystal() {
        const CRYSTAL: u64 = 90;
        let mut fsm = OssirianFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface =
            MockWorld::new().with_nearby_gameobject(super::super::GO_OSSIRIAN_CRYSTAL, CRYSTAL);
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.in_combat = true;
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        assert!(
            iface
                .events()
                .iter()
                .any(|e| matches!(e, MockEvent::UseGameObject(h) if *h == CRYSTAL)),
            "bot clicks the weakness crystal"
        );
    }

    #[test]
    fn no_crystal_returns_failure() {
        let mut fsm = OssirianFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new();
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.in_combat = true;
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Failure);
    }
}
