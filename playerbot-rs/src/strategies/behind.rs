use crate::bot::settings::StrategyFlags;
use crate::engine::bt::Bt;
use crate::Seq;

/// Behind strategy — stay behind the current target to avoid cleaves
/// and parries. Used by melee DPS (rogues, feral druids).
///
/// PB2: `BehindStrategy` — gated on the `behind` strategy flag.
pub fn build() -> Bt {
    Seq!(
        Bt::StrategyEnabled(StrategyFlags::BEHIND),
        Bt::throttle(1_000, Bt::MoveBehind(2.0)),
    )
}
