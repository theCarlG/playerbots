pub mod arcane;
pub mod fire;
pub mod frost;

use crate::{
    Seq, Sel,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::mage::{
        ARCANE_BRILLIANCE, ARCANE_INTELLECT, ARCANE_POWER, COMBUSTION, PRESENCE_OF_MIND,
    },
    engine::bt::Bt::{self, CastOnSelf, StrategyEnabled, InCombat},
    noncombat::GroupBuff,
};

// Cast Arcane Brilliance to apply Arcane Intellect aura to whole party.
const BUFFS: &[GroupBuff] = &[GroupBuff::on_party_aura(
    ARCANE_BRILLIANCE,
    ARCANE_INTELLECT,
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

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{MageArcane, MageFire, MageFrost};
    let tree = match spec {
        MageArcane => arcane::build_tree(),
        MageFire => fire::build_tree(),
        MageFrost => frost::build_tree(),
        _ => unreachable!("non-mage spec passed to mage::kit"),
    };
    ClassKit { tree, buffs: BUFFS }
}
