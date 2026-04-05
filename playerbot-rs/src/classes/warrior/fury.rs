/// Fury Warrior behavior tree (Classic / Vanilla).
///
/// Priority: close gap → emergency fear → execute → bloodthirst → whirlwind → cleave/heroic strike
use crate::{
    data::spells::vanilla::warrior::*,
    engine::bt::{Bt::{self, *}, Op::*, Resource::*},
};
use crate::{Seq, Sel};

pub fn build_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (warrior-wide list).
        super::boost(),
        // Close gap: Intercept (Berserker), Charge (Battle), then stick.
        CastOnTarget(INTERCEPT),
        CastOnTarget(CHARGE),
        StickToTarget(5.0),
        Seq!(
            InCombat,
            Sel!(
                // Emergency fear.
                Seq!(Cmp(SelfHealthPct, Below(15)), CastOnTarget(INTIMIDATING_SHOUT)),
                // Execute.
                Seq!(Cmp(TargetHealthPct, Below(20)), CastOnTarget(EXECUTE)),
                // Core damage.
                CastOnTarget(BLOODTHIRST),
                CastOnTarget(WHIRLWIND),
                // Rage dumps (same swing slot).
                CastOnTarget(HEROIC_STRIKE),
                Seq!(Cmp(NearbyCount, AtLeast(2)), CastOnTarget(CLEAVE)),
            ),
        ),
    )
}
