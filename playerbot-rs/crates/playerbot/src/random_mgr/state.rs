//! Top-level container for every piece of state the C++
//! `RandomPlayerbotMgr` class held as private fields.
//!
//! [`RandomMgrState`] is the Rust-side singleton. Both the
//! background worker thread and the FFI main-thread entry points
//! take `&mut RandomMgrState` through the usual
//! `OnceLock<Mutex<Option<State>>>` pattern the other Phase E/G
//! subsystems use.
//!
//! # Field map (C++ → Rust)
//!
//! | C++ field                        | Rust equivalent                    |
//! |----------------------------------|------------------------------------|
//! | `pid`                            | [`pid`](RandomMgrState::pid)       |
//! | `activityMod`                    | `activity_mod`                     |
//! | `databaseDelay`                  | `database_delay_ms`                |
//! | `eventCache`                     | `events`                           |
//! | `locsPerLevelCache` + friends    | `teleport`                         |
//! | `namedLocations`                 | `teleport.named_locations`         |
//! | `BgPlayers`/`BgBots`/…           | `buckets`                          |
//! | `BattleMastersCache`             | `buckets.battle_masters`           |
//! | `LfgDungeons`                    | `buckets.lfg_dungeons`             |
//! | `currentBots`                    | `current_bots`                     |
//! | `playersLevel`                   | `players_level`                    |
//! | `processTicks`                   | `process_ticks`                    |
//! | `BgCheckTimer`                   | `timers.bg_check`                  |
//! | `LfgCheckTimer`                  | `timers.lfg_check`                 |
//! | `PlayersCheckTimer`              | `timers.players_check`             |
//! | `EventTimeSyncTimer`             | `timers.event_sync`                |
//! | `OfflineGroupBotsTimer`          | `timers.offline_group_bots`        |
//! | `guildsDeleted` / `arenaTeamsDeleted` | `guilds_deleted` / `arena_teams_deleted` |
//! | `arenaTeamMembers`               | `arena_team_members`               |
//! | `showLoginWarning`               | `show_login_warning`               |
//! | `ahMirror`                       | `ah_mirror`                        |
//!
//! Fields still missing from this struct will land in Phase H.2 / H.3
//! as the worker-tick code that owns them is ported.

use std::collections::HashMap;

use super::buckets::BgBuckets;
use super::events::EventCache;
use super::pid::PidController;
use super::teleport_cache::TeleportCache;

/// Bundle of the periodic "last fired" timestamps the C++
/// `UpdateAIInternal` runs its own rate-limited sub-tasks against.
///
/// Every field is a unix-epoch-seconds stamp (the C++ used `time_t`).
/// The update loop compares `now - timer >= interval` before firing.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateTimers {
    pub bg_check: u32,
    pub lfg_check: u32,
    pub players_check: u32,
    pub event_sync: u32,
    pub offline_group_bots: u32,
}

impl UpdateTimers {
    /// Zero every timer — used by the `reset` console command.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Aggregate state for the Rust random-playerbot manager.
///
/// Construct with [`RandomMgrState::new`] at startup, then drive it
/// from the worker tick via `update_tick(&mut state, world)`.
#[derive(Clone, Debug)]
pub struct RandomMgrState {
    /// PID controller that scales bot activity against the measured
    /// world diff. Matches the C++ `pid` field.
    pub pid: PidController,

    /// `0.0 ..= 1.0` scaling factor that gets multiplied into the
    /// per-tick bot activity budget. The C++ code initialises this
    /// to `0.25` and then nudges it with the PID output every few
    /// seconds.
    pub activity_mod: f32,

    /// Per-database round-trip time in ms — keyed by the legacy C++
    /// database name (e.g. `"CharacterDatabase"`). Updated by the
    /// async ping pass.
    pub database_delay_ms: HashMap<String, u32>,

    /// Event KV store — the `ai_playerbot_random_bots` mirror.
    pub events: EventCache,

    /// Teleport + named-locations cache.
    pub teleport: TeleportCache,

    /// BG / Arena / LFG / battle-master bucket matrices.
    pub buckets: BgBuckets,

    /// FIFO of in-world bot guids the `ProcessBot` loop walks each
    /// tick. Populated by `AddRandomBots` / `LoginFreeBots`.
    pub current_bots: Vec<u32>,

    /// Arena team member guids that need to be kept around on
    /// shutdown/reset. Loaded from `arena_team_member` once at
    /// startup. Classic doesn't use this — the C++ `#ifndef
    /// MANGOSBOT_ZERO` guards the load path.
    pub arena_team_members: Vec<u32>,

    /// Rolling average of real-player levels. Set by `CheckPlayers`
    /// each time it samples online real players. Used by
    /// `AddRandomBots` to keep random bot levels close to real
    /// players.
    pub players_level: u32,

    /// Counts every `UpdateAIInternal` tick that runs — used by
    /// stats output and by the first-run gating in `CheckPlayers`.
    pub process_ticks: u32,

    /// Last-fire timestamps for each of the sub-task cadences the
    /// update loop rate-limits.
    pub timers: UpdateTimers,

    /// True once `CheckPlayers` has deleted dead guilds. One-shot
    /// flag — set on first `CheckPlayers` pass, never unset.
    pub guilds_deleted: bool,

