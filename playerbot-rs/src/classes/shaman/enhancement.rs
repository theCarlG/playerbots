/// Enhancement Shaman behavior tree (Classic / Vanilla).
///
/// Priority: Lightning Shield + totem upkeep → panic heal → Stormstrike →
///   Earth Shock interrupt → Flame Shock DoT → Earth Shock filler
use crate::{
    data::spells::vanilla::shaman::*,
    engine::bt::Bt::{self, *},
    ffi::SpellId,
};

// Higher-rank auras for self buffs.
const LIGHTNING_SHIELD_RANKS: &[SpellId] = &[LIGHTNING_SHIELD, SpellId(10432)];
const WINDFURY_TOTEM_RANKS: &[SpellId] = &[WINDFURY_TOTEM, SpellId(25587)];

pub fn build_tree() -> Bt {
    Sel(vec![
        // Self buffs.
        Seq(vec![
            SelfMissingAnyRank(LIGHTNING_SHIELD_RANKS),
            CastOnSelf(LIGHTNING_SHIELD),
        ]),
        Seq(vec![
            SelfMissingAnyRank(WINDFURY_TOTEM_RANKS),
            CastOnSelf(WINDFURY_TOTEM),
        ]),
        Seq(vec![
            SelfMissingAura(STRENGTH_OF_EARTH_TOTEM),
            CastOnSelf(STRENGTH_OF_EARTH_TOTEM),
        ]),
        StickToTarget(5.0),
        Seq(vec![
            InCombat,
            Sel(vec![
                // Panic heal chain.
                Seq(vec![HpBelow(0.25), CastOnSelf(NATURE_SWIFTNESS)]),
                Seq(vec![HpBelow(0.35), CastOnSelf(LESSER_HEALING_WAVE)]),
                // Primary damage.
                CastOnTarget(STORMSTRIKE),
                // Interrupt.
                Seq(vec![TargetIsCasting, CastOnTarget(EARTH_SHOCK)]),
                // DoT.
                Seq(vec![
                    TargetMissingAura(FLAME_SHOCK),
                    CastOnTarget(FLAME_SHOCK),
                ]),
                // Filler instant.
                CastOnTarget(EARTH_SHOCK),
            ]),
        ]),
    ])
}
