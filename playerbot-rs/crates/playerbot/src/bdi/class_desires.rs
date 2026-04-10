/// Per-class desire weight adjustments.
///
/// Each class has different priorities — a rogue cares more about
/// positioning and CC than a warrior. These weights multiply the base
/// desire urgency scores to create class-appropriate behavior.
///
/// A weight of 1.0 means "use base score." 0.0 means "never want this"
/// (used for actions a class literally cannot perform — no heal spell,
/// no dispel, no rez). Values > 1.0 boost the desire beyond base urgency.
use super::desires::DesireKind;
use crate::bot::state::PlayerClass;
use cmangos::BotRole;

/// Per-desire weight multiplier for a class+role combination.
/// Applied after base scoring and personality, before intention selection.
pub fn class_desire_weight(class: PlayerClass, role: BotRole, desire: DesireKind) -> f32 {
    // Class-specific overrides take priority, then role defaults.
    // 0.0 = class literally cannot do this (no spell).
    if let Some(w) = class_override(class, role, desire) {
        return w;
    }

    // Role-based defaults
    match desire {
        // ── Tanks ──────────────────────────────────────────
        DesireKind::TankBoss if role.is_tank() => 1.5,
        DesireKind::ManageThreat if role.is_tank() => 1.3,
        DesireKind::PullMobs if role.is_tank() => 1.2,
        DesireKind::FleeFromDanger if role.is_tank() => 0.1,

        // ── Healers ───────────────────────────────────────
        DesireKind::HealGroup if role.is_heal() => 1.5,
        DesireKind::ResurrectDead if role.is_heal() => 1.3,
        DesireKind::DispelDebuffs if role.is_heal() => 1.2,
        DesireKind::KillTarget if role.is_heal() => 0.3,

        // ── DPS ───────────────────────────────────────────
        DesireKind::KillTarget if role.is_dps() => 1.3,
        DesireKind::CrowdControl if role.is_dps() => 1.1,
        DesireKind::InterruptCast if role.is_dps() => 1.1,

        _ => 1.0,
    }
}

