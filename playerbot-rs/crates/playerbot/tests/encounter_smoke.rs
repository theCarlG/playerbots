//! End-to-end smoke coverage for encounter FSMs driven against `MockWorld`.
//!
//! This integration test proves that the Rust AI crate can be exercised
//! cross-crate — from a real `tests/` harness — against the in-memory
//! [`cmangos::MockWorld`] with zero `CMaNGOS` dependency. It is the primary
//! artifact validating the Phase C "testable without `CMaNGOS`" exit
//! criterion: a fresh clone on a stock Ubuntu box runs `cargo test
//! --workspace --features <expansion>` and this file is part of the
//! result.
//!
//! The tests below are deliberately a *scaffold*, not exhaustive coverage:
//! the in-tree `#[cfg(test)]` modules under `crates/playerbot/src/encounters/`
//! exercise every encounter in depth. These smoke tests pick a representative
//! mix (simple FSM, BT execution with class-gated branches, multi-phase FSM,
//! pre-pull invariants) and assert either on FSM state transitions or on
//! `MockWorld.events()` as appropriate.
//!
//! **Note**: the `TestCtxOwned` / `make_encounter_ctx` helpers in
//! `crates/playerbot/src/engine/context.rs` are `#[cfg(test)]`-gated at
//! module scope, which Cargo does not propagate to integration tests in
//! `tests/*.rs`. A minimal equivalent (`SmokeCtx`) is inlined below.

use std::cell::Cell;

use cmangos::{
    BotRole, BotWorldSnapshot, MockEvent, SpellId, UnitHandle, World,
};
use playerbot_rs::bot::settings::{BotSettings, StrategyFlags};
use playerbot_rs::bot::state::PlayerClass;
use playerbot_rs::encounters::{
    EncounterEvent, EncounterFsm,
    molten_core::{
        baron_geddon::{AURA_LIVING_BOMB, BaronGeddonFsm},
        lucifron::LucifronFsm,
        ragnaros::{RagnarosFsm, RagnarosPhase},
    },
};
use playerbot_rs::engine::{
    blackboard::Blackboard,
    bt_nodes::{BtNode, BtResult},
    context::TickContext,
    macro_fsm::ActiveFsm,
    throttles::Throttles,
    timers::BotTimers,
};

/// Spell IDs referenced by the Baron Geddon BT. Kept private to this file
/// — the encounter module re-exports `AURA_LIVING_BOMB` but keeps these
/// internal.
const ICE_BLOCK: SpellId = SpellId(11958);

/// Owning scaffold for a `TickContext`. Mirrors `TestCtxOwned` from
/// `engine::context::tests`, minus the fields this file's tests don't
/// need. Leaking the settings is fine — the leak is tiny and
/// process-scoped, same pattern the in-tree helpers use.
struct SmokeCtx {
    snap: BotWorldSnapshot,
    nearby: Vec<UnitHandle>,
    attackers: Vec<UnitHandle>,
    blackboard: Blackboard,
    timers: BotTimers,
    throttles: Throttles,
    settings: BotSettings,
}

impl SmokeCtx {
    fn new() -> Self {
        Self {
            snap: BotWorldSnapshot::default(),
            nearby: Vec::new(),
            attackers: Vec::new(),
            blackboard: Blackboard::default(),
            timers: BotTimers::new(),
            throttles: Throttles::new(),
            settings: BotSettings::default(),
        }
    }

    fn make_ctx<'a>(
        &'a mut self,
        world: &'a dyn World,
        encounter: &'a dyn EncounterFsm,
        class: PlayerClass,
        role: BotRole,
    ) -> TickContext<'a> {
        TickContext {
            snap: &self.snap,
            nearby: &self.nearby,
            attackers: &self.attackers,
            group_state: None,
            group_handle: None,
            interface: world,
            blackboard: &mut self.blackboard,
            timers: &mut self.timers,
            throttles: &mut self.throttles,
            server_time_ms: 10_000,
            elapsed_ms: 100,
            minimal: false,
            bot_handle: 0,
            master_guid: None,
            active_fsm: ActiveFsm::Combat,
            encounter: Some(encounter),
            class,
            role,
            settings: &self.settings,
            goap_flags: StrategyFlags::NONE,
            monitor_trace: None,
            pending_target: Cell::new(None),
        }
    }
}

/* ── Tests ──────────────────────────────────────────────────────────────── */

