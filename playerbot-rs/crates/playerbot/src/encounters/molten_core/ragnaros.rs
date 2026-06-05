/// Ragnaros encounter — Molten Core final boss.
///
/// States:
///   Ground (100%→75%): melee stack *behind* the boss (Wrath of Ragnaros is a
///     frontal knockback); ranged stay at range AND spread out (Elemental Fire
///     chains between stacked players).
///   Submerged (75%/50%/25%): nuke down the 8 Sons of Flame.
///   Phase 2 (< 25%): same as Ground, adds spawn continuously.
///
/// NOTE: the tank's exact "get knocked up the pillar" spot is a precise world
/// position + knockback-timing problem that isn't scripted — the tank holds
/// the boss normally instead.
use super::super::{EncounterEvent, EncounterFsm};
use crate::encounters::bt::Bt::{self, AttackNearest, IsMeleeDps, IsRanged, MoveBehind};
use crate::engine::bt::BehaviorLeaf;
use crate::engine::bt_nodes::BtResult;
use crate::engine::context::TickContext;
use crate::{Sel, Seq};
use cmangos::{SpellId, UnitHandle};

pub const ENTRY_SON_OF_FLAME: u32 = 12143;

pub const SPELL_WRATH_OF_RAGNAROS: SpellId = SpellId(20566);
pub const SPELL_HAND_OF_RAGNAROS: SpellId = SpellId(19780);
pub const SPELL_ELEMENTAL_FIRE: SpellId = SpellId(20563);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RagnarosPhase {
    #[default]
    Idle,
    Ground,
    Submerged,
    Phase2,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct RagnarosFsm {
    pub phase: RagnarosPhase,
    pub submerge_count: u8,
    pub sons_killed: u8,
    done: bool,
}

impl RagnarosFsm {
    pub const PHASE_IDLE: u32 = 0;
    pub const PHASE_GROUND: u32 = 1;
    pub const PHASE_SUBMERGED: u32 = 2;
    pub const PHASE_2: u32 = 3;
}

impl EncounterFsm for RagnarosFsm {
    fn update(&mut self, event: &EncounterEvent, boss_hp_pct: f32, _time_ms: u64) {
        if self.done {
            return;
        }
        match event {
            EncounterEvent::CombatStarted => self.phase = RagnarosPhase::Ground,
            EncounterEvent::UnitDied { .. } => {
                if self.phase == RagnarosPhase::Submerged {
                    self.sons_killed += 1;
                    if self.sons_killed >= 8 {
                        self.phase = RagnarosPhase::Ground;
                        self.sons_killed = 0;
                    }
                } else {
                    self.done = true;
                }
            }
            EncounterEvent::GroupWipe => {
                self.phase = RagnarosPhase::Idle;
                self.submerge_count = 0;
                self.sons_killed = 0;
            }
            EncounterEvent::None => {
                if self.phase == RagnarosPhase::Ground {
                    let threshold = match self.submerge_count {
                        0 => 0.75,
                        1 => 0.50,
                        2 => 0.25,
                        _ => 0.0,
                    };
                    if threshold > 0.0 && boss_hp_pct < threshold {
                        self.phase = RagnarosPhase::Submerged;
                        self.submerge_count += 1;
                        self.sons_killed = 0;
                    }
                }
                if self.phase == RagnarosPhase::Ground
                    && self.submerge_count >= 3
                    && boss_hp_pct < 0.25
                {
                    self.phase = RagnarosPhase::Phase2;
                }
            }
            _ => {}
        }
    }

    fn phase_id(&self) -> u32 {
        match self.phase {
            RagnarosPhase::Idle => Self::PHASE_IDLE,
            RagnarosPhase::Ground => Self::PHASE_GROUND,
            RagnarosPhase::Submerged => Self::PHASE_SUBMERGED,
            RagnarosPhase::Phase2 => Self::PHASE_2,
        }
    }

    fn is_active(&self) -> bool {
        self.phase != RagnarosPhase::Idle
    }
    fn is_done(&self) -> bool {
        self.done
    }
    fn boss_entry(&self) -> u32 {
        super::ENTRY_RAGNAROS
    }

    fn phase_bt(&self, _fsm: crate::engine::macro_fsm::ActiveFsm) -> Option<Bt> {
        match self.phase {
            RagnarosPhase::Idle => None,
            // Submerged: nuke the Sons of Flame before Ragnaros re-emerges.
            RagnarosPhase::Submerged => {
                Some(Sel!(Bt::Custom(FOCUS_SON_OF_FLAME), AttackNearest))
            }
            RagnarosPhase::Ground | RagnarosPhase::Phase2 => Some(Sel!(
                // Melee stack behind the boss — Wrath of Ragnaros knocks back
                // everyone in front of him.
                Seq!(IsMeleeDps, MoveBehind(3.0)),
                // Ranged: stay at range AND spread to distinct points around
                // the boss so Elemental Fire can't chain between them.
                Seq!(IsRanged, Bt::Custom(RANGED_SPREAD)),
            )),
        }
    }
}

// ── Behavior leaves ───────────────────────────────────────────────────────

/// Attack the nearest Son of Flame — the submerge-phase adds that must be
/// burned down before Ragnaros re-emerges.
const FOCUS_SON_OF_FLAME: BehaviorLeaf = BehaviorLeaf {
    label: "rag_focus_son_of_flame",
    handler: |ctx: &mut TickContext<'_>| -> BtResult {
        let units = ctx.interface.get_nearby_units(40.0, true);
        let mut best: Option<UnitHandle> = None;
        let mut best_dist = f32::MAX;
        for &u in units.iter() {
            if ctx.interface.get_unit_snapshot(u).npc_entry != ENTRY_SON_OF_FLAME {
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
    display_text: Some("Nuking Son of Flame"),
};

/// Move this ranged bot to its own spread point around the boss (Elemental
/// Fire chains between stacked players). The bot's slot is its index in the
/// group roster, so each ranged member lands at a distinct angle.
const RANGED_SPREAD: BehaviorLeaf = BehaviorLeaf {
    label: "rag_ranged_spread",
    handler: |ctx: &mut TickContext<'_>| -> BtResult {
        let Some(boss) = ctx.current_target() else {
            return BtResult::Failure;
        };
        let total = ctx.snap.group_size.max(1);
        let idx = ctx.snap.group_members[..ctx.snap.group_size as usize]
            .iter()
            .position(|&h| h == ctx.bot_handle)
            .unwrap_or(0) as u8;
        let pos = ctx.interface.get_spread_position(boss, 30.0, idx, total);
        let me = ctx.snap.self_.pos;
        if (me.x - pos.x).powi(2) + (me.y - pos.y).powi(2) <= 4.0 * 4.0 {
            return BtResult::Success; // already spread out
        }
        if ctx.interface.move_to(pos.x, pos.y, pos.z) {
            BtResult::Running
        } else {
            BtResult::Failure
        }
    },
    display_text: Some("Spreading"),
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
    fn submerges_at_75pct() {
        let mut fsm = RagnarosFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        fsm.update(&EncounterEvent::None, 0.74, 1000);
        assert_eq!(fsm.phase, RagnarosPhase::Submerged);
    }

    #[test]
    fn reemerges_after_8_sons() {
        let mut fsm = RagnarosFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        fsm.update(&EncounterEvent::None, 0.74, 0);
        for _ in 0..8 {
            fsm.update(&EncounterEvent::UnitDied { victim: 999 }, 0.74, 0);
        }
        assert_eq!(fsm.phase, RagnarosPhase::Ground);
    }

    #[test]
    fn enters_phase2_after_3rd_submerge() {
        let mut fsm = RagnarosFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        for hp in &[0.74_f32, 0.49, 0.24] {
            fsm.update(&EncounterEvent::None, *hp, 0);
            for _ in 0..8 {
                fsm.update(&EncounterEvent::UnitDied { victim: 999 }, *hp, 0);
            }
        }
        fsm.update(&EncounterEvent::None, 0.24, 0);
        assert_eq!(fsm.phase, RagnarosPhase::Phase2);
    }

    #[test]
    fn submerge_bt_attacks_adds() {
        let mut fsm = RagnarosFsm::default();
        fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
        fsm.update(&EncounterEvent::None, 0.74, 0);
        let bt = fsm
            .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
            .unwrap();
        let iface = MockWorld::new();
        let mut owned = TestCtxOwned::new();
        owned.attackers = vec![42];
        let mut ctx =
            make_encounter_ctx(&mut owned, &iface, &fsm, PlayerClass::Warrior, BotRole::DPS);
        assert_eq!(bt.tick(&mut ctx), BtResult::Success);
    }

    #[test]
    fn no_bt_when_idle() {
        assert!(
            RagnarosFsm::default()
                .phase_bt(crate::engine::macro_fsm::ActiveFsm::Combat)
                .is_none()
        );
    }
}
