/// Subtlety Rogue behavior tree (Classic / Vanilla).
///
/// Priority: Vanish → Kick interrupt → Gouge panic → Slice and Dice →
///   Ambush (stealth opener) → Hemorrhage → Eviscerate → Sinister Strike
use crate::{
    data::spells::vanilla::rogue::*,
    engine::bt::Bt::{self, *},
};

pub fn build_tree() -> Bt {
    Sel(vec![
        StickToTarget(5.0),
        Seq(vec![HpBelow(0.15), CastOnSelf(VANISH)]),
        Seq(vec![
            InCombat,
            Sel(vec![
                // Interrupt.
                Seq(vec![TargetIsCasting, CastOnTarget(KICK)]),
                // Panic stun.
                Seq(vec![HpBelow(0.40), CastOnTarget(GOUGE)]),
                // Slice and Dice upkeep.
                Seq(vec![
                    SelfMissingAura(SLICE_AND_DICE),
                    CastOnSelf(SLICE_AND_DICE),
                ]),
                // Stealth opener (can_cast gates on stealth aura).
                CastOnTarget(AMBUSH),
                CastOnTarget(HEMORRHAGE),
                CastOnTarget(EVISCERATE),
                CastOnTarget(SINISTER_STRIKE),
            ]),
        ]),
    ])
}
