/// Frost Mage behavior tree (Classic / Vanilla).
///
/// Priority: Ice Block → Evocation → Counterspell → Frost Nova + Blink (melee escape)
///   → Cone of Cold (on frozen target) → Fire Blast execute → Frostbolt
use crate::{
    data::spells::vanilla::mage::*,
    engine::bt::Bt::{self, Sel, MaintainRange, Seq, HpBelow, CastOnSelf, ManaBelow, InCombat, TargetIsCasting, CastOnTarget, TargetCloserThan, TargetHasAura, TargetHpBelow},
    ffi::SpellId,
};

// Frozen auras applied by Frost Nova.
const FROST_NOVA_AURA_A: SpellId = SpellId(122);
const FROST_NOVA_AURA_B: SpellId = SpellId(42397);

pub fn build_tree() -> Bt {
    Sel(vec![
        MaintainRange(10.0),
        Seq(vec![HpBelow(0.20), CastOnSelf(ICE_BLOCK)]),
        Seq(vec![ManaBelow(0.15), CastOnSelf(EVOCATION)]),
        Seq(vec![
            InCombat,
            Sel(vec![
                // Interrupt.
                Seq(vec![TargetIsCasting, CastOnTarget(COUNTERSPELL)]),
                // Enemy in melee — Frost Nova then Blink away.
                Seq(vec![
                    TargetCloserThan(5.0),
                    Sel(vec![CastOnTarget(FROST_NOVA), CastOnSelf(BLINK)]),
                ]),
                // Cone of Cold shines while target is rooted by Nova.
                Seq(vec![
                    Sel(vec![
                        TargetHasAura(FROST_NOVA_AURA_A),
                        TargetHasAura(FROST_NOVA_AURA_B),
                    ]),
                    CastOnSelf(CONE_OF_COLD),
                ]),
                // Instant execute.
                Seq(vec![TargetHpBelow(0.20), CastOnTarget(FIRE_BLAST)]),
                // Main nuke.
                CastOnTarget(FROSTBOLT),
            ]),
        ]),
    ])
}
