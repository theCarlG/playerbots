use crate::{Sel, Seq};
/// Discipline Priest behavior tree (Classic / Vanilla).
///
/// Hybrid spec — proactive PW:Shield and Inner Fire, heals like Holy with more mitigation.
use crate::{
    data::spells::vanilla::priest::*,
    engine::bt::{
        Bt::{self, *},
        Op::*,
        Resource::*,
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
        // `co +boost` burst cooldowns (priest-wide list).
        super::boost(),
        // Maintain Inner Fire.
        Seq!(Bt::self_missing(INNER_FIRE), CastOnSelf(INNER_FIRE)),
        // Fade aggro dump.
        Seq!(Cmp(AttackerCount, AtLeast(1)), CastOnSelf(FADE)),
        // Proactive shield on self when shield usable.
        Seq!(
            Cmp(SelfHealthPct, Below(80)),
            Bt::self_missing(POWER_WORD_SHIELD),
            CastOnSelf(POWER_WORD_SHIELD),
        ),
        // Critical heals.
        HealLowest(FLASH_HEAL, 0.40),
        HealInjuredParty(FLASH_HEAL, 0.40),
        // Sustained heals.
        HealLowest(GREATER_HEAL, 0.65),
        HealInjuredParty(GREATER_HEAL, 0.65),
        // HoT top-off.
        HealInjuredParty(RENEW, 0.85),
        HealLowest(RENEW, 0.85),
    )
}
