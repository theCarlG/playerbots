/// Restoration Shaman behavior tree (Classic / Vanilla).
///
/// Priority: Mana Spring Totem → Nature's Swiftness panic → critical heals →
///   fast heals → Chain Heal when multiple hurt → sustained heals → Purge
use crate::{
    data::spells::vanilla::shaman::*,
    engine::bt::Bt::{self, *},
};

pub fn build_tree() -> Bt {
    Sel(vec![
        // Totem upkeep.
        Seq(vec![GroupSizeAtLeast(2), SelfMissingAura(MANA_SPRING_TOTEM),
                 CastOnSelf(MANA_SPRING_TOTEM)]),

        // Nature's Swiftness panic window for instant critical heal.
        Seq(vec![
            GroupMembersBelow(1, 0.20),
            SelfMissingAura(NATURE_SWIFTNESS),
            CastOnSelf(NATURE_SWIFTNESS),
        ]),

        // Critical heals.
        HealLowest(HEALING_WAVE, 0.30),
        HealInjuredParty(HEALING_WAVE, 0.30),

        // Fast heals.
        HealLowest(LESSER_HEALING_WAVE, 0.55),
        HealInjuredParty(LESSER_HEALING_WAVE, 0.55),

        // Chain Heal when multiple hurt.
        Seq(vec![GroupMembersBelow(2, 0.75), HealInjuredParty(CHAIN_HEAL, 0.75)]),

        // Sustained.
        HealLowest(HEALING_WAVE, 0.80),
        HealInjuredParty(HEALING_WAVE, 0.80),

        // Purge enemy buffs.
        Seq(vec![InCombat, CastOnTarget(PURGE)]),
    ])
}
