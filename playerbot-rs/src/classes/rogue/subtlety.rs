use crate::{Sel, Seq};
/// Subtlety Rogue behavior tree (Classic / Vanilla).
///
/// Priority: Vanish → Kick interrupt → Gouge panic → Slice and Dice →
///   Ambush (stealth opener) → Hemorrhage → Eviscerate → Sinister Strike
use crate::{
    data::spells::vanilla::rogue::*,
    engine::bt::{
        Bt::{self, *},
        Op::*,
        Resource::*,
    },
    engine::macro_fsm::ActiveFsm,
};

pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => combat_tree(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}

fn combat_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (rogue-wide list).
        super::boost(),
        // Out-of-combat: keep weapon poisons applied.
        Seq!(InCombat.not(), Bt::throttle(30_000, ApplyPoisons),),
        StickToTarget(5.0),
        Seq!(Cmp(SelfHealthPct, Below(15)), CastOnSelf(VANISH)),
        Seq!(
            InCombat,
            Sel!(
                // Interrupt.
                Seq!(TargetIsCasting, CastOnTarget(KICK)),
                // Panic stun.
                Seq!(Cmp(SelfHealthPct, Below(40)), CastOnTarget(GOUGE)),
                // Slice and Dice upkeep (requires at least 1 CP).
                Seq!(
                    Cmp(SelfComboPoints, Above(0)),
                    Bt::self_missing(SLICE_AND_DICE),
                    CastOnSelf(SLICE_AND_DICE),
                ),
                // Stealth opener (can_cast gates on stealth aura).
                CastOnTarget(AMBUSH),
                // Finisher at 4+ CP.
                Seq!(Cmp(SelfComboPoints, Above(3)), CastOnTarget(EVISCERATE),),
                // Hemorrhage: Subtlety talent — gate so low-level Sub bots
                // that haven't talented yet fall through to Sinister Strike.
                Seq!(KnowsSpell(HEMORRHAGE), CastOnTarget(HEMORRHAGE)),
                CastOnTarget(SINISTER_STRIKE),
            ),
        ),
    )
}
