pub mod discipline;
pub mod holy;
pub mod shadow;

use crate::{
    Seq,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    data::spells::vanilla::priest::{INNER_FIRE, INNER_FOCUS, POWER_WORD_FORTITUDE},
    engine::{
        bt::Bt::{self, CastOnSelf, InCombat, StrategyEnabled},
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

pub fn build_tree(fsm: ActiveFsm, spec: PlayerSpec) -> Bt {
    use PlayerSpec::{PriestDiscipline, PriestHoly, PriestShadow};
    match spec {
        PriestHoly => holy::build_tree(fsm),
        PriestDiscipline => discipline::build_tree(fsm),
        PriestShadow => shadow::build_tree(fsm),
        _ => unreachable!("non-priest spec passed to priest::build_tree"),
    }
}

pub fn buffs(_spec: PlayerSpec) -> &'static [GroupBuff] {
    BUFFS
}

/// Per-spec default combat strategy flags (PB2 `AiFactory.cpp` priest branch).
pub fn default_strategies(spec: PlayerSpec) -> StrategyFlags {
    use PlayerSpec::{PriestDiscipline, PriestHoly, PriestShadow};
    use StrategyFlags as F;
    let common =
        F::DPS_ASSIST | F::FLEE | F::CURE | F::RANGED | F::CC | F::BUFF | F::AOE | F::BOOST;
    match spec {
        PriestDiscipline => F::DISCIPLINE | F::OFFHEAL | common,
        PriestHoly => F::HOLY | F::OFFDPS | common,
        PriestShadow => F::SHADOW | F::OFFHEAL | common,
        _ => F::NONE,
    }
}

/// Reverse-map strategy flags to a priest `PlayerSpec`.
pub fn spec_from_flags(flags: StrategyFlags) -> Option<PlayerSpec> {
    use PlayerSpec::{PriestDiscipline, PriestHoly, PriestShadow};
    use StrategyFlags as F;
    if flags.contains(F::DISCIPLINE) {
        return Some(PriestDiscipline);
    }
    if flags.contains(F::HOLY) {
        return Some(PriestHoly);
    }
    if flags.contains(F::SHADOW) {
        return Some(PriestShadow);
    }
    None
}
