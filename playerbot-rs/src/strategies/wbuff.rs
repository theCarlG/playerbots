use crate::{Sel, Seq};
use crate::bot::settings::StrategyFlags;
use crate::engine::bt::Bt;

/// World buff strategy — apply missing world buffs from config, then optionally
/// travel to world buff locations.
/// PB2: `WBuffStrategy` — gated on the `wbuff` strategy flag.
pub fn build() -> Bt {
    Seq!(
        Bt::StrategyEnabled(StrategyFlags::WBUFF),
        Sel!(
            // First: apply any missing config-driven world buffs directly.
            Bt::ApplyWorldBuffs,
            // Fallback: travel to world buff location if travel is enabled.
            Bt::TravelToBlackboard,
        ),
    )
}
