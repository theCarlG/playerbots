/// A* GOAP planner — finds the cheapest action sequence from current to goal.
///
/// The state space is `WorldState` (64-bit bitfield), so hashing and
/// comparison are O(1). The heuristic is `popcount(goal & !current)` —
/// admissible and cheap. Typical plans complete in < 50 node expansions.
///
/// The planner is budget-limited: at most `max_expansions` nodes per call.
/// If exceeded, returns `PlanResult::Incomplete` and the caller retries
/// next tick. This prevents long planning from blocking the tick.
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

use super::action::{ActionId, ActionMask, GoapAction, CLASS_ANY, ROLE_ANY};
use super::plan::{GoapPlan, MAX_PLAN_STEPS};
use super::world_state::WorldState;

/// Result of a planning attempt.
#[derive(Debug)]
pub enum PlanResult {
    /// Found a valid plan.
    Found(GoapPlan),
    /// No plan exists that can reach the goal from the current state.
    Impossible,
    /// Planning budget exhausted; try again next tick.
    Incomplete,
}

/// A node in the A* search.
#[derive(Debug, Clone)]
struct SearchNode {
    state: WorldState,
    cost: u16,
    heuristic: u16,
    plan: [ActionId; MAX_PLAN_STEPS],
    plan_len: u8,
}

impl SearchNode {
    fn f_score(&self) -> u16 {
        self.cost.saturating_add(self.heuristic)
    }
}

// BinaryHeap is a max-heap; we want min-cost, so reverse ordering.
impl PartialEq for SearchNode {
    fn eq(&self, other: &Self) -> bool {
        self.f_score() == other.f_score()
    }
}
impl Eq for SearchNode {}

impl PartialOrd for SearchNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for SearchNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed for min-heap behavior
        other.f_score().cmp(&self.f_score())
    }
}

