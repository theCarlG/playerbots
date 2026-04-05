/// Grind mode — autonomously find and kill mobs.
///
/// Target selection: find level-appropriate, attackable mobs nearby.
use crate::engine::bt::Bt::{self, *};
use crate::Seq;

pub fn grind_subtree() -> Bt {
    Seq!(InCombat.not(), Bt::throttle(2_000, GrindTarget))
}
