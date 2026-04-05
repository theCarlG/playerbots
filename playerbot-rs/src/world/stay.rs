/// Stay mode — hold position, fight if attacked.
use crate::engine::bt::Bt::{self, *};
use crate::Seq;

pub fn stay_subtree() -> Bt {
    Seq!(InCombat.not(), StopMoving)
}
