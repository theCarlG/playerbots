pub mod beast_mastery;
pub mod marksmanship;
pub mod prefs;
pub mod survival;

use crate::{
    Sel, Seq,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    data::spells::vanilla::hunter::{BESTIAL_WRATH, RAPID_FIRE},
    engine::{
        bt::Bt::{self, CastOnSelf, InCombat, StrategyEnabled},
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

pub fn build_tree(fsm: ActiveFsm, spec: PlayerSpec) -> Bt {
    use PlayerSpec::{HunterBeastMastery, HunterMarksmanship, HunterSurvival};
    match spec {
        HunterBeastMastery => beast_mastery::build_tree(fsm),
        HunterMarksmanship => marksmanship::build_tree(fsm),
        HunterSurvival => survival::build_tree(fsm),
        _ => unreachable!("non-hunter spec passed to hunter::build_tree"),
    }
}

pub fn buffs(_spec: PlayerSpec) -> &'static [GroupBuff] {
    BUFFS
}

/// Per-spec default combat strategy flags (PB2 `AiFactory.cpp` hunter branch).
pub fn default_strategies(spec: PlayerSpec) -> StrategyFlags {
    use PlayerSpec::*;
    use StrategyFlags as F;
    let common = F::DPS_ASSIST
        | F::RANGED
        | F::CC
        | F::AOE
        | F::BUFF
        | F::BOOST
        | F::ASPECT
        | F::STING
        | F::PET;
    match spec {
        HunterBeastMastery => F::BEAST_MASTERY | common,
        HunterMarksmanship => F::MARKSMANSHIP | common,
        HunterSurvival => F::SURVIVAL | common,
        _ => F::NONE,
    }
}

/// Reverse-map strategy flags to a hunter `PlayerSpec`.
pub fn spec_from_flags(flags: StrategyFlags) -> Option<PlayerSpec> {
    use PlayerSpec::*;
    use StrategyFlags as F;
    if flags.contains(F::BEAST_MASTERY) {
        return Some(HunterBeastMastery);
    }
    if flags.contains(F::MARKSMANSHIP) {
        return Some(HunterMarksmanship);
    }
    if flags.contains(F::SURVIVAL) {
        return Some(HunterSurvival);
    }
    None
}
