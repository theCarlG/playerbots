pub mod beast_mastery;
pub mod marksmanship;
pub mod survival;

use crate::{bot::state::PlayerSpec, classes::ClassKit, noncombat::GroupBuff};

const BUFFS: &[GroupBuff] = &[];

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
