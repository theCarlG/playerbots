/// Destruction Warlock behavior tree (Classic / Vanilla).
///
/// Priority: Demon Armor → Life Tap → Curse of Elements → Immolate → Conflagrate
///   → Corruption → Curse of Agony → Shadowburn execute → Shadow Bolt
use crate::{
    data::spells::vanilla::warlock::*,
    engine::bt::Bt::{self, *},
    ffi::SpellId,
};

const CURSE_OF_ELEMENTS: SpellId = SpellId(17937);
const CURSE_OF_AGONY: SpellId = SpellId(11722);

pub fn build_tree() -> Bt {
    Sel(vec![
        MaintainRange(25.0),

        Seq(vec![SelfMissingAura(DEMON_ARMOR), CastOnSelf(DEMON_ARMOR)]),
        Seq(vec![ManaBelow(0.20), Not(Box::new(HpBelow(0.50))), CastOnSelf(LIFE_TAP)]),

        Seq(vec![InCombat, Sel(vec![
            // Fire damage amp.
            Seq(vec![TargetMissingAura(CURSE_OF_ELEMENTS), CastOnTarget(CURSE_OF_ELEMENTS)]),

            // Immolate (required for Conflagrate).
            Seq(vec![TargetMissingAura(IMMOLATE), CastOnTarget(IMMOLATE)]),

            // Conflagrate burst — consumes Immolate (can_cast gates on it).
            CastOnTarget(CONFLAGRATE),

            Seq(vec![TargetMissingAura(CORRUPTION), CastOnTarget(CORRUPTION)]),
            Seq(vec![TargetMissingAura(CURSE_OF_AGONY), CastOnTarget(CURSE_OF_AGONY)]),

            // Execute.
            Seq(vec![TargetHpBelow(0.20), CastOnTarget(SHADOWBURN)]),

            // Main nuke.
            CastOnTarget(SHADOW_BOLT),
        ])]),
    ])
}
