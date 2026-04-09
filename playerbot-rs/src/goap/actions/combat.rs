/// Combat GOAP actions — attack, CC, interrupt, threat management.
use crate::bdi::desires::DesireKind;
use crate::bot::settings::StrategyFlags;
use crate::goap::action::{ActionId, GoapAction};
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
    });
}
