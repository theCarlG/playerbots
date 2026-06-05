/// Bloodlord Mandokir — Zul'Gurub.
///
/// Single-phase fight. Signature mechanic:
///   - **Threatening Gaze** (24314): Mandokir locks onto a random player. If
///     that player moves, attacks, or casts while gazed, he Charges them for a
///     near-lethal hit and levels up (permanent buff). The gazed bot must do
///     NOTHING — `FreezeActions` stops movement + auto-attack and, by returning
///     Success, suppresses the rotation so no cast leaks out.
///   - Charge / Whirlwind / Mortal Strike / Cleave are handled by the server
///     and the reactive tank-swap layer.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, FreezeActions};
use cmangos::SpellId;
use crate::{Sel, Seq};

pub const AURA_THREATENING_GAZE: SpellId = SpellId(24314);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct MandokirFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for MandokirFsm {
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
        super::ENTRY_MANDOKIR
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            // Gazed → freeze. Otherwise fall through to the normal rotation.
            Some(Sel!(Seq!(
                Bt::self_has(AURA_THREATENING_GAZE),
                FreezeActions,
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
    fn gazed_bot_freezes() {
        let mut fsm = MandokirFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new().with_aura(AURA_THREATENING_GAZE);
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        let events = iface.events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MockEvent::AutoAttack(false))),
            "gazed bot stops auto-attacking"
        );
        assert!(
            events.iter().any(|e| matches!(e, MockEvent::StopMoving)),
            "gazed bot stops moving"
        );
    }

    #[test]
    fn no_gaze_returns_failure() {
        let mut fsm = MandokirFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn no_bt_when_idle() {
        assert!(
            MandokirFsm::default()
                .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
                .is_none()
        );
    }
}
