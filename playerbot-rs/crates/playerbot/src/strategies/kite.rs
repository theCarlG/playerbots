use crate::bot::settings::StrategyFlags;
use crate::engine::bt::{Bt, Op, Resource};
use crate::Seq;

/// Kite strategy — move away from target when it's in melee range.
/// Used by ranged DPS to maintain distance. The default kite distance
/// is 8 yards (melee range + safety margin).
///
/// PB2: `KiteStrategy` — gated on the `kite` strategy flag and fires
/// when an attacker is within 8 yards.
pub fn build() -> Bt {
    build_at(8.0)
}

/// Kite at a custom distance threshold.
pub fn build_at(dist: f32) -> Bt {
    Seq!(
        Bt::StrategyEnabled(StrategyFlags::CLOSE), // reusing CLOSE flag for kite
        Bt::Cmp(Resource::AttackerCount, Op::Above(0)),
        Bt::KiteFromTarget(dist),
    )
}
