pub mod discipline;
pub mod holy;
pub mod shadow;

use crate::{
    Seq,
    bot::settings::CombatOrder,
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::priest::{INNER_FIRE, INNER_FOCUS, POWER_WORD_FORTITUDE},
    engine::bt::Bt::{self, CastOnSelf, CombatOrderHas, InCombat},
    noncombat::GroupBuff,
};

const BUFFS: &[GroupBuff] = &[
    GroupBuff::on_party(POWER_WORD_FORTITUDE),
    GroupBuff::on_self(INNER_FIRE),
];

/// `co +boost` burst subtree — priest offensive cooldowns.
pub fn boost() -> Bt {
    Seq!(
        CombatOrderHas(CombatOrder::BOOST),
        InCombat,
        CastOnSelf(INNER_FOCUS),
    )
}

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{PriestHoly, PriestDiscipline, PriestShadow};
    let tree = match spec {
        PriestHoly => holy::build_tree(),
        PriestDiscipline => discipline::build_tree(),
        PriestShadow => shadow::build_tree(),
        _ => unreachable!("non-priest spec passed to priest::kit"),
    };
    ClassKit { tree, buffs: BUFFS }
}