    /// Same pattern as [`guilds_deleted`](Self::guilds_deleted) but
    /// for arena teams.
    pub arena_teams_deleted: bool,

    /// True until the first time an unexpected login error surfaces.
    /// Used to emit a single warning to the server log.
    pub show_login_warning: bool,

    /// Per-item-id auction snapshot. Rebuilt by `MirrorAh`.
    pub ah_mirror: HashMap<u32, Vec<AhMirrorEntry>>,
}

/// One mirrored auction row — matches the payload the C++ code
/// pulled from `AuctionEntry`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AhMirrorEntry {
    pub item_id: u32,
    pub buyout: u32,
    pub count: u32,
}

impl RandomMgrState {
    /// Construct a freshly-initialised state container with the
    /// same starting values the C++ constructor used:
    ///
    /// * `pid` = default, then `adjust(0.05, 0.001, 0.05)`.
    /// * `activity_mod` = `0.25`.
    /// * `show_login_warning` = `true`.
    /// * All other fields zeroed / empty.
    #[must_use]
    pub fn new() -> Self {
        let mut pid = PidController::default();
        pid.adjust(0.05, 0.001, 0.05);
        Self {
            pid,
            activity_mod: 0.25,
            database_delay_ms: HashMap::new(),
            events: EventCache::new(),
            teleport: TeleportCache::new(),
            buckets: BgBuckets::new(),
            current_bots: Vec::new(),
            arena_team_members: Vec::new(),
            players_level: 0,
            process_ticks: 0,
            timers: UpdateTimers::default(),
            guilds_deleted: false,
            arena_teams_deleted: false,
            show_login_warning: true,
            ah_mirror: HashMap::new(),
        }
    }

    /// Read the current activity percentage, `0 ..= 100`. Matches
    /// the C++ `getActivityPercentage()` accessor.
    #[must_use]
    pub fn activity_percentage(&self) -> f32 {
        self.activity_mod * 100.0
    }

    /// Set the activity percentage. Mirrors `setActivityPercentage`.
    pub fn set_activity_percentage(&mut self, percentage: f32) {
        self.activity_mod = percentage / 100.0;
    }

    /// Set the current-bots roster from an iterator. Shared helper
    /// used by `AddRandomBots` and by tests.
    pub fn set_current_bots<I: IntoIterator<Item = u32>>(&mut self, bots: I) {
        self.current_bots.clear();
        self.current_bots.extend(bots);
    }

    /// Drop every piece of in-memory state. Mirrors the console
    /// `reset` command (which ALSO issues a DB-side clear through
    /// the trait — callers must invoke `world.delete_all_events()`
    /// separately).
    pub fn reset_in_memory(&mut self) {
        self.pid.reset();
        self.activity_mod = 0.25;
        self.database_delay_ms.clear();
        self.events.clear();
        self.teleport.clear();
        self.buckets.clear_all();
        self.current_bots.clear();
        self.arena_team_members.clear();
        self.players_level = 0;
        self.process_ticks = 0;
        self.timers.reset();
        self.guilds_deleted = false;
        self.arena_teams_deleted = false;
        self.show_login_warning = true;
        self.ah_mirror.clear();
    }
}

impl Default for RandomMgrState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_matches_cpp_constructor_defaults() {
        let s = RandomMgrState::new();
        assert!((s.activity_mod - 0.25).abs() < 1e-6);
        assert!((s.activity_percentage() - 25.0).abs() < 1e-6);
        assert_eq!(s.players_level, 0);
        assert_eq!(s.process_ticks, 0);
        assert!(s.show_login_warning);
        assert!(!s.guilds_deleted);
        assert!(!s.arena_teams_deleted);
        assert!(s.current_bots.is_empty());
        assert!(s.ah_mirror.is_empty());
    }

    #[test]
    fn activity_percentage_setter_roundtrips() {
        let mut s = RandomMgrState::new();
        s.set_activity_percentage(75.0);
        assert!((s.activity_mod - 0.75).abs() < 1e-6);
        assert!((s.activity_percentage() - 75.0).abs() < 1e-6);
    }

    #[test]
    fn reset_in_memory_zeroes_everything() {
        let mut s = RandomMgrState::new();
        s.players_level = 40;
        s.process_ticks = 999;
        s.guilds_deleted = true;
        s.timers.bg_check = 12_345;
        s.current_bots = vec![1, 2, 3];
        s.ah_mirror.insert(
            1234,
            vec![AhMirrorEntry {
                item_id: 1234,
                buyout: 1000,
                count: 1,
            }],
        );

        s.reset_in_memory();
        assert_eq!(s.players_level, 0);
        assert_eq!(s.process_ticks, 0);
        assert!(!s.guilds_deleted);
        assert_eq!(s.timers.bg_check, 0);
        assert!(s.current_bots.is_empty());
        assert!(s.ah_mirror.is_empty());
    }

    #[test]
    fn set_current_bots_replaces_contents() {
        let mut s = RandomMgrState::new();
        s.set_current_bots([1, 2, 3]);
        assert_eq!(s.current_bots, vec![1, 2, 3]);
        s.set_current_bots([4, 5]);
        assert_eq!(s.current_bots, vec![4, 5]);
    }
}
