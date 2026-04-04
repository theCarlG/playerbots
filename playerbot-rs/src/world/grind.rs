/// Grind mode — autonomously find and kill mobs.
///
/// Target selection: find level-appropriate, attackable mobs nearby.
use crate::engine::bt::Bt::{self, Seq, InCombat, GrindTarget};

pub fn grind_subtree() -> Bt {
    Seq(vec![InCombat.not(), Bt::throttle(2_000, GrindTarget)])
}
