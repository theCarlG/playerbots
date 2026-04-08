use crate::{Sel, Seq};
/// Holy Priest behavior tree (Classic / Vanilla).
///
/// Priority: Power Word: Shield → Fade (aggro) → PW:Shield on group →
///   Flash Heal (critical) → Greater Heal → Renew → Prayer of Healing (`AoE`)
use crate::{
    data::spells::vanilla::priest::*,
    engine::bt::{
        Bt::{self, *},
        Op::*,
        Resource::*,
    },
    engine::macro_fsm::ActiveFsm,
    ffi::SpellId,
};

const WEAKENED_SOUL: SpellId = SpellId(6788);

pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => combat_tree(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}

fn combat_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (priest-wide list).
        super::boost(),
        // Shield self when shield usable (no PW:S, no Weakened Soul).
        Seq!(
            Cmp(SelfHealthPct, Below(50)),
            Bt::self_missing(POWER_WORD_SHIELD),
            Bt::self_missing(WEAKENED_SOUL),
            CastOnSelf(POWER_WORD_SHIELD),
        ),
        // Fade if being attacked.
        Seq!(Cmp(AttackerCount, AtLeast(1)), CastOnSelf(FADE)),
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
        Seq!(
            GroupMembersBelow(3, 0.70),
            HealInjuredParty(PRAYER_OF_HEALING, 0.70),
        ),
    )
}
