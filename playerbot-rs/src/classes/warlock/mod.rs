pub mod affliction;
pub mod demonology;
pub mod destruction;
pub mod prefs;

use crate::{
    Seq,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::warlock::{DARK_PACT, DEMON_ARMOR},
    engine::{
        bt::Bt::{self, CastOnSelf, StrategyEnabled, InCombat},
        macro_fsm::ActiveFsm,
    },
    noncombat::GroupBuff,
};

const BUFFS: &[GroupBuff] = &[GroupBuff::on_self(DEMON_ARMOR)];

/// `co +boost` burst subtree — warlock offensive cooldowns.
pub fn boost() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::BOOST),
        InCombat,
        CastOnSelf(DARK_PACT),
    )
}

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{WarlockAffliction, WarlockDemonology, WarlockDestruction};
    let combat = match spec {
        WarlockAffliction => affliction::build_tree(ActiveFsm::Combat),
        WarlockDemonology => demonology::build_tree(ActiveFsm::Combat),
        WarlockDestruction => destruction::build_tree(ActiveFsm::Combat),
        _ => unreachable!("non-warlock spec passed to warlock::kit"),
    };
    let world = match spec {
        WarlockAffliction => affliction::build_tree(ActiveFsm::World),
        WarlockDemonology => demonology::build_tree(ActiveFsm::World),
        WarlockDestruction => destruction::build_tree(ActiveFsm::World),
        _ => unreachable!("non-warlock spec passed to warlock::kit"),
    };
    ClassKit { combat, world, buffs: BUFFS }
}
