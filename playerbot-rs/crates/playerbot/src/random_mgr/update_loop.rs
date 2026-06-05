//! `UpdateAIInternal` port — the main worker-tick entry point that
//! runs every `randomBotUpdateInterval` milliseconds and fans out to
//! every sub-task the C++ `RandomPlayerbotMgr::UpdateAIInternal`
//! drove:
//!
//! * `ScaleBotActivity` — PID-scaled activity modifier.
//! * `SaveCurTime` — persist the current wall-clock into the KV
//!   store so `SyncEventTimers` can catch up after a restart.
//! * `CheckPlayers` — compute `playersLevel = max real-player level`.
//! * `AddRandomBots` — top up the in-world bot count.
//! * `ProcessBot` loop — walk each bot, fire its scheduled actions.
//! * `LoginFreeBots` — (deferred to Phase E's login worker).
//! * `MirrorAh` — (deferred to Phase H.3).
//! * `database_ping` — async latency probe.
//!
//! This module holds the top-level [`tick`] function plus a handful
//! of small helpers. `tick` is deliberately pure: it takes
//! `&mut RandomMgrState`, `&dyn RandomMgrWorld`, `&TickConfig` and a
//! wall-clock `now_epoch_s`, and makes zero assumptions about how
//! the worker thread drives it.

use cmangos::RandomMgrWorld;

use super::process::{self, ProcessContext, ProcessOutcome};
use super::scheduler::ScheduleBounds;
use super::state::RandomMgrState;
use crate::config::BotConfig;

/// Per-tick configuration bundle. The worker thread packs up the
/// config values it needs once at startup + every reload, and passes
/// a borrow here instead of dragging the full `BotConfig` through the
/// call chain. Keeping this narrow makes the tick unit-testable
/// without a global config dance.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TickConfig {
    /// `randomBotAutologin && enabled` — top-level kill switch. When
    /// false, [`tick`] returns immediately after the initial Rust
    /// side-effects (this mirrors the C++ early return).
    pub random_bot_autologin: bool,
    /// `randomBotLoginWithPlayer` — when true, bots only log in when
    /// at least one real player is online.
    pub random_bot_login_with_player: bool,
    /// `minRandomBots`, `maxRandomBots` — the `bot_count` event
    /// bracket rolled whenever the current event is absent/stale.
    pub min_random_bots: u32,
    pub max_random_bots: u32,
    /// Interval bracket passed to `SetEventValue("bot_count", …)`.
    pub bot_count_change_min_s: u32,
    pub bot_count_change_max_s: u32,
    /// `syncLevelNoPlayer` — fallback `playersLevel` used when no
    /// real players are online and the rolling average hasn't been
    /// seeded yet.
    pub sync_level_no_player: u32,
    /// `syncLevelWithPlayers` — gate for `CheckPlayers` sub-task.
    pub sync_level_with_players: bool,
    /// `randomBotJoinLfg` — gate for `CheckLfgQueue` sub-task.
    pub random_bot_join_lfg: bool,
    /// `randomBotJoinBG` — gate for `CheckBgQueue` sub-task.
    pub random_bot_join_bg: bool,
    /// `randomBotsPerInterval` — how many `ProcessBot` fires are
    /// allowed per tick before we break out of the loop. `0` means
    /// "unlimited".
    pub random_bots_per_interval: u32,
    /// Process context passed to [`process::process_bot`] for each
    /// bot the tick walks.
    pub process_context: ProcessContext,
    /// `diffEmpty`, `diffWithPlayer` — PID setpoints. Picked based
    /// on whether real players are currently online.
    pub diff_empty_ms: f64,
    pub diff_with_player_ms: f64,
}

impl Default for TickConfig {
    fn default() -> Self {
        Self {
            random_bot_autologin: true,
            random_bot_login_with_player: false,
            min_random_bots: 50,
            max_random_bots: 200,
            bot_count_change_min_s: 300,
            bot_count_change_max_s: 900,
            sync_level_no_player: 25,
            sync_level_with_players: true,
            random_bot_join_lfg: false,
            random_bot_join_bg: false,
            random_bots_per_interval: 0,
            process_context: ProcessContext::default(),
            diff_empty_ms: 100.0,
            diff_with_player_ms: 150.0,
        }
    }
}

