pub mod elemental;
pub mod enhancement;
pub mod imbues;
pub mod restoration;
pub mod totems;

use crate::{
    Sel, Seq,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    data::spells::vanilla::shaman::{ELEMENTAL_MASTERY, LIGHTNING_SHIELD, NATURE_SWIFTNESS},
    engine::{
        bt::Bt::{self, CastOnSelf, InCombat, StrategyEnabled},
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

pub fn build_tree(fsm: ActiveFsm, spec: PlayerSpec) -> Bt {
    use PlayerSpec::{ShamanElemental, ShamanEnhancement, ShamanRestoration};
    match spec {
        ShamanElemental => elemental::build_tree(fsm),
        ShamanEnhancement => enhancement::build_tree(fsm),
        ShamanRestoration => restoration::build_tree(fsm),
        _ => unreachable!("non-shaman spec passed to shaman::build_tree"),
    }
}

pub fn buffs(spec: PlayerSpec) -> &'static [GroupBuff] {
    use PlayerSpec::ShamanEnhancement;
    match spec {
        ShamanEnhancement => ENH_BUFFS,
        _ => NONE,
    }
}

/// Per-spec default combat strategy flags (PB2 `AiFactory.cpp` shaman branch).
pub fn default_strategies(spec: PlayerSpec) -> StrategyFlags {
    use PlayerSpec::{ShamanElemental, ShamanEnhancement, ShamanRestoration};
    use StrategyFlags as F;
    match spec {
        ShamanElemental => {
            F::ELEMENTAL
                | F::OFFHEAL
                | F::AOE
                | F::CC
                | F::FLEE
                | F::RANGED
                | F::DPS_ASSIST
                | F::CURE
                | F::TOTEMS
                | F::BUFF
                | F::BOOST
        }
        ShamanEnhancement => {
            F::ENHANCEMENT
                | F::OFFHEAL
                | F::AOE
                | F::CC
                | F::CLOSE
                | F::DPS_ASSIST
                | F::CURE
                | F::TOTEMS
                | F::BUFF
                | F::BOOST
        }
        ShamanRestoration => {
            F::RESTORATION
                | F::OFFDPS
                | F::FLEE
                | F::RANGED
                | F::DPS_ASSIST
                | F::CURE
                | F::TOTEMS
                | F::BUFF
                | F::BOOST
        }
        _ => F::NONE,
    }
}

/// Reverse-map strategy flags to a shaman `PlayerSpec`.
pub fn spec_from_flags(flags: StrategyFlags) -> Option<PlayerSpec> {
    use PlayerSpec::{ShamanElemental, ShamanEnhancement, ShamanRestoration};
    use StrategyFlags as F;
    if flags.contains(F::ELEMENTAL) {
        return Some(ShamanElemental);
    }
    if flags.contains(F::ENHANCEMENT) {
        return Some(ShamanEnhancement);
    }
    if flags.contains(F::RESTORATION) {
        return Some(ShamanRestoration);
    }
    None
}
