/// RPG mode — wander town, interact with NPCs, emote.
///
/// Low-intensity behavior for idle bots that should look like real players.
use crate::engine::bt::Bt::{self, *};
use crate::{Seq, Sel};

pub fn rpg_subtree() -> Bt {
    Seq!(
        InCombat.not(),
        Sel!(
            // Interact with a nearby gossip NPC.
            Bt::throttle(30_000, RpgInteractNpc),
            // Emote occasionally.
            Bt::throttle(60_000, RpgEmote),
            // Wander to a random nearby point.
            Bt::throttle(10_000, RpgWander),
            // Stand still if nothing to do.
            StopMoving,
        ),
    )
}
