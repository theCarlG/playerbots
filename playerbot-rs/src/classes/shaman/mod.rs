pub mod elemental;
pub mod enhancement;
pub mod restoration;

use crate::{
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::shaman::LIGHTNING_SHIELD,
    noncombat::GroupBuff,
};

// Only enhancement maintains a persistent self buff; ele/resto rely on
// situational totems handled inside their rotations.
const ENH_BUFFS: &[GroupBuff] = &[GroupBuff::on_self(LIGHTNING_SHIELD)];
const NONE:      &[GroupBuff] = &[];

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::*;
    match spec {
        ShamanElemental   => ClassKit { tree: elemental::build_tree(),   buffs: NONE },
        ShamanEnhancement => ClassKit { tree: enhancement::build_tree(), buffs: ENH_BUFFS },
        ShamanRestoration => ClassKit { tree: restoration::build_tree(), buffs: NONE },
        _ => unreachable!("non-shaman spec passed to shaman::kit"),
    }
}
