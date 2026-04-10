use cmangos::SpellId;
use crate::{Sel, Seq};
/// Affliction Warlock behavior tree (Classic / Vanilla).
///
/// Priority: Demon Armor → Life Tap → Curse of Agony → Corruption → Immolate →
///   Drain Life (self sustain) → Shadow Bolt
use crate::{
    data::spells::vanilla::warlock::*,
    engine::bt::{
        Bt::{self, CastOnSelf, CastAoEOnTarget, Cmp, Not, InCombat, CastOnTarget},
        Op::{Below, AtLeast},
        Resource::{SelfManaPct, SelfHealthPct, AttackerCount},
    },
    engine::macro_fsm::ActiveFsm,
};

const CURSE_OF_AGONY: SpellId = SpellId(11722);

pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => combat_tree(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}

fn combat_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (warlock-wide list).
        super::boost(),
        // Positioning handled by reactive::ranged_subtree().
        // Self buff.
        Seq!(Bt::self_missing(DEMON_ARMOR), CastOnSelf(DEMON_ARMOR)),
        // Life tap for mana.
        Seq!(
            Cmp(SelfManaPct, Below(20)),
            Not(Box::new(Cmp(SelfHealthPct, Below(50)))),
            CastOnSelf(LIFE_TAP),
        ),
        Seq!(
            InCombat,
            Sel!(
                // DoT upkeep.
                Seq!(
                    Bt::target_missing(CURSE_OF_AGONY),
                    CastOnTarget(CURSE_OF_AGONY),
                ),
                Seq!(Bt::target_missing(CORRUPTION), CastOnTarget(CORRUPTION),),
                Seq!(Bt::target_missing(IMMOLATE), CastOnTarget(IMMOLATE)),
                // AoE: Rain of Fire when 3+ attackers.
                Seq!(Cmp(AttackerCount, AtLeast(3)), CastAoEOnTarget(RAIN_OF_FIRE)),
                // Self sustain.
                Seq!(Cmp(SelfHealthPct, Below(60)), CastOnTarget(DRAIN_LIFE)),
                // Filler nuke.
                CastOnTarget(SHADOW_BOLT),
            ),
        ),
    )
}
