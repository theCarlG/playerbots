pub mod beast_mastery;
pub mod marksmanship;
pub mod prefs;
pub mod survival;

use crate::{
    Seq, Sel,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::hunter::{BESTIAL_WRATH, RAPID_FIRE},
    engine::{
        bt::Bt::{self, CastOnSelf, StrategyEnabled, InCombat},
        macro_fsm::ActiveFsm,
    },
    noncombat::GroupBuff,
};

const BUFFS: &[GroupBuff] = &[];

/// `co +boost` burst subtree — hunter offensive cooldowns.
pub fn boost() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::BOOST),
        InCombat,
        Sel!(CastOnSelf(RAPID_FIRE), CastOnSelf(BESTIAL_WRATH)),
    )
}

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{HunterBeastMastery, HunterMarksmanship, HunterSurvival};
    let combat = match spec {
        HunterBeastMastery => beast_mastery::build_tree(ActiveFsm::Combat),
        HunterMarksmanship => marksmanship::build_tree(ActiveFsm::Combat),
        HunterSurvival => survival::build_tree(ActiveFsm::Combat),
        _ => unreachable!("non-hunter spec passed to hunter::kit"),
    };
    let world = match spec {
        HunterBeastMastery => beast_mastery::build_tree(ActiveFsm::World),
        HunterMarksmanship => marksmanship::build_tree(ActiveFsm::World),
        HunterSurvival => survival::build_tree(ActiveFsm::World),
        _ => unreachable!("non-hunter spec passed to hunter::kit"),
    };
    ClassKit { combat, world, buffs: BUFFS }
}
