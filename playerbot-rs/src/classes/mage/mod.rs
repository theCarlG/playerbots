pub mod arcane;
pub mod fire;
pub mod frost;

use crate::{
    Sel, Seq,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    data::spells::vanilla::mage::{
        ARCANE_BRILLIANCE, ARCANE_POWER, COMBUSTION, PRESENCE_OF_MIND,
    },
    engine::{
        aura_helpers::ARCANE_INTELLECT_RANKS,
        bt::Bt::{self, CastOnSelf, InCombat, StrategyEnabled},
        macro_fsm::ActiveFsm,
    },
    noncombat::GroupBuff,
};

// Cast Arcane Brilliance to apply Arcane Intellect aura to whole party.
// Check all AI ranks + Arcane Brilliance itself.
const BUFFS: &[GroupBuff] = &[GroupBuff::on_party_aura(
    ARCANE_BRILLIANCE,
    ARCANE_INTELLECT_RANKS,
)];

/// `co +boost` burst subtree — mage offensive cooldowns.
pub fn boost() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::BOOST),
        InCombat,
        Sel!(
            CastOnSelf(COMBUSTION),
            CastOnSelf(ARCANE_POWER),
            CastOnSelf(PRESENCE_OF_MIND),
        ),
    )
}

pub fn build_tree(fsm: ActiveFsm, spec: PlayerSpec) -> Bt {
    use PlayerSpec::{MageArcane, MageFire, MageFrost};
    match spec {
        MageArcane => arcane::build_tree(fsm),
        MageFire => fire::build_tree(fsm),
        MageFrost => frost::build_tree(fsm),
        _ => unreachable!("non-mage spec passed to mage::build_tree"),
    }
}

pub fn buffs(_spec: PlayerSpec) -> &'static [GroupBuff] {
    BUFFS
}

/// Per-spec default combat strategy flags (PB2 `AiFactory.cpp` mage branch).
pub fn default_strategies(spec: PlayerSpec) -> StrategyFlags {
    use PlayerSpec::{MageArcane, MageFire, MageFrost};
    use StrategyFlags as F;
    let common =
        F::DPS_ASSIST | F::FLEE | F::CURE | F::RANGED | F::CC | F::BUFF | F::AOE | F::BOOST;
    match spec {
        MageArcane => F::ARCANE | common,
        MageFire => F::FIRE | common,
        MageFrost => F::FROST | common,
        _ => F::NONE,
    }
}

/// Reverse-map strategy flags to a mage `PlayerSpec`.
pub fn spec_from_flags(flags: StrategyFlags) -> Option<PlayerSpec> {
    use PlayerSpec::{MageArcane, MageFire, MageFrost};
    use StrategyFlags as F;
    if flags.contains(F::ARCANE) {
        return Some(MageArcane);
    }
    if flags.contains(F::FIRE) {
        return Some(MageFire);
    }
    if flags.contains(F::FROST) {
        return Some(MageFrost);
    }
    None
}
