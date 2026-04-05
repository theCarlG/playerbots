/// Protection Paladin behavior tree (Classic / Vanilla).
///
/// No taunt in vanilla — threat from Righteous Fury + seal/judgement/consecration.
/// Priority: Righteous Fury → Divine Shield → Holy Shield → Exorcism → Hammer of Wrath
///   → Judgement (with seal) → re-seal → Consecration → Holy Wrath
use crate::{
    data::spells::vanilla::paladin::*,
    engine::bt::{Bt::{self, *}, Op::*, Resource::*},
    ffi::SpellId,
};
use crate::{Seq, Sel};

const HOLY_SHIELD: SpellId = SpellId(27179); // rank 4 talent
const RIGHTEOUS_FURY: SpellId = SpellId(25780);

const SEAL_RANKS: &[SpellId] = &[
    SEAL_OF_RIGHTEOUSNESS,
    SEAL_OF_COMMAND,
    SpellId(20154),
    SpellId(20287),
    SpellId(20288),
    SpellId(20289),
    SpellId(20290),
    SpellId(20291),
    SpellId(20292),
    SpellId(20293),
    SpellId(21082),
    SpellId(20920),
];

pub fn build_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (paladin-wide list).
        super::boost(),
        // Maintain Righteous Fury (100% bonus threat).
        Seq!(
            Bt::self_missing(RIGHTEOUS_FURY),
            CastOnSelf(RIGHTEOUS_FURY),
        ),
        // Emergency bubble.
        Seq!(Cmp(SelfHealthPct, Below(15)), CastOnSelf(DIVINE_SHIELD)),
        // Close gap.
        StickToTarget(5.0),
        Seq!(
            InCombat,
            Sel!(
                // Holy Shield mitigation + damage.
                CastOnSelf(HOLY_SHIELD),
                // Instant damage/threat.
                CastOnTarget(EXORCISM),
                // Execute phase.
                Seq!(Cmp(TargetHealthPct, Below(20)), CastOnTarget(HAMMER_OF_WRATH)),
                // Judgement with seal.
                Seq!(
                    Bt::self_missing_any_rank(SEAL_RANKS).not(),
                    CastOnTarget(JUDGEMENT),
                ),
                // Re-seal.
                Seq!(
                    Bt::self_missing_any_rank(SEAL_RANKS),
                    CastOnSelf(SEAL_OF_RIGHTEOUSNESS),
                ),
                // AoE threat.
                CastOnSelf(CONSECRATION),
                // Undead AoE.
                CastOnSelf(HOLY_WRATH),
            ),
        ),
    )
}
