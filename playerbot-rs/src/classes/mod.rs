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

/// Everything `init.rs` needs to assemble a bot's per-FSM trees for a given
/// (class, spec). Each class module exposes a `kit(spec) -> ClassKit`.
pub struct ClassKit {
    /// The class combat rotation BT (used when `ActiveFsm::Combat`).
    pub combat: Bt,
    /// Class-specific world behavior (used when `ActiveFsm::World`).
    /// Most classes return `Bt::Noop` here — generic world behavior
    /// (follow, grind, quest) is handled by `init.rs`.
    pub world: Bt,
    /// Persistent group buffs this spec maintains. `&'static` so buff lists
    /// are compile-time constants — no per-bot allocation.
    pub buffs: &'static [GroupBuff],
}
