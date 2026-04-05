/// Arcane Mage behavior tree (Classic / Vanilla).
///
/// Priority: Ice Block → Evocation → Counterspell → Fire Blast execute →
///   Arcane Missiles → Frostbolt filler → Arcane Explosion `AoE`
use crate::{
    data::spells::vanilla::mage::*,
    engine::bt::{Bt::{self, *}, Op::*, Resource::*},
};
use crate::{Seq, Sel};

pub fn build_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (mage-wide list).
        super::boost(),
        // Kite melee attackers.
        MaintainRange(10.0),
        // Emergency Ice Block.
        Seq!(Cmp(SelfHealthPct, Below(20)), CastOnSelf(ICE_BLOCK)),
        // OOM Evocation.
        Seq!(Cmp(SelfManaPct, Below(10)), CastOnSelf(EVOCATION)),
        Seq!(
            InCombat,
            Sel!(
                // Counterspell.
                Seq!(TargetIsCasting, CastOnTarget(COUNTERSPELL)),
                // Execute instant.
                Seq!(Cmp(TargetHealthPct, Below(20)), CastOnTarget(FIRE_BLAST)),
                // Main channel nuke.
                CastOnTarget(ARCANE_MISSILES),
                // Efficient filler.
                CastOnTarget(FROSTBOLT),
                // AoE when swarmed.
                Seq!(Cmp(NearbyCount, AtLeast(3)), CastOnSelf(ARCANE_EXPLOSION)),
            ),
        ),
    )
}
