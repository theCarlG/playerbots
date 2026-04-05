/// Marksmanship Hunter behavior tree (Classic / Vanilla).
///
/// Priority: Feign Death → Aspect of the Hawk → Hunter's Mark →
///   Scatter Shot (interrupt) → Aimed/Multi/Arcane Shot → Serpent Sting
///   → Wing Clip / Raptor Strike in melee
use crate::{
    data::spells::vanilla::hunter::*,
    engine::{
        aura_helpers::{HUNTERS_MARK_RANKS, SERPENT_STING_RANKS},
        bt::{Bt::{self, *}, Op::*, Resource::*},
    },
};
use crate::{Seq, Sel};

pub fn build_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (hunter-wide list).
        super::boost(),
        // Emergency FD.
        Seq!(Cmp(SelfHealthPct, Below(15)), CastOnSelf(FEIGN_DEATH)),
        // Maintain Aspect of the Hawk.
        Seq!(
            Bt::self_missing(ASPECT_OF_THE_HAWK),
            CastOnSelf(ASPECT_OF_THE_HAWK),
        ),
        // Kite melee attackers.
        MaintainRange(8.0),
        Seq!(
            InCombat,
            Sel!(
                Seq!(
                    Bt::target_missing_any_rank(HUNTERS_MARK_RANKS),
                    CastOnTarget(HUNTERS_MARK),
                ),
                // Melee fallback.
                Seq!(
                    Cmp(TargetDistance, Below(5)),
                    Sel!(CastOnTarget(WING_CLIP), CastOnTarget(RAPTOR_STRIKE)),
                ),
                // Interrupt caster.
                Seq!(TargetIsCasting, CastOnTarget(SCATTER_SHOT)),
                // Ranged rotation.
                CastOnTarget(AIMED_SHOT),
                CastOnTarget(MULTI_SHOT),
                CastOnTarget(ARCANE_SHOT),
                Seq!(
                    Bt::target_missing_any_rank(SERPENT_STING_RANKS),
                    CastOnTarget(SERPENT_STING),
                ),
            ),
        ),
    )
}
