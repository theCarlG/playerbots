use crate::{Sel, Seq};
/// Retribution Paladin behavior tree (Classic / Vanilla).
///
/// Priority: emergency shield → self-heal → Hammer of Wrath → Exorcism →
///   Judgement (with seal) → maintain seal → Consecration
use crate::{
    data::spells::vanilla::paladin::*,
    engine::bt::{
        Bt::{self, *},
        Op::*,
        Resource::*,
    },
    engine::macro_fsm::ActiveFsm,
    ffi::SpellId,
};

// Seal of Righteousness rank IDs for the has-any-rank check.
const SEAL_RANKS: &[SpellId] = &[
    SpellId(20154),
    SpellId(20287),
    SpellId(20288),
    SpellId(20289),
    SpellId(20290),
    SpellId(20291),
    SpellId(20292),
    SpellId(20293),
    SpellId(21082),
    SEAL_OF_RIGHTEOUSNESS,
    SEAL_OF_COMMAND,
];

pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => combat_tree(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}

fn combat_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (paladin-wide list).
        super::boost(),
        // Close gap.
        StickToTarget(5.0),
        // Emergency bubble.
        Seq!(Cmp(SelfHealthPct, Below(20)), CastOnSelf(DIVINE_SHIELD)),
        // Self-heal on serious damage.
        Seq!(Cmp(SelfHealthPct, Below(40)), CastOnSelf(FLASH_OF_LIGHT)),
        Seq!(
            InCombat,
            Sel!(
                // Execute phase.
                Seq!(
                    Cmp(TargetHealthPct, Below(20)),
                    CastOnTarget(HAMMER_OF_WRATH)
                ),
                // Talented instants.
                CastOnTarget(HOLY_SHOCK),
                CastOnTarget(EXORCISM),
                // Judgement — only with a seal up.
                Seq!(
                    Bt::self_missing_any_rank(SEAL_RANKS).not(),
                    CastOnTarget(JUDGEMENT),
                ),
                // Re-seal.
                Seq!(
                    Bt::self_missing_any_rank(SEAL_RANKS),
                    Sel!(
                        CastOnSelf(SEAL_OF_COMMAND),
                        CastOnSelf(SEAL_OF_RIGHTEOUSNESS),
                    ),
                ),
                // AoE threat/damage.
                CastOnSelf(CONSECRATION),
            ),
        ),
    )
}
