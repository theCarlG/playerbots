use crate::{Sel, Seq};
/// Arms Warrior behavior tree (Classic / Vanilla).
///
/// Priority: charge/stick → emergency Intimidating Shout → Execute → Overpower
///   → Mortal Strike → Whirlwind → Heroic Strike → Rend → Battle Shout upkeep
use crate::{
    data::spells::vanilla::warrior::*,
    engine::{
        aura_helpers::{BATTLE_SHOUT_RANKS, REND_RANKS},
        bt::{
            Bt::{self, CastOnSelf, CastOnTarget, Cmp, InCombat, InShapeshift, Not},
            Op::{AtLeast, Below},
            Resource::{NearbyCount, SelfHealthPct, SelfRage, TargetHealthPct},
        },
        macro_fsm::ActiveFsm,
    },
};

pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => combat_tree(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}

fn combat_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (warrior-wide list).
        super::boost(),
        // Gap closer: Charge in Battle Stance. Melee approach is handled
        // by combat_wrapper's close_subtree (StrategyFlags::CLOSE).
        Seq!(InShapeshift(FORM_BATTLE_STANCE), CastOnTarget(CHARGE)),
        // In-combat rotation.
        Seq!(
            InCombat,
            Sel!(
                // Switch to Battle Stance for Arms rotation.
                Seq!(Not(Box::new(InShapeshift(FORM_BATTLE_STANCE))), CastOnSelf(BATTLE_STANCE)),
                // Emergency fear at very low HP.
                Seq!(
                    Cmp(SelfHealthPct, Below(15)),
                    CastOnTarget(INTIMIDATING_SHOUT)
                ),
                // Execute on low-HP target.
                Seq!(Cmp(TargetHealthPct, Below(20)), CastOnTarget(EXECUTE)),
                // Overpower proc (server gates via can_cast).
                CastOnTarget(OVERPOWER),
                // Core damage.
                CastOnTarget(MORTAL_STRIKE),
                CastOnTarget(WHIRLWIND),
                // AoE: Cleave when 2+ nearby.
                Seq!(Cmp(NearbyCount, AtLeast(2)), Cmp(SelfRage, AtLeast(30)), CastOnTarget(CLEAVE)),
                CastOnTarget(HEROIC_STRIKE),
                // Sunder Armor debuff — stacks to 5, low priority.
                CastOnTarget(SUNDER_ARMOR),
                // Bleed upkeep.
                Seq!(Bt::target_missing_any_rank(REND_RANKS), CastOnTarget(REND)),
            ),
        ),
        // Out-of-combat: maintain Battle Shout.
        Seq!(
            InCombat.not(),
            Bt::self_missing_any_rank(BATTLE_SHOUT_RANKS),
            CastOnSelf(BATTLE_SHOUT),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::bt_nodes::BtNode;
    use crate::engine::context::tests::make_test_ctx;

    #[test]
    fn tree_builds_and_runs() {
        let tree = build_tree(ActiveFsm::Combat);
        let mut owned = make_test_ctx();
        let _ = tree.tick(&mut owned.ctx());
    }

    #[test]
    fn world_tree_is_noop() {
        let tree = build_tree(ActiveFsm::World);
        assert!(matches!(tree, Bt::Noop));
    }
}
