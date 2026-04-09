/// GOAP action registry — compile-time array of all available actions.
///
/// Phase 1: skeleton with a small set of representative actions.
/// Phase 3 will populate the full combat/healing/movement/support/tanking sets.
pub mod combat;
pub mod healing;
pub mod movement;
pub mod support;
pub mod tanking;

use super::action::{ActionMask, GoapAction};
use crate::bot::state::PlayerClass;
use crate::ffi::BotRole;

/// The global action registry. All bots share this — a bot's "available
/// actions" are a bitmask over indices into this array.
///
/// Phase 1: minimal set. Phase 3 will add the full library.
pub fn registry() -> &'static [GoapAction] {
    static REGISTRY: std::sync::LazyLock<Vec<GoapAction>> = std::sync::LazyLock::new(|| {
        let mut actions = Vec::new();
        combat::register(&mut actions);
        healing::register(&mut actions);
        movement::register(&mut actions);
        support::register(&mut actions);
        tanking::register(&mut actions);
        // Assign IDs based on position
        for (i, action) in actions.iter_mut().enumerate() {
            action.id = super::action::ActionId(i as u16);
        }
        actions
    });
    &REGISTRY
}

/// Build an `ActionMask` for a specific class + role combination.
///
/// Filters the global registry to only actions this bot can perform.
/// Called once at bot init, not per tick.
pub fn available_actions(class: PlayerClass, role: BotRole) -> ActionMask {
    let reg = registry();
    let mut mask = ActionMask::default();

    for (idx, _action) in reg.iter().enumerate() {
        // Phase 1: all actions available to all bots.
        // Phase 3 will filter by class abilities and role.
        mask.set(idx);
    }

    // Suppress role-inappropriate actions
    if !role.is_tank() {
        // Skip tank-only actions (will be tagged in Phase 3)
    }
    if !role.is_heal() {
        // Skip heal-only actions (will be tagged in Phase 3)
    }

    let _ = class; // Used in Phase 3 for class-specific filtering

    mask
}
