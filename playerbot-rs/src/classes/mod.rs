pub mod deathknight;
pub mod druid;
pub mod hunter;
pub mod mage;
pub mod paladin;
pub mod priest;
pub mod rogue;
pub mod shaman;
pub mod warlock;
pub mod warrior;

use crate::{engine::bt::Bt, noncombat::GroupBuff};

/// Everything `init.rs` needs to assemble a bot's root tree for a given
/// (class, spec). Each class module exposes a `kit(spec) -> ClassKit`.
pub struct ClassKit {
    /// The class rotation BT.
    pub tree: Bt,
    /// Persistent group buffs this spec maintains. `&'static` so buff lists
    /// are compile-time constants — no per-bot allocation.
    pub buffs: &'static [GroupBuff],
}
