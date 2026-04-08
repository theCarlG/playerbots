pub mod blood;
pub mod frost;
pub mod unholy;

use crate::{
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    classes::ClassKit,
    engine::bt::Bt::{self, *},
    noncombat::GroupBuff,
};
use crate::{Seq, Sel};

// Death Knights have no persistent party buffs.
const BUFFS: &[GroupBuff] = &[];

/// `co +boost` burst subtree — DK cooldowns. Currently empty since the
/// spell data module doesn't include Empower Rune Weapon / Unbreakable
/// Armor / etc. yet. Keeping the structure so adding them is trivial.
pub fn boost() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::BOOST),
        InCombat,
        Sel!(
            // Placeholder: add Empower Rune Weapon, Unbreakable Armor,
            // Gargoyle etc. when spell constants are added.
            Bt::Noop,
        ),
    )
}

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
