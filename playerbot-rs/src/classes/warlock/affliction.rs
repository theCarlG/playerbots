/// Affliction Warlock behavior tree (Classic / Vanilla).
///
/// Priority: Demon Armor → Life Tap → Curse of Agony → Corruption → Immolate →
///   Drain Life (self sustain) → Shadow Bolt
use crate::{
    data::spells::vanilla::warlock::*,
    engine::bt::Bt::{self, *},
    ffi::SpellId,
};

const CURSE_OF_AGONY: SpellId = SpellId(11722);

pub fn build_tree() -> Bt {
    Sel(vec![
        MaintainRange(25.0),
        // Self buff.
        Seq(vec![SelfMissingAura(DEMON_ARMOR), CastOnSelf(DEMON_ARMOR)]),
        // Life tap for mana.
        Seq(vec![
            ManaBelow(0.20),
            Not(Box::new(HpBelow(0.50))),
            CastOnSelf(LIFE_TAP),
        ]),
        Seq(vec![
            InCombat,
            Sel(vec![
                // DoT upkeep.
                Seq(vec![
                    TargetMissingAura(CURSE_OF_AGONY),
                    CastOnTarget(CURSE_OF_AGONY),
                ]),
                Seq(vec![
                    TargetMissingAura(CORRUPTION),
                    CastOnTarget(CORRUPTION),
                ]),
                Seq(vec![TargetMissingAura(IMMOLATE), CastOnTarget(IMMOLATE)]),
                // Self sustain.
                Seq(vec![HpBelow(0.60), CastOnTarget(DRAIN_LIFE)]),
                // Filler nuke.
                CastOnTarget(SHADOW_BOLT),
            ]),
        ]),
    ])
}
