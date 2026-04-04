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
use crate::encounters::bt::Bt::{self, Sel, Seq, HasDebuff, IsClass, CastOnSelf, MoveAwayFromRaid, TargetHasAura, FleeToSafe};
use crate::ffi::SpellId;

pub const AURA_LIVING_BOMB: SpellId = SpellId(20475);
pub const AURA_INFERNO: SpellId = SpellId(19695);

const ICE_BLOCK: SpellId = SpellId(11958);
const DIVINE_SHIELD: SpellId = SpellId(642);
const FIRE_WARD: SpellId = SpellId(543);

#[derive(Clone, Debug)]
pub struct BaronGeddonFsm {
    active: bool,
    done: bool,
    bt: Bt,
}

impl PartialEq for BaronGeddonFsm {
    fn eq(&self, other: &Self) -> bool {
        self.active == other.active && self.done == other.done
    }
}

impl BaronGeddonFsm {
    pub fn new() -> Self {
        Self {
            active: false,
            done: false,
            bt: Self::build_bt(),
        }
    }

    fn build_bt() -> Bt {
        Sel(vec![Self::living_bomb(), Self::inferno()])
    }

    /// Living Bomb: only the affected bot reacts.
    fn living_bomb() -> Bt {
        Seq(vec![
            HasDebuff(AURA_LIVING_BOMB),
            Sel(vec![
                Seq(vec![IsClass(PlayerClass::Mage), CastOnSelf(ICE_BLOCK)]),
                Seq(vec![
                    IsClass(PlayerClass::Paladin),
                    CastOnSelf(DIVINE_SHIELD),
                ]),
                MoveAwayFromRaid(40.0),
            ]),
        ])
    }

    /// Inferno: mages Fire Ward first, everyone flee.
    fn inferno() -> Bt {
        Seq(vec![
            TargetHasAura(AURA_INFERNO),
            Sel(vec![
                Seq(vec![IsClass(PlayerClass::Mage), CastOnSelf(FIRE_WARD)]),
                FleeToSafe(30.0),
            ]),
        ])
    }
}

impl Default for BaronGeddonFsm {
    fn default() -> Self {
        Self::new()
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

    fn phase_bt(&self) -> Option<&Bt> {
        if self.active { Some(&self.bt) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::bt_nodes::{BtNode, BtResult};
    use crate::engine::context::tests::{TestCtxOwned, TestInterface, make_encounter_ctx};
    use crate::ffi::BotRole;

    #[test]
    fn living_bomb_mage_ice_blocks() {
        let mut fsm = BaronGeddonFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm.phase_bt().unwrap();
        let iface = TestInterface::new().with_aura(AURA_LIVING_BOMB);
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn living_bomb_warrior_flees() {
        let mut fsm = BaronGeddonFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm.phase_bt().unwrap();
        let iface = TestInterface::new()
            .with_aura(AURA_LIVING_BOMB)
            .with_safe_pos();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert!(matches!(bt.tick(&mut ctx), BtResult::Running));
    }

    #[test]
    fn no_mechanic_returns_failure() {
        let mut fsm = BaronGeddonFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm.phase_bt().unwrap();
        let iface = TestInterface::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn no_bt_when_idle() {
        assert!(BaronGeddonFsm::new().phase_bt().is_none());
    }
}