impl TickConfig {
    /// Snapshot the relevant slice of [`BotConfig`] into a fresh
    /// [`TickConfig`]. Called once per tick by the worker thread so we
    /// pick up `/bot reload` changes between iterations.
    #[must_use]
    pub fn from_bot_config(cfg: &BotConfig) -> Self {
        let bounds = ScheduleBounds {
            teleport_min_s: cfg.random_bot_teleport_min_interval,
            teleport_max_s: cfg.random_bot_teleport_max_interval,
            change_strategy_min_s: cfg.min_random_bot_change_strategy_time,
            change_strategy_max_s: cfg.max_random_bot_change_strategy_time,
            randomize_min_s: cfg.min_random_bot_randomize_time,
            randomize_max_s: cfg.max_random_bot_randomize_time,
        };
        let process_context = ProcessContext {
            enable_random_teleports: cfg.enable_random_teleports,
            rpg_chance: cfg.random_bot_rpg_chance,
            disable_random_levels: cfg.disable_random_levels,
            // `has_real_players` and `bots_allowed_in_world` are
            // recomputed in `tick()` against the world snapshot. The
            // values we seed here are replaced before any bot is
            // processed.
            has_real_players: true,
            bots_allowed_in_world: true,
            timed_logout: cfg.random_bot_timed_logout,
            timed_offline: cfg.random_bot_timed_offline,
            async_bot_login: cfg.async_bot_login,
            bounds,
            in_world_time_bounds: (
                cfg.min_random_bot_in_world_time,
                cfg.max_random_bot_in_world_time,
            ),
        };
        Self {
            random_bot_autologin: cfg.random_bot_autologin,
            random_bot_login_with_player: cfg.random_bot_login_with_player,
            min_random_bots: cfg.min_random_bots,
            max_random_bots: cfg.max_random_bots,
            bot_count_change_min_s: cfg.random_bot_count_change_min_interval,
            bot_count_change_max_s: cfg.random_bot_count_change_max_interval,
            sync_level_no_player: cfg.sync_level_no_player,
            sync_level_with_players: cfg.sync_level_with_players,
            random_bot_join_lfg: cfg.random_bot_join_lfg,
            random_bot_join_bg: cfg.random_bot_join_bg,
            random_bots_per_interval: cfg.random_bots_per_interval,
            process_context,
            diff_empty_ms: f64::from(cfg.diff_empty),
            diff_with_player_ms: f64::from(cfg.diff_with_player),
        }
    }
}

/// Summary of what a single [`tick`] call did — the worker thread
/// reports these back to the main thread for logging.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct TickStats {
    pub bots_processed: u32,
    pub bots_randomized: u32,
    pub bots_changed_strategy: u32,
    pub bots_teleported: u32,
    pub bots_logged_out: u32,
    pub bots_skipped: u32,
    pub bots_idle: u32,
    pub bot_count_target: u32,
    /// Snapshot of `state.players_level` at the end of the tick.
    /// Copied back to the main thread so it can answer
    /// `GetPlayersLevel()` without crossing the worker channel.
    pub players_level: u32,
}

