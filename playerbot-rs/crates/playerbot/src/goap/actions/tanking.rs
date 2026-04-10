/// Tanking GOAP actions — taunt, pick up add, hold aggro.
use crate::bdi::desires::DesireKind;
use crate::bot::settings::StrategyFlags;
use crate::bot::state::PlayerClass;
use crate::goap::action::*;
use crate::goap::world_state::Atom;

pub fn register(actions: &mut Vec<GoapAction>) {
    actions.push(GoapAction {
        id: ActionId(0),
        name: "tank_target",
        precondition_set: (1 << Atom::HasTarget as u8)
            | (1 << Atom::TargetInMeleeRange as u8)
            | (1 << Atom::SelfAlive as u8)
            | (1 << Atom::InCombat as u8),
        precondition_clear: 0,
        effect_set: (1 << Atom::BossTargeted as u8) | (1 << Atom::ThreatSafe as u8),
        effect_clear: 0,
        cost: 2,
        bt_flags: StrategyFlags::TANK_ASSIST,
        satisfies: DesireKind::TankBoss.as_bit() | DesireKind::ManageThreat.as_bit(),
        role_mask: ROLE_TANK,
        class_mask: class_bit(PlayerClass::Warrior as u8)
            | class_bit(PlayerClass::Paladin as u8)
            | class_bit(PlayerClass::Druid as u8),
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "pick_up_add",
        precondition_set: (1 << Atom::SelfAlive as u8) | (1 << Atom::InCombat as u8),
        precondition_clear: 1 << Atom::AddControlled as u8,
        effect_set: 1 << Atom::AddControlled as u8,
        effect_clear: 0,
        cost: 3,
        bt_flags: StrategyFlags::TANK_ASSIST,
        satisfies: DesireKind::TankBoss.as_bit() | DesireKind::ProtectAlly.as_bit(),
        role_mask: ROLE_TANK,
        class_mask: class_bit(PlayerClass::Warrior as u8)
            | class_bit(PlayerClass::Paladin as u8)
            | class_bit(PlayerClass::Druid as u8),
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "protect_ally",
        precondition_set: (1 << Atom::SelfAlive as u8) | (1 << Atom::InCombat as u8),
        precondition_clear: 1 << Atom::AllyProtected as u8,
        effect_set: 1 << Atom::AllyProtected as u8,
        effect_clear: 0,
        cost: 4,
        bt_flags: StrategyFlags::PROTECT,
        satisfies: DesireKind::ProtectAlly.as_bit(),
        role_mask: ROLE_TANK_DPS,
        class_mask: class_bit(PlayerClass::Warrior as u8)
            | class_bit(PlayerClass::Paladin as u8),
    });
}
