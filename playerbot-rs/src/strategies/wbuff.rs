use crate::{Sel, Seq};
use crate::bot::settings::StrategyFlags;
use crate::engine::bt::Bt;

/// World buff strategy — travel to world buff locations.
/// PB2: `WBuffStrategy` — gated on the `wbuff` strategy flag.
/// Stub until the travel subsystem (Step 5.6) lands.
pub fn build() -> Bt {
    Seq!(
        Bt::StrategyEnabled(StrategyFlags::WBUFF),
        // Placeholder — will be replaced by TravelToWorldBuff when
        // the travel subsystem is implemented.
        Bt::TravelToBlackboard,
    )
}
