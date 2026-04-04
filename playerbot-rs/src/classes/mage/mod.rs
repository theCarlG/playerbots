pub mod arcane;
pub mod fire;
pub mod frost;

use crate::{
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::mage::{ARCANE_BRILLIANCE, ARCANE_INTELLECT},
    noncombat::GroupBuff,
};

// Cast Arcane Brilliance to apply Arcane Intellect aura to whole party.
const BUFFS: &[GroupBuff] = &[GroupBuff::on_party_aura(
    ARCANE_BRILLIANCE,
    ARCANE_INTELLECT,
)];

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::*;
    let tree = match spec {
        MageArcane => arcane::build_tree(),
        MageFire => fire::build_tree(),
        MageFrost => frost::build_tree(),
        _ => unreachable!("non-mage spec passed to mage::kit"),
    };
    ClassKit { tree, buffs: BUFFS }
}
