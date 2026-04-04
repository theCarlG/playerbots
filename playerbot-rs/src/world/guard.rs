/// Guard mode — stay near a position, fight hostiles in range.
use crate::engine::bt::Bt::{self, *};

pub fn guard_subtree() -> Bt {
    Seq(vec![
        InCombat.not(),
        Sel(vec![
            // Return to guard position if too far.
            Bt::throttle(2_000, GuardReturn),
            // Hold position.
            StopMoving,
        ]),
    ])
}
