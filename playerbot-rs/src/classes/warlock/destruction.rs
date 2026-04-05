/// Destruction Warlock behavior tree (Classic / Vanilla).
///
/// Priority: Demon Armor → Life Tap → Curse of Elements → Immolate → Conflagrate
///   → Corruption → Curse of Agony → Shadowburn execute → Shadow Bolt
use crate::{
    data::spells::vanilla::warlock::*,
    engine::bt::{Bt::{self, *}, Op::*, Resource::*},
    ffi::SpellId,
};
use crate::{Seq, Sel};

const CURSE_OF_ELEMENTS: SpellId = SpellId(17937);
const CURSE_OF_AGONY: SpellId = SpellId(11722);

pub fn build_tree() -> Bt {
    Sel!(
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
                Seq!(
                    Bt::target_missing(CORRUPTION),
                    CastOnTarget(CORRUPTION),
                ),
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
