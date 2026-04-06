use crate::{Sel, Seq};
use crate::bot::settings::StrategyFlags;
use crate::engine::bt::Bt;

/// Ranged strategy — maintain distance from the current target.
/// Used by ranged DPS/healers to stay out of melee/cleave range.
///
/// PB2: `RangedStrategy` — gated on the `ranged` strategy flag.
pub fn build() -> Bt {
    build_at(25.0)
}

/// Maintain a custom minimum range.
pub fn build_at(min_range: f32) -> Bt {
    Seq!(
        Bt::StrategyEnabled(StrategyFlags::RANGED),
        Bt::MaintainRange(min_range),
    )
}
