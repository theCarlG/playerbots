/// RPG behaviour — run town errands, visit NPCs, wander. Mirrors the old PB2
/// `RpgStrategy`: an idle bot in a town acts like a real player passing through
/// — repairing, selling junk, handing in quests, strolling between NPCs — and
/// only falls back to aimless wandering when there's nothing to do.
use crate::engine::bt::Bt::{self, InCombat, RpgEmote, RpgErrands, RpgVisitNpc, RpgWander, StopMoving};
use crate::{Sel, Seq};

pub fn rpg_subtree() -> Bt {
    Seq!(
        InCombat.not(),
        Sel!(
            // Productive business first — walk to a nearby vendor / repairer /
            // quest giver and do something useful. Throttled so a bot with
            // nothing to do isn't grid-scanning every tick, but commits to the
            // walk once it finds an errand (Running is throttle-transparent).
            Bt::throttle(5_000, RpgErrands),
            // Otherwise stroll over to a town NPC and open a chat — looks busy
            // and human. Holds its target until reached, so it's a continuous
            // walk, not a dither.
            Bt::throttle(12_000, RpgVisitNpc),
            // Occasional emote.
            Bt::throttle(60_000, RpgEmote),
            // Last resort: wander to a random nearby point. Short re-roll gap
            // (a few seconds, not 10) so the bot strings legs together and
            // walks around continuously like a player, instead of taking one
            // short hop then standing still for ages (reads as "tapping W").
            Bt::throttle(3_000, RpgWander),
            // Stand still if truly nothing to do.
            StopMoving,
        ),
    )
}
