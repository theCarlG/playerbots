use crate::{Sel, Seq};
/// Restoration Druid behavior tree (Classic / Vanilla).
///
/// Priority: Innervate (OOM) → Barkskin (damaged) → critical heals →
///   Tranquility (`AoE` panic) → `HoT` maintenance
use crate::{
    data::spells::vanilla::druid::*,
    engine::bt::{
        Bt::{self, IsClass, ResurrectParty, Cmp, CastOnSelf, HealLowest, HealInjuredParty, GroupMembersBelow},
        Op::{Below, AtLeast},
        Resource::{SelfManaPct, SelfHealthPct, AttackerCount},
    },
    engine::macro_fsm::ActiveFsm,
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
        // Rebirth (battle res) on a dead party member. Runs at the top of
        // the resto rotation so a combat wipe can still be partially
        // saved. `ResurrectParty` handles dead-target lookup, class
        // spell selection, and the in-combat allowance for Rebirth
        // specifically. It is also wired into the generic reactive
        // subtree as a fallback, but placing it here first gives the
        // resto druid priority over lower heal nodes when a teammate
        // has just died.
        Seq!(
            IsClass(crate::bot::state::PlayerClass::Druid),
            ResurrectParty
        ),
        // Innervate self when very low mana.
        Seq!(Cmp(SelfManaPct, Below(10)), CastOnSelf(INNERVATE)),
        // Barkskin when taking damage.
        Seq!(
            Cmp(SelfHealthPct, Below(40)),
            Cmp(AttackerCount, AtLeast(1)),
            CastOnSelf(BARKSKIN),
        ),
        // Critical heals.
        HealLowest(REGROWTH, 0.30),
        HealInjuredParty(REGROWTH, 0.30),
        HealLowest(HEALING_TOUCH, 0.35),
        HealInjuredParty(HEALING_TOUCH, 0.35),
        // Medium heals.
        HealLowest(REGROWTH, 0.60),
        HealInjuredParty(REGROWTH, 0.60),
        // Tranquility AoE (self-cast channel).
        Seq!(GroupMembersBelow(3, 0.60), CastOnSelf(TRANQUILITY)),
        // Rejuvenation HoT upkeep.
        HealInjuredParty(REJUVENATION, 0.90),
        HealLowest(REJUVENATION, 0.90),
    )
}
