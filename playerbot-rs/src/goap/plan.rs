/// GOAP plan — a sequence of action IDs to execute in order.
///
/// Fixed-size inline array — no heap allocation. Plans are short
/// (typically 2-5 steps) because GOAP operates at the tactical level,
/// not the spell level.
use super::action::ActionId;
use super::world_state::WorldState;

/// Maximum steps in a GOAP plan.
pub const MAX_PLAN_STEPS: usize = 8;

/// A sequence of GOAP actions to execute.
#[derive(Debug, Clone, Copy)]
pub struct GoapPlan {
    /// Action IDs in execution order.
    pub steps: [ActionId; MAX_PLAN_STEPS],
    /// Number of valid steps (0 = empty plan).
    pub len: u8,
    /// Index of the step currently being executed.
    pub current_step: u8,
    /// The goal world state this plan was created to achieve.
    pub goal: WorldState,
    /// Server time (ms) when this plan was created.
    pub created_at_ms: u64,
}

impl Default for GoapPlan {
    fn default() -> Self {
        Self {
            steps: [ActionId::default(); MAX_PLAN_STEPS],
            len: 0,
            current_step: 0,
            goal: WorldState::default(),
            created_at_ms: 0,
        }
    }
}

impl GoapPlan {
    /// Is this plan empty (no steps)?
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Is the plan completed (all steps executed)?
    pub fn is_complete(&self) -> bool {
        self.current_step >= self.len
    }

    /// Get the current step's action ID, or `None` if the plan is complete.
    pub fn current_action(&self) -> Option<ActionId> {
        if self.current_step < self.len {
            Some(self.steps[self.current_step as usize])
        } else {
            None
        }
    }

    /// Advance to the next step. Returns `true` if there are more steps.
    pub fn advance(&mut self) -> bool {
        if self.current_step < self.len {
            self.current_step += 1;
        }
        !self.is_complete()
    }
}

/// Cached plan state on each bot.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlanCache {
    /// The current GOAP plan (if any).
    pub plan: GoapPlan,
    /// World state when the plan was created (for invalidation detection).
    pub planned_world_state: WorldState,
}

/// Maximum plan age before forced replan (milliseconds).
const PLAN_STALE_MS: u64 = 5_000;

