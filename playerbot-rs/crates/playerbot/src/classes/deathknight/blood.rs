/// Blood Death Knight behavior tree (`WotLK` only).
///
/// Priority: Death Grip pull → Dancing Rune Weapon → diseases → Death Strike heal
///   → Heart Strike → Blood Strike → Death Coil → `AoE`
#[allow(unused_imports)]
use crate::engine::bt::{
    Bt::{self, Cmp, CastOnTarget, InCombat, CastOnSelf},
    Op::{Above, Below, AtLeast},
    Resource::{TargetDistance, SelfHealthPct, NearbyCount},
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
        // `co +boost` burst cooldowns (DK-wide list).
        super::boost(),
        // Melee approach handled by combat_wrapper's close_subtree.
        // Death Grip pull on ranged targets.
        Seq!(Cmp(TargetDistance, Above(15)), CastOnTarget(DEATH_GRIP)),
        Seq!(
            InCombat,
            Sel!(
                // Cooldown.
                CastOnSelf(DANCING_RUNE_WEAPON),
                // Taunt.
                CastOnTarget(DARK_COMMAND),
                // Diseases.
                Seq!(Bt::target_missing(FROST_FEVER), CastOnTarget(ICY_TOUCH),),
                Seq!(
                    Bt::target_missing(BLOOD_PLAGUE),
                    CastOnTarget(PLAGUE_STRIKE),
                ),
                // Self-sustain.
                Seq!(Cmp(SelfHealthPct, Below(70)), CastOnTarget(DEATH_STRIKE)),
                // Main damage.
                CastOnTarget(HEART_STRIKE),
                CastOnTarget(BLOOD_STRIKE),
                CastOnTarget(DEATH_COIL),
                // AoE.
                Seq!(Cmp(NearbyCount, AtLeast(2)), CastOnTarget(BLOOD_BOIL)),
                Seq!(Cmp(NearbyCount, AtLeast(2)), CastOnSelf(DEATH_AND_DECAY)),
            ),
        ),
    )
}
