/// GOAP (Goal-Oriented Action Planning) layer — tactical planning.
///
/// Takes the current BDI intention and finds the cheapest action sequence
/// to achieve it. Each action maps to BT strategy flags, so the plan
/// steers the existing behavior trees without modifying them.
///
/// # Integration
///
/// Called from `tick.rs` after BDI evaluation. The plan's current step
/// writes strategy flags to the blackboard before the BT runs.
pub mod action;
pub mod actions;
pub mod plan;
pub mod planner;
pub mod world_state;

use crate::bdi::desires::DesireKind;
use crate::bot::settings::StrategyFlags;
use crate::engine::blackboard::{Blackboard, Key as BbKey, Value as BbValue};

use action::ActionMask;
use plan::{GoapPlan, PlanCache};
use planner::PlanResult;
use world_state::{Atom, WorldState};

/// Default planning budget (max node expansions per call).
const DEFAULT_MAX_EXPANSIONS: u16 = 50;

/// Map a `DesireKind` to the GOAP goal `WorldState` that would satisfy it.
pub fn desire_to_goal(desire: DesireKind) -> WorldState {
    match desire {
        DesireKind::Survive => WorldState::with(Atom::SelfHealthy) | WorldState::with(Atom::ThreatSafe),
        DesireKind::FleeFromDanger => WorldState::with(Atom::ThreatSafe),
        DesireKind::HealGroup => WorldState::with(Atom::GroupHealthy),
        DesireKind::ResurrectDead => WorldState::with(Atom::GroupDeadRezzed),
        DesireKind::DispelDebuffs => WorldState::with(Atom::GroupDebuffsCleansed),
        DesireKind::ProtectAlly => WorldState::with(Atom::AllyProtected),
        DesireKind::TankBoss => WorldState::with(Atom::BossTargeted) | WorldState::with(Atom::ThreatSafe),
        DesireKind::KillTarget => WorldState::with(Atom::TargetDead),
        DesireKind::CrowdControl => WorldState::with(Atom::CcApplied),
        DesireKind::InterruptCast => WorldState::with(Atom::InterruptReady),
        DesireKind::ManageThreat => WorldState::with(Atom::ThreatSafe),
        DesireKind::PullMobs => WorldState::with(Atom::PullInitiated),
        DesireKind::PositionForMechanic => WorldState::with(Atom::MechanicPositioned),
        DesireKind::ExecuteEncounterDuty => WorldState::with(Atom::EncounterDutyDone),
        DesireKind::RecoverResources => WorldState::with(Atom::ResourcesRecovered),
        DesireKind::BuffGroup => WorldState::with(Atom::GroupBuffed),
        DesireKind::RepairGear => WorldState::with(Atom::GearRepaired),
        DesireKind::FollowLeader => WorldState::with(Atom::FollowingLeader),
        DesireKind::GrindMobs => WorldState::with(Atom::TargetDead),
        DesireKind::Travel => WorldState::with(Atom::AtTravelDest),
        DesireKind::Idle => WorldState::default(), // no goal — already satisfied
    }
}

/// Run the GOAP planner for the current intention.
///
/// Returns a `GoapPlan` on success, or keeps the current plan on failure.
pub fn plan_for_intention(
    desire: DesireKind,
    current_ws: WorldState,
    available: &ActionMask,
    server_time_ms: u64,
) -> Option<GoapPlan> {
    let goal = desire_to_goal(desire);

    match planner::plan(
        current_ws,
        goal,
        available,
        actions::registry(),
        DEFAULT_MAX_EXPANSIONS,
        server_time_ms,
    ) {
        PlanResult::Found(plan) => Some(plan),
        PlanResult::Impossible | PlanResult::Incomplete => None,
    }
}

/// Get the strategy flags for the current GOAP plan step.
///
/// Returns `StrategyFlags::NONE` if there is no active plan or the
/// plan is complete.
pub fn current_plan_flags(plan_cache: &PlanCache) -> StrategyFlags {
    let plan = &plan_cache.plan;
    if let Some(action_id) = plan.current_action() {
        let registry = actions::registry();
        if let Some(action) = registry.get(action_id.0 as usize) {
            return action.bt_flags;
        }
    }
    StrategyFlags::NONE
}

/// Write the current GOAP plan state to the blackboard.
pub fn write_to_blackboard(plan_cache: &PlanCache, bb: &mut Blackboard) {
    let plan = &plan_cache.plan;
    bb.set(
        BbKey::GoapCurrentAction,
        BbValue::U32(
            plan.current_action()
                .map_or(u32::MAX, |a| a.0 as u32),
        ),
    );
    bb.set(BbKey::GoapPlanStep, BbValue::U32(plan.current_step as u32));
    bb.set(BbKey::GoapPlanLength, BbValue::U32(plan.len as u32));
}
