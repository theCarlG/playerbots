/// Combat Rogue behavior tree (Classic / Vanilla).
///
/// Priority: Vanish → Evasion → Kick interrupt → Riposte → Blade Flurry `AoE`
///   → Slice and Dice upkeep → Eviscerate → Rupture → Sinister Strike
use crate::{
    data::spells::vanilla::rogue::*,
    engine::{
        aura_helpers::RUPTURE_RANKS,
        bt::{Bt::{self, *}, Op::*, Resource::*},
    },
    ffi::SpellId,
};
use crate::{Seq, Sel};

// Riposte: proc after parry.
const RIPOSTE: SpellId = SpellId(14251);

pub fn build_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (rogue-wide list).
        super::boost(),
        // Out-of-combat: keep weapon poisons applied.
        Seq!(
            InCombat.not(),
            Bt::throttle(30_000, ApplyPoisons),
        ),
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
                // Slice and Dice upkeep.
                Seq!(
                    Bt::self_missing(SLICE_AND_DICE),
                    CastOnSelf(SLICE_AND_DICE),
                ),
                // Finisher when SnD is up.
                CastOnTarget(EVISCERATE),
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
