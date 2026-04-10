/// GOAP action registry — compile-time array of all available actions.
pub mod combat;
pub mod healing;
pub mod movement;
pub mod support;
pub mod tanking;

use super::action::{ActionMask, GoapAction, CLASS_ANY, ROLE_ANY};
use crate::bot::state::PlayerClass;
use crate::ffi::BotRole;

/// The global action registry. All bots share this — a bot's "available
/// actions" are a bitmask over indices into this array.
pub fn registry() -> &'static [GoapAction] {
    static REGISTRY: std::sync::LazyLock<Vec<GoapAction>> = std::sync::LazyLock::new(|| {
        let mut actions = Vec::new();
        combat::register(&mut actions);
        healing::register(&mut actions);
        movement::register(&mut actions);
        support::register(&mut actions);
        tanking::register(&mut actions);
        // Assign IDs based on position
        for (i, action) in actions.iter_mut().enumerate() {
            action.id = super::action::ActionId(i as u16);
        }
        actions
    });
    &REGISTRY
}

/// Build an `ActionMask` for a specific class + role combination.
///
/// Filters the global registry by each action's `role_mask` and `class_mask`.
/// Called once at bot init, not per tick.
pub fn available_actions(class: PlayerClass, role: BotRole) -> ActionMask {
    let reg = registry();
    let mut mask = ActionMask::default();

    let role_bit = if role.is_tank() {
        1u8
    } else if role.is_heal() {
        2u8
    } else {
        4u8
    };
    let class_bit = 1u16 << (class as u8);

    for (idx, action) in reg.iter().enumerate() {
        if action.role_mask != ROLE_ANY && (action.role_mask & role_bit) == 0 {
            continue;
        }
        if action.class_mask != CLASS_ANY && (action.class_mask & class_bit) == 0 {
            continue;
        }
        mask.set(idx);
    }

    mask
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::state::PlayerClass;
    use crate::ffi::BotRole;

    #[test]
    fn warrior_tank_gets_tank_actions() {
        let mask = available_actions(PlayerClass::Warrior, BotRole::TANK);
        let reg = registry();
        let names: Vec<&str> = reg
            .iter()
            .enumerate()
            .filter(|(i, _)| mask.has(*i))
            .map(|(_, a)| a.name)
            .collect();
        assert!(names.contains(&"tank_target"), "tank should have tank_target");
        assert!(names.contains(&"pick_up_add"), "tank should have pick_up_add");
        assert!(!names.contains(&"heal_group"), "warrior tank shouldn't have heal_group");
        assert!(!names.contains(&"threat_dump"), "tank shouldn't have threat_dump");
    }

    #[test]
    fn mage_dps_gets_cc_and_interrupt() {
        let mask = available_actions(PlayerClass::Mage, BotRole::DPS);
        let reg = registry();
        let names: Vec<&str> = reg
            .iter()
            .enumerate()
            .filter(|(i, _)| mask.has(*i))
            .map(|(_, a)| a.name)
            .collect();
        assert!(names.contains(&"crowd_control"), "mage should have CC");
        assert!(names.contains(&"interrupt_cast"), "mage should have interrupt");
        assert!(!names.contains(&"tank_target"), "mage shouldn't tank");
        assert!(!names.contains(&"heal_group"), "mage shouldn't heal");
    }

    #[test]
    fn priest_healer_gets_heal_and_dispel() {
        let mask = available_actions(PlayerClass::Priest, BotRole::HEAL);
        let reg = registry();
        let names: Vec<&str> = reg
            .iter()
            .enumerate()
            .filter(|(i, _)| mask.has(*i))
            .map(|(_, a)| a.name)
            .collect();
        assert!(names.contains(&"heal_group"), "priest healer should heal");
        assert!(names.contains(&"dispel_debuffs"), "priest should dispel");
        assert!(names.contains(&"resurrect_dead"), "priest should rez");
        assert!(!names.contains(&"tank_target"), "priest shouldn't tank");
        assert!(!names.contains(&"attack_target"), "healer shouldn't have attack_target");
    }

    #[test]
    fn rogue_dps_cannot_heal_or_rez() {
        let mask = available_actions(PlayerClass::Rogue, BotRole::DPS);
        let reg = registry();
        let names: Vec<&str> = reg
            .iter()
            .enumerate()
            .filter(|(i, _)| mask.has(*i))
            .map(|(_, a)| a.name)
            .collect();
        assert!(!names.contains(&"heal_group"), "rogue can't heal");
        assert!(!names.contains(&"resurrect_dead"), "rogue can't rez");
        assert!(!names.contains(&"dispel_debuffs"), "rogue can't dispel");
        assert!(names.contains(&"crowd_control"), "rogue has sap/blind");
        assert!(names.contains(&"threat_dump"), "rogue has vanish");
    }

    #[test]
    fn all_classes_have_universal_actions() {
        // Every class/role combo should get eat_and_drink, follow_leader, flee_to_safety
        for class in [
            PlayerClass::Warrior,
            PlayerClass::Mage,
            PlayerClass::Rogue,
            PlayerClass::Priest,
        ] {
            let mask = available_actions(class, BotRole::DPS);
            let reg = registry();
            let names: Vec<&str> = reg
                .iter()
                .enumerate()
                .filter(|(i, _)| mask.has(*i))
                .map(|(_, a)| a.name)
                .collect();
            assert!(names.contains(&"eat_and_drink"), "{:?} should eat/drink", class);
            assert!(names.contains(&"follow_leader"), "{:?} should follow", class);
            assert!(names.contains(&"flee_to_safety"), "{:?} should flee", class);
        }
    }
}
