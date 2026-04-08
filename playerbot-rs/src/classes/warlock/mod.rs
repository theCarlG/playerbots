pub mod affliction;
pub mod demonology;
pub mod destruction;
pub mod prefs;

use crate::{
    Seq,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    data::spells::vanilla::warlock::{DARK_PACT, DEMON_ARMOR},
    engine::{
        bt::Bt::{self, CastOnSelf, InCombat, StrategyEnabled},
        macro_fsm::ActiveFsm,
    },
    noncombat::GroupBuff,
};

const BUFFS: &[GroupBuff] = &[GroupBuff::on_self(DEMON_ARMOR)];

/// `co +boost` burst subtree — warlock offensive cooldowns.
pub fn boost() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::BOOST),
        InCombat,
        CastOnSelf(DARK_PACT),
    )
}

pub fn build_tree(fsm: ActiveFsm, spec: PlayerSpec) -> Bt {
    use PlayerSpec::{WarlockAffliction, WarlockDemonology, WarlockDestruction};
    match spec {
        WarlockAffliction => affliction::build_tree(fsm),
        WarlockDemonology => demonology::build_tree(fsm),
        WarlockDestruction => destruction::build_tree(fsm),
        _ => unreachable!("non-warlock spec passed to warlock::build_tree"),
    }
}

pub fn buffs(_spec: PlayerSpec) -> &'static [GroupBuff] {
    BUFFS
}

/// Per-spec default combat strategy flags (PB2 `AiFactory.cpp` warlock branch).
pub fn default_strategies(spec: PlayerSpec) -> StrategyFlags {
    use PlayerSpec::*;
    use StrategyFlags as F;
    let common = F::DPS_ASSIST
        | F::FLEE
        | F::RANGED
        | F::CC
        | F::PET
        | F::AOE
        | F::BUFF
        | F::BOOST
        | F::CURSE;
    match spec {
        WarlockAffliction => F::AFFLICTION | common,
        WarlockDemonology => F::DEMONOLOGY | common,
        WarlockDestruction => F::DESTRUCTION | common,
        _ => F::NONE,
    }
}

/// Reverse-map strategy flags to a warlock `PlayerSpec`.
pub fn spec_from_flags(flags: StrategyFlags) -> Option<PlayerSpec> {
    use PlayerSpec::*;
    use StrategyFlags as F;
    if flags.contains(F::AFFLICTION) {
        return Some(WarlockAffliction);
    }
    if flags.contains(F::DEMONOLOGY) {
        return Some(WarlockDemonology);
    }
    if flags.contains(F::DESTRUCTION) {
        return Some(WarlockDestruction);
    }
    None
}
