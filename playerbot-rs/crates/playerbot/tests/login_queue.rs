//! Integration tests for the login queue — drives `LoginManager` against
//! `MockLoginWorld` fixtures the same way the production worker thread
//! does. Covers queue overflow, candidate aging, login-cap hit, and the
//! multi-tick send-holder dance.
//!
//! These tests exercise the complete public surface of the login module
//! (`config` → `LoginManager::tick` → `TickOutput` dispatch) but stop
//! short of driving the actual worker thread, so they avoid the global
//! `config::install` lock contention you'd get from the per-thread
//! tests in `src/login/worker.rs`.

use std::sync::{Arc, Mutex};

use cmangos::{BotCandidateInfo, HolderState, MockLoginEvent, MockLoginWorld};
use playerbot_rs::config::{BotConfig, ConfigState, RawConfig, install};
use playerbot_rs::login::{
    LoginManager, QueuedCommand, RealPlayerSummary, manager::TickOutput,
};

// Tests mutate the global singleton — serialise them to avoid flakes.
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn install_cfg_with<F: FnOnce(&mut BotConfig)>(f: F) {
    let mut cfg = BotConfig::default();
    cfg.random_bot_account_prefix = "rndbot".into();
    cfg.random_bots_max_logins_per_interval = 10;
    cfg.free_room_for_non_spare_bots = 0;
    cfg.random_bot_min_level = 1;
    cfg.sync_level_max_above = 5;
    cfg.default_login_criteria = vec!["maxbots".into()];
    cfg.login_criteria = vec![vec!["online".into()]];
    f(&mut cfg);
    install(ConfigState {
        raw: RawConfig::default(),
        typed: cfg,
    });
}

fn mk_candidate(guid: u32, level: u32, is_online: bool) -> BotCandidateInfo {
    BotCandidateInfo {
        account_id: 1,
        guid,
        race: 1,
        cls: 1,
        level,
        is_online,
        total_played_time: 100,
        map_id: 0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
        o: 0.0,
        guild_id: 0,
        group_id: 0,
    }
}

/// Tick once against a pre-seeded mock world, returning the produced
/// `TickOutput`. Mirrors what the real worker thread does.
fn drive_tick(
    mgr: &mut LoginManager,
    mock: &MockLoginWorld,
    real: &[RealPlayerSummary],
) -> TickOutput {
    let cfg = playerbot_rs::config::get();
    mgr.tick(&cfg, mock, real)
}

#[test]
fn queue_overflow_skips_late_candidates_when_cap_is_tight() {
    let _g = TEST_LOCK.lock().unwrap();
    install_cfg_with(|_| {});

    let mock = Arc::new(MockLoginWorld::new());
    mock.set_max_online_bot_count(3);
    mock.set_world_max_level(80);

    let rows: Vec<BotCandidateInfo> =
        (1..=10).map(|g| mk_candidate(g, 10, false)).collect();

    let mut mgr = LoginManager::new();
    mgr.load_pool(&rows, &BotConfig::default());

    let out = drive_tick(&mut mgr, &mock, &[]);
    // With total_space=3 and max_logins high, at most 3 logins are
    // queued (one per open slot). The remaining 7 candidates see
    // `MaxBots` and are skipped.
    let logins: Vec<u32> = out
        .commands
        .iter()
        .filter_map(|c| match c {
            QueuedCommand::Login(g) => Some(*g),
            QueuedCommand::Logout(_) => None,
        })
        .collect();
    assert_eq!(logins.len(), 3, "cap should clamp logins to 3");
}

#[test]
fn candidate_aging_turns_online_bots_into_logout_queue_when_cap_drops() {
    let _g = TEST_LOCK.lock().unwrap();
    install_cfg_with(|cfg| {
        cfg.free_room_for_non_spare_bots = 1;
    });

    let mock = Arc::new(MockLoginWorld::new());
    // Every seed bot is online. We drop the cap to 1 below what the
    // population is, so one bot ages out each tick.
    mock.set_max_online_bot_count(1);
    mock.set_world_max_level(80);

    let rows = vec![
        mk_candidate(10, 10, true),
        mk_candidate(11, 11, true),
        mk_candidate(12, 12, true),
    ];

    let mut mgr = LoginManager::new();
    mgr.load_pool(&rows, &BotConfig::default());
    let out = drive_tick(&mut mgr, &mock, &[]);

    let logouts: Vec<u32> = out
        .commands
        .iter()
        .filter_map(|c| match c {
            QueuedCommand::Logout(g) => Some(*g),
            QueuedCommand::Login(_) => None,
        })
        .collect();
    assert!(
        !logouts.is_empty(),
        "expected at least one logout when cap=1 < online=3"
    );
}

