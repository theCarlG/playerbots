use crate::bot::settings::StrategyFlags;
use crate::engine::bt::Bt;
use crate::{Sel, Seq};

/// Close strategy — close to melee range on the current target.
/// Used by melee classes to re-engage after knockbacks or target swaps.
///
/// PB2: `CloseStrategy` — gated on the `close` strategy flag.
pub fn build() -> Bt {
    build_to(5.0)
}

/// Close to a custom distance.
pub fn build_to(dist: f32) -> Bt {
    Seq!(
        Bt::StrategyEnabled(StrategyFlags::CLOSE),
        Bt::CloseToTarget(dist),
    )
}
