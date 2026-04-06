use crate::{Sel, Seq};
use crate::bot::settings::StrategyFlags;
use crate::engine::bt::Bt;

/// Travel strategy — non-combat destination selection and navigation.
///
/// PB2: `TravelStrategy` — chooses quest/vendor/repair/grind destinations
/// and moves toward them when not in combat. Gated on the `travel`
/// strategy flag and the absence of follow/stay/guard modes.
pub fn build() -> Bt {
    Seq!(
        Bt::StrategyEnabled(StrategyFlags::TRAVEL),
        Bt::Not(Box::new(Bt::InCombat)),
        Sel!(
            // If we already have a travel destination, keep moving.
            Seq!(
                Bt::HasTravelDest,
                Bt::TravelToBlackboard,
            ),
            // Otherwise pick a new destination.
            Bt::throttle(5_000, Bt::ChooseTravelTarget),
        ),
    )
}
