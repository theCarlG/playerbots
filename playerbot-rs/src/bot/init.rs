/// Bot initialization — builds the root behavior tree from (class, spec).
///
/// Each class/spec module exports a `build_tree(spec_info: &SpecInfo) -> Box<dyn BtNode>`.
/// This function dispatches to the right one.
use crate::{
    bot::state::{BotRole, BotState, PlayerClass, PlayerSpec},
    engine::bt_nodes::{BtNode, Selector, cond, sel},
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
fn build_root_tree(_class: PlayerClass, _spec: PlayerSpec) -> Box<dyn BtNode> {
    // Phase 1 stub: just follow the master and don't do anything harmful.
    // Class-specific combat trees will be added in Phase 2.
    sel(vec![
        // Placeholder combat: auto-attack current target if in combat
        cond(|ctx| ctx.in_combat() && ctx.current_target().is_some()),
        // Placeholder non-combat: do nothing
        cond(|_| false),
    ])
}

// TODO Phase 2: import and wire up class-specific BTs:
// use crate::classes::warrior::{arms, fury, protection};
// use crate::classes::priest::{holy, discipline, shadow};
// etc.
