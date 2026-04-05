/// Gathering — mining, herbalism, skinning.
///
/// Finds nearby gatherable nodes and collects them while out of combat.
/// Runs as part of the maintenance subtree if the bot has a gathering skill.
use crate::engine::bt::Bt::{self, *};
use crate::Seq;

pub fn gather_subtree() -> Bt {
    Seq!(
        InCombat.not(),
        HasGatheringSkill,
        Bt::throttle(5_000, GatherNode),
    )
}
