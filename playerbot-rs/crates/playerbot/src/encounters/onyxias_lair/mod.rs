/// Onyxia's Lair — 3-phase fight.
///
/// States:
///   Phase 1 (100%→65%): Ground. Melee attack from the *flank* — Onyxia
///     cleaves/breathes to the front and tail-sweeps to the rear, so "behind"
///     is a danger zone; the side is the only safe melee spot.
///   Phase 2 (65%→40%): Air. Everyone dodges Deep Breath; melee can't reach
///     the airborne boss, so they kill the Onyxian Whelps instead.
///   Phase 3 (<40%): Ground again + whelp waves — melee clear whelps, then
///     fight the boss from the flank.
use super::{EncounterEvent, EncounterFsm};
use crate::engine::bt::Bt::{self, FleeToSafe, HoldPosition, IsMeleeDps, MoveToFlank};
use crate::engine::bt::BehaviorLeaf;
use crate::engine::bt_nodes::BtResult;
use crate::engine::context::TickContext;
use crate::{Sel, Seq};
use cmangos::{SpellId, UnitHandle};

pub const ENTRY_ONYXIA: u32 = 10184;
/// Onyxian Whelps — adds dropped in phases 2 and 3; must be cleared.
pub const ENTRY_ONYXIAN_WHELP: u32 = 11262;

pub const SPELL_DEEP_BREATH: SpellId = SpellId(22267);
pub const SPELL_FLAME_BREATH: SpellId = SpellId(18435);
pub const SPELL_FIREBALL_VOLLEY: SpellId = SpellId(18392);
pub const SPELL_WING_BUFFET: SpellId = SpellId(18500);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnyxiaPhase {
    #[default]
    Idle,
    Phase1,
    Phase2,
    Phase3,
}

#[derive(Default)]
pub struct OnyxiaFsm {
    pub phase: OnyxiaPhase,
    done: bool,
}

impl OnyxiaFsm {
    pub const PHASE_IDLE: u32 = 0;
    pub const PHASE_GROUND: u32 = 1;
    pub const PHASE_AIR: u32 = 2;
}

impl EncounterFsm for OnyxiaFsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, _time: u64) {
        if self.done {
            return;
        }
        match event {
            EncounterEvent::CombatStarted => self.phase = OnyxiaPhase::Phase1,
            EncounterEvent::UnitDied { .. } if self.phase != OnyxiaPhase::Idle => {
                self.done = true;
            }
            EncounterEvent::GroupWipe => self.phase = OnyxiaPhase::Idle,
            EncounterEvent::None => match self.phase {
                OnyxiaPhase::Phase1 if boss_hp_pct < 0.65 => self.phase = OnyxiaPhase::Phase2,
                OnyxiaPhase::Phase2 if boss_hp_pct < 0.40 => self.phase = OnyxiaPhase::Phase3,
                _ => {}
            },
            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        match self.phase {
            OnyxiaPhase::Idle => Self::PHASE_IDLE,
            OnyxiaPhase::Phase1 | OnyxiaPhase::Phase3 => Self::PHASE_GROUND,
            OnyxiaPhase::Phase2 => Self::PHASE_AIR,
        }
    }

    fn is_active(&self) -> bool {
        self.phase != OnyxiaPhase::Idle
    }
    fn is_done(&self) -> bool {
        self.done
    }
    fn boss_entry(&self) -> u32 {
        ENTRY_ONYXIA
    }

    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        match self.phase {
            OnyxiaPhase::Idle => None,
            OnyxiaPhase::Phase2 => Some(Sel!(
                // Everyone moves out of the Deep Breath fire path.
                Seq!(Bt::target_has(SPELL_DEEP_BREATH), FleeToSafe(40.0)),
                // Melee can't reach the airborne boss — kill the whelps.
                Seq!(IsMeleeDps, Bt::Custom(FOCUS_ONYXIAN_WHELP)),
                Seq!(IsMeleeDps, HoldPosition),
            )),
            OnyxiaPhase::Phase3 => Some(Sel!(
                // Whelp waves — melee clear them, then fight from the flank.
                Seq!(IsMeleeDps, Bt::Custom(FOCUS_ONYXIAN_WHELP)),
                Seq!(IsMeleeDps, MoveToFlank(5.0)),
            )),
            OnyxiaPhase::Phase1 => Some(Seq!(IsMeleeDps, MoveToFlank(5.0))),
        }
    }
}

/// Attack the nearest Onyxian Whelp — the adds dropped in phases 2 and 3.
/// In phase 2 the boss is airborne and out of melee reach, so clearing whelps
/// is the only thing melee can do; in phase 3 the whelp waves swarm healers.
const FOCUS_ONYXIAN_WHELP: BehaviorLeaf = BehaviorLeaf {
    label: "ony_focus_whelp",
    handler: |ctx: &mut TickContext<'_>| -> BtResult {
        let units = ctx.interface.get_nearby_units(40.0, true);
        let mut best: Option<UnitHandle> = None;
        let mut best_dist = f32::MAX;
        for &u in units.iter() {
            if ctx.interface.get_unit_snapshot(u).npc_entry != ENTRY_ONYXIAN_WHELP {
                continue;
            }
            let d = ctx.interface.unit_distance(u);
            if d < best_dist {
                best_dist = d;
                best = Some(u);
            }
        }
        match best {
            Some(u) if ctx.attack(u) => BtResult::Success,
            _ => BtResult::Failure,
        }
    },
    display_text: Some("Killing whelps"),
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::state::PlayerClass;
    use crate::encounters::EncounterEvent;
    use crate::engine::bt_nodes::{BtNode, BtResult};
    use crate::engine::context::tests::{TestCtxOwned, make_encounter_ctx};
    use cmangos::MockWorld;
    use cmangos::BotRole;

    #[test]
    fn transitions_phases() {
        let mut fsm = OnyxiaFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        assert_eq!(fsm.phase, OnyxiaPhase::Phase1);
        fsm.update(&EncounterEvent::None, 0.64, 0);
        assert_eq!(fsm.phase, OnyxiaPhase::Phase2);
        fsm.update(&EncounterEvent::None, 0.39, 0);
        assert_eq!(fsm.phase, OnyxiaPhase::Phase3);
    }

    #[test]
    fn air_melee_holds_position() {
        let mut fsm = OnyxiaFsm::default();
        fsm.phase = OnyxiaPhase::Phase2;
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new();
        let mut owned = TestCtxOwned::new();
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Rogue, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn phase1_melee_moves_to_flank() {
        let mut fsm = OnyxiaFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        assert_eq!(fsm.phase, OnyxiaPhase::Phase1);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        // Not yet on the flank → bot repositions to the side (chase issued),
        // rather than standing behind in the tail-sweep zone.
        let iface = MockWorld::new();
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 100; // Onyxia
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Running);
    }

    #[test]
    fn phase1_melee_at_flank_lets_rotation_run() {
        let mut fsm = OnyxiaFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        // Already on the flank and in melee range → MoveToFlank yields so the
        // class rotation runs instead of re-issuing movement.
        let iface = MockWorld::new().with_at_flank();
        let mut owned = TestCtxOwned::new();
        owned.snap.self_.current_target = 100;
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Failure);
    }

    #[test]
    fn no_bt_when_idle() {
        assert!(
            OnyxiaFsm::default()
                .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
                .is_none()
        );
    }
}
