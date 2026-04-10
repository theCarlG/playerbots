/// Battleground autonomous behavior.
///
/// Priority: fight nearby enemies > capture objectives > follow group.
use crate::engine::bt::Bt::{self, InCombat, InBattleground, BgAttackEnemy, BgCaptureObjective, Follow};
use crate::{Sel, Seq};

pub fn bg_subtree() -> Bt {
    Seq!(
        InCombat.not(),
        InBattleground,
        Sel!(
            // Fight nearby enemy players.
            Bt::throttle(1_000, BgAttackEnemy),
            // Capture a nearby objective (flag, base).
            Bt::throttle(3_000, BgCaptureObjective),
            // Fall back to following the group.
            Bt::throttle(2_000, Follow),
        ),
    )
}