/// Class-specific overrides. Returns `None` to fall through to role defaults.
fn class_override(class: PlayerClass, role: BotRole, desire: DesireKind) -> Option<f32> {
    use DesireKind::{Survive, InterruptCast, FleeFromDanger, HealGroup, ResurrectDead, DispelDebuffs, CrowdControl, ManageThreat, BuffGroup, PullMobs, RecoverResources};
    use PlayerClass::{Warrior, Rogue, Priest, Paladin, Hunter, Mage, Warlock, Shaman, Druid, DeathKnight};

    match (class, desire) {
        // ── Warrior ───────────────────────────────────────
        (Warrior, Survive) => Some(1.3),      // Shield Wall, Last Stand
        (Warrior, InterruptCast) => Some(1.3), // Pummel / Shield Bash
        (Warrior, FleeFromDanger) if role.is_tank() => Some(0.1),
        (Warrior, HealGroup | ResurrectDead | DispelDebuffs) => Some(0.0),

        // ── Rogue ─────────────────────────────────────────
        (Rogue, CrowdControl) => Some(1.4),   // Sap — best CC
        (Rogue, ManageThreat) => Some(1.3),    // Vanish
        (Rogue, InterruptCast) => Some(1.2),   // Kick
        (Rogue, HealGroup | ResurrectDead | DispelDebuffs) => Some(0.0),

        // ── Priest ────────────────────────────────────────
        (Priest, DispelDebuffs) if role.is_heal() => Some(1.5), // Dispel Magic — best
        (Priest, ResurrectDead) if role.is_heal() => Some(1.3),
        (Priest, BuffGroup) if role.is_heal() => Some(1.2),     // Fortitude
        (Priest, DispelDebuffs) => Some(1.2), // Shadow still has Dispel Magic
        (Priest, HealGroup) if role.is_dps() => Some(0.6), // Can offheal, shouldn't prioritize

        // ── Paladin ───────────────────────────────────────
        (Paladin, DispelDebuffs) => Some(1.3), // Cleanse — magic/disease/poison
        (Paladin, BuffGroup) => Some(1.4),     // Blessings are high-value
        (Paladin, ResurrectDead) => Some(1.2),

        // ── Hunter ────────────────────────────────────────
        (Hunter, PullMobs) => Some(1.4),       // Best puller — ranged
        (Hunter, CrowdControl) => Some(1.2),   // Freezing Trap
        (Hunter, HealGroup | ResurrectDead | DispelDebuffs) => Some(0.0),

        // ── Mage ──────────────────────────────────────────
        (Mage, CrowdControl) => Some(1.5),     // Polymorph — best CC
        (Mage, RecoverResources) => Some(1.2), // Conjure Food/Water
        (Mage, InterruptCast) => Some(1.2),    // Counterspell
        (Mage, DispelDebuffs) => Some(0.8),    // Remove Curse only
        (Mage, HealGroup | ResurrectDead) => Some(0.0),

        // ── Warlock ───────────────────────────────────────
        (Warlock, CrowdControl) => Some(1.1),  // Banish/Fear — niche
        (Warlock, HealGroup | ResurrectDead) => Some(0.0),

        // ── Shaman ────────────────────────────────────────
        (Shaman, InterruptCast) => Some(1.3),  // Earth Shock — best interrupt
        (Shaman, DispelDebuffs) if role.is_heal() => Some(1.3), // Purge + totems
        (Shaman, BuffGroup) if role.is_heal() => Some(1.3),     // Totems

        // ── Druid ─────────────────────────────────────────
        (Druid, ResurrectDead) if role.is_heal() => Some(1.1), // Rebirth — combat rez
        (Druid, DispelDebuffs) if role.is_heal() => Some(1.1), // Remove Curse, Abolish Poison
        (Druid, ManageThreat) if role.is_tank() => Some(1.4),  // Harder threat in classic
        (Druid, FleeFromDanger) if role.is_tank() => Some(0.1),

        // ── DeathKnight ───────────────────────────────────
        (DeathKnight, InterruptCast) => Some(1.2), // Mind Freeze
        (DeathKnight, HealGroup | DispelDebuffs) => Some(0.0),
        (DeathKnight, ResurrectDead) => Some(0.0),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rogue_cannot_heal() {
        let w = class_desire_weight(PlayerClass::Rogue, BotRole::DPS, DesireKind::HealGroup);
        assert_eq!(w, 0.0);
    }

    #[test]
    fn rogue_cannot_rez() {
        let w = class_desire_weight(PlayerClass::Rogue, BotRole::DPS, DesireKind::ResurrectDead);
        assert_eq!(w, 0.0);
    }

    #[test]
    fn mage_best_cc() {
        let w = class_desire_weight(PlayerClass::Mage, BotRole::DPS, DesireKind::CrowdControl);
        assert_eq!(w, 1.5);
    }

    #[test]
    fn priest_healer_best_dispel() {
        let w = class_desire_weight(PlayerClass::Priest, BotRole::HEAL, DesireKind::DispelDebuffs);
        assert_eq!(w, 1.5);
    }

    #[test]
    fn warrior_tank_no_flee() {
        let w = class_desire_weight(PlayerClass::Warrior, BotRole::TANK, DesireKind::FleeFromDanger);
        assert_eq!(w, 0.1);
    }

    #[test]
    fn paladin_high_buff() {
        let w = class_desire_weight(PlayerClass::Paladin, BotRole::HEAL, DesireKind::BuffGroup);
        assert_eq!(w, 1.4);
    }

    #[test]
    fn hunter_best_puller() {
        let w = class_desire_weight(PlayerClass::Hunter, BotRole::DPS, DesireKind::PullMobs);
        assert_eq!(w, 1.4);
    }

    #[test]
    fn warrior_cannot_heal() {
        let w = class_desire_weight(PlayerClass::Warrior, BotRole::DPS, DesireKind::HealGroup);
        assert_eq!(w, 0.0);
    }

    #[test]
    fn shadow_priest_offheal_deprioritized() {
        let w = class_desire_weight(PlayerClass::Priest, BotRole::DPS, DesireKind::HealGroup);
        assert_eq!(w, 0.6);
    }

    #[test]
    fn default_weight_is_one() {
        // A generic desire with no override should return 1.0
        let w = class_desire_weight(PlayerClass::Warrior, BotRole::DPS, DesireKind::Travel);
        assert_eq!(w, 1.0);
    }
}
