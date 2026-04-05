pub mod assassination;
pub mod combat;
pub mod poisons;
pub mod subtlety;

use crate::{bot::state::PlayerSpec, classes::ClassKit, noncombat::GroupBuff};

const BUFFS: &[GroupBuff] = &[];

pub fn kit(spec: PlayerSpec) -> ClassKit {
    use PlayerSpec::{RogueAssassination, RogueCombat, RogueSubtlety};
    let tree = match spec {
        RogueAssassination => assassination::build_tree(),
        RogueCombat => combat::build_tree(),
        RogueSubtlety => subtlety::build_tree(),
        _ => unreachable!("non-rogue spec passed to rogue::kit"),
    };
    ClassKit { tree, buffs: BUFFS }
}
