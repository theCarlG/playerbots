/// Retribution Paladin behavior tree (Classic / Vanilla).
///
/// Priority: emergency shield → self-heal → Hammer of Wrath → Exorcism →
///   Judgement (with seal) → maintain seal → Consecration
use crate::{
    data::spells::vanilla::paladin::*,
    engine::bt::Bt::{self, Sel, StickToTarget, Seq, HpBelow, CastOnSelf, InCombat, TargetHpBelow, CastOnTarget, SelfMissingAnyRank},
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

pub fn build_tree() -> Bt {
    Sel(vec![
        // Close gap.
        StickToTarget(5.0),
        // Emergency bubble.
        Seq(vec![HpBelow(0.20), CastOnSelf(DIVINE_SHIELD)]),
        // Self-heal on serious damage.
        Seq(vec![HpBelow(0.40), CastOnSelf(FLASH_OF_LIGHT)]),
        Seq(vec![
            InCombat,
            Sel(vec![
                // Execute phase.
                Seq(vec![TargetHpBelow(0.20), CastOnTarget(HAMMER_OF_WRATH)]),
                // Talented instants.
                CastOnTarget(HOLY_SHOCK),
                CastOnTarget(EXORCISM),
                // Judgement — only with a seal up.
                Seq(vec![
                    SelfMissingAnyRank(SEAL_RANKS).not(),
                    CastOnTarget(JUDGEMENT),
                ]),
                // Re-seal.
                Seq(vec![
                    SelfMissingAnyRank(SEAL_RANKS),
                    Sel(vec![
                        CastOnSelf(SEAL_OF_COMMAND),
                        CastOnSelf(SEAL_OF_RIGHTEOUSNESS),
                    ]),
                ]),
                // AoE threat/damage.
                CastOnSelf(CONSECRATION),
            ]),
        ]),
    ])
}
