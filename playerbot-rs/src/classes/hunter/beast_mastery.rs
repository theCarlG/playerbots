use crate::{Sel, Seq};
/// Beast Mastery Hunter behavior tree (Classic / Vanilla).
///
/// BM relies heavily on pet damage.
/// Priority: Feign Death → Aspect of the Hawk → Hunter's Mark → Bestial Wrath →
///   shots → Serpent Sting → Raptor Strike (melee fallback)
use crate::{
    data::spells::vanilla::hunter::*,
    engine::{
        aura_helpers::{HUNTERS_MARK_RANKS, SERPENT_STING_RANKS},
        bt::{
            Bt::{self, Cmp, CastOnSelf, MaintainRange, InCombat, CastOnTarget},
            Op::Below,
            Resource::SelfHealthPct,
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
        // `co +boost` burst cooldowns (hunter-wide list).
        super::boost(),
        // Emergency FD.
        Seq!(Cmp(SelfHealthPct, Below(15)), CastOnSelf(FEIGN_DEATH)),
        // Maintain Aspect of the Hawk.
        Seq!(
            Bt::self_missing(ASPECT_OF_THE_HAWK),
            CastOnSelf(ASPECT_OF_THE_HAWK),
        ),
        // Kite melee attackers (MaintainRange only fires when too close).
        MaintainRange(8.0),
        Seq!(
            InCombat,
            Sel!(
                Seq!(
                    Bt::target_missing_any_rank(HUNTERS_MARK_RANKS),
                    CastOnTarget(HUNTERS_MARK),
                ),
                CastOnTarget(BESTIAL_WRATH),
                CastOnTarget(AIMED_SHOT),
                CastOnTarget(MULTI_SHOT),
                CastOnTarget(ARCANE_SHOT),
                Seq!(
                    Bt::target_missing_any_rank(SERPENT_STING_RANKS),
                    CastOnTarget(SERPENT_STING),
                ),
                // Melee fallback.
                CastOnTarget(RAPTOR_STRIKE),
            ),
        ),
    )
}
