pub mod discipline;
pub mod holy;
pub mod shadow;

use crate::{
    Seq,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::priest::{INNER_FIRE, INNER_FOCUS, POWER_WORD_FORTITUDE},
    engine::{
        bt::Bt::{self, CastOnSelf, StrategyEnabled, InCombat},
        macro_fsm::ActiveFsm,
    },
    noncombat::GroupBuff,
};

const BUFFS: &[GroupBuff] = &[
    GroupBuff::on_party(POWER_WORD_FORTITUDE),
    GroupBuff::on_self(INNER_FIRE),
];

/// `co +boost` burst subtree — priest offensive cooldowns.
pub fn boost() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::BOOST),
        InCombat,
        CastOnSelf(INNER_FOCUS),
    )
}

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{PriestHoly, PriestDiscipline, PriestShadow};
    let combat = match spec {
        PriestHoly => holy::build_tree(ActiveFsm::Combat),
        PriestDiscipline => discipline::build_tree(ActiveFsm::Combat),
        PriestShadow => shadow::build_tree(ActiveFsm::Combat),
        _ => unreachable!("non-priest spec passed to priest::kit"),
    };
    let world = match spec {
        PriestHoly => holy::build_tree(ActiveFsm::World),
        PriestDiscipline => discipline::build_tree(ActiveFsm::World),
        PriestShadow => shadow::build_tree(ActiveFsm::World),
        _ => unreachable!("non-priest spec passed to priest::kit"),
    };
    ClassKit { combat, world, buffs: BUFFS }
}
