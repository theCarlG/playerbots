pub mod holy;
pub mod prefs;
pub mod protection;
pub mod retribution;

use crate::{
    Seq,
    bot::settings::StrategyFlags,
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::paladin::{
        BLESSING_OF_KINGS, BLESSING_OF_MIGHT, BLESSING_OF_WISDOM, DEVOTION_AURA, DIVINE_FAVOR,
        RETRIBUTION_AURA,
    },
    engine::{
        bt::Bt::{self, CastOnSelf, StrategyEnabled, InCombat},
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

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{PaladinHoly, PaladinProtection, PaladinRetribution};
    match spec {
        PaladinHoly => ClassKit {
            combat: holy::build_tree(ActiveFsm::Combat),
            world: holy::build_tree(ActiveFsm::World),
            buffs: HOLY_BUFFS,
        },
        PaladinProtection => ClassKit {
            combat: protection::build_tree(ActiveFsm::Combat),
            world: protection::build_tree(ActiveFsm::World),
            buffs: PROT_BUFFS,
        },
        PaladinRetribution => ClassKit {
            combat: retribution::build_tree(ActiveFsm::Combat),
            world: retribution::build_tree(ActiveFsm::World),
            buffs: RET_BUFFS,
        },
        _ => unreachable!("non-paladin spec passed to paladin::kit"),
    }
}
