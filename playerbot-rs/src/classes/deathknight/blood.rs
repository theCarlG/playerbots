/// Blood Death Knight behavior tree (WotLK only).
///
/// Priority: Death Grip pull → Dancing Rune Weapon → diseases → Death Strike heal
///   → Heart Strike → Blood Strike → Death Coil → AoE
use crate::engine::bt::Bt::{self, *};

#[cfg(feature = "wotlk")]
use crate::{data::spells::vanilla::deathknight::*, ffi::SpellId};

#[cfg(not(feature = "wotlk"))]
pub fn build_tree() -> Bt {
    Sel(vec![])
}

#[cfg(feature = "wotlk")]
const FROST_FEVER: SpellId = SpellId(55095);
#[cfg(feature = "wotlk")]
const BLOOD_PLAGUE: SpellId = SpellId(55078);

#[cfg(feature = "wotlk")]
pub fn build_tree() -> Bt {
    Sel(vec![
        StickToTarget(5.0),

        // Death Grip pull on ranged targets.
        Seq(vec![TargetFartherThan(15.0), CastOnTarget(DEATH_GRIP)]),

        Seq(vec![InCombat, Sel(vec![
            // Cooldown.
            CastOnSelf(DANCING_RUNE_WEAPON),

            // Taunt.
            CastOnTarget(DARK_COMMAND),

            // Diseases.
            Seq(vec![TargetMissingAura(FROST_FEVER), CastOnTarget(ICY_TOUCH)]),
            Seq(vec![TargetMissingAura(BLOOD_PLAGUE), CastOnTarget(PLAGUE_STRIKE)]),

            // Self-sustain.
            Seq(vec![HpBelow(0.70), CastOnTarget(DEATH_STRIKE)]),

            // Main damage.
            CastOnTarget(HEART_STRIKE),
            CastOnTarget(BLOOD_STRIKE),
            CastOnTarget(DEATH_COIL),

            // AoE.
            Seq(vec![NearbyAtLeast(2), CastOnTarget(BLOOD_BOIL)]),
            Seq(vec![NearbyAtLeast(2), CastOnSelf(DEATH_AND_DECAY)]),
        ])]),
    ])
}
