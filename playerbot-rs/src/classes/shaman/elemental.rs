/// Elemental Shaman behavior tree (Classic / Vanilla).
///
/// Priority: totem upkeep → Earth Shock interrupt → Flame Shock DoT →
///   Chain Lightning AoE → Lightning Bolt → Frost Shock filler
use crate::{
    data::spells::vanilla::shaman::*,
    engine::bt::Bt::{self, *},
};

pub fn build_tree() -> Bt {
    Sel(vec![
        MaintainRange(20.0),

        // Totem upkeep.
        Seq(vec![GroupSizeAtLeast(2), SelfMissingAura(GRACE_OF_AIR_TOTEM),
                 CastOnSelf(GRACE_OF_AIR_TOTEM)]),
        Seq(vec![SelfMissingAura(MANA_SPRING_TOTEM), CastOnSelf(MANA_SPRING_TOTEM)]),

        Seq(vec![InCombat, Sel(vec![
            // Interrupt.
            Seq(vec![TargetIsCasting, CastOnTarget(EARTH_SHOCK)]),

            // Flame Shock DoT.
            Seq(vec![TargetMissingAura(FLAME_SHOCK), CastOnTarget(FLAME_SHOCK)]),

            // AoE.
            Seq(vec![NearbyAtLeast(3), CastOnTarget(CHAIN_LIGHTNING)]),

            // Main nuke.
            CastOnTarget(LIGHTNING_BOLT),

            // Filler/snare.
            CastOnTarget(FROST_SHOCK),
        ])]),
    ])
}
