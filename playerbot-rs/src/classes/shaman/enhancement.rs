use crate::{Sel, Seq};
/// Enhancement Shaman behavior tree (Classic / Vanilla).
///
/// Priority: Lightning Shield + totem upkeep → panic heal → Stormstrike →
///   Earth Shock interrupt → Flame Shock `DoT` → Earth Shock filler
use crate::{
    data::spells::vanilla::shaman::*,
    engine::bt::{
        Bt::{self, CastOnSelf, DropConfiguredTotems, InCombat, Cmp, CastOnTarget, TargetIsCasting},
        Op::{Below, AtLeast},
        Resource::{SelfHealthPct, NearbyCount},
    },
    engine::macro_fsm::ActiveFsm,
    ffi::SpellId,
};

// Higher-rank auras for self buffs.
const LIGHTNING_SHIELD_RANKS: &[SpellId] = &[LIGHTNING_SHIELD, SpellId(10432)];

pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => combat_tree(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}

fn combat_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (shaman-wide list).
        super::boost(),
        // Self buffs.
        Seq!(
            Bt::self_missing_any_rank(LIGHTNING_SHIELD_RANKS),
            CastOnSelf(LIGHTNING_SHIELD),
        ),
        // Melee approach handled by combat_wrapper's close_subtree.
        Seq!(
            InCombat,
            Sel!(
                // Totem upkeep — drop as soon as we're in position.
                Bt::throttle(2_000, DropConfiguredTotems),
                // Panic heal chain.
                Seq!(Cmp(SelfHealthPct, Below(25)), CastOnSelf(NATURE_SWIFTNESS)),
                Seq!(
                    Cmp(SelfHealthPct, Below(35)),
                    CastOnSelf(LESSER_HEALING_WAVE)
                ),
                // AoE: Chain Lightning when 3+ nearby.
                Seq!(Cmp(NearbyCount, AtLeast(3)), CastOnTarget(CHAIN_LIGHTNING)),
                // Primary damage.
                CastOnTarget(STORMSTRIKE),
                // Interrupt.
                Seq!(TargetIsCasting, CastOnTarget(EARTH_SHOCK)),
                // DoT.
                Seq!(Bt::target_missing(FLAME_SHOCK), CastOnTarget(FLAME_SHOCK)),
                // Filler instant.
                CastOnTarget(EARTH_SHOCK),
            ),
        ),
    )
}
