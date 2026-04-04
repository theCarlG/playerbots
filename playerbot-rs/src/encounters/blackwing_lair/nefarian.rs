/// Nefarian encounter — Blackwing Lair final boss.
///
/// 3-phase fight with the iconic Class Call mechanic.
///
/// Phase 1 (100% → 22%): Ground phase with Drakonid adds.
///   Nefarian periodically calls out a class, forcing debilitating effects:
///   - Priest Call (23401): Priests are silenced — stop casting.
///   - Mage Call (23410): Mages polymorph each other — flee from group.
///   - Warlock Call (23419): Warlocks channel — stop casting.
///   - Warrior Call (23397): Warriors stuck in Berserker Stance.
///   - Druid Call (23398): Druids forced into Cat Form.
///   - Paladin Call (23418): Paladins unequip weapons.
///   - Shaman Call (23425): Shaman totems pulse.
///   - Hunter Call (23409): Pets become uncontrollable.
///   - Rogue Call (23414): Rogues forced to Eviscerate.
///
/// Phase 2 (< 22%): Nefarian takes flight briefly, zombie drakonids spawn.
/// Phase 3: Nefarian lands, fight continues until death.
///
/// Bot behavior per state:
///   - Ground: check for class call auras, react accordingly.
///   - Air/Final: normal rotation (no class calls during transition).
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, Sel, Seq, HasDebuff, HoldPosition, MoveAwayFromRaid};
use crate::ffi::SpellId;

// ── Class Call spell IDs (auras applied to affected class) ───────────────

pub const WARRIOR_CALL: SpellId = SpellId(23397);
pub const DRUID_CALL: SpellId = SpellId(23398);
pub const PRIEST_CALL: SpellId = SpellId(23401);
pub const PALADIN_CALL: SpellId = SpellId(23418);
pub const SHAMAN_CALL: SpellId = SpellId(23425);
pub const MAGE_CALL: SpellId = SpellId(23410);
pub const WARLOCK_CALL: SpellId = SpellId(23419);
pub const HUNTER_CALL: SpellId = SpellId(23409);
pub const ROGUE_CALL: SpellId = SpellId(23414);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NefPhase {
    Idle,
    Ground,
    Air,
    FinalGround,
}

#[derive(Clone, Debug)]
pub struct NefarianFsm {
    pub phase: NefPhase,
    done: bool,
    ground_bt: Bt,
}

impl PartialEq for NefarianFsm {
    fn eq(&self, other: &Self) -> bool {
        self.phase == other.phase && self.done == other.done
    }
}

impl NefarianFsm {
    pub fn new() -> Self {
        Self {
            phase: NefPhase::Idle,
            done: false,
            ground_bt: Sel(vec![
                // Priest Call: silenced — stop casting
                Seq(vec![HasDebuff(PRIEST_CALL), HoldPosition]),
                // Mage Call: polymorphing each other — flee from group
                Seq(vec![HasDebuff(MAGE_CALL), MoveAwayFromRaid(20.0)]),
                // Warlock Call: shadow bolts heal boss — stop casting
                Seq(vec![HasDebuff(WARLOCK_CALL), HoldPosition]),
                // Other class calls: handled by server/aura system, not bot logic.
            ]),
        }
    }
}

impl Default for NefarianFsm {
    fn default() -> Self {
        Self::new()
    }
}

impl EncounterFsm for NefarianFsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp: f32, _time: u64) {
        if self.done {
            return;
        }

        match event {
            EncounterEvent::CombatStarted => {
                self.phase = NefPhase::Ground;
            }
            EncounterEvent::UnitDied { .. } => {
                self.done = true;
            }
            EncounterEvent::GroupWipe => {
                self.phase = NefPhase::Idle;
            }
            EncounterEvent::None => {
                if self.phase == NefPhase::Ground && boss_hp < 0.22 {
                    self.phase = NefPhase::Air;
                } else if self.phase == NefPhase::Air && boss_hp < 0.20 {
                    self.phase = NefPhase::FinalGround;
                }
            }
            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        match self.phase {
            NefPhase::Idle => 0,
            NefPhase::Ground => 1,
            NefPhase::Air => 2,
            NefPhase::FinalGround => 3,
        }
    }

    fn is_active(&self) -> bool {
        self.phase != NefPhase::Idle
    }
    fn is_done(&self) -> bool {
        self.done
    }
    fn boss_entry(&self) -> u32 {
        super::ENTRY_NEFARIAN
    }

    fn phase_bt(&self) -> Option<&Bt> {
        match self.phase {
            NefPhase::Ground | NefPhase::FinalGround => Some(&self.ground_bt),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::state::PlayerClass;
    use crate::encounters::EncounterEvent;
    use crate::engine::bt_nodes::{BtNode, BtResult};
    use crate::engine::context::tests::{TestCtxOwned, TestInterface, make_encounter_ctx};
    use crate::ffi::BotRole;

    #[test]
    fn priest_call_holds_position() {
        let mut fsm = NefarianFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);

        let bt = fsm.phase_bt().expect("ground phase should have BT");
        let iface = TestInterface::new().with_aura(PRIEST_CALL);
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Priest, BotRole::HEAL);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn mage_call_flees() {
        let mut fsm = NefarianFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);

        let bt = fsm.phase_bt().expect("ground phase should have BT");
        let iface = TestInterface::new().with_aura(MAGE_CALL).with_safe_pos();
        let mut owned = TestCtxOwned::new();
        let mut ctx = make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Mage, BotRole::DPS);
        assert!(matches!(bt.tick(&mut ctx), BtResult::Running));
    }

    #[test]
    fn no_call_returns_failure() {
        let mut fsm = NefarianFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);

        let bt = fsm.phase_bt().expect("ground phase should have BT");
        let iface = TestInterface::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn no_bt_during_air_phase() {
        let mut fsm = NefarianFsm::new();
        fsm.phase = NefPhase::Air;
        assert!(fsm.phase_bt().is_none());
    }
}
