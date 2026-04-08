/// Blood Death Knight behavior tree (`WotLK` only).
///
/// Priority: Death Grip pull → Dancing Rune Weapon → diseases → Death Strike heal
///   → Heart Strike → Blood Strike → Death Coil → `AoE`
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
const FROST_FEVER: SpellId = SpellId(55095);
#[cfg(feature = "wotlk")]
const BLOOD_PLAGUE: SpellId = SpellId(55078);

#[cfg(feature = "wotlk")]
pub fn build_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (DK-wide list).
        super::boost(),
        StickToTarget(5.0),
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
                Seq!(
                    Bt::target_missing(FROST_FEVER),
                    CastOnTarget(ICY_TOUCH),
                ),
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
