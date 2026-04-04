/// Assassination Rogue behavior tree (Classic / Vanilla).
///
/// Priority: Vanish (emergency) → Kick interrupt → Slice and Dice upkeep →
///   Backstab → Hemorrhage → Eviscerate → Rupture → Sinister Strike
use crate::{
    data::spells::vanilla::rogue::*,
    engine::bt::Bt::{self, *},
};

pub fn build_tree() -> Bt {
    Sel(vec![
        StickToTarget(5.0),

        Seq(vec![HpBelow(0.15), CastOnSelf(VANISH)]),

        Seq(vec![InCombat, Sel(vec![
            // Interrupt.
            Seq(vec![TargetIsCasting, CastOnTarget(KICK)]),

            // Slice and Dice upkeep (self buff).
            Seq(vec![SelfMissingAura(SLICE_AND_DICE), CastOnSelf(SLICE_AND_DICE)]),

            // Positional — can_cast handles behind-check.
            CastOnTarget(BACKSTAB),
            CastOnTarget(HEMORRHAGE),
            CastOnTarget(EVISCERATE),

            // Rupture DoT.
            Seq(vec![TargetMissingAura(RUPTURE), CastOnTarget(RUPTURE)]),

            // Builder.
            CastOnTarget(SINISTER_STRIKE),
        ])]),
    ])
}
