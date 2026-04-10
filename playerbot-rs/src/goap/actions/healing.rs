/// Healing GOAP actions — single heal, AoE heal, dispel, rez.
use crate::bdi::desires::DesireKind;
use crate::bot::settings::StrategyFlags;
use crate::bot::state::PlayerClass;
use crate::goap::action::*;
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
        role_mask: ROLE_HEAL_DPS,
        class_mask: class_bit(PlayerClass::Priest as u8)
            | class_bit(PlayerClass::Paladin as u8)
            | class_bit(PlayerClass::Druid as u8)
            | class_bit(PlayerClass::Shaman as u8),
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
        role_mask: ROLE_HEAL_DPS,
        class_mask: class_bit(PlayerClass::Priest as u8)
            | class_bit(PlayerClass::Paladin as u8)
            | class_bit(PlayerClass::Druid as u8)
            | class_bit(PlayerClass::Shaman as u8),
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
        role_mask: ROLE_HEAL_DPS,
        class_mask: class_bit(PlayerClass::Priest as u8)
            | class_bit(PlayerClass::Paladin as u8)
            | class_bit(PlayerClass::Druid as u8)
            | class_bit(PlayerClass::Mage as u8)
            | class_bit(PlayerClass::Shaman as u8),
    });

    // ── Phase 3 additions ──────────────────────────────────

    actions.push(GoapAction {
        id: ActionId(0),
        name: "offheal_group",
        precondition_set: (1 << Atom::SelfAlive as u8)
            | (1 << Atom::SelfHasMana as u8)
            | (1 << Atom::InCombat as u8),
        precondition_clear: 1 << Atom::GroupHealthy as u8,
        effect_set: 1 << Atom::GroupHealthy as u8,
        effect_clear: 0,
        cost: 5,
        bt_flags: StrategyFlags::OFFHEAL,
        satisfies: DesireKind::HealGroup.as_bit(),
        role_mask: ROLE_DPS,
        class_mask: class_bit(PlayerClass::Priest as u8)
            | class_bit(PlayerClass::Paladin as u8)
            | class_bit(PlayerClass::Druid as u8)
            | class_bit(PlayerClass::Shaman as u8),
    });

    actions.push(GoapAction {
        id: ActionId(0),
        name: "protect_ally_heal",
        precondition_set: (1 << Atom::SelfAlive as u8)
            | (1 << Atom::SelfHasMana as u8)
            | (1 << Atom::InCombat as u8),
        precondition_clear: 1 << Atom::AllyProtected as u8,
        effect_set: 1 << Atom::AllyProtected as u8,
        effect_clear: 0,
        cost: 3,
        bt_flags: StrategyFlags::OFFHEAL,
        satisfies: DesireKind::ProtectAlly.as_bit(),
        role_mask: ROLE_HEAL,
        class_mask: class_bit(PlayerClass::Priest as u8)
            | class_bit(PlayerClass::Paladin as u8)
            | class_bit(PlayerClass::Druid as u8)
            | class_bit(PlayerClass::Shaman as u8),
    });
}
