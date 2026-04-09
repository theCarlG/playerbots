use crate::{Sel, Seq};
/// Protection Warrior behavior tree (Classic / Vanilla).
///
/// Tank priority: survival CDs → taunt → Shield Block → Bloodrage → Revenge →
///   Shield Slam → Sunder Armor → Heroic Strike → Demoralizing Shout
use crate::{
    data::spells::vanilla::warrior::*,
    engine::{
        aura_helpers::DEMORALIZING_SHOUT_RANKS,
        bt::{
            Bt::{self, Cmp, CastOnSelf, InShapeshift, CastOnTarget, InCombat, Not},
            Op::{Below, AtLeast},
            Resource::{SelfHealthPct, NearbyCount, SelfRage},
        },
        macro_fsm::ActiveFsm,
    },
};

pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => combat_tree(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}

fn combat_tree() -> Bt {
    Sel!(
        // NOTE: no super::boost() here — the shared warrior boost uses
        // Recklessness/Death Wish/Berserker Rage which require Berserker
        // Stance and always fail for protection warriors in Defensive Stance.
        // Bloodrage is already in the rotation below.
        //
        // Emergency survival CDs.
        Seq!(
            Cmp(SelfHealthPct, Below(20)),
            Sel!(CastOnSelf(SHIELD_WALL), CastOnSelf(LAST_STAND)),
        ),
        // Gap closer (Charge requires Battle Stance). Melee approach
        // handled by combat_wrapper's close_subtree.
        Seq!(InShapeshift(FORM_BATTLE_STANCE), CastOnTarget(CHARGE)),
        Seq!(
            InCombat,
            Sel!(
                // Switch to Defensive Stance for Prot rotation.
                Seq!(
                    Not(Box::new(InShapeshift(FORM_DEFENSIVE_STANCE))),
                    CastOnSelf(DEFENSIVE_STANCE)
                ),
                // Taunt is gated by can_cast (only fires on aggro loss).
                CastOnTarget(TAUNT),
                // Shield Block mitigation.
                CastOnSelf(SHIELD_BLOCK),
                // Bloodrage while HP is safe.
                Seq!(Cmp(SelfHealthPct, Below(30)).not(), CastOnSelf(BLOODRAGE)),
                // Revenge proc.
                CastOnTarget(REVENGE),
                // AoE threat when multiple mobs.
                Seq!(Cmp(NearbyCount, AtLeast(3)), CastOnTarget(THUNDER_CLAP)),
                Seq!(Cmp(NearbyCount, AtLeast(2)), Cmp(SelfRage, AtLeast(30)), CastOnTarget(CLEAVE)),
                // Core threat.
                CastOnTarget(SHIELD_SLAM),
                CastOnTarget(SUNDER_ARMOR),
                CastOnTarget(HEROIC_STRIKE),
                // Demo Shout upkeep.
                Seq!(
                    Bt::target_missing_any_rank(DEMORALIZING_SHOUT_RANKS),
                    CastOnTarget(DEMORALIZING_SHOUT),
                ),
            ),
        ),
    )
}
