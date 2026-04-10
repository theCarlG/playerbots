/// Combat GOAP actions — attack, CC, interrupt, threat management.
use crate::bdi::desires::DesireKind;
use crate::bot::settings::StrategyFlags;
use crate::bot::state::PlayerClass;
use crate::goap::action::*;
use crate::goap::world_state::Atom;

pub fn register(actions: &mut Vec<GoapAction>) {
    actions.push(GoapAction {
        id: ActionId(0), // reassigned by registry
        name: "acquire_target",
        precondition_set: 1 << Atom::SelfAlive as u8,
        precondition_clear: 1 << Atom::HasTarget as u8,
        effect_set: 1 << Atom::HasTarget as u8,
        effect_clear: 0,
        cost: 1,
        bt_flags: StrategyFlags::NONE,
        satisfies: DesireKind::KillTarget.as_bit()
            | DesireKind::TankBoss.as_bit()
            | DesireKind::CrowdControl.as_bit(),
        role_mask: ROLE_ANY,
        class_mask: CLASS_ANY,
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "close_to_melee",
        precondition_set: (1 << Atom::HasTarget as u8) | (1 << Atom::SelfAlive as u8),
        precondition_clear: 1 << Atom::TargetInMeleeRange as u8,
        effect_set: 1 << Atom::TargetInMeleeRange as u8,
        effect_clear: 0,
        cost: 2,
        bt_flags: StrategyFlags::CLOSE,
        satisfies: DesireKind::KillTarget.as_bit() | DesireKind::TankBoss.as_bit(),
        role_mask: ROLE_TANK_DPS,
        class_mask: CLASS_ANY,
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "attack_target",
        precondition_set: (1 << Atom::TargetInMeleeRange as u8)
            | (1 << Atom::SelfAlive as u8)
            | (1 << Atom::InCombat as u8),
        precondition_clear: 0,
        effect_set: 1 << Atom::TargetDead as u8,
        effect_clear: 0,
        cost: 3,
        bt_flags: StrategyFlags::DPS_ASSIST,
        satisfies: DesireKind::KillTarget.as_bit(),
        role_mask: ROLE_TANK_DPS,
        class_mask: CLASS_ANY,
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "crowd_control",
        precondition_set: (1 << Atom::HasTarget as u8)
            | (1 << Atom::SelfAlive as u8)
            | (1 << Atom::TargetInSpellRange as u8),
        precondition_clear: 0,
        effect_set: 1 << Atom::CcApplied as u8,
        effect_clear: 0,
        cost: 2,
        bt_flags: StrategyFlags::CC,
        satisfies: DesireKind::CrowdControl.as_bit(),
        role_mask: ROLE_ANY,
        class_mask: class_bit(PlayerClass::Mage as u8)
            | class_bit(PlayerClass::Rogue as u8)
            | class_bit(PlayerClass::Hunter as u8)
            | class_bit(PlayerClass::Warlock as u8)
            | class_bit(PlayerClass::Druid as u8)
            | class_bit(PlayerClass::Priest as u8),
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "interrupt_cast",
        precondition_set: (1 << Atom::HasTarget as u8)
            | (1 << Atom::SelfAlive as u8)
            | (1 << Atom::InCombat as u8),
        precondition_clear: 0,
        effect_set: 1 << Atom::InterruptReady as u8,
        effect_clear: 0,
        cost: 1,
        bt_flags: StrategyFlags::NONE,
        satisfies: DesireKind::InterruptCast.as_bit(),
        role_mask: ROLE_ANY,
        class_mask: class_bit(PlayerClass::Warrior as u8)
            | class_bit(PlayerClass::Rogue as u8)
            | class_bit(PlayerClass::Shaman as u8)
            | class_bit(PlayerClass::Mage as u8),
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "threat_dump",
        precondition_set: (1 << Atom::InCombat as u8) | (1 << Atom::SelfAlive as u8),
        precondition_clear: 1 << Atom::ThreatSafe as u8,
        effect_set: 1 << Atom::ThreatSafe as u8,
        effect_clear: 0,
        cost: 2,
        bt_flags: StrategyFlags::NONE,
        satisfies: DesireKind::ManageThreat.as_bit(),
        role_mask: ROLE_DPS,
        class_mask: class_bit(PlayerClass::Rogue as u8)
            | class_bit(PlayerClass::Hunter as u8)
            | class_bit(PlayerClass::Mage as u8)
            | class_bit(PlayerClass::Warlock as u8),
    });

    // ── Phase 3 additions ──────────────────────────────────

    actions.push(GoapAction {
        id: ActionId(0),
        name: "use_aoe",
        precondition_set: (1 << Atom::InCombat as u8)
            | (1 << Atom::SelfAlive as u8)
            | (1 << Atom::HasTarget as u8),
        precondition_clear: 0,
        effect_set: 1 << Atom::TargetDead as u8,
        effect_clear: 0,
        cost: 4,
        bt_flags: StrategyFlags::AOE,
        satisfies: DesireKind::KillTarget.as_bit(),
        role_mask: ROLE_TANK_DPS,
        class_mask: CLASS_ANY,
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "use_boost",
        precondition_set: (1 << Atom::InCombat as u8) | (1 << Atom::SelfAlive as u8),
        precondition_clear: 0,
        effect_set: 1 << Atom::TargetDead as u8,
        effect_clear: 0,
        cost: 5,
        bt_flags: StrategyFlags::BOOST,
        satisfies: DesireKind::KillTarget.as_bit() | DesireKind::TankBoss.as_bit(),
        role_mask: ROLE_ANY,
        class_mask: CLASS_ANY,
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "position_behind",
        precondition_set: (1 << Atom::HasTarget as u8)
            | (1 << Atom::TargetInMeleeRange as u8)
            | (1 << Atom::SelfAlive as u8),
        precondition_clear: 1 << Atom::BehindTarget as u8,
        effect_set: 1 << Atom::BehindTarget as u8,
        effect_clear: 0,
        cost: 2,
        bt_flags: StrategyFlags::BEHIND,
        satisfies: DesireKind::KillTarget.as_bit(),
        role_mask: ROLE_DPS,
        class_mask: CLASS_ANY,
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "initiate_pull",
        precondition_set: (1 << Atom::SelfAlive as u8) | (1 << Atom::HasTarget as u8),
        precondition_clear: 1 << Atom::InCombat as u8,
        effect_set: (1 << Atom::InCombat as u8) | (1 << Atom::PullInitiated as u8),
        effect_clear: 0,
        cost: 3,
        bt_flags: StrategyFlags::PULL,
        satisfies: DesireKind::PullMobs.as_bit(),
        role_mask: ROLE_TANK_DPS,
        class_mask: CLASS_ANY,
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "enter_stealth",
        precondition_set: 1 << Atom::SelfAlive as u8,
        precondition_clear: 1 << Atom::InCombat as u8,
        effect_set: 1 << Atom::Stealthed as u8,
        effect_clear: 0,
        cost: 2,
        bt_flags: StrategyFlags::STEALTH,
        satisfies: DesireKind::KillTarget.as_bit(),
        role_mask: ROLE_DPS,
        class_mask: class_bit(PlayerClass::Rogue as u8)
            | class_bit(PlayerClass::Druid as u8),
    });
}
