/// RPG mode — wander town, interact with NPCs, emote.
///
/// Low-intensity behavior for idle bots that should look like real players.
use crate::engine::bt::Bt::{self, InCombat, RpgInteractNpc, RpgEmote, RpgWander, StopMoving};
use crate::{Sel, Seq};

pub fn rpg_subtree() -> Bt {
    Seq!(
        InCombat.not(),
        Sel!(
            // Interact with a nearby gossip NPC.
            Bt::throttle(30_000, RpgInteractNpc),
            // Emote occasionally.
            Bt::throttle(60_000, RpgEmote),
            // Wander to a random nearby point. Short re-roll gap (a few
            // seconds, not 10) so the bot strings legs together and walks
            // around continuously like a player, instead of taking one short
            // hop then standing still for ages (reads as "tapping W").
            Bt::throttle(3_000, RpgWander),
            // Stand still if nothing to do.
            StopMoving,
        ),
    )
}
