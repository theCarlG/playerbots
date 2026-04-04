/// Protection Warrior behavior tree (Classic / Vanilla).
///
/// Tank priority: survival CDs → taunt → Shield Block → Bloodrage → Revenge →
///   Shield Slam → Sunder Armor → Heroic Strike → Demoralizing Shout
use crate::{
    data::spells::vanilla::warrior::*,
    engine::{
        aura_helpers::DEMORALIZING_SHOUT_RANKS,
        bt::Bt::{self, *},
    },
};

pub fn build_tree() -> Bt {
    Sel(vec![
        // Emergency survival CDs.
        Seq(vec![
            HpBelow(0.20),
            Sel(vec![CastOnSelf(SHIELD_WALL), CastOnSelf(LAST_STAND)]),
        ]),
        // Close gap.
        CastOnTarget(CHARGE),
        StickToTarget(5.0),
        Seq(vec![
            InCombat,
            Sel(vec![
                // Taunt is gated by can_cast (only fires on aggro loss).
                CastOnTarget(TAUNT),
                // Shield Block mitigation.
                CastOnSelf(SHIELD_BLOCK),
                // Bloodrage while HP is safe.
                Seq(vec![HpBelow(0.30).not(), CastOnSelf(BLOODRAGE)]),
                // Revenge proc.
                CastOnTarget(REVENGE),
                // Core threat.
                CastOnTarget(SHIELD_SLAM),
                CastOnTarget(SUNDER_ARMOR),
                CastOnTarget(HEROIC_STRIKE),
                // Demo Shout upkeep.
                Seq(vec![
                    TargetMissingAnyRank(DEMORALIZING_SHOUT_RANKS),
                    CastOnTarget(DEMORALIZING_SHOUT),
                ]),
            ]),
        ]),
    ])
}