impl PlanCache {
    /// Check if the cached plan needs replanning.
    ///
    /// Triggers:
    /// 1. Plan is complete (all steps executed).
    /// 2. Plan is empty (no plan exists).
    /// 3. Plan is stale (older than PLAN_STALE_MS).
    /// 4. Goal is already satisfied in current world state.
    /// 5. Current step's preconditions no longer met.
    pub fn needs_replan(
        &self,
        current_ws: WorldState,
        server_time_ms: u64,
        registry: &[super::action::GoapAction],
    ) -> bool {
        // No plan
        if self.plan.is_empty() {
            return true;
        }

        // Plan completed
        if self.plan.is_complete() {
            return true;
        }

        // Stale
        if server_time_ms.saturating_sub(self.plan.created_at_ms) > PLAN_STALE_MS {
            return true;
        }

        // Goal already satisfied
        if current_ws.unsatisfied_count(self.plan.goal) == 0 {
            return true;
        }

        // Current step's preconditions no longer met
        if let Some(action_id) = self.plan.current_action() {
            if let Some(action) = registry.get(action_id.0 as usize) {
                if !action.is_applicable(current_ws) {
                    return true;
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::settings::StrategyFlags;
    use crate::goap::action::GoapAction;
    use crate::goap::world_state::{Atom, WorldState};

    fn make_action(id: u16, pre_set: u64, eff_set: u64) -> GoapAction {
        GoapAction {
            id: ActionId(id),
            name: "test",
            precondition_set: pre_set,
            precondition_clear: 0,
            effect_set: eff_set,
            effect_clear: 0,
            cost: 10,
            bt_flags: StrategyFlags::NONE,
            satisfies: 0,
        }
    }

    fn make_plan(len: u8, current_step: u8, created_at_ms: u64, goal: WorldState) -> GoapPlan {
        GoapPlan {
            steps: [ActionId(0), ActionId(1), ActionId(2), ActionId(3),
                    ActionId(4), ActionId(5), ActionId(6), ActionId(7)],
            len,
            current_step,
            goal,
            created_at_ms,
        }
    }

    // ── GoapPlan tests ─────────────────────────────────────────

    #[test]
    fn empty_plan() {
        let p = GoapPlan::default();
        assert!(p.is_empty());
        assert!(p.is_complete());
        assert!(p.current_action().is_none());
    }

    #[test]
    fn advance_through_plan() {
        let mut p = make_plan(3, 0, 0, WorldState::default());
        assert!(!p.is_complete());
        assert_eq!(p.current_action(), Some(ActionId(0)));

        assert!(p.advance()); // step 0→1, more steps remain
        assert_eq!(p.current_action(), Some(ActionId(1)));

        assert!(p.advance()); // step 1→2
        assert_eq!(p.current_action(), Some(ActionId(2)));

        assert!(!p.advance()); // step 2→3, plan complete
        assert!(p.is_complete());
        assert!(p.current_action().is_none());
    }

    #[test]
    fn advance_past_end_is_idempotent() {
        let mut p = make_plan(1, 0, 0, WorldState::default());
        assert!(!p.advance()); // complete
        assert!(!p.advance()); // still complete, no panic
        assert_eq!(p.current_step, 1);
    }

    // ── PlanCache::needs_replan tests ──────────────────────────

    #[test]
    fn replan_when_empty() {
        let cache = PlanCache::default();
        let ws = WorldState::default();
        assert!(cache.needs_replan(ws, 0, &[]));
    }

    #[test]
    fn replan_when_complete() {
        let cache = PlanCache {
            plan: make_plan(2, 2, 1000, WorldState::with(Atom::TargetDead)),
            planned_world_state: WorldState::default(),
        };
        assert!(cache.needs_replan(WorldState::default(), 2000, &[]));
    }

    #[test]
    fn replan_when_stale() {
        let goal = WorldState::with(Atom::TargetDead);
        let action = make_action(0, 0, 0);
        let cache = PlanCache {
            plan: make_plan(2, 0, 1000, goal),
            planned_world_state: WorldState::default(),
        };
        // 6001ms after creation > PLAN_STALE_MS (5000)
        assert!(cache.needs_replan(WorldState::default(), 6001, &[action]));
    }

    #[test]
    fn no_replan_when_fresh() {
        let goal = WorldState::with(Atom::TargetDead);
        let action = make_action(0, 0, 0); // no preconditions, always applicable
        let cache = PlanCache {
            plan: make_plan(2, 0, 1000, goal),
            planned_world_state: WorldState::default(),
        };
        // Goal not yet satisfied, plan is fresh, action applicable
        assert!(!cache.needs_replan(WorldState::default(), 2000, &[action]));
    }

    #[test]
    fn replan_when_goal_satisfied() {
        let goal = WorldState::with(Atom::TargetDead);
        let action = make_action(0, 0, 0);
        let cache = PlanCache {
            plan: make_plan(2, 0, 1000, goal),
            planned_world_state: WorldState::default(),
        };
        // Current world state already has TargetDead — goal satisfied
        let current = WorldState::with(Atom::TargetDead);
        assert!(cache.needs_replan(current, 2000, &[action]));
    }

    #[test]
    fn replan_when_preconditions_broken() {
        let goal = WorldState::with(Atom::TargetDead);
        // Action requires HasTarget
        let action = make_action(0, 1u64 << Atom::HasTarget as u8, 0);
        let cache = PlanCache {
            plan: make_plan(2, 0, 1000, goal),
            planned_world_state: WorldState::default(),
        };
        // Current WS does NOT have HasTarget — precondition broken
        assert!(cache.needs_replan(WorldState::default(), 2000, &[action]));
    }

    #[test]
    fn no_replan_when_preconditions_met() {
        let goal = WorldState::with(Atom::TargetDead);
        let action = make_action(0, 1u64 << Atom::HasTarget as u8, 0);
        let cache = PlanCache {
            plan: make_plan(2, 0, 1000, goal),
            planned_world_state: WorldState::default(),
        };
        // Current WS has HasTarget — precondition met
        let current = WorldState::with(Atom::HasTarget);
        assert!(!cache.needs_replan(current, 2000, &[action]));
    }
}
