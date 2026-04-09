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
