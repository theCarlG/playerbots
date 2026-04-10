/// Movement GOAP actions — approach, flee, position, follow.
use crate::bdi::desires::DesireKind;
use crate::bot::settings::StrategyFlags;
use crate::goap::action::*;
use crate::goap::world_state::Atom;

pub fn register(actions: &mut Vec<GoapAction>) {
    actions.push(GoapAction {
        id: ActionId(0),
        name: "follow_leader",
        precondition_set: 1 << Atom::SelfAlive as u8,
        precondition_clear: (1 << Atom::InCombat as u8) | (1 << Atom::FollowingLeader as u8),
        effect_set: 1 << Atom::FollowingLeader as u8,
        effect_clear: 0,
        cost: 2,
        bt_flags: StrategyFlags::FOLLOW,
        satisfies: DesireKind::FollowLeader.as_bit(),
        role_mask: ROLE_ANY,
        class_mask: CLASS_ANY,
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "flee_to_safety",
        precondition_set: 1 << Atom::SelfAlive as u8,
        precondition_clear: 0,
        effect_set: 1 << Atom::ThreatSafe as u8,
        effect_clear: 1 << Atom::InCombat as u8,
        cost: 1,
        bt_flags: StrategyFlags::FLEE,
        satisfies: DesireKind::FleeFromDanger.as_bit() | DesireKind::Survive.as_bit(),
        role_mask: ROLE_ANY,
        class_mask: CLASS_ANY,
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "position_for_mechanic",
        precondition_set: 1 << Atom::SelfAlive as u8,
        precondition_clear: 1 << Atom::MechanicPositioned as u8,
        effect_set: 1 << Atom::MechanicPositioned as u8,
        effect_clear: 0,
        cost: 2,
        bt_flags: StrategyFlags::NONE,
        satisfies: DesireKind::PositionForMechanic.as_bit(),
        role_mask: ROLE_ANY,
        class_mask: CLASS_ANY,
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "get_in_spell_range",
        precondition_set: (1 << Atom::HasTarget as u8) | (1 << Atom::SelfAlive as u8),
        precondition_clear: 1 << Atom::TargetInSpellRange as u8,
        effect_set: 1 << Atom::TargetInSpellRange as u8,
        effect_clear: 0,
        cost: 2,
        bt_flags: StrategyFlags::RANGED,
        satisfies: DesireKind::KillTarget.as_bit()
            | DesireKind::HealGroup.as_bit()
            | DesireKind::CrowdControl.as_bit(),
        role_mask: ROLE_ANY,
        class_mask: CLASS_ANY,
    });

    // ── Phase 3 additions ──────────────────────────────────

    actions.push(GoapAction {
        id: ActionId(0),
        name: "mount_up",
        precondition_set: 1 << Atom::SelfAlive as u8,
        precondition_clear: (1 << Atom::InCombat as u8) | (1 << Atom::SelfMounted as u8),
        effect_set: 1 << Atom::SelfMounted as u8,
        effect_clear: 0,
        cost: 3,
        bt_flags: StrategyFlags::MOUNT,
        satisfies: DesireKind::Travel.as_bit() | DesireKind::FollowLeader.as_bit(),
        role_mask: ROLE_ANY,
        class_mask: CLASS_ANY,
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "kite_target",
        precondition_set: (1 << Atom::InCombat as u8)
            | (1 << Atom::SelfAlive as u8)
            | (1 << Atom::HasTarget as u8),
        precondition_clear: 0,
        effect_set: 1 << Atom::ThreatSafe as u8,
        effect_clear: 0,
        cost: 3,
        bt_flags: StrategyFlags::RANGED,
        satisfies: DesireKind::Survive.as_bit() | DesireKind::FleeFromDanger.as_bit(),
        role_mask: ROLE_DPS,
        class_mask: CLASS_ANY,
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "travel_to_dest",
        precondition_set: 1 << Atom::SelfAlive as u8,
        precondition_clear: 1 << Atom::InCombat as u8,
        effect_set: 1 << Atom::AtTravelDest as u8,
        effect_clear: 0,
        cost: 5,
        bt_flags: StrategyFlags::TRAVEL,
        satisfies: DesireKind::Travel.as_bit(),
        role_mask: ROLE_ANY,
        class_mask: CLASS_ANY,
    });
}
