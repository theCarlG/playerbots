use crate::bot::settings::StrategyFlags;
use crate::engine::bt::Bt;
use crate::{Sel, Seq};

/// World buff strategy — apply missing world buffs from config, then optionally
/// travel to world buff locations.
/// PB2: `WBuffStrategy` — gated on the `wbuff` strategy flag.
///
/// Throttled at 30s: once buffs are applied they persist for a long time, so
/// there's no need to check every tick. This also prevents spam if the C++ side
/// has a one-tick delay before the aura is visible to `get_needed_world_buffs`.
pub fn build() -> Bt {
    Seq!(
        Bt::StrategyEnabled(StrategyFlags::WBUFF),
        Bt::throttle(
            30_000,
            Sel!(
                // First: apply any missing config-driven world buffs directly.
                Bt::ApplyWorldBuffs,
                // Fallback: travel to world buff location if travel is enabled.
                Bt::TravelToBlackboard,
            ),
        ),
    )
}
