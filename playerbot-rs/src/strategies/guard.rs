use crate::engine::bt::Bt;
use crate::Sel;

/// Guard strategy — stay near a fixed position, return if displaced.
/// PB2: `GuardStrategy` — bot holds a guard point and returns to it
/// after combat or displacement.
pub fn build() -> Bt {
    Sel!(Bt::GuardReturn, Bt::StopMoving)
}
