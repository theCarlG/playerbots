/// Death handling — corpse run, accept resurrect, spirit healer.
///
/// Runs as highest-priority node in the root BT. When the bot is dead,
/// nothing else should execute.
use crate::engine::bt::Bt::{self, *};
use crate::{Seq, Sel};

pub fn death_subtree() -> Bt {
    Seq!(
        IsAlive.not(),
        Sel!(
            // Accept pending resurrect from another player.
            AcceptResurrect,
            // Corpse run — move toward corpse position.
            Bt::throttle(3_000, CorpseRun),
            // Spirit healer as last resort (try periodically).
            Bt::throttle(30_000, UseSpiritHealer),
        ),
    )
}
