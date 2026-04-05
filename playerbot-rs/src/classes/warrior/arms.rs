/// Arms Warrior behavior tree (Classic / Vanilla).
///
/// Priority: charge/stick → emergency Intimidating Shout → Execute → Overpower
///   → Mortal Strike → Whirlwind → Heroic Strike → Rend → Battle Shout upkeep
use crate::{
    data::spells::vanilla::warrior::*,
    engine::{
        aura_helpers::{BATTLE_SHOUT_RANKS, REND_RANKS},
        bt::{Bt::{self, *}, Op::*, Resource::*},
    },
};
use crate::{Seq, Sel};

pub fn build_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (warrior-wide list).
        super::boost(),
        // Close the gap: Charge if out of range, otherwise stick.
        CastOnTarget(CHARGE),
        StickToTarget(5.0),
        // In-combat rotation.
        Seq!(
            InCombat,
            Sel!(
                // Emergency fear at very low HP.
                Seq!(Cmp(SelfHealthPct, Below(15)), CastOnTarget(INTIMIDATING_SHOUT)),
                // Execute on low-HP target.
                Seq!(Cmp(TargetHealthPct, Below(20)), CastOnTarget(EXECUTE)),
                // Overpower proc (server gates via can_cast).
                CastOnTarget(OVERPOWER),
                // Core damage.
                CastOnTarget(MORTAL_STRIKE),
                CastOnTarget(WHIRLWIND),
                CastOnTarget(HEROIC_STRIKE),
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
        let tree = build_tree();
        let mut owned = make_test_ctx();
        let _ = tree.tick(&mut owned.ctx());
    }
}
