/// Frost Death Knight behavior tree (`WotLK` only).
///
/// Priority: Death Grip → Anti-Magic Shell vs casters → Howling Blast → Obliterate
///   → diseases → Frost Strike → Chains of Ice
#[allow(unused_imports)]
use crate::engine::bt::{
    Bt::{self, Cmp, CastOnTarget, InCombat, TargetIsCasting, CastOnSelf},
    Op::Above,
    Resource::TargetDistance,
};
use crate::engine::macro_fsm::ActiveFsm;
#[allow(unused_imports)]
use crate::{Sel, Seq};

#[cfg(feature = "wotlk")]
use cmangos::SpellId;
#[cfg(feature = "wotlk")]
use crate::data::spells::vanilla::deathknight::*;

#[cfg(not(feature = "wotlk"))]
pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => Sel!(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}

#[cfg(feature = "wotlk")]
const FROST_FEVER: SpellId = SpellId(55095);
#[cfg(feature = "wotlk")]
const BLOOD_PLAGUE: SpellId = SpellId(55078);

#[cfg(feature = "wotlk")]
pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => combat_tree(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}

#[cfg(feature = "wotlk")]
fn combat_tree() -> Bt {
    Sel!(
        // Melee approach handled by combat_wrapper's close_subtree.
        Seq!(Cmp(TargetDistance, Above(15)), CastOnTarget(DEATH_GRIP)),
        Seq!(
            InCombat,
            Sel!(
                // Absorb vs casters.
                Seq!(TargetIsCasting, CastOnSelf(ANTI_MAGIC_SHELL)),
                // Diseases first — Obliterate benefits from both active.
                Seq!(Bt::target_missing(FROST_FEVER), CastOnTarget(ICY_TOUCH),),
                Seq!(
                    Bt::target_missing(BLOOD_PLAGUE),
                    CastOnTarget(PLAGUE_STRIKE),
                ),
                // Burst / AoE.
                CastOnTarget(HOWLING_BLAST),
                // Main melee.
                CastOnTarget(OBLITERATE),
                // RP dump.
                CastOnTarget(FROST_STRIKE),
                // Snare fleeing target.
                Seq!(Cmp(TargetDistance, Above(8)), CastOnTarget(CHAINS_OF_ICE)),
            ),
        ),
    )
}
