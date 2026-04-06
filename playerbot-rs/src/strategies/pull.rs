use crate::{Sel, Seq};
use crate::bot::settings::StrategyFlags;
use crate::engine::bt::Bt;

/// Pull strategy — initiate combat with the current target using the
/// generic pull dispatch (auto-shoot → taunt → attack). Class-specific
/// pull abilities (e.g. Charge, Hunter's Mark) layer on top of this in
/// the class file, not here.
///
/// PB2: `PullStrategy` — gated on the `pull` strategy flag.
pub fn build() -> Bt {
    Seq!(
        Bt::StrategyEnabled(StrategyFlags::PULL),
        Bt::PullTarget,
    )
}
