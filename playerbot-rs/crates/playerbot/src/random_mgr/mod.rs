//! `random_mgr` — Rust port of the legacy C++ `RandomPlayerbotMgr`.
//!
//! This module owns the "random bot manager" subsystem that spawns,
//! schedules, teleports, and grooms background bots when no explicit
//! owner is online. It is the Rust replacement for
//! `playerbot/RandomPlayerbotMgr.{h,cpp}` (≈4,115 LOC).
//!
//! # Layout
//!
//! * [`pid`] — classical PID controller that scales bot activity
//!   against the world-diff counter. Direct port of `botPIDImpl`.
//! * [`events`] — `ai_playerbot_random_bots` KV store (the
//!   `eventCache` / `GetEventValue` / `SetEventValue` trio) with
//!   TTL semantics and the "sticky" events whitelist.
//! * [`teleport_cache`] — level-bucketed teleport spawn lists plus
//!   the `namedLocations` table.
//! * [`buckets`] — BG / Arena / LFG / battle-master count matrices.
//! * [`state`] — top-level [`state::RandomMgrState`] container that
//!   owns every piece of in-memory state.
//!
//! # FFI boundary
//!
//! Everything in this module operates against `&dyn RandomMgrWorld`
//! (defined in the `cmangos` crate). That trait is implemented by
//! * `VtableRandomMgrWorld` — production impl backed by the C++
//!   `RandomMgrCallbacks` vtable,
//! * `MockRandomMgrWorld` — test-only in-memory impl (feature = `mock`).
//!
//! The worker thread and the FFI-entry main-thread path both go
//! through the same [`state::RandomMgrState`] container.

pub mod ah_mirror;
pub mod bg_lfg;
pub mod buckets;
pub mod commands;
pub mod events;
pub mod pid;
pub mod process;
pub mod scheduler;
pub mod state;
pub mod stats;
pub mod teleport_cache;
pub mod update_loop;
pub mod worker;

#[allow(unsafe_code)]
pub mod ffi;

pub use ah_mirror::{into_mirror_entry, mirror_ah};
pub use bg_lfg::{
    apply_bg_row, apply_lfg_row, check_bg_queue, check_lfg_queue, BG_CHECK_INTERVAL_S,
    LFG_CHECK_INTERVAL_S,
};
pub use stats::{is_alliance_race, print_stats};
pub use buckets::{ArenaKey, BgBuckets, BgKey, TEAM_ALLIANCE, TEAM_BOTH_ALLOWED, TEAM_HORDE};
pub use commands::{
    help_table, parse as parse_command, run_bot_command, run_console_command, BotCommand,
    CommandOutput, ConsoleCommand, ParsedCommand,
};
pub use events::{CachedEvent, EventCache, DEFAULT_VALUE_TTL_S};
pub use pid::PidController;
pub use process::{process_bot, step_bot_actions, step_bot_lifecycle, ProcessContext, ProcessOutcome};
pub use scheduler::{schedule_change_strategy, schedule_randomize, schedule_teleport, ScheduleBounds};
pub use state::{AhMirrorEntry, RandomMgrState, UpdateTimers};
pub use teleport_cache::{InnLocation, TeleportCache, WorldLocation};
pub use update_loop::{
    check_players, ensure_bot_count, save_cur_time, scale_bot_activity, sync_event_timers, tick,
    TickConfig, TickStats,
};
pub use worker::{RandomMgrWorkerHandle, WorkerRequest, WorkerResponse};
