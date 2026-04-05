/// Travel — long-distance waypoint navigation via chained `move_to` calls.
///
/// Used by quest and grind modes to reach distant objectives.
use crate::engine::bt::Bt::{self, *};
use crate::Seq;

pub fn travel_subtree() -> Bt {
    Seq!(
        InCombat.not(),
        HasTravelDest,
        Bt::throttle(3_000, TravelToBlackboard),
    )
}
