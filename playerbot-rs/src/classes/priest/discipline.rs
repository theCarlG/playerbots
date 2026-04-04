/// Discipline Priest behavior tree (Classic / Vanilla).
///
/// Hybrid spec — proactive PW:Shield and Inner Fire, heals like Holy with more mitigation.
use crate::{
    data::spells::vanilla::priest::*,
    engine::bt::Bt::{self, Sel, Seq, SelfMissingAura, CastOnSelf, AttackersAtLeast, HpBelow, HealLowest, HealInjuredParty},
};

pub fn build_tree() -> Bt {
    Sel(vec![
        // Maintain Inner Fire.
        Seq(vec![SelfMissingAura(INNER_FIRE), CastOnSelf(INNER_FIRE)]),
        // Fade aggro dump.
        Seq(vec![AttackersAtLeast(1), CastOnSelf(FADE)]),
        // Proactive shield on self when shield usable.
        Seq(vec![
            HpBelow(0.80),
            SelfMissingAura(POWER_WORD_SHIELD),
            CastOnSelf(POWER_WORD_SHIELD),
        ]),
        // Critical heals.
        HealLowest(FLASH_HEAL, 0.40),
        HealInjuredParty(FLASH_HEAL, 0.40),
        // Sustained heals.
        HealLowest(GREATER_HEAL, 0.65),
        HealInjuredParty(GREATER_HEAL, 0.65),
        // HoT top-off.
        HealInjuredParty(RENEW, 0.85),
        HealLowest(RENEW, 0.85),
    ])
}
