/// Support GOAP actions — buff, consume, recover resources.
use crate::bdi::desires::DesireKind;
use crate::bot::settings::StrategyFlags;
use crate::goap::action::{ActionId, GoapAction};
use crate::goap::world_state::Atom;

pub fn register(actions: &mut Vec<GoapAction>) {
    actions.push(GoapAction {
        id: ActionId(0),
        name: "eat_and_drink",
        precondition_set: 1 << Atom::SelfAlive as u8,
        precondition_clear: (1 << Atom::InCombat as u8) | (1 << Atom::ResourcesRecovered as u8),
        effect_set: (1 << Atom::SelfHealthy as u8)
            | (1 << Atom::SelfHasMana as u8)
            | (1 << Atom::ResourcesRecovered as u8),
        effect_clear: 0,
        cost: 5,
        bt_flags: StrategyFlags::NONE,
        satisfies: DesireKind::RecoverResources.as_bit() | DesireKind::Survive.as_bit(),
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "buff_group",
        precondition_set: (1 << Atom::SelfAlive as u8) | (1 << Atom::SelfHasMana as u8),
        precondition_clear: (1 << Atom::InCombat as u8) | (1 << Atom::GroupBuffed as u8),
        effect_set: 1 << Atom::GroupBuffed as u8,
        effect_clear: 0,
        cost: 3,
        bt_flags: StrategyFlags::BUFF,
        satisfies: DesireKind::BuffGroup.as_bit(),
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "repair_gear",
        precondition_set: 1 << Atom::SelfAlive as u8,
        precondition_clear: (1 << Atom::InCombat as u8) | (1 << Atom::GearRepaired as u8),
        effect_set: 1 << Atom::GearRepaired as u8,
        effect_clear: 0,
        cost: 8,
        bt_flags: StrategyFlags::NONE,
        satisfies: DesireKind::RepairGear.as_bit(),
    });
}
