pub mod arms;
pub mod fury;
pub mod prefs;
pub mod protection;

use crate::{
    Sel, Seq,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    data::spells::vanilla::warrior::{
        BATTLE_SHOUT, BERSERKER_RAGE, BLOODRAGE, DEATH_WISH, RECKLESSNESS,
    },
    engine::{
        bt::Bt::{self, CastOnSelf, InCombat, StrategyEnabled},
        macro_fsm::ActiveFsm,
    },
    noncombat::GroupBuff,
};

/// All warrior specs maintain Battle Shout.
const BUFFS: &[GroupBuff] = &[GroupBuff::on_party(BATTLE_SHOUT)];

/// `co +boost` burst subtree — warrior offensive cooldowns, off-GCD. Fires
/// once per combat; each `CastOnSelf` fails on cooldown so subsequent ticks
/// fall through to the normal rotation.
pub fn boost() -> Bt {
    Seq!(
        StrategyEnabled(StrategyFlags::BOOST),
        InCombat,
        Sel!(
            CastOnSelf(RECKLESSNESS),
            CastOnSelf(DEATH_WISH),
            CastOnSelf(BERSERKER_RAGE),
            CastOnSelf(BLOODRAGE),
        ),
    )
}

/// Build the behavior tree for the given FSM state and spec.
///
/// `ActiveFsm` is a first-class parameter — the caller (init.rs) invokes
/// this once per FSM state to produce separate combat/world/dead trees.
pub fn build_tree(fsm: ActiveFsm, spec: PlayerSpec) -> Bt {
    use PlayerSpec::{WarriorArms, WarriorFury, WarriorProtection};
    match spec {
        WarriorArms => arms::build_tree(fsm),
        WarriorFury => fury::build_tree(fsm),
        WarriorProtection => protection::build_tree(fsm),
        _ => unreachable!("non-warrior spec passed to warrior::build_tree"),
    }
}

pub fn buffs(_spec: PlayerSpec) -> &'static [GroupBuff] {
    BUFFS
}

/// Per-spec default combat strategy flags (PB2 `AiFactory.cpp` warrior branch).
pub fn default_strategies(spec: PlayerSpec) -> StrategyFlags {
    use PlayerSpec::{WarriorProtection, WarriorArms, WarriorFury};
    use StrategyFlags as F;
    match spec {
        WarriorProtection => {
            F::PROTECTION
                | F::TANK
                | F::TANK_ASSIST
                | F::PULL
                | F::PULL_BACK
                | F::CLOSE
                | F::AOE
                | F::CC
                | F::BUFF
                | F::BOOST
        }
        WarriorArms => F::ARMS | F::DPS_ASSIST | F::BEHIND | F::AOE | F::CC | F::BUFF | F::BOOST,
        WarriorFury => F::FURY | F::DPS_ASSIST | F::BEHIND | F::AOE | F::CC | F::BUFF | F::BOOST,
        _ => F::NONE,
    }
}

/// Reverse-map strategy flags to a warrior `PlayerSpec`.
pub fn spec_from_flags(flags: StrategyFlags) -> Option<PlayerSpec> {
    use PlayerSpec::{WarriorArms, WarriorFury, WarriorProtection};
    use StrategyFlags as F;
    if flags.contains(F::ARMS) {
        return Some(WarriorArms);
    }
    if flags.contains(F::FURY) {
        return Some(WarriorFury);
    }
    if flags.contains(F::PROTECTION) {
        return Some(WarriorProtection);
    }
    None
}
