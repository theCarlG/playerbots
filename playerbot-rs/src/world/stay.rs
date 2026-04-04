/// Stay mode — hold position, fight if attacked.
use crate::engine::bt::Bt::{self, *};

pub fn stay_subtree() -> Bt {
    Seq(vec![InCombat.not(), StopMoving])
}
