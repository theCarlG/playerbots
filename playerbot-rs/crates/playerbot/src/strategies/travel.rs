use crate::bot::settings::StrategyFlags;
use crate::engine::bt::Bt;
use crate::{Sel, Seq};

/// Travel strategy — non-combat destination selection and navigation.
///
/// PB2: `TravelStrategy` — chooses quest/vendor/repair/grind destinations
/// and moves toward them when not in combat. Gated on the `travel`
/// strategy flag and the absence of follow/stay/guard modes.
pub fn build() -> Bt {
    Seq!(
        Bt::StrategyEnabled(StrategyFlags::TRAVEL),
        Bt::Not(Box::new(Bt::InCombat)),
        // Never world-travel from inside a dungeon/raid instance — every travel
        // destination is on the world map, so the bot would just path to the
        // instance entrance and back (the "stagger to the exit" bug).
        Bt::Not(Box::new(Bt::InInstance)),
        Sel!(
            // Cross-continent destination → board a boat/zeppelin. Falls
            // through for same-continent destinations.
            Seq!(Bt::HasTravelDest, Bt::CrossContinentTravel),
            // Far same-continent destination → fly: walk to the nearest flight
            // master, then take a taxi. While the flight is in progress TakeTaxi
            // returns Running so it does NOT fall through to TravelToBlackboard
            // (a `move_to` issued during a taxi flight fights the flight spline —
            // the bot "glides on the ground at high speed" / clips under the
            // map). Falls through to walking only for near dests / no route.
            Seq!(Bt::HasTravelDest, Bt::TakeTaxi),
            // If we already have a travel destination, keep walking.
            Seq!(Bt::HasTravelDest, Bt::TravelToBlackboard,),
            // Otherwise pick a new destination.
            Bt::throttle(5_000, Bt::ChooseTravelTarget),
        ),
    )
}
