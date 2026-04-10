use crate::engine::bt::Bt;

/// Mark RTI strategy — tank/lead marks the current target with skull
/// (kill order) on a 3-second throttle so every group member's RTI
/// assist leaf can converge on the same mob.
///
/// PB2: `MarkRtiStrategy` — wired into the tank-assist flow.
pub fn build() -> Bt {
    // Default icon: skull (7, 0-indexed).
    Bt::throttle(3_000, Bt::MarkRti(7))
}

/// Mark a specific icon (0..=7). Used by strategy builders that need
/// a non-skull mark (e.g. CC assist icon).
pub fn build_with_icon(icon: u8) -> Bt {
    Bt::throttle(3_000, Bt::MarkRti(icon))
}
