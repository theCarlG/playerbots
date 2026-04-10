/// Baron Geddon encounter — Molten Core.
///
/// Single-phase fight with two critical mechanics:
///
/// 1. **Living Bomb** (aura 20475): The debuffed bot must move away from raid.
///    - Mages: Ice Block (immune to explosion + fall damage).
///    - Paladins: Divine Shield (immune to explosion).
///    - Others: run to edge of room (40yd from raid).
///    Bots WITHOUT the debuff are unaffected — `HasDebuff` checks this bot only.
///
/// 2. **Inferno** (aura 19695): Geddon channels `AoE` ring around himself.
///    - Mages: Fire Ward before fleeing (reduces fire damage).
///    - Others: flee 30 yards.
use super::super::{EncounterEvent, EncounterFsm};
use crate::bot::state::PlayerClass;
use crate::encounters::bt::Bt::{self, IsClass, CastOnSelf, MoveAwayFromRaid, FleeToSafe};
use cmangos::SpellId;
use crate::{Sel, Seq};

pub const AURA_LIVING_BOMB: SpellId = SpellId(20475);
pub const AURA_INFERNO: SpellId = SpellId(19695);

const ICE_BLOCK: SpellId = SpellId(11958);
const DIVINE_SHIELD: SpellId = SpellId(642);
const FIRE_WARD: SpellId = SpellId(543);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct BaronGeddonFsm {
    active: bool,
    done: bool,
}

impl BaronGeddonFsm {
    fn build_bt() -> Bt {
        Sel!(Self::living_bomb(), Self::inferno())
    }

    /// Living Bomb: only the affected bot reacts.
    fn living_bomb() -> Bt {
        Seq!(
            Bt::self_has(AURA_LIVING_BOMB),
            Sel!(
                Seq!(IsClass(PlayerClass::Mage), CastOnSelf(ICE_BLOCK)),
                Seq!(IsClass(PlayerClass::Paladin), CastOnSelf(DIVINE_SHIELD),),
                MoveAwayFromRaid(40.0),
            ),
        )
    }

    /// Inferno: mages Fire Ward first, everyone flee.
    fn inferno() -> Bt {
        Seq!(
            Bt::target_has(AURA_INFERNO),
            Sel!(
                Seq!(IsClass(PlayerClass::Mage), CastOnSelf(FIRE_WARD)),
                FleeToSafe(30.0),
            ),
        )
    }
}

impl EncounterFsm for BaronGeddonFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, _time: u64) {
        match event {
            EncounterEvent::CombatStarted => self.active = true,
            EncounterEvent::UnitDied { victim: _ } => self.done = true,
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
        super::ENTRY_BARON_GEDDON
    }

    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        if self.active {
            Some(Self::build_bt())
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
    use cmangos::MockWorld;
    use cmangos::BotRole;

    #[test]
    fn living_bomb_mage_ice_blocks() {
        let mut fsm = BaronGeddonFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new().with_aura(AURA_LIVING_BOMB);
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn living_bomb_warrior_flees() {
        let mut fsm = BaronGeddonFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new()
            .with_aura(AURA_LIVING_BOMB)
            .with_safe_pos();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert!(matches!(bt.tick(&mut ctx), BtResult::Running));
    }

    #[test]
    fn no_mechanic_returns_failure() {
        let mut fsm = BaronGeddonFsm::default();
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
            BaronGeddonFsm::default()
                .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
                .is_none()
        );
    }
}
