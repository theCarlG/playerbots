pub mod beast_mastery;
pub mod marksmanship;
pub mod prefs;
pub mod survival;

use crate::{
    Seq, Sel,
    bot::settings::CombatOrder,
    bot::state::PlayerSpec,
    classes::ClassKit,
    data::spells::vanilla::hunter::{BESTIAL_WRATH, RAPID_FIRE},
    engine::bt::Bt::{self, CastOnSelf, CombatOrderHas, InCombat},
    noncombat::GroupBuff,
};

const BUFFS: &[GroupBuff] = &[];

/// `co +boost` burst subtree — hunter offensive cooldowns.
pub fn boost() -> Bt {
    Seq!(
        CombatOrderHas(CombatOrder::BOOST),
        InCombat,
        Sel!(CastOnSelf(RAPID_FIRE), CastOnSelf(BESTIAL_WRATH)),
    )
}

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{HunterBeastMastery, HunterMarksmanship, HunterSurvival};
    let tree = match spec {
        HunterBeastMastery => beast_mastery::build_tree(),
        HunterMarksmanship => marksmanship::build_tree(),
        HunterSurvival => survival::build_tree(),
        _ => unreachable!("non-hunter spec passed to hunter::kit"),
    };
    ClassKit { tree, buffs: BUFFS }
}