#[test]
fn lucifron_fsm_transitions() {
    // Simple single-phase FSM. Drive it through the full happy path
    // (pull → active → kill → done) and a wipe path, asserting the
    // public state accessors at each step.
    let mut fsm = LucifronFsm::default();
    assert!(!fsm.is_active(), "default FSM is inactive");
    assert!(!fsm.is_done(), "default FSM is not done");
    assert_eq!(fsm.phase_id(), 0, "default phase id is 0 (pre-pull)");

    fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
    assert!(fsm.is_active(), "CombatStarted makes the FSM active");
    assert!(!fsm.is_done(), "still not done immediately after pull");
    assert_eq!(fsm.phase_id(), 1, "active phase id is 1");

    fsm.update(&EncounterEvent::UnitDied { victim: 42 }, 0.0, 1000);
    assert!(fsm.is_done(), "UnitDied marks the FSM done");

    // Wipe path starts fresh.
    let mut fsm = LucifronFsm::default();
    fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
    fsm.update(&EncounterEvent::GroupWipe, 1.0, 500);
    assert!(!fsm.is_active(), "GroupWipe deactivates the FSM");
}

#[test]
fn baron_geddon_living_bomb_mage_ice_blocks() {
    // Mage with living bomb → BT fires Ice Block and reports Success.
    // The MockWorld event log should contain the Ice Block cast.
    let mut fsm = BaronGeddonFsm::default();
    fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
    let bt = fsm
        .phase_bt(ActiveFsm::Combat)
        .expect("active FSM returns a phase BT");

    let world = cmangos::MockWorld::new().with_aura(AURA_LIVING_BOMB);
    let mut owned = SmokeCtx::new();
    let mut ctx = owned.make_ctx(&world, &fsm, PlayerClass::Mage, BotRole::DPS);

    assert_eq!(
        bt.tick(&mut ctx),
        BtResult::Success,
        "mage ice-block path returns Success"
    );

    let events = world.events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            MockEvent::CastSpell { spell, .. } if *spell == ICE_BLOCK
        )),
        "expected Ice Block cast in event log, got: {events:?}"
    );
}

#[test]
fn baron_geddon_living_bomb_warrior_flees() {
    // Warrior with living bomb → BT flees to the safe position and
    // reports Running. A MoveTo must land in the event log.
    let mut fsm = BaronGeddonFsm::default();
    fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
    let bt = fsm
        .phase_bt(ActiveFsm::Combat)
        .expect("active FSM returns a phase BT");

    let world = cmangos::MockWorld::new()
        .with_aura(AURA_LIVING_BOMB)
        .with_safe_pos();
    let mut owned = SmokeCtx::new();
    let mut ctx = owned.make_ctx(&world, &fsm, PlayerClass::Warrior, BotRole::DPS);

    assert!(
        matches!(bt.tick(&mut ctx), BtResult::Running),
        "warrior flee path returns Running while movement is in progress"
    );

    let events = world.events();
    assert!(
        events.iter().any(|e| matches!(e, MockEvent::MoveTo { .. })),
        "expected MoveTo in event log, got: {events:?}"
    );
}

#[test]
fn ragnaros_ground_to_submerged_phase() {
    // Multi-phase FSM: pull → Ground → submerge at 75% → Submerged.
    // Then kill 8 Sons of Flame → back to Ground.
    let mut fsm = RagnarosFsm::default();
    assert_eq!(fsm.phase, RagnarosPhase::Idle);
    assert!(!fsm.is_active());

    fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
    assert_eq!(fsm.phase, RagnarosPhase::Ground);
    assert!(fsm.is_active());
    assert_eq!(fsm.phase_id(), RagnarosFsm::PHASE_GROUND);

    // Drop below the 75% threshold — the tick-event path should submerge.
    fsm.update(&EncounterEvent::None, 0.74, 1000);
    assert_eq!(fsm.phase, RagnarosPhase::Submerged);
    assert_eq!(fsm.phase_id(), RagnarosFsm::PHASE_SUBMERGED);

    // 8 sons killed → return to Ground.
    for i in 0..8_u64 {
        fsm.update(
            &EncounterEvent::UnitDied { victim: 1000 + i },
            0.74,
            2000 + i,
        );
    }
    assert_eq!(fsm.phase, RagnarosPhase::Ground);
    assert_eq!(
        fsm.phase_id(),
        RagnarosFsm::PHASE_GROUND,
        "after 8 sons the FSM returns to Ground"
    );
    assert!(!fsm.is_done(), "boss is not dead yet");
}

#[test]
fn encounter_fsm_pre_pull_returns_no_bt() {
    // Every encounter FSM in its default state returns None from
    // phase_bt(). The BT only becomes available once CombatStarted has
    // fired. This guards against regressions where a new FSM returns a
    // BT pre-pull and the bot starts running mechanics before the boss
    // is engaged.
    assert!(
        LucifronFsm::default().phase_bt(ActiveFsm::Combat).is_none(),
        "LucifronFsm pre-pull must not return a BT"
    );
    assert!(
        BaronGeddonFsm::default()
            .phase_bt(ActiveFsm::Combat)
            .is_none(),
        "BaronGeddonFsm pre-pull must not return a BT"
    );
    assert!(
        RagnarosFsm::default().phase_bt(ActiveFsm::Combat).is_none(),
        "RagnarosFsm pre-pull must not return a BT"
    );
}
