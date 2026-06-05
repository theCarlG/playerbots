/// Princess Huhuran — Temple of Ahn'Qiraj. A nature-resistance burn with a
/// **Frenzy** (26051) haste-enrage that hunters must Tranquilizing Shot off,
/// exactly like Magmadar / Chromaggus. (The Berserk at 30% is a hard enrage
/// that can't be dispelled — it's a DPS check, nothing to script.)
use super::super::{EncounterEvent, EncounterFsm};
use crate::bot::state::PlayerClass;
use crate::encounters::bt::Bt::{self, CastOnTarget, IsClass};
use cmangos::SpellId;
use crate::{Sel, Seq};

pub const AURA_FRENZY: SpellId = SpellId(26051);
const TRANQUILIZING_SHOT: SpellId = SpellId(19801);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct HuhuranFsm {
    active: bool,
    done: bool,
}

impl EncounterFsm for HuhuranFsm {
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
        super::ENTRY_HUHURAN
    }
    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Sel!(Seq!(
                IsClass(PlayerClass::Hunter),
                Bt::target_has(AURA_FRENZY),
                CastOnTarget(TRANQUILIZING_SHOT),
            )))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::bt_nodes::{BtNode, BtResult};
    use crate::engine::context::tests::{TestCtxOwned, make_encounter_ctx};
    use cmangos::BotRole;
    use cmangos::MockEvent;
    use cmangos::MockWorld;

    #[test]
    fn hunter_tranq_shots_frenzy() {
        let mut fsm = HuhuranFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new().with_aura(AURA_FRENZY);
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 100;
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Hunter, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
        assert!(iface.events().iter().any(|e| matches!(
            e,
            MockEvent::CastSpell { spell, .. } if *spell == TRANQUILIZING_SHOT
        )));
    }

    #[test]
    fn no_bt_when_idle() {
        assert!(
            HuhuranFsm::default()
                .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
                .is_none()
        );
    }
}
