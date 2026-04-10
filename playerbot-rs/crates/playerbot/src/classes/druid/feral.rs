use cmangos::SpellId;
use crate::{Sel, Seq};
/// Feral Druid behavior tree (Classic / Vanilla).
///
/// Handles both Bear (tank) and Cat (DPS) based on role.
/// Bear: Bear Form → Frenzied Regen → Growl → Faerie Fire → Demo Roar → Maul → Swipe
/// Cat:  Cat Form → Ferocious Bite → Rip → Rake → Shred → Claw
use crate::{
    data::spells::vanilla::druid::*,
    engine::{
        aura_helpers::{DEMO_ROAR_RANKS, FAERIE_FIRE_RANKS, RAKE_RANKS, RIP_RANKS},
        bt::{
            Bt::{self, IsTank, CastOnSelf, InCombat, Cmp, CastOnTarget},
            Op::Below,
            Resource::SelfHealthPct,
        },
        macro_fsm::ActiveFsm,
    },
};

const GROWL: SpellId = SpellId(6795);
const FRENZIED_REGENERATION: SpellId = SpellId(22842);

pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => combat_tree(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}

fn combat_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (druid-wide list).
        super::boost(),
        // Melee approach is handled by combat_wrapper's close_subtree()
        // (StrategyFlags::CLOSE). Do NOT put StickToTarget here — it
        // returns Running while chasing, which short-circuits the Sel
        // and prevents the bear/cat rotation from ever executing.
        // Tank path.
        Seq!(IsTank, bear_tree()),
        // DPS path.
        Seq!(IsTank.not(), cat_tree()),
    )
}

fn bear_tree() -> Bt {
    Sel!(
        // Ensure Bear Form.
        Seq!(Bt::self_missing(BEAR_FORM), CastOnSelf(BEAR_FORM)),
        Seq!(
            InCombat,
            Sel!(
                Seq!(
                    Cmp(SelfHealthPct, Below(30)),
                    CastOnSelf(FRENZIED_REGENERATION)
                ),
                CastOnTarget(GROWL),
                Seq!(
                    Bt::target_missing_any_rank(FAERIE_FIRE_RANKS),
                    CastOnTarget(FAERIE_FIRE_FERAL),
                ),
                Seq!(
                    Bt::target_missing_any_rank(DEMO_ROAR_RANKS),
                    CastOnSelf(DEMORALIZING_ROAR),
                ),
                CastOnTarget(MAUL),
                CastOnTarget(SWIPE_BEAR),
            ),
        ),
    )
}

fn cat_tree() -> Bt {
    Sel!(
        // Ensure Cat Form.
        Seq!(Bt::self_missing(CAT_FORM), CastOnSelf(CAT_FORM)),
        Seq!(
            InCombat,
            Sel!(
                // Finishers (server gates via can_cast on CPs).
                CastOnTarget(FEROCIOUS_BITE),
                Seq!(Bt::target_missing_any_rank(RIP_RANKS), CastOnTarget(RIP)),
                // Builders.
                Seq!(Bt::target_missing_any_rank(RAKE_RANKS), CastOnTarget(RAKE)),
                CastOnTarget(SHRED),
                CastOnTarget(CLAW),
            ),
        ),
    )
}
