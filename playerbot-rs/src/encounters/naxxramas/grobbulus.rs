/// Grobbulus encounter — Naxxramas, Construct Wing.
///
/// Single-phase fight with one critical mechanic:
/// **Mutating Injection** (aura 28169): Applied to a random player.
/// The afflicted player MUST immediately run 20+ yards from the raid.
/// After ~10s the injection explodes, dealing `AoE` damage.
///
/// Additionally: Slime Spray (28158) frontal cone — tank keeps boss turned away.
///
/// Bot behavior (via `phase_bt`):
///   - Mutating Injection on self → flee 20yd from raid.
///   - Otherwise: normal rotation.
use super::super::{EncounterEvent, EncounterFsm};
use crate::Seq;
use crate::engine::bt::Bt::{self, *};
use crate::ffi::SpellId;

pub const AURA_MUTATING_INJECTION: SpellId = SpellId(28169);
pub const SPELL_SLIME_SPRAY: SpellId = SpellId(28158);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct GrobbolusFsm {
    active: bool,
    done: bool,
}

impl GrobbolusFsm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl EncounterFsm for GrobbolusFsm {
    fn update(&mut self, event: &EncounterEvent, _boss_hp: f32, _time: u64) {
        match event {
            EncounterEvent::CombatStarted => self.active = true,
            EncounterEvent::UnitDied { victim: _ } => self.done = true,
            EncounterEvent::GroupWipe => {
                self.active = false;
            }
            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        if self.active { 1 } else { 0 }
    }
    fn is_active(&self) -> bool {
        self.active
    }
    fn is_done(&self) -> bool {
        self.done
    }
    fn boss_entry(&self) -> u32 {
        super::ENTRY_GROBBULUS
    }

    fn phase_bt(&self) -> Option<Bt> {
        if self.active {
            Some(Seq!(
                Bt::self_has(AURA_MUTATING_INJECTION),
                MoveAwayFromRaid(20.0),
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
    use crate::encounters::EncounterEvent;
    use crate::engine::bt_nodes::{BtNode, BtResult};
    use crate::engine::context::tests::{TestCtxOwned, TestInterface, make_encounter_ctx};
    use crate::ffi::BotRole;

    #[test]
    fn injection_flees() {
        let mut fsm = GrobbolusFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);

        let bt = fsm.phase_bt().expect("should have BT when active");
        let iface = TestInterface::new()
            .with_aura(AURA_MUTATING_INJECTION)
            .with_safe_pos();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Rogue, BotRole::DPS);
        assert!(matches!(bt.tick(&mut ctx), BtResult::Running));
    }

    #[test]
    fn no_injection_returns_failure() {
        let mut fsm = GrobbolusFsm::new();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);

        let bt = fsm.phase_bt().expect("should have BT when active");
        let iface = TestInterface::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Failure);
    }
}
