/// Battleground autonomous behavior.
///
/// Priority: fight nearby enemies > capture objectives > follow group.
use crate::engine::bt::Bt::{self, *};

pub fn bg_subtree() -> Bt {
    Seq(vec![
        InCombat.not(),
        InBattleground,
        Sel(vec![
            // Fight nearby enemy players.
            Bt::throttle(1_000, BgAttackEnemy),
            // Capture a nearby objective (flag, base).
            Bt::throttle(3_000, BgCaptureObjective),
            // Fall back to following the group.
            Bt::throttle(2_000, Follow),
        ]),
    ])
}
