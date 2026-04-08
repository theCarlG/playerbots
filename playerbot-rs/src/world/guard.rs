/// Guard mode — stay near a position, fight hostiles in range.
use crate::engine::bt::Bt::{self, InCombat, GuardReturn, StopMoving};
use crate::{Sel, Seq};

pub fn guard_subtree() -> Bt {
    Seq!(
        InCombat.not(),
        Sel!(
            // Return to guard position if too far.
            Bt::throttle(2_000, GuardReturn),
            // Hold position.
            StopMoving,
        ),
    )
}
