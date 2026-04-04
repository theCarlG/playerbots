/// Arcane Mage behavior tree (Classic / Vanilla).
///
/// Priority: Ice Block → Evocation → Counterspell → Fire Blast execute →
///   Arcane Missiles → Frostbolt filler → Arcane Explosion AoE
use crate::{
    data::spells::vanilla::mage::*,
    engine::bt::Bt::{self, *},
};

pub fn build_tree() -> Bt {
    Sel(vec![
        // Kite melee attackers.
        MaintainRange(10.0),

        // Emergency Ice Block.
        Seq(vec![HpBelow(0.20), CastOnSelf(ICE_BLOCK)]),
        // OOM Evocation.
        Seq(vec![ManaBelow(0.10), CastOnSelf(EVOCATION)]),

        Seq(vec![InCombat, Sel(vec![
            // Counterspell.
            Seq(vec![TargetIsCasting, CastOnTarget(COUNTERSPELL)]),
            // Execute instant.
            Seq(vec![TargetHpBelow(0.20), CastOnTarget(FIRE_BLAST)]),
            // Main channel nuke.
            CastOnTarget(ARCANE_MISSILES),
            // Efficient filler.
            CastOnTarget(FROSTBOLT),
            // AoE when swarmed.
            Seq(vec![NearbyAtLeast(3), CastOnSelf(ARCANE_EXPLOSION)]),
        ])]),
    ])
}
