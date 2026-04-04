/// Frost Death Knight behavior tree (WotLK only).
///
/// Priority: Death Grip → Anti-Magic Shell vs casters → Howling Blast → Obliterate
///   → diseases → Frost Strike → Chains of Ice
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
        Seq(vec![TargetFartherThan(15.0), CastOnTarget(DEATH_GRIP)]),
        Seq(vec![
            InCombat,
            Sel(vec![
                // Absorb vs casters.
                Seq(vec![TargetIsCasting, CastOnSelf(ANTI_MAGIC_SHELL)]),
                // Burst / AoE.
                CastOnTarget(HOWLING_BLAST),
                // Main melee.
                CastOnTarget(OBLITERATE),
                // Diseases.
                Seq(vec![
                    TargetMissingAura(FROST_FEVER),
                    CastOnTarget(ICY_TOUCH),
                ]),
                Seq(vec![
                    TargetMissingAura(BLOOD_PLAGUE),
                    CastOnTarget(PLAGUE_STRIKE),
                ]),
                // RP dump.
                CastOnTarget(FROST_STRIKE),
                // Snare fleeing target.
                Seq(vec![TargetFartherThan(8.0), CastOnTarget(CHAINS_OF_ICE)]),
            ]),
        ]),
    ])
}
