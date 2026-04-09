use crate::bot::settings::StrategyFlags;
use crate::engine::bt::Bt;
use crate::Seq;

/// Behind strategy — stay behind the current target to avoid cleaves
/// and parries. Used by melee DPS (rogues, feral druids).
///
/// Gated on `InCombatFsm` so the MoveBehind chase command does NOT fire
/// out-of-combat. Without this gate the bot issues combat chase() calls
/// while the world tree tries to Follow, causing rogues to "glide away"
/// from the group as the two movement systems fight each other.
///
/// PB2: `BehindStrategy` — gated on the `behind` strategy flag.
pub fn build() -> Bt {
    Seq!(
        Bt::StrategyEnabled(StrategyFlags::BEHIND),
        Bt::InCombatFsm,
        Bt::throttle(1_000, Bt::MoveBehind(2.0)),
    )
}
