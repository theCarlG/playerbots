use crate::{Sel, Seq};
/// Frost Mage behavior tree (Classic / Vanilla).
///
/// Priority: Ice Block → Evocation → Counterspell → Frost Nova + Blink (melee escape)
///   → Cone of Cold (on frozen target) → Fire Blast execute → Frostbolt
use crate::{
    data::spells::vanilla::mage::*,
    engine::bt::{
        Bt::{self, Cmp, CastOnSelf, CastAoEOnTarget, InCombat, TargetIsCasting, CastOnTarget},
        Op::{Below, AtLeast},
        Resource::{SelfHealthPct, SelfManaPct, TargetDistance, TargetHealthPct, AttackerCount},
    },
    engine::macro_fsm::ActiveFsm,
    ffi::SpellId,
};

// Frozen auras applied by Frost Nova.
const FROST_NOVA_AURA_A: SpellId = SpellId(122);
const FROST_NOVA_AURA_B: SpellId = SpellId(42397);

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
        // Positioning handled by reactive::ranged_subtree().
        Seq!(Cmp(SelfHealthPct, Below(20)), CastOnSelf(ICE_BLOCK)),
        Seq!(
            InCombat,
            Sel!(
                // Evocation: low mana, channeled — only when not moving.
                Seq!(
                    Cmp(SelfManaPct, Below(15)),
                    Bt::IsMoving.not(),
                    CastOnSelf(EVOCATION),
                ),
                // Interrupt.
                Seq!(TargetIsCasting, CastOnTarget(COUNTERSPELL)),
                // Enemy in melee — Frost Nova then Blink away.
                Seq!(
                    Cmp(TargetDistance, Below(5)),
                    Sel!(CastOnTarget(FROST_NOVA), CastOnSelf(BLINK)),
                ),
                // Cone of Cold shines while target is rooted by Nova.
                Seq!(
                    Sel!(
                        Bt::target_has(FROST_NOVA_AURA_A),
                        Bt::target_has(FROST_NOVA_AURA_B),
                    ),
                    CastOnSelf(CONE_OF_COLD),
                ),
                // AoE: Blizzard when 3+ attackers.
                Seq!(Cmp(AttackerCount, AtLeast(3)), CastAoEOnTarget(BLIZZARD)),
                // Instant execute.
                Seq!(Cmp(TargetHealthPct, Below(20)), CastOnTarget(FIRE_BLAST)),
                // Main nuke.
                CastOnTarget(FROSTBOLT),
            ),
        ),
    )
}
