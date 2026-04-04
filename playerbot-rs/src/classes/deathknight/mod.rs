pub mod blood;
pub mod frost;
pub mod unholy;

use crate::{bot::state::PlayerSpec, classes::ClassKit, noncombat::GroupBuff};

// Death Knights have no persistent party buffs.
const BUFFS: &[GroupBuff] = &[];

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{DeathKnightBlood, DeathKnightFrost, DeathKnightUnholy};
    let tree = match spec {
        DeathKnightBlood => blood::build_tree(),
        DeathKnightFrost => frost::build_tree(),
        DeathKnightUnholy => unholy::build_tree(),
        _ => unreachable!("non-deathknight spec passed to deathknight::kit"),
    };
    ClassKit { tree, buffs: BUFFS }
}
