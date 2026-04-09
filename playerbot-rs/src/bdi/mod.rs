/// BDI (Belief-Desire-Intention) layer — the bot's "psychology."
///
/// Manages what the bot believes about the world, what it wants, and
/// what it has committed to doing. Runs before GOAP planning each tick
/// to determine the current high-level goal.
///
/// # Integration
///
/// Called from `tick.rs` after snapshot refresh and event processing.
/// Outputs a `DesireKind` (the current intention) which GOAP translates
/// into a plan. The plan's strategy flags steer the existing BT trees.
pub mod beliefs;
pub mod class_desires;
pub mod desires;
pub mod intentions;
pub mod personality;

use beliefs::BeliefSet;
use desires::{DesireKind, ScoredDesire};
use intentions::{CooldownEntry, ForcedIntention, Intention};
use personality::Personality;

use crate::bot::settings::StrategyFlags;
use crate::bot::state::PlayerClass;
use crate::engine::blackboard::{Blackboard, Key as BbKey, Value as BbValue};
use crate::ffi::BotRole;
use crate::goap::action::ActionMask;
use crate::goap::plan::PlanCache;

/// Maximum anti-oscillation cooldown slots.
const MAX_COOLDOWNS: usize = 4;

/// All BDI + GOAP state for one bot. Lives as a field on `BotState`.
///
/// ~300 bytes per bot. For 1,800 bots that's 540KB — negligible.
pub struct BdiState {
    /// Current beliefs about the world. Updated every tick.
    pub beliefs: BeliefSet,
    /// Previous tick's beliefs (for diff detection).
    pub prev_beliefs: BeliefSet,
    /// Per-bot personality weights.
    pub personality: Personality,
    /// Current committed intention.
    pub intention: Intention,
    /// Forced intention from a player command (overrides BDI evaluation).
    pub forced_intention: Option<ForcedIntention>,
    /// Anti-oscillation cooldowns for recently-abandoned desires.
    pub cooldowns: [CooldownEntry; MAX_COOLDOWNS],
    /// Last scored desires (for debugging/monitor).
    pub last_scores: [ScoredDesire; DesireKind::COUNT],
    /// Server time (ms) when BDI last evaluated desires.
    pub last_eval_ms: u64,
    /// Whether the intention changed this tick (signals GOAP to replan).
    pub intention_changed: bool,
    /// GOAP plan cache — holds the current plan and invalidation state.
    pub plan_cache: PlanCache,
    /// Bitmask of GOAP actions available to this bot (class + role filtered).
    /// Computed once at init, not per tick.
    pub available_actions: ActionMask,
}

impl BdiState {
    /// Create a new BDI state with role-appropriate personality defaults.
    pub fn new(class: PlayerClass, role: BotRole) -> Self {
        Self {
            beliefs: BeliefSet::default(),
            prev_beliefs: BeliefSet::default(),
            personality: Personality::default_for_role(role),
            intention: Intention::default(),
            forced_intention: None,
            cooldowns: [CooldownEntry::default(); MAX_COOLDOWNS],
            last_scores: [ScoredDesire {
                kind: DesireKind::Idle,
                urgency: 0.0,
            }; DesireKind::COUNT],
            last_eval_ms: 0,
            intention_changed: false,
            plan_cache: PlanCache::default(),
            available_actions: crate::goap::actions::available_actions(class, role),
        }
    }

    /// The current active desire (forced or autonomous).
    pub fn active_desire(&self) -> DesireKind {
        if let Some(forced) = &self.forced_intention {
            forced.desire
        } else {
            self.intention.desire
        }
    }

    /// Whether the GOAP planner should re-plan this tick.
    pub fn needs_replan(&self, server_time_ms: u64) -> bool {
        if self.intention_changed {
            return true;
        }
        let current_ws = crate::goap::world_state::from_beliefs(&self.beliefs);
        self.plan_cache
            .needs_replan(current_ws, server_time_ms, crate::goap::actions::registry())
    }

    /// Get the strategy flags from the current GOAP plan step.
    /// Returns `NONE` if there is no active plan.
    pub fn goap_strategy_flags(&self) -> StrategyFlags {
        crate::goap::current_plan_flags(&self.plan_cache)
    }
}

/// Should this bot run BDI evaluation this tick?
///
/// Time-slicing: spreads 1,800 bots across 5 ticks (100ms each = 500ms
/// cycle). Bot handle is used as a stable hash key so the same bot
/// always evaluates on the same slot.
pub fn should_evaluate(bot_handle: u64, server_time_ms: u64) -> bool {
    let slot = bot_handle % 5;
    let tick_slot = (server_time_ms / 100) % 5;
    slot == tick_slot
}

/// Run BDI evaluation: update beliefs, score desires, select/maintain intention.
///
/// Called from `tick.rs` step 4.8. The heavy lifting (desire scoring,
/// intention evaluation) only runs when beliefs changed or on the
/// time-sliced schedule.
pub fn evaluate(
    state: &mut BdiState,
    class: PlayerClass,
    role: BotRole,
    encounter_active: bool,
    server_time_ms: u64,
    mode: crate::bot::settings::BehaviorMode,
    group_desires: Option<&desires::GroupDesireCounts>,
) {
    state.intention_changed = false;

    // If there's a forced intention from a command, skip autonomous evaluation
    if state.forced_intention.is_some() {
        return;
    }

    // Score all desires
    let mut scores =
        desires::score_desires(&state.beliefs, role, &state.personality, encounter_active, mode, group_desires);

    // Apply class-specific weights
    for score in &mut scores {
        if score.urgency > 0.0 {
            score.urgency *= class_desires::class_desire_weight(class, role, score.kind);
            score.urgency = score.urgency.clamp(0.0, 1.0);
        }
    }

    state.last_scores = scores;
    state.last_eval_ms = server_time_ms;

    // Evaluate whether to switch intention
    if let Some(new_intention) = intentions::evaluate_intention(
        &state.intention,
        &scores,
        &state.cooldowns,
        server_time_ms,
        state.personality.discipline,
    ) {
        // Record cooldown for the abandoned desire
        intentions::record_cooldown(
            &mut state.cooldowns,
            state.intention.desire,
            server_time_ms,
        );
        state.intention = new_intention;
        state.intention_changed = true;
    }

    // Snapshot beliefs for next tick's diff
    state.prev_beliefs = state.beliefs;
}

/// Write the current BDI intention to the blackboard so BT nodes and
/// the monitor can read it.
///
/// Returns the `DesireKind` for the caller (tick.rs) to pass to GOAP.
pub fn write_to_blackboard(state: &BdiState, bb: &mut Blackboard) {
    bb.set(
        BbKey::BdiActiveDesire,
        BbValue::U32(state.active_desire() as u32),
    );
    bb.set(
        BbKey::BdiIntentionChangedThisTick,
        BbValue::Bool(state.intention_changed),
    );
}
