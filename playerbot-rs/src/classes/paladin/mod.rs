pub mod holy;
pub mod prefs;
pub mod protection;
pub mod retribution;

use crate::{
    Seq,
    bot::settings::CombatOrder,
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::paladin::{
        BLESSING_OF_KINGS, BLESSING_OF_MIGHT, BLESSING_OF_WISDOM, DEVOTION_AURA, DIVINE_FAVOR,
        RETRIBUTION_AURA,
    },
    engine::bt::Bt::{self, CastOnSelf, CombatOrderHas, InCombat},
    noncombat::GroupBuff,
};

/// `co +boost` burst subtree — paladin offensive cooldowns.
pub fn boost() -> Bt {
    Seq!(
        CombatOrderHas(CombatOrder::BOOST),
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
            tree: holy::build_tree(),
            buffs: HOLY_BUFFS,
        },
        PaladinProtection => ClassKit {
            tree: protection::build_tree(),
            buffs: PROT_BUFFS,
        },
        PaladinRetribution => ClassKit {
            tree: retribution::build_tree(),
            buffs: RET_BUFFS,
        },
        _ => unreachable!("non-paladin spec passed to paladin::kit"),
    }
}
