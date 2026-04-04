/// Fire Mage behavior tree (Classic / Vanilla).
///
/// Priority: Ice Block → Evocation → Counterspell → Fire Blast execute →
///   Scorch (build Fire Vulnerability stacks) → Fireball
use crate::{
    data::spells::vanilla::mage::*,
    engine::bt::Bt::{self, *},
    ffi::SpellId,
};

// Improved Scorch stacks: aura 22959, max 5 stacks.
const FIRE_VULNERABILITY: SpellId = SpellId(22959);
const FIRE_VULN_MAX: u8 = 5;

pub fn build_tree() -> Bt {
    Sel(vec![
        MaintainRange(10.0),

        Seq(vec![HpBelow(0.20), CastOnSelf(ICE_BLOCK)]),
        Seq(vec![ManaBelow(0.15), CastOnSelf(EVOCATION)]),

        Seq(vec![InCombat, Sel(vec![
            Seq(vec![TargetIsCasting, CastOnTarget(COUNTERSPELL)]),
            Seq(vec![TargetHpBelow(0.20), CastOnTarget(FIRE_BLAST)]),
            // Scorch to stack Fire Vulnerability.
            Seq(vec![
                TargetAuraStacksBelow(FIRE_VULNERABILITY, FIRE_VULN_MAX),
                CastOnTarget(SCORCH),
            ]),
            // Main nuke.
            CastOnTarget(FIREBALL),
            // Filler.
            CastOnTarget(SCORCH),
        ])]),
    ])
}