/// Run A* from `current` to `goal`, considering only actions in `available`.
///
/// # Arguments
/// - `current`: The current world state.
/// - `goal`: The desired world state (bits that must be set).
/// - `available`: Bitmask of which registry actions this bot can use.
/// - `registry`: The global action registry.
/// - `max_expansions`: Maximum node expansions before returning `Incomplete`.
/// - `server_time_ms`: Current server time (for plan timestamp).
pub fn plan(
    current: WorldState,
    goal: WorldState,
    available: &ActionMask,
    registry: &[GoapAction],
    max_expansions: u16,
    server_time_ms: u64,
) -> PlanResult {
    // Goal already satisfied?
    if current.unsatisfied_count(goal) == 0 {
        return PlanResult::Found(GoapPlan {
            goal,
            created_at_ms: server_time_ms,
            ..GoapPlan::default()
        });
    }

    let mut open = BinaryHeap::with_capacity(64);
    let mut closed = HashSet::with_capacity(64);
    let mut expansions = 0u16;

    // Seed with start node
    open.push(SearchNode {
        state: current,
        cost: 0,
        heuristic: current.unsatisfied_count(goal) as u16,
        plan: [ActionId::default(); MAX_PLAN_STEPS],
        plan_len: 0,
    });

    while let Some(node) = open.pop() {
        // Budget check
        expansions += 1;
        if expansions > max_expansions {
            return PlanResult::Incomplete;
        }

        // Already visited this state?
        if !closed.insert(node.state) {
            continue;
        }

        // Goal reached?
        if node.state.unsatisfied_count(goal) == 0 {
            // Plan is built in forward order (first action = steps[0])
            return PlanResult::Found(GoapPlan {
                steps: node.plan,
                len: node.plan_len,
                current_step: 0,
                goal,
                created_at_ms: server_time_ms,
            });
        }

        // Plan too long?
        if node.plan_len as usize >= MAX_PLAN_STEPS {
            continue;
        }

        // Expand: try each available action
        for (idx, action) in registry.iter().enumerate() {
            if !available.has(idx) {
                continue;
            }
            if !action.is_applicable(node.state) {
                continue;
            }

            let new_state = action.apply(node.state);

            // Skip if this produces a state we've already visited
            if closed.contains(&new_state) {
                continue;
            }

            let mut new_plan = node.plan;
            new_plan[node.plan_len as usize] = action.id;

            open.push(SearchNode {
                state: new_state,
                cost: node.cost + action.cost,
                heuristic: new_state.unsatisfied_count(goal) as u16,
                plan: new_plan,
                plan_len: node.plan_len + 1,
            });
        }
    }

    PlanResult::Impossible
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::settings::StrategyFlags;
    use crate::goap::world_state::Atom;

    fn make_registry() -> Vec<GoapAction> {
        vec![
            GoapAction {
                id: ActionId(0),
                name: "acquire_target",
                precondition_set: 1 << Atom::SelfAlive as u8,
                precondition_clear: 0,
                effect_set: 1 << Atom::HasTarget as u8,
                effect_clear: 0,
                cost: 1,
                bt_flags: StrategyFlags::NONE,
                satisfies: 0,
                role_mask: ROLE_ANY,
                class_mask: CLASS_ANY,
            },
            GoapAction {
                id: ActionId(1),
                name: "approach_target",
                precondition_set: (1 << Atom::HasTarget as u8) | (1 << Atom::SelfAlive as u8),
                precondition_clear: 0,
                effect_set: 1 << Atom::TargetInMeleeRange as u8,
                effect_clear: 0,
                cost: 2,
                bt_flags: StrategyFlags::NONE,
                satisfies: 0,
                role_mask: ROLE_ANY,
                class_mask: CLASS_ANY,
            },
            GoapAction {
                id: ActionId(2),
                name: "attack_target",
                precondition_set: (1 << Atom::TargetInMeleeRange as u8)
                    | (1 << Atom::SelfAlive as u8),
                precondition_clear: 0,
                effect_set: 1 << Atom::TargetDead as u8,
                effect_clear: 0,
                cost: 3,
                bt_flags: StrategyFlags::NONE,
                satisfies: 0,
                role_mask: ROLE_ANY,
                class_mask: CLASS_ANY,
            },
        ]
    }

    fn make_mask(count: usize) -> ActionMask {
        let mut mask = ActionMask::default();
        for i in 0..count {
            mask.set(i);
        }
        mask
    }

    #[test]
    fn finds_three_step_plan() {
        let registry = make_registry();
        let mask = make_mask(registry.len());
        let current = WorldState::with(Atom::SelfAlive);
        let goal = WorldState::with(Atom::TargetDead) | WorldState::with(Atom::SelfAlive);

        match plan(current, goal, &mask, &registry, 100, 0) {
            PlanResult::Found(p) => {
                assert_eq!(p.len, 3);
                assert_eq!(p.steps[0], ActionId(0)); // acquire_target
                assert_eq!(p.steps[1], ActionId(1)); // approach_target
                assert_eq!(p.steps[2], ActionId(2)); // attack_target
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }

    #[test]
    fn goal_already_satisfied() {
        let registry = make_registry();
        let mask = make_mask(registry.len());
        let current = WorldState::with(Atom::SelfAlive) | WorldState::with(Atom::TargetDead);
        let goal = current;

        match plan(current, goal, &mask, &registry, 100, 0) {
            PlanResult::Found(p) => assert_eq!(p.len, 0),
            other => panic!("expected Found(empty), got {:?}", other),
        }
    }

    #[test]
    fn impossible_when_no_actions() {
        let registry = make_registry();
        let mask = ActionMask::default(); // no actions available
        let current = WorldState::with(Atom::SelfAlive);
        let goal = WorldState::with(Atom::TargetDead);

        match plan(current, goal, &mask, &registry, 100, 0) {
            PlanResult::Impossible => {}
            other => panic!("expected Impossible, got {:?}", other),
        }
    }

    #[test]
    fn respects_budget() {
        let registry = make_registry();
        let mask = make_mask(registry.len());
        let current = WorldState::with(Atom::SelfAlive);
        let goal = WorldState::with(Atom::TargetDead);

        // Budget of 1 expansion — not enough to find the 3-step plan
        match plan(current, goal, &mask, &registry, 1, 0) {
            PlanResult::Incomplete => {}
            other => panic!("expected Incomplete, got {:?}", other),
        }
    }
}