/// Run one worker-tick pass against `state`. Returns per-tick stats
/// for caller logging.
pub fn tick(
    state: &mut RandomMgrState,
    world: &dyn RandomMgrWorld,
    cfg: &TickConfig,
    now_epoch_s: u32,
) -> TickStats {
    let mut stats = TickStats::default();

    if !cfg.random_bot_autologin {
        return stats;
    }

    state.process_ticks = state.process_ticks.saturating_add(1);

    // --- ScaleBotActivity --------------------------------------------
    scale_bot_activity(state, world, cfg);

    // Seed players_level if CheckPlayers hasn't run yet.
    if state.players_level == 0 {
        state.players_level = cfg.sync_level_no_player;
    }

    // --- Ensure bot_count event is fresh -----------------------------
    let target = ensure_bot_count(state, world, cfg, now_epoch_s);
    stats.bot_count_target = target;

    // --- SaveCurTime ------------------------------------------------
    if now_epoch_s > state.timers.event_sync + 30 {
        save_cur_time(state, world, now_epoch_s);
    }

    // --- CheckPlayers ----------------------------------------------
    if cfg.sync_level_with_players
        && !world.query_real_player_levels().is_empty()
        && now_epoch_s > state.timers.players_check + 60
    {
        check_players(state, world, now_epoch_s);
    }

    // --- Process bots ----------------------------------------------
    let available = world.owned_bot_guids();
    let max_per_tick = if cfg.random_bots_per_interval == 0 {
        u32::MAX
    } else {
        cfg.random_bots_per_interval
    };
    let mut fired = 0u32;

    // Build an immutable snapshot of the process context so we don't
    // have to deal with borrow conflicts against `state.events`.
    let mut ctx = cfg.process_context;
    ctx.has_real_players = !world.query_real_player_levels().is_empty();
    // C++: `botsAllowedInWorld = !randomBotLoginWithPlayer || !players.empty()`.
    ctx.bots_allowed_in_world =
        !cfg.random_bot_login_with_player || ctx.has_real_players;

    for bot in &available {
        let outcome = process::process_bot(&mut state.events, world, *bot, ctx, now_epoch_s);
        stats.bots_processed += 1;
        match outcome {
            ProcessOutcome::Randomized => {
                stats.bots_randomized += 1;
                fired += 1;
            }
            ProcessOutcome::ChangedStrategy => {
                stats.bots_changed_strategy += 1;
                fired += 1;
            }
            ProcessOutcome::Teleported => {
                stats.bots_teleported += 1;
                fired += 1;
            }
            ProcessOutcome::LoggedOut => {
                stats.bots_logged_out += 1;
                fired += 1;
            }
            ProcessOutcome::Skipped => stats.bots_skipped += 1,
            ProcessOutcome::Idle => stats.bots_idle += 1,
        }
        if fired >= max_per_tick {
            break;
        }
    }

    // --- Tail sub-tasks --------------------------------------------
    // The C++ tick ends with `LoginFreeBots`, `DelayedFacingFix`,
    // `MirrorAh`, and `AsyncPQuery("SELECT 1")`. Phase H.3 will port
    // the AH mirror pass; the facing-fix and login-free helpers are
    // owned by other phases (they touch `sMapMgr` and the login
    // queue respectively). Here we only keep the DB ping.
    world.database_ping();

    stats.players_level = state.players_level;
    stats
}

/// Mirror of `ScaleBotActivity`: run the PID against the current
/// world diff and clamp the result into `[0, 100]`.
pub fn scale_bot_activity(
    state: &mut RandomMgrState,
    world: &dyn RandomMgrWorld,
    cfg: &TickConfig,
) {
    let sample = world.world_diff_sample();
    let setpoint = if world.query_real_player_levels().is_empty() {
        cfg.diff_empty_ms
    } else {
        cfg.diff_with_player_ms
    };
    let pv = f64::from(sample.average_diff_ms);
    let modifier = state.pid.calculate(setpoint, pv);
    let pct = (modifier + 50.0).clamp(0.0, 100.0) as f32;
    state.set_activity_percentage(pct);
}

/// Top up the `(0, "bot_count")` event when it is stale or out of
/// range. Returns the value that is now in effect for the tick.
pub fn ensure_bot_count(
    state: &mut RandomMgrState,
    world: &dyn RandomMgrWorld,
    cfg: &TickConfig,
    now_epoch_s: u32,
) -> u32 {
    let current = state.events.get_value(0, "bot_count", now_epoch_s, world);
    if current >= cfg.min_random_bots && current <= cfg.max_random_bots {
        return current;
    }
    let roll = world.urand_range(cfg.min_random_bots, cfg.max_random_bots);
    let ttl = world.urand_range(cfg.bot_count_change_min_s, cfg.bot_count_change_max_s);
    state.events.set_value(0, "bot_count", roll, ttl, "", now_epoch_s, world);
    roll
}

