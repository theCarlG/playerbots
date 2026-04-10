/// Non-combat behavior — buff execution + consumables.
///
/// Follow, consumables, and looting are now handled by `engine::bt::Bt` variants.
/// This module owns only the buff execution machinery; per-class buff lists
/// live in `crate::classes::<class>::BUFFS` alongside the rotation code that
/// owns that knowledge.
pub mod buffing;
pub mod consumables;

pub use buffing::{BuffTarget, GroupBuff};
