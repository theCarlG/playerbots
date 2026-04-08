use crate::{Sel, Seq};
/// Elemental Shaman behavior tree (Classic / Vanilla).
///
/// Priority: totem upkeep → Earth Shock interrupt → Flame Shock `DoT` →
///   Chain Lightning `AoE` → Lightning Bolt → Frost Shock filler
use crate::{
    data::spells::vanilla::shaman::*,
    engine::bt::{
        Bt::{self, MaintainRange, DropConfiguredTotems, InCombat, TargetIsCasting, CastOnTarget, Cmp},
        Op::AtLeast,
        Resource::NearbyCount,
    },
    engine::macro_fsm::ActiveFsm,
};

pub fn build_tree(fsm: ActiveFsm) -> Bt {
    match fsm {
        ActiveFsm::Combat => combat_tree(),
        ActiveFsm::World => Bt::Noop,
        ActiveFsm::Dead => Bt::Noop,
    }
}

fn combat_tree() -> Bt {
    Sel!(
        // `co +boost` burst cooldowns (shaman-wide list).
        super::boost(),
        MaintainRange(20.0),
        // Totem upkeep driven by per-bot preferences
        // (see `bot::class_prefs::ShamanPrefs`).
        Bt::throttle(2_000, DropConfiguredTotems),
        Seq!(
            InCombat,
            Sel!(
                // Interrupt.
                Seq!(TargetIsCasting, CastOnTarget(EARTH_SHOCK)),
                // Flame Shock DoT.
                Seq!(Bt::target_missing(FLAME_SHOCK), CastOnTarget(FLAME_SHOCK),),
                // AoE.
                Seq!(Cmp(NearbyCount, AtLeast(3)), CastOnTarget(CHAIN_LIGHTNING)),
                // Main nuke.
                CastOnTarget(LIGHTNING_BOLT),
                // Filler/snare.
                CastOnTarget(FROST_SHOCK),
            ),
        ),
    )
}
