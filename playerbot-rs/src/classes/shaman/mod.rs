pub mod elemental;
pub mod enhancement;
pub mod imbues;
pub mod restoration;
pub mod totems;

use crate::{
    Seq, Sel,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::shaman::{ELEMENTAL_MASTERY, LIGHTNING_SHIELD, NATURE_SWIFTNESS},
    engine::{
        bt::Bt::{self, CastOnSelf, StrategyEnabled, InCombat},
        macro_fsm::ActiveFsm,
    },
    noncombat::GroupBuff,
};

/// `co +boost` burst subtree — shaman offensive cooldowns.
pub fn boost() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::BOOST),
        InCombat,
        Sel!(CastOnSelf(ELEMENTAL_MASTERY), CastOnSelf(NATURE_SWIFTNESS)),
    )
}

// Only enhancement maintains a persistent self buff; ele/resto rely on
// situational totems handled inside their rotations.
const ENH_BUFFS: &[GroupBuff] = &[GroupBuff::on_self(LIGHTNING_SHIELD)];
const NONE: &[GroupBuff] = &[];

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{ShamanElemental, ShamanEnhancement, ShamanRestoration};
    match spec {
        ShamanElemental => ClassKit {
            combat: elemental::build_tree(ActiveFsm::Combat),
            world: elemental::build_tree(ActiveFsm::World),
            buffs: NONE,
        },
        ShamanEnhancement => ClassKit {
            combat: enhancement::build_tree(ActiveFsm::Combat),
            world: enhancement::build_tree(ActiveFsm::World),
            buffs: ENH_BUFFS,
        },
        ShamanRestoration => ClassKit {
            combat: restoration::build_tree(ActiveFsm::Combat),
            world: restoration::build_tree(ActiveFsm::World),
            buffs: NONE,
        },
        _ => unreachable!("non-shaman spec passed to shaman::kit"),
    }
}
