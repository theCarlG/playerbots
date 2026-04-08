pub mod balance;
pub mod feral;
pub mod restoration;

use crate::{
    Sel, Seq,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    data::spells::vanilla::druid::{NATURE_SWIFTNESS, TIGERS_FURY},
    engine::{
        bt::Bt::{self, CastOnSelf, InCombat, StrategyEnabled},
        macro_fsm::ActiveFsm,
    },
    ffi::SpellId,
    noncombat::GroupBuff,
};

// Mark of the Wild rank 7.
const MARK_OF_THE_WILD: SpellId = SpellId(9885);

const BUFFS: &[GroupBuff] = &[GroupBuff::on_party(MARK_OF_THE_WILD)];

/// `co +boost` burst subtree — druid offensive cooldowns.
pub fn boost() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::BOOST),
        InCombat,
        Sel!(CastOnSelf(TIGERS_FURY), CastOnSelf(NATURE_SWIFTNESS)),
    )
}

pub fn build_tree(fsm: ActiveFsm, spec: PlayerSpec) -> Bt {
    use PlayerSpec::{DruidBalance, DruidFeral, DruidRestoration};
    match spec {
        DruidBalance => balance::build_tree(fsm),
        DruidFeral => feral::build_tree(fsm),
        DruidRestoration => restoration::build_tree(fsm),
        _ => unreachable!("non-druid spec passed to druid::build_tree"),
    }
}

pub fn buffs(_spec: PlayerSpec) -> &'static [GroupBuff] {
    BUFFS
}

/// Per-spec default combat strategy flags (PB2 `AiFactory.cpp` druid branch).
pub fn default_strategies(spec: PlayerSpec) -> StrategyFlags {
    use PlayerSpec::{DruidBalance, DruidFeral, DruidRestoration};
    use StrategyFlags as F;
    match spec {
        DruidBalance => {
            F::BALANCE
                | F::OFFHEAL
                | F::DPS_ASSIST
                | F::FLEE
                | F::RANGED
                | F::CURE
                | F::AOE
                | F::CC
                | F::BUFF
                | F::BOOST
        }
        DruidFeral => {
            F::DPS_FERAL
                | F::OFFHEAL
                | F::DPS_ASSIST
                | F::CLOSE
                | F::BEHIND
                | F::CURE
                | F::AOE
                | F::CC
                | F::BUFF
                | F::BOOST
        }
        DruidRestoration => {
            F::RESTORATION
                | F::OFFDPS
                | F::DPS_ASSIST
                | F::FLEE
                | F::RANGED
                | F::CURE
                | F::AOE
                | F::CC
                | F::BUFF
                | F::BOOST
        }
        _ => F::NONE,
    }
}

/// Reverse-map strategy flags to a druid `PlayerSpec`.
pub fn spec_from_flags(flags: StrategyFlags) -> Option<PlayerSpec> {
    use PlayerSpec::{DruidBalance, DruidFeral, DruidRestoration};
    use StrategyFlags as F;
    if flags.contains(F::BALANCE) {
        return Some(DruidBalance);
    }
    if flags.contains(F::TANK_FERAL) {
        return Some(DruidFeral);
    }
    if flags.contains(F::DPS_FERAL) {
        return Some(DruidFeral);
    }
    if flags.contains(F::RESTORATION) {
        return Some(DruidRestoration);
    }
    None
}
