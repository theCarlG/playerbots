use crate::bot::settings::StrategyFlags;
use crate::engine::bt::Bt;
use crate::{Sel, Seq};

/// Mount strategy — auto-mount when out of combat and appropriate.
/// PB2: `MountStrategy` — gated on `mount` strategy flag.
pub fn build() -> Bt {
    Seq!(
        Bt::StrategyEnabled(StrategyFlags::MOUNT),
        Bt::InCombat.not(),
        Bt::throttle(5_000, Bt::MountUp),
    )
}
