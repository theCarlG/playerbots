/// Feral Druid behavior tree (Classic / Vanilla).
///
/// Handles both Bear (tank) and Cat (DPS) based on role.
/// Bear: Bear Form → Frenzied Regen → Growl → Faerie Fire → Demo Roar → Maul → Swipe
/// Cat:  Cat Form → Ferocious Bite → Rip → Rake → Shred → Claw
use crate::{
    data::spells::vanilla::druid::*,
    engine::{
        aura_helpers::{DEMO_ROAR_RANKS, FAERIE_FIRE_RANKS, RAKE_RANKS, RIP_RANKS},
        bt::Bt::{self, *},
    },
    ffi::SpellId,
};

const GROWL: SpellId = SpellId(6795);
const FRENZIED_REGENERATION: SpellId = SpellId(22842);

pub fn build_tree() -> Bt {
    Sel(vec![
        // Close gap.
        StickToTarget(5.0),

        // Tank path.
        Seq(vec![IsTank, bear_tree()]),
        // DPS path.
        Seq(vec![IsTank.not(), cat_tree()]),
    ])
}

fn bear_tree() -> Bt {
    Sel(vec![
        // Ensure Bear Form.
        Seq(vec![SelfMissingAura(BEAR_FORM), CastOnSelf(BEAR_FORM)]),

        Seq(vec![InCombat, Sel(vec![
            Seq(vec![HpBelow(0.30), CastOnSelf(FRENZIED_REGENERATION)]),
            CastOnTarget(GROWL),
            Seq(vec![TargetMissingAnyRank(FAERIE_FIRE_RANKS), CastOnTarget(FAERIE_FIRE_FERAL)]),
            Seq(vec![TargetMissingAnyRank(DEMO_ROAR_RANKS), CastOnSelf(DEMORALIZING_ROAR)]),
            CastOnTarget(MAUL),
            CastOnTarget(SWIPE_BEAR),
        ])]),
    ])
}

fn cat_tree() -> Bt {
    Sel(vec![
        // Ensure Cat Form.
        Seq(vec![SelfMissingAura(CAT_FORM), CastOnSelf(CAT_FORM)]),

        Seq(vec![InCombat, Sel(vec![
            // Finishers (server gates via can_cast on CPs).
            CastOnTarget(FEROCIOUS_BITE),
            Seq(vec![TargetMissingAnyRank(RIP_RANKS), CastOnTarget(RIP)]),
            // Builders.
            Seq(vec![TargetMissingAnyRank(RAKE_RANKS), CastOnTarget(RAKE)]),
            CastOnTarget(SHRED),
            CastOnTarget(CLAW),
        ])]),
    ])
}
