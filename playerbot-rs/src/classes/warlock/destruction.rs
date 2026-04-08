use crate::{Sel, Seq};
/// Destruction Warlock behavior tree (Classic / Vanilla).
///
/// Priority: Demon Armor → Life Tap → Curse of Elements → Immolate → Conflagrate
///   → Corruption → Curse of Agony → Shadowburn execute → Shadow Bolt
use crate::{
    data::spells::vanilla::warlock::*,
    engine::bt::{
        Bt::{self, MaintainRange, CastOnSelf, Cmp, Not, InCombat, CastOnTarget},
        Op::Below,
        Resource::{SelfManaPct, SelfHealthPct, TargetHealthPct},
    },
    engine::macro_fsm::ActiveFsm,
    ffi::SpellId,
};

const CURSE_OF_ELEMENTS: SpellId = SpellId(17937);
const CURSE_OF_AGONY: SpellId = SpellId(11722);

pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => combat_tree(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}

fn combat_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (warlock-wide list).
        super::boost(),
        MaintainRange(25.0),
        Seq!(Bt::self_missing(DEMON_ARMOR), CastOnSelf(DEMON_ARMOR)),
        Seq!(
            Cmp(SelfManaPct, Below(20)),
            Not(Box::new(Cmp(SelfHealthPct, Below(50)))),
            CastOnSelf(LIFE_TAP),
        ),
        Seq!(
            InCombat,
            Sel!(
                // Fire damage amp.
                Seq!(
                    Bt::target_missing(CURSE_OF_ELEMENTS),
                    CastOnTarget(CURSE_OF_ELEMENTS),
                ),
                // Immolate (required for Conflagrate).
                Seq!(Bt::target_missing(IMMOLATE), CastOnTarget(IMMOLATE)),
                // Conflagrate burst — consumes Immolate (can_cast gates on it).
                CastOnTarget(CONFLAGRATE),
                Seq!(Bt::target_missing(CORRUPTION), CastOnTarget(CORRUPTION),),
                Seq!(
                    Bt::target_missing(CURSE_OF_AGONY),
                    CastOnTarget(CURSE_OF_AGONY),
                ),
                // Execute.
                Seq!(Cmp(TargetHealthPct, Below(20)), CastOnTarget(SHADOWBURN)),
                // Main nuke.
                CastOnTarget(SHADOW_BOLT),
            ),
        ),
    )
}
