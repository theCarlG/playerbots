use cmangos::SpellId;
use crate::{Sel, Seq};
/// Frost Mage behavior tree (Classic / Vanilla).
///
/// Priority: Ice Block → Evocation → Counterspell → Frost Nova → turn + Blink
///   → Cone of Cold (on frozen target) → Fire Blast execute → Frostbolt
use crate::{
    data::spells::vanilla::mage::*,
    engine::bt::{
        Bt::{
            self, Cmp, CastOnSelf, CastAoEOnTarget, InCombat, TargetIsCasting, CastOnTarget,
            HasFocusTarget,
        },
        Op::{Below, AtLeast},
        Resource::{SelfHealthPct, SelfManaPct, TargetDistance, TargetHealthPct, AttackerCount},
    },
    engine::macro_fsm::ActiveFsm,
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

/// The frost leveling rotation. Exposed to the sibling spec modules because it
/// uses ONLY baseline mage spells (Frostbolt, Frost Nova, Cone of Cold, Blink,
/// Evocation, Counterspell, Blizzard, Fire Blast — no frost-talent-locked
/// spells), so every mage spec uses it while leveling. Per Icy Veins, all mages
/// level as frost: Frostbolt is the reliable single-target nuke at every level.
pub(super) fn combat_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (mage-wide list).
        super::boost(),
        // Positioning handled by reactive::ranged_subtree().
        Seq!(Cmp(SelfHealthPct, Below(20)), CastOnSelf(ICE_BLOCK)),
        Seq!(
            // `InCombat` OR a deliberate focus target: a focus engagement (set
            // by a pull/attack command or the autonomous quest engage) opens
            // the offensive block pre-combat so the bot CASTS its opener
            // (Frostbolt) at a NEUTRAL quest mob to start the fight — without
            // this it stood at range holding a melee order it could never land.
            Sel!(InCombat, HasFocusTarget),
            Sel!(
                // Evocation: low mana, channeled — only when not moving.
                Seq!(
                    Cmp(SelfManaPct, Below(15)),
                    Bt::IsMoving.not(),
                    CastOnSelf(EVOCATION),
                ),
                // Interrupt.
                Seq!(TargetIsCasting, CastOnTarget(COUNTERSPELL)),
                // Enemy in melee — Frost Nova to root, then turn 180° and Blink
                // away. CRITICAL: the FaceAwayFromTarget→Blink escape is gated on
                // actually KNOWING Blink. Without that gate, a low-level mage
                // (no Frost Nova/Blink until 10) turned 180° away EVERY tick a
                // mob was within 5y — which cancelled its own Frostbolt and made
                // it spin helplessly in melee, never landing a nuke (the "won't
                // kill a mob, cancels the cast by turning the other way" bug).
                // FrostNova self-gates via CastOnTarget (skipped if unknown); the
                // turn must NOT fire unless the Blink that justifies it is usable.
                // With neither, this whole arm fails and the bot nukes at
                // point-blank (CB_CastSpell re-faces the target before casting).
                Seq!(
                    Cmp(TargetDistance, Below(5)),
                    Sel!(
                        // 1. Root the mob in place (instant; fails if on CD /
                        //    not learned → fall through).
                        CastOnTarget(FROST_NOVA),
                        // 2. Rooted now → ESCAPE, then DPS from range. The IDEAL
                        //    is Frost Nova → BLINK (instant ~20y teleport) when we
                        //    have it; otherwise Frost Nova → RUN a short way out of
                        //    melee (~15y). Either way the root holds the mob while
                        //    we open distance, and once we're >5y this whole arm
                        //    fails so the rotation falls through to Frostbolt.
                        Seq!(
                            Sel!(
                                Bt::target_has(FROST_NOVA_AURA_A),
                                Bt::target_has(FROST_NOVA_AURA_B),
                            ),
                            Sel!(
                                Seq!(
                                    Bt::KnowsSpell(BLINK),
                                    Bt::FaceAwayFromTarget,
                                    CastOnSelf(BLINK),
                                ),
                                Bt::KiteFromTarget(15.0),
                            ),
                        ),
                        // 3. Mob in melee but Frost Nova on CD / not learned —
                        //    if we know Blink, blink out anyway; else this arm
                        //    fails and we nuke at point-blank.
                        Seq!(
                            Bt::KnowsSpell(BLINK),
                            Bt::FaceAwayFromTarget,
                            CastOnSelf(BLINK),
                        ),
                    ),
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
                // Low-level fallback: a mage has Fireball from level 1 but
                // doesn't learn Frostbolt until level 4 (and no spec talents
                // below 10). Fireball keeps a low-level frost mage attacking
                // with a spell instead of flailing in melee. Downranks to the
                // known rank in CB_CastSpell.
                CastOnTarget(FIREBALL),
            ),
        ),
    )
}
