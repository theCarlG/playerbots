/// Enhancement Shaman behavior tree (Classic / Vanilla).
///
/// Priority: Lightning Shield + totem upkeep → panic heal → Stormstrike →
///   Earth Shock interrupt → Flame Shock `DoT` → Earth Shock filler
use crate::{
    data::spells::vanilla::shaman::*,
    engine::bt::{Bt::{self, *}, Op::*, Resource::*},
    ffi::SpellId,
};
use crate::{Seq, Sel};

// Higher-rank auras for self buffs.
const LIGHTNING_SHIELD_RANKS: &[SpellId] = &[LIGHTNING_SHIELD, SpellId(10432)];

pub fn build_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (shaman-wide list).
        super::boost(),
        // Self buffs.
        Seq!(
            Bt::self_missing_any_rank(LIGHTNING_SHIELD_RANKS),
            CastOnSelf(LIGHTNING_SHIELD),
        ),
        // Totem loadout driven by per-bot preferences (see
        // `bot::class_prefs::ShamanPrefs`). Throttled so we don't probe
        // the slot mask every tick.
        Bt::throttle(2_000, DropConfiguredTotems),
        StickToTarget(5.0),
        Seq!(
            InCombat,
            Sel!(
                // Panic heal chain.
                Seq!(Cmp(SelfHealthPct, Below(25)), CastOnSelf(NATURE_SWIFTNESS)),
                Seq!(Cmp(SelfHealthPct, Below(35)), CastOnSelf(LESSER_HEALING_WAVE)),
                // Primary damage.
                CastOnTarget(STORMSTRIKE),
                // Interrupt.
                Seq!(TargetIsCasting, CastOnTarget(EARTH_SHOCK)),
                // DoT.
                Seq!(
                    Bt::target_missing(FLAME_SHOCK),
                    CastOnTarget(FLAME_SHOCK),
                ),
                // Filler instant.
                CastOnTarget(EARTH_SHOCK),
            ),
        ),
    )
}
