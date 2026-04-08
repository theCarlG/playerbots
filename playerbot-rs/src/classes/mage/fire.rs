use crate::{Sel, Seq};
/// Fire Mage behavior tree (Classic / Vanilla).
///
/// Priority: Ice Block → Evocation → Counterspell → Fire Blast execute →
///   Scorch (build Fire Vulnerability stacks) → Fireball
use crate::{
    data::spells::vanilla::mage::*,
    engine::bt::{
        Bt::{self, *},
        Op::*,
        Resource::*,
    },
    engine::macro_fsm::ActiveFsm,
    ffi::SpellId,
};

// Improved Scorch stacks: aura 22959, max 5 stacks.
const FIRE_VULNERABILITY: SpellId = SpellId(22959);
const FIRE_VULN_MAX: u8 = 5;

pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => combat_tree(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}

fn combat_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (mage-wide list).
        super::boost(),
        MaintainRange(10.0),
        Seq!(Cmp(SelfHealthPct, Below(20)), CastOnSelf(ICE_BLOCK)),
        Seq!(Cmp(SelfManaPct, Below(15)), CastOnSelf(EVOCATION)),
        Seq!(
            InCombat,
            Sel!(
                Seq!(TargetIsCasting, CastOnTarget(COUNTERSPELL)),
                Seq!(Cmp(TargetHealthPct, Below(20)), CastOnTarget(FIRE_BLAST)),
                // Scorch to stack Fire Vulnerability.
                Seq!(
                    Bt::target_aura_stacks_below(FIRE_VULNERABILITY, FIRE_VULN_MAX),
                    CastOnTarget(SCORCH),
                ),
                // Main nuke.
                CastOnTarget(FIREBALL),
                // Filler.
                CastOnTarget(SCORCH),
            ),
        ),
    )
}
