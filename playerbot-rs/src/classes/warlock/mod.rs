pub mod affliction;
pub mod demonology;
pub mod destruction;
pub mod prefs;

use crate::{
    Seq,
    bot::settings::CombatOrder,
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::warlock::{DARK_PACT, DEMON_ARMOR},
    engine::bt::Bt::{self, CastOnSelf, CombatOrderHas, InCombat},
    noncombat::GroupBuff,
};

const BUFFS: &[GroupBuff] = &[GroupBuff::on_self(DEMON_ARMOR)];

/// `co +boost` burst subtree — warlock offensive cooldowns.
pub fn boost() -> Bt {
    Seq!(
        CombatOrderHas(CombatOrder::BOOST),
        InCombat,
        CastOnSelf(DARK_PACT),
    )
}

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{WarlockAffliction, WarlockDemonology, WarlockDestruction};
    let tree = match spec {
        WarlockAffliction => affliction::build_tree(),
        WarlockDemonology => demonology::build_tree(),
        WarlockDestruction => destruction::build_tree(),
        _ => unreachable!("non-warlock spec passed to warlock::kit"),
    };
    ClassKit { tree, buffs: BUFFS }
}
