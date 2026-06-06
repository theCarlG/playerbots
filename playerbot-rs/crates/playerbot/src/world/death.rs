/// Death handling — wait for rez, corpse run, spirit healer.
///
/// Runs as highest-priority node in the root BT. When the bot is dead,
/// nothing else should execute.
///
/// PB2 behavior (`ReleaseSpiritAction.h:78-119`):
/// - If in party with alive members: wait ~30s for resurrection before releasing.
/// - Spirit healer: only after many deaths, or dead > 10 min.
/// - Accept pending resurrect immediately.
use crate::engine::bt::Bt::{self, RecordDeathTime, AcceptResurrect, HasAliveGroupMember, DeadForLessThan, ReleaseSpirit, ReclaimCorpse, CorpseRun, UseSpiritHealer};
use crate::{Sel, Seq};

pub fn death_subtree() -> Bt {
    // Called from the root FSM's Dead state — IsAlive.not() is already
    // guaranteed by the caller. This subtree always succeeds (Noop fallback).
    Seq!(
        // Record when we died (idempotent — only sets if not already set).
        RecordDeathTime,
        Sel!(
            // 1. Accept pending resurrect from another player — always first.
            AcceptResurrect,
            // 2. If in a group with alive members, wait up to 30s for rez
            //    before releasing spirit. This matches PB2 behavior where
            //    bots wait for a player healer to resurrect them.
            Seq!(
                HasAliveGroupMember,
                DeadForLessThan(30_000),
                Bt::Noop, // Consume the tick — do nothing, just wait
            ),
            // 3. Release the spirit if we haven't yet — become a ghost at the
            //    graveyard so a corpse exists to run back to. (Failure once
            //    we're already a ghost, so we fall through to the run/reclaim.)
            ReleaseSpirit,
            // 4. Standing on the corpse? Reclaim it (resurrect, no sickness).
            ReclaimCorpse,
            // 5. Otherwise corpse-run: move the ghost toward its body.
            Bt::throttle(3_000, CorpseRun),
            // 6. Spirit healer as last resort (long dead / corpse unreachable).
            Bt::throttle(30_000, UseSpiritHealer),
            // 7. Fallback: idle. The root FSM ensures no other state runs.
            Bt::Noop,
        ),
    )
}
