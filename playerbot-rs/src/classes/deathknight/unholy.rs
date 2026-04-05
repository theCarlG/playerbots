/// Unholy Death Knight behavior tree (`WotLK` only) — Disease DPS spec.
///
/// Priority: Bone Shield → Death Grip → diseases → Scourge Strike →
///   `AoE` Blood Boil → Death Coil → Death and Decay
#[allow(unused_imports)]
use crate::engine::bt::{Bt::{self, *}, Op::*, Resource::*};
#[allow(unused_imports)]
use crate::{Seq, Sel};

#[cfg(feature = "wotlk")]
use crate::{data::spells::vanilla::deathknight::*, ffi::SpellId};

#[cfg(not(feature = "wotlk"))]
pub fn build_tree() -> Bt {
    Sel!()
}

#[cfg(feature = "wotlk")]
const SCOURGE_STRIKE: SpellId = SpellId(55271);
#[cfg(feature = "wotlk")]
const FROST_FEVER: SpellId = SpellId(55095);
#[cfg(feature = "wotlk")]
const BLOOD_PLAGUE: SpellId = SpellId(55078);

#[cfg(feature = "wotlk")]
pub fn build_tree() -> Bt {
    Sel!(
        StickToTarget(5.0),
        // Self-buff.
        Seq!(Bt::self_missing(BONE_SHIELD), CastOnSelf(BONE_SHIELD)),
        // Pull.
        Seq!(Cmp(TargetDistance, Above(15)), CastOnTarget(DEATH_GRIP)),
        Seq!(
            InCombat,
            Sel!(
                // Diseases.
                Seq!(
                    Bt::target_missing(FROST_FEVER),
                    CastOnTarget(ICY_TOUCH),
                ),
                Seq!(
                    Bt::target_missing(BLOOD_PLAGUE),
                    CastOnTarget(PLAGUE_STRIKE),
                ),
                // Main damage.
                CastOnTarget(SCOURGE_STRIKE),
                // AoE spread.
                Seq!(Cmp(NearbyCount, AtLeast(2)), CastOnTarget(BLOOD_BOIL)),
                // RP dump.
                CastOnTarget(DEATH_COIL),
                // AoE ground.
                Seq!(Cmp(NearbyCount, AtLeast(2)), CastOnSelf(DEATH_AND_DECAY)),
            ),
        ),
    )
}
