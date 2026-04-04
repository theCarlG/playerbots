/// Balance Druid behavior tree (Classic / Vanilla).
///
/// Priority: emergency self-heal → Barkskin → Faerie Fire → Insect Swarm →
///   Moonfire → Starfire → Wrath filler
use crate::{
    data::spells::vanilla::druid::*,
    engine::{
        aura_helpers::{FAERIE_FIRE_RANKS, INSECT_SWARM_RANKS, MOONFIRE_RANKS},
        bt::Bt::{self, *},
    },
};

pub fn build_tree() -> Bt {
    Sel(vec![
        // Emergency self-heal + Barkskin.
        Seq(vec![
            HpBelow(0.35),
            Sel(vec![CastOnSelf(REGROWTH), CastOnSelf(BARKSKIN)]),
        ]),
        Seq(vec![
            InCombat,
            Sel(vec![
                // Debuff upkeep.
                Seq(vec![
                    TargetMissingAnyRank(FAERIE_FIRE_RANKS),
                    CastOnTarget(FAERIE_FIRE_FERAL),
                ]),
                Seq(vec![
                    TargetMissingAnyRank(INSECT_SWARM_RANKS),
                    CastOnTarget(INSECT_SWARM),
                ]),
                Seq(vec![
                    TargetMissingAnyRank(MOONFIRE_RANKS),
                    CastOnTarget(MOONFIRE),
                ]),
                // Nukes.
                CastOnTarget(STARFIRE),
                CastOnTarget(WRATH),
            ]),
        ]),
    ])
}
