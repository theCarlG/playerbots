pub mod arms;
pub mod fury;
pub mod protection;

use crate::{
    bot::state::PlayerSpec, classes::ClassKit, data::spells::vanilla::warrior::BATTLE_SHOUT,
    noncombat::GroupBuff,
};

/// All warrior specs maintain Battle Shout.
const BUFFS: &[GroupBuff] = &[GroupBuff::on_party(BATTLE_SHOUT)];

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{WarriorArms, WarriorFury, WarriorProtection};
    let tree = match spec {
        WarriorArms => arms::build_tree(),
        WarriorFury => fury::build_tree(),
        WarriorProtection => protection::build_tree(),
        _ => unreachable!("non-warrior spec passed to warrior::kit"),
    };
    ClassKit { tree, buffs: BUFFS }
}
