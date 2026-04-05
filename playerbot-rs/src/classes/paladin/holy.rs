/// Holy Paladin behavior tree (Classic / Vanilla).
///
/// Priority: Lay on Hands (critical) → Divine Shield self → critical heals →
///   medium heals → light heals → maintain blessing.
use crate::{
    data::spells::vanilla::paladin::*,
    engine::bt::{Bt::{self, *}, Op::*, Resource::*},
    ffi::SpellId,
};
use crate::{Seq, Sel};

// Any blessing ID we can detect on self; if none present, reapply Wisdom.
const BLESSING_RANKS: &[SpellId] = &[
    BLESSING_OF_WISDOM,
    BLESSING_OF_MIGHT,
    BLESSING_OF_KINGS,
    SpellId(19742), // Greater Blessing of Wisdom
];

pub fn build_tree() -> Bt {
    Sel!(
        // Emergency: Lay on Hands on anyone dying.
        HealLowest(LAY_ON_HANDS, 0.10),
        // Bubble self when critical.
        Seq!(Cmp(SelfHealthPct, Below(15)), CastOnSelf(DIVINE_SHIELD)),
        // Critical heals.
        HealLowest(HOLY_LIGHT, 0.30),
        HealInjuredParty(HOLY_LIGHT, 0.30),
        HealLowest(HOLY_SHOCK, 0.40),
        HealInjuredParty(HOLY_SHOCK, 0.40),
        // Medium heals.
        HealLowest(HOLY_LIGHT, 0.65),
        HealInjuredParty(HOLY_LIGHT, 0.65),
        // Efficient top-off.
        HealLowest(FLASH_OF_LIGHT, 0.85),
        HealInjuredParty(FLASH_OF_LIGHT, 0.85),
        // Maintain self blessing.
        Seq!(
            Bt::self_missing_any_rank(BLESSING_RANKS),
            CastOnSelf(BLESSING_OF_WISDOM),
        ),
    )
}
