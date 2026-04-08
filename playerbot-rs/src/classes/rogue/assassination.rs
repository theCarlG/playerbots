use crate::{Sel, Seq};
/// Assassination Rogue behavior tree (Classic / Vanilla).
///
/// Priority: Vanish (emergency) → Kick interrupt → Slice and Dice upkeep →
///   Backstab → Hemorrhage → Eviscerate → Rupture → Sinister Strike
use crate::{
    data::spells::vanilla::rogue::*,
    engine::bt::{
        Bt::{self, *},
        Op::*,
        Resource::*,
        WeaponType,
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
        // 0. OUT-OF-COMBAT MAINTENANCE: keep weapon poisons applied.
        //    Throttled so we don't spam cast attempts on a failed apply.
        Seq!(InCombat.not(), Bt::throttle(30_000, ApplyPoisons),),
        // 1. UTILITY & POSITIONING (Highest Priority)
        StickToTarget(5.0),
        // 2. DEFENSIVE: "Oh Crap" Logic
        Seq!(Cmp(SelfHealthPct, Below(15)), CastOnSelf(VANISH)),
        // 3. MAIN COMBAT LOOP
        Seq!(
            InCombat,
            Sel!(
                // INTERRUPTS: Don't check energy, just Kick if they cast.
                Seq!(TargetIsCasting, CastOnTarget(KICK)),
                // MAINTENANCE: Slice and Dice is a ~40% DPS increase.
                // We check this before finishers to ensure 100% uptime.
                Seq!(
                    Cmp(SelfComboPoints, Above(0)),
                    Bt::self_missing(SLICE_AND_DICE),
                    CastOnSelf(SLICE_AND_DICE),
                ),
                // 4. FINISHERS (The Big Damage)
                // Pro Tip: We only finish at 4+ CP to maximize Energy-to-Damage ratio.
                Seq!(
                    Cmp(SelfComboPoints, Above(3)),
                    Sel!(
                        // Rupture: Keep this DoT up on the target.
                        Seq!(Bt::target_missing(RUPTURE), CastOnTarget(RUPTURE)),
                        // Eviscerate: The default CP dump.
                        CastOnTarget(EVISCERATE),
                    ),
                ),
                // 5. GENERATORS (The "Rhythm" Logic)
                // Pro Tip: "Pooling" — Don't spam builders if energy is low
                // unless we are about to cap (100 energy).
                Seq!(
                    Sel!(Cmp(SelfEnergy, Above(59)), Cmp(TargetHealthPct, Below(35)),),
                    Sel!(
                        // Backstab: The priority builder (requires Dagger + Behind).
                        Seq!(
                            MainHandIs(WeaponType::Dagger),
                            IsBehindTarget,
                            CastOnTarget(BACKSTAB),
                        ),
                        // Hemorrhage: Efficient builder if talented (Subtlety).
                        // Gated on KnowsSpell so Assa/Combat rogues without
                        // the talent fall through to Sinister Strike.
                        Seq!(KnowsSpell(HEMORRHAGE), CastOnTarget(HEMORRHAGE)),
                        // Sinister Strike: The "I have nothing else" fallback.
                        CastOnTarget(SINISTER_STRIKE),
                    ),
                ),
            ),
        ),
    )
}
