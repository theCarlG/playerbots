pub mod holy;
pub mod prefs;
pub mod protection;
pub mod retribution;

use crate::{
    Seq,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    data::spells::vanilla::paladin::{
        BLESSING_OF_KINGS, BLESSING_OF_MIGHT, BLESSING_OF_WISDOM, DEVOTION_AURA, DIVINE_FAVOR,
        RETRIBUTION_AURA,
    },
    engine::{
        bt::Bt::{self, CastOnSelf, InCombat, StrategyEnabled},
        macro_fsm::ActiveFsm,
    },
    noncombat::GroupBuff,
};

/// `co +boost` burst subtree — paladin offensive cooldowns.
pub fn boost() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::BOOST),
        InCombat,
        CastOnSelf(DIVINE_FAVOR),
    )
}

const HOLY_BUFFS: &[GroupBuff] = &[
    GroupBuff::on_party(BLESSING_OF_WISDOM),
    GroupBuff::on_self(DEVOTION_AURA),
];

const PROT_BUFFS: &[GroupBuff] = &[
    GroupBuff::on_party(BLESSING_OF_KINGS),
    GroupBuff::on_self(DEVOTION_AURA),
];

const RET_BUFFS: &[GroupBuff] = &[
    GroupBuff::on_party(BLESSING_OF_MIGHT),
    GroupBuff::on_self(RETRIBUTION_AURA),
];

pub fn build_tree(fsm: ActiveFsm, spec: PlayerSpec) -> Bt {
    use PlayerSpec::{PaladinHoly, PaladinProtection, PaladinRetribution};
    match spec {
        PaladinHoly => holy::build_tree(fsm),
        PaladinProtection => protection::build_tree(fsm),
        PaladinRetribution => retribution::build_tree(fsm),
        _ => unreachable!("non-paladin spec passed to paladin::build_tree"),
    }
}

pub fn buffs(spec: PlayerSpec) -> &'static [GroupBuff] {
    use PlayerSpec::{PaladinHoly, PaladinProtection, PaladinRetribution};
    match spec {
        PaladinHoly => HOLY_BUFFS,
        PaladinProtection => PROT_BUFFS,
        PaladinRetribution => RET_BUFFS,
        _ => &[],
    }
}

/// Per-spec default combat strategy flags (PB2 `AiFactory.cpp` paladin branch).
pub fn default_strategies(spec: PlayerSpec) -> StrategyFlags {
    use PlayerSpec::{PaladinProtection, PaladinHoly, PaladinRetribution};
    use StrategyFlags as F;
    match spec {
        PaladinProtection => {
            F::PROTECTION
                | F::TANK
                | F::TANK_ASSIST
                | F::PULL
                | F::PULL_BACK
                | F::CLOSE
                | F::CURE
                | F::AOE
                | F::CC
                | F::BUFF
                | F::BOOST
                | F::AURA
                | F::BLESSING
        }
        PaladinHoly => {
            F::HOLY
                | F::OFFDPS
                | F::DPS_ASSIST
                | F::FLEE
                | F::RANGED
                | F::CURE
                | F::AOE
                | F::CC
                | F::BUFF
                | F::BOOST
                | F::AURA
                | F::BLESSING
        }
        PaladinRetribution => {
            F::RETRIBUTION
                | F::OFFHEAL
                | F::DPS_ASSIST
                | F::CLOSE
                | F::CURE
                | F::AOE
                | F::CC
                | F::BUFF
                | F::BOOST
                | F::AURA
                | F::BLESSING
        }
        _ => F::NONE,
    }
}

/// Reverse-map strategy flags to a paladin `PlayerSpec`.
pub fn spec_from_flags(flags: StrategyFlags) -> Option<PlayerSpec> {
    use PlayerSpec::{PaladinProtection, PaladinHoly, PaladinRetribution};
    use StrategyFlags as F;
    if flags.contains(F::PROTECTION) {
        return Some(PaladinProtection);
    }
    if flags.contains(F::HOLY) {
        return Some(PaladinHoly);
    }
    if flags.contains(F::RETRIBUTION) {
        return Some(PaladinRetribution);
    }
    None
}
