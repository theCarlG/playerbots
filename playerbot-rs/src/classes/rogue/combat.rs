/// Combat Rogue behavior tree (Classic / Vanilla).
///
/// Priority: Vanish → Evasion → Kick interrupt → Riposte → Blade Flurry `AoE`
///   → Slice and Dice upkeep → Eviscerate → Rupture → Sinister Strike
use crate::{
    data::spells::vanilla::rogue::*,
    engine::{
        aura_helpers::RUPTURE_RANKS,
        bt::Bt::{self, Sel, StickToTarget, Seq, HpBelow, CastOnSelf, InCombat, TargetIsCasting, CastOnTarget, NearbyAtLeast, SelfMissingAura, TargetMissingAnyRank},
    },
    ffi::SpellId,
};

// Riposte: proc after parry.
const RIPOSTE: SpellId = SpellId(14251);

pub fn build_tree() -> Bt {
    Sel(vec![
        StickToTarget(5.0),
        Seq(vec![HpBelow(0.15), CastOnSelf(VANISH)]),
        Seq(vec![HpBelow(0.30), CastOnSelf(EVASION)]),
        Seq(vec![
            InCombat,
            Sel(vec![
                // Interrupt.
                Seq(vec![TargetIsCasting, CastOnTarget(KICK)]),
                // Riposte — proc after parry (can_cast gates).
                CastOnTarget(RIPOSTE),
                // AoE when swarmed.
                Seq(vec![NearbyAtLeast(2), CastOnSelf(BLADE_FLURRY)]),
                // Slice and Dice upkeep.
                Seq(vec![
                    SelfMissingAura(SLICE_AND_DICE),
                    CastOnSelf(SLICE_AND_DICE),
                ]),
                // Finisher when SnD is up.
                CastOnTarget(EVISCERATE),
                // Rupture DoT.
                Seq(vec![
                    TargetMissingAnyRank(RUPTURE_RANKS),
                    CastOnTarget(RUPTURE),
                ]),
                // Builder.
                CastOnTarget(SINISTER_STRIKE),
            ]),
        ]),
    ])
}