/// Persist the current wall-clock into the `(0, "current_time")`
/// event. Mirrors the C++ `SaveCurTime()` helper verbatim.
pub fn save_cur_time(state: &mut RandomMgrState, world: &dyn RandomMgrWorld, now_epoch_s: u32) {
    state.timers.event_sync = now_epoch_s;
    state.events.set_value(
        0,
        "current_time",
        now_epoch_s,
        0,
        "",
        now_epoch_s,
        world,
    );
}

/// Run the post-restart TTL catch-up pass. Mirrors
/// `SyncEventTimers()` — reads back the stored `current_time`,
/// computes the delta since then, and bumps every row's TTL
/// timestamp through `bump_event_times`.
pub fn sync_event_timers(state: &mut RandomMgrState, world: &dyn RandomMgrWorld, now_epoch_s: u32) {
    let old_time = state.events.get_value(0, "current_time", now_epoch_s, world);
    if old_time == 0 {
        return;
    }
    let delta = now_epoch_s.saturating_sub(old_time);
    if delta == 0 {
        return;
    }
    world.bump_event_times(delta);
    // Keep the in-memory cache consistent with the DB side.
    state.events.bump_all_times(delta);
}

/// Update `players_level` by scanning the real-player snapshot the
/// world trait hands us. Matches the C++ `CheckPlayers()` function.
pub fn check_players(state: &mut RandomMgrState, world: &dyn RandomMgrWorld, now_epoch_s: u32) {
    state.timers.players_check = now_epoch_s;
    let rows = world.query_real_player_levels();
    let max_level = rows.iter().map(|r| r.level).max().unwrap_or(0);
    state.players_level = max_level;
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::{MockRandomMgrEvent, MockRandomMgrWorld, RealPlayerLevel, WorldDiffSample};

    fn ctx_cfg() -> TickConfig {
        TickConfig {
            random_bot_autologin: true,
            random_bot_login_with_player: false,
            min_random_bots: 5,
            max_random_bots: 10,
            bot_count_change_min_s: 60,
            bot_count_change_max_s: 120,
            sync_level_no_player: 25,
            sync_level_with_players: true,
            random_bot_join_lfg: false,
            random_bot_join_bg: false,
            random_bots_per_interval: 0,
            process_context: ProcessContext::default(),
            diff_empty_ms: 100.0,
            diff_with_player_ms: 150.0,
        }
    }

    #[test]
    fn tick_returns_early_when_autologin_disabled() {
        let world = MockRandomMgrWorld::new();
        let mut state = RandomMgrState::new();
        let mut cfg = ctx_cfg();
        cfg.random_bot_autologin = false;
        let stats = tick(&mut state, &world, &cfg, 1000);
        assert_eq!(stats.bots_processed, 0);
        assert_eq!(state.process_ticks, 0);
    }

    #[test]
    fn tick_seeds_players_level_and_bumps_process_ticks() {
        let world = MockRandomMgrWorld::new();
        world.set_world_diff_sample(WorldDiffSample {
            current_diff_ms: 100.0,
            average_diff_ms: 80.0,
            max_diff_ms: 120.0,
        });
        let mut state = RandomMgrState::new();

        let stats = tick(&mut state, &world, &ctx_cfg(), 1000);

        assert_eq!(state.process_ticks, 1);
        assert_eq!(state.players_level, 25);
        // No bots to process → target still gets rolled.
        assert!(stats.bot_count_target >= 5 && stats.bot_count_target <= 10);
    }

    #[test]
    fn tick_processes_each_owned_bot() {
        let world = MockRandomMgrWorld::new();
        world.set_owned_guids(vec![1, 2, 3]);
        world.set_world_diff_sample(WorldDiffSample {
            current_diff_ms: 100.0,
            average_diff_ms: 100.0,
            max_diff_ms: 100.0,
        });
        let mut state = RandomMgrState::new();
        // Prevent the logout branch from firing so we can watch the
        // randomize path fire instead.
        state.events.set_value(1, "add", 1, 10_000, "", 0, &world);
        state.events.set_value(2, "add", 1, 10_000, "", 0, &world);
        state.events.set_value(3, "add", 1, 10_000, "", 0, &world);

        let stats = tick(&mut state, &world, &ctx_cfg(), 1000);
        assert_eq!(stats.bots_processed, 3);
        assert_eq!(stats.bots_randomized, 3);
    }

    #[test]
    fn scale_bot_activity_applies_pid_output() {
        let world = MockRandomMgrWorld::new();
        world.set_world_diff_sample(WorldDiffSample {
            current_diff_ms: 80.0,
            average_diff_ms: 80.0,
            max_diff_ms: 80.0,
        });
        let mut state = RandomMgrState::new();
        // C++ default gains: (0.05, 0.001, 0.05), setpoint 100, pv 80
        // → error=20, p=1, i=0.02, d=(20-0)/1*0.05=1 → 2.02 → +50 = 52.02
        scale_bot_activity(&mut state, &world, &ctx_cfg());
        assert!((state.activity_percentage() - 52.02).abs() < 0.1);
    }

    #[test]
    fn ensure_bot_count_keeps_fresh_value() {
        let world = MockRandomMgrWorld::new();
        let mut state = RandomMgrState::new();
        // Pre-seed a valid `bot_count` event.
        state
            .events
            .set_value(0, "bot_count", 7, 10_000, "", 0, &world);

        let target = ensure_bot_count(&mut state, &world, &ctx_cfg(), 100);
        assert_eq!(target, 7);
        // No new upsert should have fired — assert by counting events
        // on the mock (only the initial set_value is present).
        let ev = world.events();
        let upserts = ev
            .iter()
            .filter(|e| matches!(e, MockRandomMgrEvent::UpsertEvent(_)))
            .count();
        assert_eq!(upserts, 1);
    }

    #[test]
    fn ensure_bot_count_rolls_when_out_of_range() {
        let world = MockRandomMgrWorld::new();
        let mut state = RandomMgrState::new();
        // Seed with an out-of-range value (below the min).
        state
            .events
            .set_value(0, "bot_count", 2, 10_000, "", 0, &world);

        let target = ensure_bot_count(&mut state, &world, &ctx_cfg(), 100);
        assert!(target >= 5 && target <= 10);
    }

    #[test]
    fn save_cur_time_updates_event_and_timer() {
        let world = MockRandomMgrWorld::new();
        let mut state = RandomMgrState::new();
        save_cur_time(&mut state, &world, 12345);
        assert_eq!(state.timers.event_sync, 12345);
        // The event should be readable back with a sticky (=0 TTL)
        // lookup.
        // `current_time` is in the STICKY_EVENTS whitelist.
        assert_eq!(state.events.get_value(0, "current_time", 99999, &world), 12345);
    }

    #[test]
    fn sync_event_timers_bumps_stored_rows() {
        let world = MockRandomMgrWorld::new();
        let mut state = RandomMgrState::new();
        save_cur_time(&mut state, &world, 1000);
        // Pretend we restarted 500 seconds later.
        sync_event_timers(&mut state, &world, 1500);
        let ev = world.events();
        assert!(
            ev.iter()
                .any(|e| matches!(e, MockRandomMgrEvent::BumpEventTimes(500))),
            "expected BumpEventTimes(500)"
        );
    }

    #[test]
    fn check_players_records_max_level() {
        let world = MockRandomMgrWorld::new();
        world.set_real_player_levels(vec![
            RealPlayerLevel {
                level: 10,
                total_time: 100,
            },
            RealPlayerLevel {
                level: 42,
                total_time: 500,
            },
            RealPlayerLevel {
                level: 30,
                total_time: 200,
            },
        ]);
        let mut state = RandomMgrState::new();
        check_players(&mut state, &world, 2000);
        assert_eq!(state.players_level, 42);
        assert_eq!(state.timers.players_check, 2000);
    }
}
