use crate::{Sel, Seq};
/// Balance Druid behavior tree (Classic / Vanilla).
///
/// Priority: emergency self-heal → Barkskin → Faerie Fire → Insect Swarm →
///   Moonfire → Starfire → Wrath filler
use crate::{
    data::spells::vanilla::druid::*,
    engine::{
        aura_helpers::{FAERIE_FIRE_RANKS, INSECT_SWARM_RANKS, MOONFIRE_RANKS},
        bt::{
            Bt::{self, *},
            Op::*,
            Resource::*,
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
        // `co +boost` burst cooldowns (druid-wide list).
        super::boost(),
        // Emergency self-heal + Barkskin.
        Seq!(
            Cmp(SelfHealthPct, Below(35)),
            Sel!(CastOnSelf(REGROWTH), CastOnSelf(BARKSKIN)),
        ),
        Seq!(
            InCombat,
            Sel!(
                // Debuff upkeep.
                Seq!(
                    Bt::target_missing_any_rank(FAERIE_FIRE_RANKS),
                    CastOnTarget(FAERIE_FIRE_FERAL),
                ),
                Seq!(
                    Bt::target_missing_any_rank(INSECT_SWARM_RANKS),
                    CastOnTarget(INSECT_SWARM),
                ),
                Seq!(
                    Bt::target_missing_any_rank(MOONFIRE_RANKS),
                    CastOnTarget(MOONFIRE),
                ),
                // Nukes.
                CastOnTarget(STARFIRE),
                CastOnTarget(WRATH),
            ),
        ),
    )
}
