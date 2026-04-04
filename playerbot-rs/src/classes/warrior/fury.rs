/// Fury Warrior behavior tree (Classic / Vanilla).
///
/// Priority: close gap → emergency fear → execute → bloodthirst → whirlwind → cleave/heroic strike
use crate::{
    data::spells::vanilla::warrior::*,
    engine::bt::Bt::{self, *},
};

pub fn build_tree() -> Bt {
    Sel(vec![
        // Close gap: Intercept (Berserker), Charge (Battle), then stick.
        CastOnTarget(INTERCEPT),
        CastOnTarget(CHARGE),
        StickToTarget(5.0),

        Seq(vec![InCombat, Sel(vec![
            // Emergency fear.
            Seq(vec![HpBelow(0.15), CastOnTarget(INTIMIDATING_SHOUT)]),
            // Execute.
            Seq(vec![TargetHpBelow(0.20), CastOnTarget(EXECUTE)]),
            // Core damage.
            CastOnTarget(BLOODTHIRST),
            CastOnTarget(WHIRLWIND),
            // Rage dumps (same swing slot).
            CastOnTarget(HEROIC_STRIKE),
            Seq(vec![NearbyAtLeast(2), CastOnTarget(CLEAVE)]),
        ])]),
    ])
}
