/// Per-class desire weight adjustments.
///
/// Each class has different priorities — a rogue cares more about
/// positioning and CC than a warrior. These weights multiply the base
/// desire urgency scores to create class-appropriate behavior.
///
/// Phase 1: stub tables. Phase 2 will populate full per-class profiles.
use super::desires::DesireKind;
use crate::bot::state::PlayerClass;
use crate::ffi::BotRole;

/// Per-desire weight multiplier for a class+role combination.
/// Applied after base scoring and personality, before intention selection.
///
/// A weight of 1.0 means "use base score." 0.0 means "never want this."
/// Values > 1.0 boost the desire beyond its base urgency.
pub fn class_desire_weight(class: PlayerClass, role: BotRole, desire: DesireKind) -> f32 {
    // Phase 1: role-based defaults. Phase 2 will add class-specific tuning.
    match desire {
        // Tanks care most about tanking and threat
        DesireKind::TankBoss if role.is_tank() => 1.5,
        DesireKind::ManageThreat if role.is_tank() => 1.3,
        DesireKind::PullMobs if role.is_tank() => 1.2,
        // Tanks don't flee as readily
        DesireKind::FleeFromDanger if role.is_tank() => 0.3,

        // Healers care most about group health
        DesireKind::HealGroup if role.is_heal() => 1.5,
        DesireKind::ResurrectDead if role.is_heal() => 1.3,
        DesireKind::DispelDebuffs if role.is_heal() => 1.2,
        // Healers don't prioritize killing
        DesireKind::KillTarget if role.is_heal() => 0.3,

        // DPS cares about killing and CC
        DesireKind::KillTarget if role.is_dps() => 1.3,
        DesireKind::CrowdControl if role.is_dps() => 1.1,
        DesireKind::InterruptCast if role.is_dps() => 1.1,

        // Class-specific overrides (Phase 2 will expand these)
        DesireKind::CrowdControl
            if matches!(class, PlayerClass::Mage | PlayerClass::Rogue) =>
        {
            1.3
        }
        DesireKind::InterruptCast
            if matches!(
                class,
                PlayerClass::Rogue | PlayerClass::Shaman | PlayerClass::Mage
            ) =>
        {
            1.2
        }

        // Default: no adjustment
        _ => 1.0,
    }
}
