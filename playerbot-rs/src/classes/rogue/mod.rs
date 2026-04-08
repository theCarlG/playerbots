pub mod assassination;
pub mod combat;
pub mod poisons;
pub mod subtlety;

use crate::{
    Sel, Seq,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    data::spells::vanilla::rogue::{ADRENALINE_RUSH, BLADE_FLURRY, COLD_BLOOD},
    engine::{
        bt::Bt::{self, CastOnSelf, InCombat, StrategyEnabled},
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

pub fn build_tree(fsm: ActiveFsm, spec: PlayerSpec) -> Bt {
    use PlayerSpec::{RogueAssassination, RogueCombat, RogueSubtlety};
    match spec {
        RogueAssassination => assassination::build_tree(fsm),
        RogueCombat => combat::build_tree(fsm),
        RogueSubtlety => subtlety::build_tree(fsm),
        _ => unreachable!("non-rogue spec passed to rogue::build_tree"),
    }
}

pub fn buffs(_spec: PlayerSpec) -> &'static [GroupBuff] {
    BUFFS
}

/// Per-spec default combat strategy flags (PB2 `AiFactory.cpp` rogue branch).
pub fn default_strategies(spec: PlayerSpec) -> StrategyFlags {
    use PlayerSpec::*;
    use StrategyFlags as F;
    let common = F::DPS_ASSIST
        | F::AOE
        | F::CLOSE
        | F::CC
        | F::BEHIND
        | F::STEALTH
        | F::POISONS
        | F::BUFF
        | F::BOOST;
    match spec {
        RogueAssassination => F::ASSASSINATION | common,
        RogueCombat => F::ROGUE_COMBAT | common,
        RogueSubtlety => F::SUBTLETY | common,
        _ => F::NONE,
    }
}

/// Reverse-map strategy flags to a rogue `PlayerSpec`.
pub fn spec_from_flags(flags: StrategyFlags) -> Option<PlayerSpec> {
    use PlayerSpec::*;
    use StrategyFlags as F;
    if flags.contains(F::ASSASSINATION) {
        return Some(RogueAssassination);
    }
    if flags.contains(F::ROGUE_COMBAT) {
        return Some(RogueCombat);
    }
    if flags.contains(F::SUBTLETY) {
        return Some(RogueSubtlety);
    }
    None
}
