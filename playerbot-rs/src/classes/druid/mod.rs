pub mod balance;
pub mod feral;
pub mod restoration;

use crate::{
    Seq, Sel,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::druid::{NATURE_SWIFTNESS, TIGERS_FURY},
    engine::bt::Bt::{self, CastOnSelf, StrategyEnabled, InCombat},
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

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{DruidBalance, DruidFeral, DruidRestoration};
    let tree = match spec {
        DruidBalance => balance::build_tree(),
        DruidFeral => feral::build_tree(),
        DruidRestoration => restoration::build_tree(),
        _ => unreachable!("non-druid spec passed to druid::kit"),
    };
    ClassKit { tree, buffs: BUFFS }
}
