pub mod affliction;
pub mod demonology;
pub mod destruction;
pub mod prefs;

use crate::{
    bot::state::PlayerSpec, classes::ClassKit, data::spells::vanilla::warlock::DEMON_ARMOR,
    noncombat::GroupBuff,
};

const BUFFS: &[GroupBuff] = &[GroupBuff::on_self(DEMON_ARMOR)];

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{WarlockAffliction, WarlockDemonology, WarlockDestruction};
    let tree = match spec {
        WarlockAffliction => affliction::build_tree(),
        WarlockDemonology => demonology::build_tree(),
        WarlockDestruction => destruction::build_tree(),
        _ => unreachable!("non-warlock spec passed to warlock::kit"),
    };
    ClassKit { tree, buffs: BUFFS }
}
