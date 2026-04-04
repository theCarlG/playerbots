pub mod discipline;
pub mod holy;
pub mod shadow;

use crate::{
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::priest::{INNER_FIRE, POWER_WORD_FORTITUDE},
    noncombat::GroupBuff,
};

const BUFFS: &[GroupBuff] = &[
    GroupBuff::on_party(POWER_WORD_FORTITUDE),
    GroupBuff::on_self(INNER_FIRE),
];

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::*;
    let tree = match spec {
        PriestHoly       => holy::build_tree(),
        PriestDiscipline => discipline::build_tree(),
        PriestShadow     => shadow::build_tree(),
        _ => unreachable!("non-priest spec passed to priest::kit"),
    };
    ClassKit { tree, buffs: BUFFS }
}
