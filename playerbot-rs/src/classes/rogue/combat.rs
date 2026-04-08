use crate::{Sel, Seq};
/// Combat Rogue behavior tree (Classic / Vanilla).
///
/// Priority: Vanish → Evasion → Kick interrupt → Riposte → Blade Flurry `AoE`
///   → Slice and Dice upkeep → Eviscerate → Rupture → Sinister Strike
use crate::{
    data::spells::vanilla::rogue::*,
    engine::{
        aura_helpers::RUPTURE_RANKS,
        bt::{
            Bt::{self, InCombat, ApplyPoisons, StickToTarget, Cmp, CastOnSelf, TargetIsCasting, CastOnTarget},
            Op::{Below, AtLeast, Above},
            Resource::{SelfHealthPct, NearbyCount, SelfComboPoints},
        },
        macro_fsm::ActiveFsm,
    },
    ffi::SpellId,
};

// Riposte: proc after parry.
const RIPOSTE: SpellId = SpellId(14251);

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
        Seq!(Cmp(SelfHealthPct, Below(30)), CastOnSelf(EVASION)),
        Seq!(
            InCombat,
            Sel!(
                // Interrupt.
                Seq!(TargetIsCasting, CastOnTarget(KICK)),
                // Riposte — proc after parry (can_cast gates).
                CastOnTarget(RIPOSTE),
                // AoE when swarmed.
                Seq!(Cmp(NearbyCount, AtLeast(2)), CastOnSelf(BLADE_FLURRY)),
                // Slice and Dice upkeep (requires at least 1 CP).
                Seq!(
                    Cmp(SelfComboPoints, Above(0)),
                    Bt::self_missing(SLICE_AND_DICE),
                    CastOnSelf(SLICE_AND_DICE),
                ),
                // Finisher when SnD is up (4+ CP for efficiency).
                Seq!(Cmp(SelfComboPoints, Above(3)), CastOnTarget(EVISCERATE),),
                // Rupture DoT.
                Seq!(
                    Bt::target_missing_any_rank(RUPTURE_RANKS),
                    CastOnTarget(RUPTURE),
                ),
                // Builder.
                CastOnTarget(SINISTER_STRIKE),
            ),
        ),
    )
}
