/// Survival Hunter behavior tree (Classic / Vanilla).
///
/// Priority: Feign Death → Hunter's Mark → Counterattack → Explosive Trap / Wing Clip
///   at melee → ranged rotation → Serpent Sting
use crate::{
    data::spells::vanilla::hunter::*,
    engine::{
        aura_helpers::{HUNTERS_MARK_RANKS, SERPENT_STING_RANKS},
        bt::Bt::{self, Sel, Seq, HpBelow, CastOnSelf, InCombat, TargetMissingAnyRank, CastOnTarget, TargetIsCasting, TargetCloserThan},
    },
    ffi::SpellId,
};

const COUNTERATTACK: SpellId = SpellId(20910); // rank 3

pub fn build_tree() -> Bt {
    Sel(vec![
        // Emergency FD.
        Seq(vec![HpBelow(0.15), CastOnSelf(FEIGN_DEATH)]),
        Seq(vec![
            InCombat,
            Sel(vec![
                Seq(vec![
                    TargetMissingAnyRank(HUNTERS_MARK_RANKS),
                    CastOnTarget(HUNTERS_MARK),
                ]),
                CastOnTarget(COUNTERATTACK),
                // Interrupt.
                Seq(vec![TargetIsCasting, CastOnTarget(SCATTER_SHOT)]),
                // Melee escapes/AoE.
                Seq(vec![TargetCloserThan(5.0), CastOnSelf(EXPLOSIVE_TRAP)]),
                Seq(vec![TargetCloserThan(5.0), CastOnTarget(WING_CLIP)]),
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
