/// Shadow Priest behavior tree (Classic / Vanilla).
///
/// Priority: Shadowform → Fade → emergency self-heal → Vampiric Embrace →
///   SW:Pain → Mind Blast → Devouring Plague → Mind Flay → Psychic Scream (`AoE` panic)
use crate::{
    data::spells::vanilla::priest::*,
    engine::bt::{Bt::{self, *}, Op::*, Resource::*},
    engine::macro_fsm::ActiveFsm,
    ffi::SpellId,
};
use crate::{Seq, Sel};

// All SW:Pain / Devouring Plague ranks worth checking for re-application.
const SW_PAIN_RANKS: &[SpellId] = &[
    SpellId(589),
    SpellId(594),
    SpellId(970),
    SpellId(992),
    SpellId(2767),
    SpellId(10892),
    SpellId(10893),
    SpellId(10894),
];
const DEVOURING_PLAGUE_RANKS: &[SpellId] = &[
    SpellId(2944),
    SpellId(19276),
    SpellId(19277),
    SpellId(19278),
    SpellId(19279),
    SpellId(19280),
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
        // `co +boost` burst cooldowns (priest-wide list).
        super::boost(),
        // Always maintain Shadowform.
        Seq!(Bt::self_missing(SHADOWFORM), CastOnSelf(SHADOWFORM)),
        // Fade on aggro.
        Seq!(Cmp(AttackerCount, AtLeast(1)), CastOnSelf(FADE)),
        // Emergency self-heal (drops form).
        Seq!(Cmp(SelfHealthPct, Below(30)), CastOnSelf(FLASH_HEAL)),
        Seq!(
            InCombat,
            Sel!(
                // Keep Vampiric Embrace applied.
                Seq!(
                    Bt::target_missing(VAMPIRIC_EMBRACE),
                    CastOnTarget(VAMPIRIC_EMBRACE),
                ),
                // Shadow Word: Pain DoT upkeep.
                Seq!(
                    Bt::target_missing_any_rank(SW_PAIN_RANKS),
                    CastOnTarget(SHADOW_WORD_PAIN),
                ),
                // Instant nuke on CD.
                CastOnTarget(MIND_BLAST),
                // Devouring Plague (Undead racial).
                Seq!(
                    Bt::target_missing_any_rank(DEVOURING_PLAGUE_RANKS),
                    CastOnTarget(DEVOURING_PLAGUE),
                ),
                // Channel filler.
                CastOnTarget(MIND_FLAY),
                // AoE panic if multiple melee attackers.
                Seq!(Cmp(AttackerCount, AtLeast(2)), CastOnSelf(PSYCHIC_SCREAM)),
            ),
        ),
    )
}
