/// Re-export the unified behavior tree enum from `engine::bt`.
///
/// All encounter modules import `Bt` through this path. The actual
/// implementation lives in `engine::bt` since `Bt` is used by both
/// encounter and world behavior code.
pub use crate::engine::bt::*;
