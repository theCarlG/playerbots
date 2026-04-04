pub mod balance;
pub mod feral;
pub mod restoration;

use crate::{
    bot::state::PlayerSpec,
    classes::ClassKit,
    ffi::SpellId,
    noncombat::GroupBuff,
};

// Mark of the Wild rank 7.
const MARK_OF_THE_WILD: SpellId = SpellId(9885);

const BUFFS: &[GroupBuff] = &[GroupBuff::on_party(MARK_OF_THE_WILD)];

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::*;
    let tree = match spec {
        DruidBalance     => balance::build_tree(),
        DruidFeral       => feral::build_tree(),
        DruidRestoration => restoration::build_tree(),
        _ => unreachable!("non-druid spec passed to druid::kit"),
    };
    ClassKit { tree, buffs: BUFFS }
}
