/// Movement GOAP actions — approach, flee, position, follow.
use crate::bdi::desires::DesireKind;
use crate::bot::settings::StrategyFlags;
use crate::goap::action::{ActionId, GoapAction};
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
        bt_flags: StrategyFlags::NONE,
        satisfies: DesireKind::FollowLeader.as_bit(),
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
    });
}
