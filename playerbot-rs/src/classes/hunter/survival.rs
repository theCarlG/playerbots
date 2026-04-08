/// Survival Hunter behavior tree (Classic / Vanilla).
///
/// Priority: Feign Death → Hunter's Mark → Counterattack → Explosive Trap / Wing Clip
///   at melee → ranged rotation → Serpent Sting
use crate::{
    data::spells::vanilla::hunter::*,
    engine::{
        aura_helpers::{HUNTERS_MARK_RANKS, SERPENT_STING_RANKS},
        bt::{Bt::{self, *}, Op::*, Resource::*},
        macro_fsm::ActiveFsm,
    },
    ffi::SpellId,
};
use crate::{Seq, Sel};

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
        // Maintain ranged positioning (deadzone at 8y).
        MaintainRange(8.0),
        // Emergency FD.
        Seq!(Cmp(SelfHealthPct, Below(15)), CastOnSelf(FEIGN_DEATH)),
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
                // Melee escapes/AoE.
                Seq!(Cmp(TargetDistance, Below(5)), CastOnSelf(EXPLOSIVE_TRAP)),
                Seq!(Cmp(TargetDistance, Below(5)), CastOnTarget(WING_CLIP)),
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
