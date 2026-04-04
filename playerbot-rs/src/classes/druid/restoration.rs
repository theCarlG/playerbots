/// Restoration Druid behavior tree (Classic / Vanilla).
///
/// Priority: Innervate (OOM) → Barkskin (damaged) → critical heals →
///   Tranquility (`AoE` panic) → `HoT` maintenance
use crate::{
    data::spells::vanilla::druid::*,
    engine::bt::Bt::{self, Sel, Seq, ManaBelow, CastOnSelf, HpBelow, AttackersAtLeast, HealLowest, HealInjuredParty, GroupMembersBelow},
};

pub fn build_tree() -> Bt {
    Sel(vec![
        // TODO: Rebirth on dead party member (needs dead-target variant).
        // Innervate self when very low mana.
        Seq(vec![ManaBelow(0.10), CastOnSelf(INNERVATE)]),
        // Barkskin when taking damage.
        Seq(vec![
            HpBelow(0.40),
            AttackersAtLeast(1),
            CastOnSelf(BARKSKIN),
        ]),
        // Critical heals.
        HealLowest(REGROWTH, 0.30),
        HealInjuredParty(REGROWTH, 0.30),
        HealLowest(HEALING_TOUCH, 0.35),
        HealInjuredParty(HEALING_TOUCH, 0.35),
        // Medium heals.
        HealLowest(REGROWTH, 0.60),
        HealInjuredParty(REGROWTH, 0.60),
        // Tranquility AoE (self-cast channel).
        Seq(vec![GroupMembersBelow(3, 0.60), CastOnSelf(TRANQUILITY)]),
        // Rejuvenation HoT upkeep.
        HealInjuredParty(REJUVENATION, 0.90),
        HealLowest(REJUVENATION, 0.90),
    ])
}
