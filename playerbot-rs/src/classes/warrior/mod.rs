pub mod arms;
pub mod fury;
pub mod prefs;
pub mod protection;

use crate::{
    Seq, Sel,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::warrior::{
        BATTLE_SHOUT, BERSERKER_RAGE, BLOODRAGE, DEATH_WISH, RECKLESSNESS,
    },
    engine::{
        bt::Bt::{self, CastOnSelf, StrategyEnabled, InCombat},
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

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{WarriorArms, WarriorFury, WarriorProtection};
    let combat = match spec {
        WarriorArms => arms::build_tree(ActiveFsm::Combat),
        WarriorFury => fury::build_tree(ActiveFsm::Combat),
        WarriorProtection => protection::build_tree(ActiveFsm::Combat),
        _ => unreachable!("non-warrior spec passed to warrior::kit"),
    };
    let world = match spec {
        WarriorArms => arms::build_tree(ActiveFsm::World),
        WarriorFury => fury::build_tree(ActiveFsm::World),
        WarriorProtection => protection::build_tree(ActiveFsm::World),
        _ => unreachable!("non-warrior spec passed to warrior::kit"),
    };
    ClassKit { combat, world, buffs: BUFFS }
}
