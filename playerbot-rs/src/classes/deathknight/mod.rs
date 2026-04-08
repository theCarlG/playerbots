pub mod blood;
pub mod frost;
pub mod unholy;

use crate::{Sel, Seq};
use crate::{
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    engine::{
        bt::Bt::{self, StrategyEnabled, InCombat},
        macro_fsm::ActiveFsm,
    },
    noncombat::GroupBuff,
};

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

pub fn build_tree(fsm: ActiveFsm, spec: PlayerSpec) -> Bt {
    use PlayerSpec::{DeathKnightBlood, DeathKnightFrost, DeathKnightUnholy};
    match spec {
        DeathKnightBlood => blood::build_tree(fsm),
        DeathKnightFrost => frost::build_tree(fsm),
        DeathKnightUnholy => unholy::build_tree(fsm),
        _ => unreachable!("non-deathknight spec passed to deathknight::build_tree"),
    }
}

pub fn buffs(_spec: PlayerSpec) -> &'static [GroupBuff] {
    BUFFS
}

/// Per-spec default combat strategy flags (PB2 `AiFactory.cpp` DK branch).
pub fn default_strategies(spec: PlayerSpec) -> StrategyFlags {
    use PlayerSpec::{DeathKnightBlood, DeathKnightFrost, DeathKnightUnholy};
    use StrategyFlags as F;
    let common = F::DKSQUEST | F::DPS_ASSIST | F::FLEE | F::CLOSE | F::CC;
    match spec {
        DeathKnightBlood => F::BLOOD | F::TANK | F::TANK_ASSIST | F::PULL | F::PULL_BACK | common,
        DeathKnightFrost => F::FROST | F::FROST_AOE | common,
        DeathKnightUnholy => F::UNHOLY | F::UNHOLY_AOE | common,
        _ => F::NONE,
    }
}

/// Reverse-map strategy flags to a death knight `PlayerSpec`.
pub fn spec_from_flags(flags: StrategyFlags) -> Option<PlayerSpec> {
    use PlayerSpec::{DeathKnightBlood, DeathKnightFrost, DeathKnightUnholy};
    use StrategyFlags as F;
    if flags.contains(F::BLOOD) {
        return Some(DeathKnightBlood);
    }
    if flags.contains(F::FROST) {
        return Some(DeathKnightFrost);
    }
    if flags.contains(F::UNHOLY) {
        return Some(DeathKnightUnholy);
    }
    None
}