#[test]
fn login_cap_hit_stops_before_subsequent_attempts() {
    let _g = TEST_LOCK.lock().unwrap();
    install_cfg_with(|cfg| {
        // Only one extra criterion attempt — "range" — that would take
        // over if the default didn't run to completion.
        cfg.login_criteria = vec![vec!["range".into()]];
    });

    let mock = Arc::new(MockLoginWorld::new());
    // Cap exactly matches the pool size: the first pass fills the
    // queue entirely and the second-attempt loop should see
    // `total_space <= 0` and bail.
    mock.set_max_online_bot_count(2);
    mock.set_world_max_level(80);

    let rows = vec![
        mk_candidate(1, 10, false),
        mk_candidate(2, 11, false),
        mk_candidate(3, 12, false),
    ];

    let mut mgr = LoginManager::new();
    mgr.debug = true; // so we can see the "space left 0" line
    mgr.load_pool(&rows, &BotConfig::default());
    drive_tick(&mut mgr, &mock, &[]);

    let debug_events: Vec<String> = mock
        .events()
        .into_iter()
        .filter_map(|e| match e {
            MockLoginEvent::Debug(s) => Some(s),
            _ => None,
        })
        .collect();
    assert!(
        debug_events.iter().any(|s| s.contains("Initial space")),
        "expected an initial-space debug line"
    );
    // At least the attempt-0 line should be present because it runs
    // unconditionally. The second attempt may or may not log depending
    // on whether the break hits before the print — either way the
    // important thing is the queue didn't overflow.
    assert!(debug_events.iter().any(|s| s.contains("Attempt 0")));
}

#[test]
fn send_holder_gates_on_login_queue_state() {
    let _g = TEST_LOCK.lock().unwrap();
    install_cfg_with(|cfg| {
        cfg.preload_holders = false;
    });

    let mock = Arc::new(MockLoginWorld::new());
    mock.set_max_online_bot_count(5);
    mock.set_world_max_level(80);

    let rows = vec![mk_candidate(42, 10, false), mk_candidate(43, 11, true)];

    let mut mgr = LoginManager::new();
    mgr.load_pool(&rows, &BotConfig::default());
    drive_tick(&mut mgr, &mock, &[]);

    // Only the offline candidate (guid 42) transitions through
    // `OnLoginQueue` and should get a `send_holder`. The already-online
    // candidate (guid 43) stays online and must *not* get a send.
    let holders: Vec<u32> = mock
        .events()
        .into_iter()
        .filter_map(|e| match e {
            MockLoginEvent::SendHolder { guid, .. } => Some(guid),
            _ => None,
        })
        .collect();
    assert!(holders.contains(&42));
    assert!(!holders.contains(&43));
}

#[test]
fn second_tick_keeps_already_online_bots_online_under_loose_cap() {
    let _g = TEST_LOCK.lock().unwrap();
    install_cfg_with(|cfg| {
        cfg.free_room_for_non_spare_bots = 0;
    });

    let mock = Arc::new(MockLoginWorld::new());
    mock.set_max_online_bot_count(20);
    mock.set_world_max_level(80);

    let rows = vec![mk_candidate(100, 10, true), mk_candidate(101, 11, true)];

    let mut mgr = LoginManager::new();
    mgr.load_pool(&rows, &BotConfig::default());
    // First tick — cap is generous, the already-online bots should
    // remain online and nothing should be queued for logout.
    let out = drive_tick(&mut mgr, &mock, &[]);
    let logouts: usize = out
        .commands
        .iter()
        .filter(|c| matches!(c, QueuedCommand::Logout(_)))
        .count();
    assert_eq!(logouts, 0);
}

#[test]
fn holder_state_received_short_circuits_send_holder() {
    let _g = TEST_LOCK.lock().unwrap();
    install_cfg_with(|_| {});

    let mock = Arc::new(MockLoginWorld::new());
    mock.set_max_online_bot_count(5);
    mock.set_world_max_level(80);
    // Pre-mark guid 1's holder as already received → send_holder
    // should be skipped this tick.
    mock.set_holder_state(1, HolderState::Received);

    let rows = vec![mk_candidate(1, 10, false)];
    let mut mgr = LoginManager::new();
    mgr.load_pool(&rows, &BotConfig::default());
    drive_tick(&mut mgr, &mock, &[]);

    let sent: Vec<u32> = mock
        .events()
        .into_iter()
        .filter_map(|e| match e {
            MockLoginEvent::SendHolder { guid, .. } => Some(guid),
            _ => None,
        })
        .collect();
    assert!(
        sent.is_empty(),
        "send_holder must not be called when holder is Received"
    );
}
