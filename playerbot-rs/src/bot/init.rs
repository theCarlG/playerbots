/// Bot initialization — builds the root behavior tree from (class, spec).
///
/// Each class/spec module exports a `build_tree() -> Box<dyn BtNode>`.
/// This function dispatches to the right one.
use crate::{
    bot::state::{BotRole, BotState, PlayerClass, PlayerSpec},
    engine::bt_nodes::{BtNode, cond, sel},
    ffi::interface::BotInterface,
};

/// Build a BotState from its handle, interface, class, and spec.
pub fn create_bot(
    handle: u64,
    interface: Box<dyn BotInterface>,
    class: PlayerClass,
    spec: PlayerSpec,
) -> Box<BotState> {
    let role = default_role_for_spec(&spec);
    let root_tree = build_root_tree(class, spec);
    Box::new(BotState::new(handle, interface, class, spec, role, root_tree))
}

fn default_role_for_spec(spec: &PlayerSpec) -> BotRole {
    use PlayerSpec::*;
    match spec {
        WarriorProtection | PaladinProtection | DruidFeral => BotRole::TANK,
        PriestHoly | PriestDiscipline | PaladinHoly | ShamanRestoration
        | DruidRestoration => BotRole::HEAL,
        _ => BotRole::DPS,
    }
}

/// Build the complete root behavior tree for a given class/spec.
///
/// Structure:
///   Selector {
///     encounter_subtree (when in instance with known encounter),
///     combat_subtree,
///     noncombat_subtree,
///   }
fn build_root_tree(class: PlayerClass, spec: PlayerSpec) -> Box<dyn BtNode> {
    use PlayerClass::*;
    use PlayerSpec::*;

    match (class, spec) {
        (Warrior, WarriorArms) => {
            crate::classes::warrior::arms::build_tree()
        }

        // TODO Phase 3: wire up remaining classes
        _ => {
            // Phase 2 stub for unimplemented classes: do nothing harmful.
            sel(vec![
                cond(|_| false),
            ])
        }
    }
}
