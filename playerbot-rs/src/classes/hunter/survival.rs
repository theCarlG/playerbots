use crate::{Sel, Seq};
/// Survival Hunter behavior tree (Classic / Vanilla).
///
/// Priority: Feign Death → Hunter's Mark → Counterattack → Explosive Trap / Wing Clip
///   at melee → ranged rotation → Serpent Sting
use crate::{
    data::spells::vanilla::hunter::*,
    engine::{
        aura_helpers::{HUNTERS_MARK_RANKS, SERPENT_STING_RANKS},
        bt::{
            Bt::{self, Cmp, CastOnSelf, CastAoEOnTarget, InCombat, CastOnTarget, TargetIsCasting, HasPet, PetAlive},
            Op::{Below, AtLeast},
            Resource::{SelfHealthPct, TargetDistance, AttackerCount, PetHealthPct},
        },
        macro_fsm::ActiveFsm,
    },
    ffi::SpellId,
};

const COUNTERATTACK: SpellId = SpellId(20910); // rank 3

pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => combat_tree(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}

fn combat_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (hunter-wide list).
        super::boost(),
        // Emergency FD.
        Seq!(Cmp(SelfHealthPct, Below(15)), CastOnSelf(FEIGN_DEATH)),
        // Mend Pet when pet HP drops below 50%.
        Seq!(HasPet, PetAlive, Cmp(PetHealthPct, Below(50)), CastOnSelf(MEND_PET)),
        Seq!(
            InCombat,
            Sel!(
                Seq!(
                    Bt::target_missing_any_rank(HUNTERS_MARK_RANKS),
                    CastOnTarget(HUNTERS_MARK),
                ),
                CastOnTarget(COUNTERATTACK),
                // Interrupt.
                Seq!(TargetIsCasting, CastOnTarget(SCATTER_SHOT)),
                // Dead zone fallback (< 9y): trap/snare while kiting.
                Seq!(
                    Cmp(TargetDistance, Below(9)),
                    Sel!(
                        CastOnSelf(EXPLOSIVE_TRAP),
                        CastOnTarget(WING_CLIP),
                        CastOnTarget(RAPTOR_STRIKE),
                    ),
                ),
                // AoE: Volley when 3+ attackers.
                Seq!(Cmp(AttackerCount, AtLeast(3)), CastAoEOnTarget(VOLLEY)),
                // Ranged rotation.
                CastOnTarget(AIMED_SHOT),
                CastOnTarget(MULTI_SHOT),
                CastOnTarget(ARCANE_SHOT),
                Seq!(
                    Bt::target_missing_any_rank(SERPENT_STING_RANKS),
                    CastOnTarget(SERPENT_STING),
                ),
            ),
        ),
    )
}
