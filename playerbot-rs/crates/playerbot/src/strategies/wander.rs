use crate::engine::bt::Bt;

/// Wander strategy — move to random nearby points.
/// PB2: `WanderStrategy` — uses `MoveRandomAction` to explore.
pub fn build() -> Bt {
    Bt::throttle(5_000, Bt::RpgWander)
}
