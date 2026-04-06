use crate::engine::bt::Bt;

/// Stay strategy — hold position, do not move unless engaged.
/// PB2: `StayStrategy` — returns the bot to its stay position and stops.
pub fn build() -> Bt {
    Bt::StopMoving
}
