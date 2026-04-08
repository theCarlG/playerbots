pub mod assassination;
pub mod combat;
pub mod poisons;
pub mod subtlety;

use crate::{
    Seq, Sel,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::rogue::{ADRENALINE_RUSH, BLADE_FLURRY, COLD_BLOOD},
    engine::{
        bt::Bt::{self, CastOnSelf, StrategyEnabled, InCombat},
        macro_fsm::ActiveFsm,
    },
    noncombat::GroupBuff,
};

const BUFFS: &[GroupBuff] = &[];

/// `co +boost` burst subtree — rogue offensive cooldowns.
pub fn boost() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::BOOST),
        InCombat,
        Sel!(
            CastOnSelf(ADRENALINE_RUSH),
            CastOnSelf(BLADE_FLURRY),
            CastOnSelf(COLD_BLOOD),
        ),
    )
}

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{RogueAssassination, RogueCombat, RogueSubtlety};
    let combat = match spec {
        RogueAssassination => assassination::build_tree(ActiveFsm::Combat),
        RogueCombat => combat::build_tree(ActiveFsm::Combat),
        RogueSubtlety => subtlety::build_tree(ActiveFsm::Combat),
        _ => unreachable!("non-rogue spec passed to rogue::kit"),
    };
    let world = match spec {
        RogueAssassination => assassination::build_tree(ActiveFsm::World),
        RogueCombat => combat::build_tree(ActiveFsm::World),
        RogueSubtlety => subtlety::build_tree(ActiveFsm::World),
        _ => unreachable!("non-rogue spec passed to rogue::kit"),
    };
    ClassKit { combat, world, buffs: BUFFS }
}
