/// Fury Warrior behavior tree (Classic / Vanilla).
///
/// Priority: close gap → emergency fear → execute → bloodthirst → whirlwind → cleave/heroic strike
use crate::{
    data::spells::vanilla::warrior::*,
    engine::{
        aura_helpers::BATTLE_SHOUT_RANKS,
        bt::{Bt::{self, *}, Op::*, Resource::*},
    },
};
use crate::{Seq, Sel};

pub fn build_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (warrior-wide list).
        super::boost(),
        // Close gap: Intercept (Berserker), Charge (Battle), then stick.
        Seq!(InShapeshift(19), CastOnTarget(INTERCEPT)),
        Seq!(InShapeshift(17), CastOnTarget(CHARGE)),
        StickToTarget(5.0),
        Seq!(
            InCombat,
            Sel!(
                // Switch to Berserker Stance for our core rotation.
                // InShapeshift(19) = Berserker Stance.
                Seq!(Not(Box::new(InShapeshift(19))), CastOnSelf(BERSERKER_STANCE)),
                // Emergency fear.
                Seq!(Cmp(SelfHealthPct, Below(15)), CastOnTarget(INTIMIDATING_SHOUT)),
                // Execute.
                Seq!(Cmp(TargetHealthPct, Below(20)), CastOnTarget(EXECUTE)),
                // Core damage.
                CastOnTarget(BLOODTHIRST),
                CastOnTarget(WHIRLWIND),
                // Berserker Rage for fear immunity + rage generation.
                CastOnSelf(BERSERKER_RAGE),
                // Battle Shout upkeep — big AP buff for entire melee group.
                Seq!(Bt::self_missing_any_rank(BATTLE_SHOUT_RANKS), CastOnSelf(BATTLE_SHOUT)),
                // Rage dumps — only when pooled enough to not starve BT/WW.
                Seq!(Cmp(SelfRage, AtLeast(50)), Sel!(
                    Seq!(Cmp(NearbyCount, AtLeast(2)), CastOnTarget(CLEAVE)),
                    CastOnTarget(HEROIC_STRIKE),
                )),
            ),
        ),
    )
}
