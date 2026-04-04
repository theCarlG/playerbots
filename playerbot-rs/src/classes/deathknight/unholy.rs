/// Unholy Death Knight behavior tree (`WotLK` only) — Disease DPS spec.
///
/// Priority: Bone Shield → Death Grip → diseases → Scourge Strike →
///   `AoE` Blood Boil → Death Coil → Death and Decay
use crate::engine::bt::Bt::{self, Sel};

#[cfg(feature = "wotlk")]
use crate::{data::spells::vanilla::deathknight::*, ffi::SpellId};

#[cfg(not(feature = "wotlk"))]
pub fn build_tree() -> Bt {
    Sel(vec![])
}

#[cfg(feature = "wotlk")]
const SCOURGE_STRIKE: SpellId = SpellId(55271);
#[cfg(feature = "wotlk")]
const FROST_FEVER: SpellId = SpellId(55095);
#[cfg(feature = "wotlk")]
const BLOOD_PLAGUE: SpellId = SpellId(55078);

#[cfg(feature = "wotlk")]
pub fn build_tree() -> Bt {
    Sel(vec![
        StickToTarget(5.0),
        // Self-buff.
        Seq(vec![SelfMissingAura(BONE_SHIELD), CastOnSelf(BONE_SHIELD)]),
        // Pull.
        Seq(vec![TargetFartherThan(15.0), CastOnTarget(DEATH_GRIP)]),
        Seq(vec![
            InCombat,
            Sel(vec![
                // Diseases.
                Seq(vec![
                    TargetMissingAura(FROST_FEVER),
                    CastOnTarget(ICY_TOUCH),
                ]),
                Seq(vec![
                    TargetMissingAura(BLOOD_PLAGUE),
                    CastOnTarget(PLAGUE_STRIKE),
                ]),
                // Main damage.
                CastOnTarget(SCOURGE_STRIKE),
                // AoE spread.
                Seq(vec![NearbyAtLeast(2), CastOnTarget(BLOOD_BOIL)]),
                // RP dump.
                CastOnTarget(DEATH_COIL),
                // AoE ground.
                Seq(vec![NearbyAtLeast(2), CastOnSelf(DEATH_AND_DECAY)]),
            ]),
        ]),
    ])
}
