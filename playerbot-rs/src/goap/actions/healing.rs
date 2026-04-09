/// Healing GOAP actions — single heal, AoE heal, dispel, rez.
use crate::bdi::desires::DesireKind;
use crate::bot::settings::StrategyFlags;
use crate::goap::action::{ActionId, GoapAction};
use crate::goap::world_state::Atom;

pub fn register(actions: &mut Vec<GoapAction>) {
    actions.push(GoapAction {
        id: ActionId(0),
        name: "heal_group",
        precondition_set: (1 << Atom::SelfAlive as u8) | (1 << Atom::SelfHasMana as u8),
        precondition_clear: 1 << Atom::GroupHealthy as u8,
        effect_set: 1 << Atom::GroupHealthy as u8,
        effect_clear: 0,
        cost: 3,
        bt_flags: StrategyFlags::OFFHEAL,
        satisfies: DesireKind::HealGroup.as_bit(),
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "resurrect_dead",
        precondition_set: (1 << Atom::SelfAlive as u8) | (1 << Atom::SelfHasMana as u8),
        precondition_clear: 1 << Atom::GroupDeadRezzed as u8,
        effect_set: 1 << Atom::GroupDeadRezzed as u8,
        effect_clear: 0,
        cost: 5,
        bt_flags: StrategyFlags::NONE,
        satisfies: DesireKind::ResurrectDead.as_bit(),
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "dispel_debuffs",
        precondition_set: (1 << Atom::SelfAlive as u8) | (1 << Atom::SelfHasMana as u8),
        precondition_clear: 1 << Atom::GroupDebuffsCleansed as u8,
        effect_set: 1 << Atom::GroupDebuffsCleansed as u8,
        effect_clear: 0,
        cost: 2,
        bt_flags: StrategyFlags::CURE,
        satisfies: DesireKind::DispelDebuffs.as_bit(),
    });
}
