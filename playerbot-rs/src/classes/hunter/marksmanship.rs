/// Marksmanship Hunter behavior tree (Classic / Vanilla).
///
/// Priority: Feign Death → Aspect of the Hawk → Hunter's Mark →
///   Scatter Shot (interrupt) → Aimed/Multi/Arcane Shot → Serpent Sting
///   → Wing Clip / Raptor Strike in melee
use crate::{
    data::spells::vanilla::hunter::*,
    engine::{
        aura_helpers::{HUNTERS_MARK_RANKS, SERPENT_STING_RANKS},
        bt::Bt::{self, *},
    },
};

pub fn build_tree() -> Bt {
    Sel(vec![
        // Emergency FD.
        Seq(vec![HpBelow(0.15), CastOnSelf(FEIGN_DEATH)]),
        // Maintain Aspect of the Hawk.
        Seq(vec![
            SelfMissingAura(ASPECT_OF_THE_HAWK),
            CastOnSelf(ASPECT_OF_THE_HAWK),
        ]),
        // Kite melee attackers.
        MaintainRange(8.0),
        Seq(vec![
            InCombat,
            Sel(vec![
                Seq(vec![
                    TargetMissingAnyRank(HUNTERS_MARK_RANKS),
                    CastOnTarget(HUNTERS_MARK),
                ]),
                // Melee fallback.
                Seq(vec![
                    TargetCloserThan(5.0),
                    Sel(vec![CastOnTarget(WING_CLIP), CastOnTarget(RAPTOR_STRIKE)]),
                ]),
                // Interrupt caster.
                Seq(vec![TargetIsCasting, CastOnTarget(SCATTER_SHOT)]),
                // Ranged rotation.
                CastOnTarget(AIMED_SHOT),
                CastOnTarget(MULTI_SHOT),
                CastOnTarget(ARCANE_SHOT),
                Seq(vec![
                    TargetMissingAnyRank(SERPENT_STING_RANKS),
                    CastOnTarget(SERPENT_STING),
                ]),
            ]),
        ]),
    ])
}
