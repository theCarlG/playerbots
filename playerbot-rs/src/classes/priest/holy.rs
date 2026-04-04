/// Holy Priest behavior tree (Classic / Vanilla).
///
/// Priority: Power Word: Shield → Fade (aggro) → PW:Shield on group →
///   Flash Heal (critical) → Greater Heal → Renew → Prayer of Healing (AoE)
use crate::{
    data::spells::vanilla::priest::*,
    engine::bt::Bt::{self, *},
    ffi::SpellId,
};

const WEAKENED_SOUL: SpellId = SpellId(6788);

pub fn build_tree() -> Bt {
    Sel(vec![
        // Shield self when shield usable (no PW:S, no Weakened Soul).
        Seq(vec![
            HpBelow(0.50),
            SelfMissingAura(POWER_WORD_SHIELD),
            SelfMissingAura(WEAKENED_SOUL),
            CastOnSelf(POWER_WORD_SHIELD),
        ]),

        // Fade if being attacked.
        Seq(vec![AttackersAtLeast(1), CastOnSelf(FADE)]),

        // Critical heals.
        HealLowest(FLASH_HEAL, 0.35),
        HealInjuredParty(FLASH_HEAL, 0.35),

        // Sustained heals.
        HealLowest(GREATER_HEAL, 0.55),
        HealInjuredParty(GREATER_HEAL, 0.55),

        // HoT maintenance.
        HealInjuredParty(RENEW, 0.90),
        HealLowest(RENEW, 0.90),

        // Raid AoE heal.
        Seq(vec![
            GroupMembersBelow(3, 0.70),
            HealInjuredParty(PRAYER_OF_HEALING, 0.70),
        ]),
    ])
}
