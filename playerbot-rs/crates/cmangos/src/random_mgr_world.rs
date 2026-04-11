//! `RandomMgrWorld` — module-level interface the Rust random-playerbot
//! manager (Phase H of the migration) uses to reach `CMaNGOS`.
//!
//! Phase H moves the old `playerbot/RandomPlayerbotMgr.cpp` (≈4100 LOC)
//! into the Rust `playerbot` crate's `random_mgr` module. Like the
//! neighbouring [`LoginWorld`](crate::LoginWorld),
//! [`ItemWorld`](crate::ItemWorld) and
//! [`RandomFactoryWorld`](crate::RandomFactoryWorld) traits, every entry
//! here is a module-level call — nothing is per-bot, because the manager
//! itself is the singleton that owns the per-bot state.
//!
//! Responsibility split mirrors the legacy C++ class:
//!
//! * **Event KV store** — reads/writes against
//!   `ai_playerbot_random_bots` (one row per bot × event name).
//! * **Per-bot action dispatch** — `Randomize`, `Revive`, `ChangeStrategy`,
//!   `Refresh`, `Teleport`, etc. The bridge owns the `PlayerbotFactory`
//!   and `Player*` objects; Rust just issues the commands.
//! * **Teleport cache building** — `ai_playerbot_tele_cache` and its RPG
//!   flavour live in DB tables; the bridge populates them on demand from
//!   creature / game-object / quest-pool iteration.
//! * **BG/LFG/arena queue inspection** — the queue tables live in `CMaNGOS`
//!   globals; the bridge reads them into POD snapshots the worker tick
//!   consumes.
//! * **Auction house mirror** — `sAuctionMgr` scrape into a per-item price
//!   table.
//! * **Misc globals** — world diff, max level, "is shutting down" flag,
//!   current `MSTime` for DB delay tracking, RNG range for `urand`.
//!
//! Two implementations exist, mirroring `LoginWorld`:
//!
//! * [`VtableRandomMgrWorld`] — production impl, wraps a
//!   `RandomMgrCallbacks` struct handed in by the C++ bridge.
//! * [`MockRandomMgrWorld`] — feature-gated under `mock`; a pure-Rust
//!   fixture impl used by the `crates/playerbot` random-mgr tests.

use crate::RandomMgrCallbacks;

/// One row from the `ai_playerbot_random_bots` table — the on-disk form
/// of the event KV store that drives `Randomize`, `logout`, `login`,
/// `bot_count` and every other runtime event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventRow {
    pub bot: u32,
    pub event: String,
    /// Numeric payload — most events store a cooldown or count here.
    pub value: u32,
    /// Seconds since epoch the row was last written.
    pub last_change_time: u32,
    /// TTL in seconds; `0` means "no TTL, don't expire".
    pub valid_in: u32,
    /// Free-form text payload — some events carry a reason string.
    pub data: String,
}

/// Snapshot of a real player used to evaluate player-level syncing,
/// distance gates on `CheckPlayers`, and "log in when a real player is
/// around" gating.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RealPlayerLevel {
    pub level: u32,
    /// Accumulated played time; used to recognise level-1 brand-new
    /// characters the legacy code ignored for level-averaging.
    pub total_time: u32,
}

/// World-diff summary used by [`RandomMgrWorld::world_diff_sample`].
/// Mirrors the three values the legacy C++ `ScaleBotActivity` pulled
/// from `sWorld` to drive the PID controller.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct WorldDiffSample {
    pub current_diff_ms: f32,
    pub average_diff_ms: f32,
    pub max_diff_ms: f32,
}

/// BG / Arena queue row used by `CheckBgQueue`. One per (queue type,
/// bracket id, team) cell the legacy C++ counter matrix stored.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BgQueueEntry {
    pub queue_type: u32,
    pub bracket_id: u32,
    /// `0 = Alliance`, `1 = Horde` — matches the C++ `TeamId` convention.
    pub team_id: u32,
    /// `true` if this row is a bot, `false` if it is a real player.
    pub is_bot: bool,
    /// Bot level (for bracketing). 0 on player rows.
    pub level: u32,
    pub map_id: u32,
    /// Arena team rating bucket — only populated when `arena_type != 0`.
    pub arena_rating: u32,
    /// Arena team type: `0 = non-arena`, `2 = 2v2`, `3 = 3v3`, `5 = 5v5`.
    pub arena_type: u8,
}

/// LFG queue summary used by `CheckLfgQueue`. One row per dungeon a
/// real player is currently queued for.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LfgQueueEntry {
    /// `0 = Alliance`, `1 = Horde`, `2 = both` (the legacy C++ maps
    /// `Team` onto these three buckets).
    pub team_id: u8,
    pub dungeon_id: u32,
    pub role_mask: u32,
}

/// Auction-house mirror row — one per row in the live auction table.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AhMirrorRow {
    pub item_id: u32,
    pub buyout: u32,
    pub count: u32,
}

bitflags::bitflags! {
    /// Flag bitmask for [`BotStatsSnapshotRow::flags`]. The bit values
    /// must stay in lock-step with the `PLAYERBOT_STATS_FLAG_*` defines
    /// in `cpp_wrapper/botffi.h` — the Vtable impl converts between
    /// them byte-for-byte.
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
    pub struct BotStatsFlags: u32 {
        /// `PlayerbotAI::IsActive()` — the bot ran at least one tick in
        /// the last second.
        const ACTIVE  = 1 << 0;
        /// `Player::IsAlive()` returned `false`.
        const DEAD    = 1 << 1;
        /// `Player::IsInCombat()` returned `true`.
        const COMBAT  = 1 << 2;
        /// `Player::IsTaxiFlying()` returned `true`.
        const TAXI    = 1 << 3;
        /// `Player::IsMoving()` returned `true` (and not on taxi/flight).
        const MOVING  = 1 << 4;
        /// `Player::IsMounted()` returned `true` (and not on taxi).
        const MOUNTED = 1 << 5;
        /// `Player::isAFK()` returned `true`.
        const AFK     = 1 << 6;
    }
}

/// Per-bot stats snapshot row, used by `print_stats` to build the
/// `/bot stats` chat output. Mirrors the iteration the C++
/// `PrintStats` did over the live `players` map.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BotStatsSnapshotRow {
    pub guid: u32,
    /// Mangos `Races` enum value.
    pub race: u8,
    /// Mangos `Classes` enum value.
    pub class_id: u8,
    /// `0 = Alliance, 1 = Horde`. Duplicated from the race lookup so
    /// the Rust side doesn't have to carry the race → team table.
    pub team: u8,
    pub level: u32,
    pub flags: BotStatsFlags,
}

/// One row from `ai_playerbot_named_location` — resolved by `LoadNamedLocations`.
#[derive(Clone, Debug, PartialEq)]
pub struct NamedLocationRow {
    pub name: String,
    pub map_id: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub orientation: f32,
}

/// One row of `ai_playerbot_tele_cache`. The bridge either yields
/// pre-built rows or triggers a regeneration pass that writes them back.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct TeleportRow {
    pub level: u32,
    pub map_id: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Abstract interface the random-playerbot manager uses to reach `CMaNGOS`.
///
/// Every method is invoked from the thread that drives
/// [`random_mgr::update_tick`](../../playerbot/random_mgr/index.html) —
/// typically the Rust worker thread. Commands that mutate world state
/// (`dispatch_*`) are forwarded back to the main thread through an
/// `mpsc` channel so the vtable is free to block on the DB without
/// stalling the world update.
pub trait RandomMgrWorld: Send + Sync {
    // ─── RNG ───────────────────────────────────────────────────────────
    /// Pseudo-random integer in `[min, max]` (both inclusive). Wraps
    /// `CMaNGOS` `urand`.
    fn urand_range(&self, min: u32, max: u32) -> u32;

    // ─── World globals ─────────────────────────────────────────────────
    /// `sWorld.GetCurrentMSTime()` — unix-millisecond counter used for
    /// DB delay tracking.
    fn current_ms_time(&self) -> u32;

    /// `sWorld.IsShutdowning()` — the login-rush brake.
    fn is_shutting_down(&self) -> bool;

    /// `sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL)` — the hard
    /// cap on random bot levels.
    fn world_max_level(&self) -> u32;

    /// Sampled world diff counters used by the PID controller.
    fn world_diff_sample(&self) -> WorldDiffSample;

    /// Async ping — `CharacterDatabase.AsyncPQuery("SELECT 1")`. The
    /// bridge records the round-trip elapsed time and exposes it via
    /// [`database_delay_ms`](Self::database_delay_ms).
    fn database_ping(&self);

    /// Last observed DB round-trip for the named database. Mirrors
    /// `sRandomPlayerbotMgr.GetDatabaseDelay("CharacterDatabase")`.
    fn database_delay_ms(&self, db: &str) -> u32;

    // ─── Event KV store (ai_playerbot_random_bots) ─────────────────────
    /// Load every row of `ai_playerbot_random_bots` with `owner = 0`.
    fn query_all_events(&self) -> Vec<EventRow>;

    /// Load every row for a specific bot (`WHERE owner = 0 AND bot = ?`).
    fn query_events_for_bot(&self, bot: u32) -> Vec<EventRow>;

    /// `REPLACE INTO ai_playerbot_random_bots (owner=0, bot, event,
    /// value, time, validIn, data)`. `time` should always be the
    /// current unix epoch seconds on the calling side.
    fn upsert_event(&self, row: &EventRow);

    /// `DELETE FROM ai_playerbot_random_bots WHERE owner = 0 AND bot =
    /// ? AND event = ?`.
    fn delete_event(&self, bot: u32, event: &str);

    /// `DELETE FROM ai_playerbot_random_bots WHERE owner = 0 AND bot =
    /// ?`. Used by `Remove(bot)`.
    fn delete_all_events_for_bot(&self, bot: u32);

    /// `DELETE FROM ai_playerbot_random_bots` — used by the console
    /// `reset` command.
    fn delete_all_events(&self);

    /// `UPDATE ai_playerbot_random_bots SET time = time + ? WHERE owner
    /// = 0 AND bot <> 0` — bumps every TTL by `delta_seconds`, called
    /// once per `SyncEventTimers` pass.
    fn bump_event_times(&self, delta_seconds: u32);

    // ─── Bot roster / per-bot actions ──────────────────────────────────
    /// Enumerate every playerbot currently owned by the manager (the
    /// `players` map in the C++ class).
    fn owned_bot_guids(&self) -> Vec<u32>;

    /// Per-bot level snapshot. Returns `None` if the guid is not in
    /// the manager's map.
    fn bot_level(&self, guid: u32) -> Option<u32>;

    /// True iff the given guid points at an in-world bot owned by this
    /// manager. Mirrors `GetPlayerBot(guid)`.
    fn has_player_bot(&self, guid: u32) -> bool;

    /// Issue a full `Randomize` pass on the given bot. The bridge uses
    /// `PlayerbotFactory` under the hood.
    fn dispatch_randomize(&self, guid: u32);

    /// Cheap randomise that doesn't touch equipment — mirrors
    /// `InstaRandomize(bot)`.
    fn dispatch_insta_randomize(&self, guid: u32);

    /// Like `Randomize` but starts the bot at level 1 and runs the
    /// factory once — mirrors `RandomizeFirst(bot)`.
    fn dispatch_randomize_first(&self, guid: u32);

    /// Re-apply gear + spell books without touching level / talents —
    /// mirrors `UpdateGearSpells(bot)`.
    fn dispatch_update_gear_spells(&self, guid: u32);

    /// Re-run the `Refresh` helper — mirrors `Refresh(bot)`.
    fn dispatch_refresh(&self, guid: u32);

    /// Teleport the bot to the next level-bucket location from the
    /// cache. Returns `false` when the cache entry for the bot's level
    /// is empty.
    fn dispatch_random_teleport_for_level(&self, guid: u32, active_only: bool) -> bool;

    /// Teleport the bot to a random RPG spot matching its level /
    /// faction. Returns `false` when the cache is empty.
    fn dispatch_random_teleport_for_rpg(&self, guid: u32, active_only: bool) -> bool;

    /// Run a generic teleport pass — mirrors the private
    /// `RandomTeleport(bot)` overload.
    fn dispatch_random_teleport(&self, guid: u32);

    /// Resurrect the bot (`Revive`). No-op if the bot isn't dead.
    fn dispatch_revive(&self, guid: u32);

    /// Swap the bot's strategy set — mirrors `ChangeStrategy(bot)`.
    fn dispatch_change_strategy(&self, guid: u32);

    /// Remove a random bot from the world — called by the `remove`
    /// command and by the `AddRandomBots` churn path when a bot's
    /// "logout" event has fired.
    fn dispatch_remove(&self, guid: u32);

    /// Force a logout + re-login flow on the given guid. Mirrors
    /// `MovePlayerBot(guid, newHolder)`.
    fn dispatch_logout(&self, guid: u32);

    // ─── Teleport cache (`ai_playerbot_tele_cache`) ────────────────────
    /// Read every row of `ai_playerbot_tele_cache` for the given
    /// `(map, level)` bucket. An empty vec triggers a regeneration pass
    /// on the caller.
    fn query_teleport_cache(&self) -> Vec<TeleportRow>;

    /// Rebuild the teleport cache table from scratch — iterates the
    /// creature, game-object, and quest pool tables and writes the
    /// resulting rows back into `ai_playerbot_tele_cache`. Called when
    /// [`query_teleport_cache`](Self::query_teleport_cache) returns
    /// empty.
    fn rebuild_teleport_cache(&self);

    // ─── Named locations (`ai_playerbot_named_location`) ───────────────
    /// Read every row of `ai_playerbot_named_location` (skipping
    /// fishing spots).
    fn query_named_locations(&self) -> Vec<NamedLocationRow>;

    // ─── BG / LFG queue inspection ─────────────────────────────────────
    /// Enumerate every current BG queue entry (real players + bots).
    /// The worker tick folds this into the `BgPlayers`/`BgBots` /
    /// `ArenaBots` bucket matrix.
    fn query_bg_queue(&self) -> Vec<BgQueueEntry>;

    /// Enumerate every LFG queue entry the random-bot manager needs
    /// to consider. On Classic/TBC this is empty.
    fn query_lfg_queue(&self) -> Vec<LfgQueueEntry>;

    // ─── Auction house mirror ──────────────────────────────────────────
    /// Enumerate every live auction row — used by `MirrorAh`.
    fn query_ah_rows(&self) -> Vec<AhMirrorRow>;

    // ─── Real-player averaging ─────────────────────────────────────────
    /// Levels of every currently-online real player. Used by
    /// `CheckPlayers` to set `playersLevel = avg(real-player levels)`.
    fn query_real_player_levels(&self) -> Vec<RealPlayerLevel>;

    // ─── Per-bot stats snapshot ────────────────────────────────────────
    /// Snapshot of every owned bot's race/class/level/combat state.
    /// Rebuilt on demand by the console `stats` command. Default impl
    /// returns an empty vec — the mock overrides this for tests.
    fn query_bot_stats(&self) -> Vec<BotStatsSnapshotRow> {
        Vec::new()
    }

    // ─── Stats / logging ───────────────────────────────────────────────
    /// Forward a log line to `sLog.outString`.
    fn log_info(&self, _msg: &str) {}

    /// Forward a log line to `sLog.outError`.
    fn log_error(&self, _msg: &str) {}

    /// Forward a log line to `sPlayerbotAIConfig.log(file, …)`.
    /// Default: no-op.
    fn log_file(&self, _file: &str, _line: &str) {}
}

/// Production impl — wraps the C `RandomMgrCallbacks` vtable.
///
/// `VtableRandomMgrWorld` is `Send + Sync` because every stored function
/// pointer targets global C++ state protected by the various `CMaNGOS`
/// locks (`CharacterDatabase`, `sObjectMgr`, `sBattleGroundMgr`, …) and
/// the random-mgr worker only holds the handle through an `Arc<dyn>`
/// that the main tick also clones when it drains the command channel.
pub struct VtableRandomMgrWorld {
    cbs: RandomMgrCallbacks,
}

#[allow(unsafe_code)]
// Safety: every stored function pointer targets thread-safe C++ state
// per the invariants above.
unsafe impl Send for VtableRandomMgrWorld {}
#[allow(unsafe_code)]
unsafe impl Sync for VtableRandomMgrWorld {}

impl VtableRandomMgrWorld {
    /// # Safety
    /// `cbs` must be a fully initialised `RandomMgrCallbacks` whose
    /// function pointers remain valid for the entire lifetime of the
    /// returned object (in practice: until server shutdown).
    #[must_use]
    #[allow(unsafe_code)]
    pub unsafe fn new(cbs: RandomMgrCallbacks) -> Self {
        Self { cbs }
    }
}

/// Copy a Rust `&str` into a stack-local NUL-terminated buffer of
/// capacity `N`. Returns the buffer; callers pass `buf.as_ptr().cast()`
/// to C. Truncation silently happens when the input is longer than
/// `N - 1` bytes.
fn make_c_buf<const N: usize>(s: &str) -> [u8; N] {
    let mut buf = [0u8; N];
    let bytes = s.as_bytes();
    let n = bytes.len().min(N - 1);
    buf[..n].copy_from_slice(&bytes[..n]);
    buf[n] = 0;
    buf
}

/// Decode a NUL-terminated `char[N]` field (from a C POD struct) as a
/// lossy Rust `String`.
fn c_field_to_string(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

#[allow(unsafe_code)]
impl RandomMgrWorld for VtableRandomMgrWorld {
    fn urand_range(&self, min: u32, max: u32) -> u32 {
        self.cbs
            .urand_range
            .map_or(min, |f| unsafe { f(min, max) })
    }

    fn current_ms_time(&self) -> u32 {
        self.cbs.current_ms_time.map_or(0, |f| unsafe { f() })
    }

    fn is_shutting_down(&self) -> bool {
        self.cbs
            .is_shutting_down
            .is_some_and(|f| unsafe { f() })
    }

    fn world_max_level(&self) -> u32 {
        self.cbs.world_max_level.map_or(0, |f| unsafe { f() })
    }

    fn world_diff_sample(&self) -> WorldDiffSample {
        let Some(f) = self.cbs.world_diff_sample else {
            return WorldDiffSample::default();
        };
        let mut out = crate::BotWorldDiffSample {
            current_diff_ms: 0.0,
            average_diff_ms: 0.0,
            max_diff_ms: 0.0,
        };
        unsafe { f(&raw mut out) };
        WorldDiffSample {
            current_diff_ms: out.current_diff_ms,
            average_diff_ms: out.average_diff_ms,
            max_diff_ms: out.max_diff_ms,
        }
    }

    fn database_ping(&self) {
        if let Some(f) = self.cbs.database_ping {
            unsafe { f() };
        }
    }

    fn database_delay_ms(&self, db: &str) -> u32 {
        let Some(f) = self.cbs.database_delay_ms else {
            return 0;
        };
        let buf = make_c_buf::<32>(db);
        unsafe { f(buf.as_ptr().cast()) }
    }

    fn query_all_events(&self) -> Vec<EventRow> {
        query_event_rows(self.cbs.query_all_events, self.cbs.free_event_row_list, 0)
    }

    fn query_events_for_bot(&self, bot: u32) -> Vec<EventRow> {
        query_event_rows(
            self.cbs.query_events_for_bot,
            self.cbs.free_event_row_list,
            bot,
        )
    }

    fn upsert_event(&self, row: &EventRow) {
        let Some(f) = self.cbs.upsert_event else {
            return;
        };
        let event_buf = make_c_buf::<32>(&row.event);
        let data_buf = make_c_buf::<256>(&row.data);
        let ffi = crate::BotEventRow {
            bot: row.bot,
            event: [0; 32],
            value: row.value,
            last_change_time: row.last_change_time,
            valid_in: row.valid_in,
            data: [0; 256],
        };
        let mut ffi = ffi;
        for (dst, src) in ffi.event.iter_mut().zip(event_buf.iter()) {
            *dst = *src as _;
        }
        for (dst, src) in ffi.data.iter_mut().zip(data_buf.iter()) {
            *dst = *src as _;
        }
        unsafe { f(&raw const ffi) };
    }

    fn delete_event(&self, bot: u32, event: &str) {
        let Some(f) = self.cbs.delete_event else {
            return;
        };
        let buf = make_c_buf::<32>(event);
        unsafe { f(bot, buf.as_ptr().cast()) };
    }

    fn delete_all_events_for_bot(&self, bot: u32) {
        if let Some(f) = self.cbs.delete_all_events_for_bot {
            unsafe { f(bot) };
        }
    }

    fn delete_all_events(&self) {
        if let Some(f) = self.cbs.delete_all_events {
            unsafe { f() };
        }
    }

    fn bump_event_times(&self, delta_seconds: u32) {
        if let Some(f) = self.cbs.bump_event_times {
            unsafe { f(delta_seconds) };
        }
    }

    fn owned_bot_guids(&self) -> Vec<u32> {
        let Some(query) = self.cbs.owned_bot_guids else {
            return Vec::new();
        };
        let Some(free) = self.cbs.free_u32_list else {
            return Vec::new();
        };
        let mut ptr: *mut u32 = core::ptr::null_mut();
        let count = unsafe { query(&raw mut ptr) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
        let out = slice.to_vec();
        unsafe { free(ptr, count) };
        out
    }

    fn bot_level(&self, guid: u32) -> Option<u32> {
        let f = self.cbs.bot_level?;
        let mut level: u32 = 0;
        let ok = unsafe { f(guid, &raw mut level) };
        ok.then_some(level)
    }

    fn has_player_bot(&self, guid: u32) -> bool {
        self.cbs
            .has_player_bot
            .is_some_and(|f| unsafe { f(guid) })
    }

    fn dispatch_randomize(&self, guid: u32) {
        if let Some(f) = self.cbs.dispatch_randomize {
            unsafe { f(guid) };
        }
    }

    fn dispatch_insta_randomize(&self, guid: u32) {
        if let Some(f) = self.cbs.dispatch_insta_randomize {
            unsafe { f(guid) };
        }
    }

    fn dispatch_randomize_first(&self, guid: u32) {
        if let Some(f) = self.cbs.dispatch_randomize_first {
            unsafe { f(guid) };
        }
    }

    fn dispatch_update_gear_spells(&self, guid: u32) {
        if let Some(f) = self.cbs.dispatch_update_gear_spells {
            unsafe { f(guid) };
        }
    }

    fn dispatch_refresh(&self, guid: u32) {
        if let Some(f) = self.cbs.dispatch_refresh {
            unsafe { f(guid) };
        }
    }

    fn dispatch_random_teleport_for_level(&self, guid: u32, active_only: bool) -> bool {
        self.cbs
            .dispatch_random_teleport_for_level
            .is_some_and(|f| unsafe { f(guid, active_only) })
    }

    fn dispatch_random_teleport_for_rpg(&self, guid: u32, active_only: bool) -> bool {
        self.cbs
            .dispatch_random_teleport_for_rpg
            .is_some_and(|f| unsafe { f(guid, active_only) })
    }

    fn dispatch_random_teleport(&self, guid: u32) {
        if let Some(f) = self.cbs.dispatch_random_teleport {
            unsafe { f(guid) };
        }
    }

    fn dispatch_revive(&self, guid: u32) {
        if let Some(f) = self.cbs.dispatch_revive {
            unsafe { f(guid) };
        }
    }

    fn dispatch_change_strategy(&self, guid: u32) {
        if let Some(f) = self.cbs.dispatch_change_strategy {
            unsafe { f(guid) };
        }
    }

    fn dispatch_remove(&self, guid: u32) {
        if let Some(f) = self.cbs.dispatch_remove {
            unsafe { f(guid) };
        }
    }

    fn dispatch_logout(&self, guid: u32) {
        if let Some(f) = self.cbs.dispatch_logout {
            unsafe { f(guid) };
        }
    }

    fn query_teleport_cache(&self) -> Vec<TeleportRow> {
        let Some(query) = self.cbs.query_teleport_cache else {
            return Vec::new();
        };
        let Some(free) = self.cbs.free_teleport_row_list else {
            return Vec::new();
        };
        let mut ptr: *mut crate::BotTeleportRow = core::ptr::null_mut();
        let count = unsafe { query(&raw mut ptr) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
        let out: Vec<TeleportRow> = slice
            .iter()
            .map(|r| TeleportRow {
                level: r.level,
                map_id: r.map_id,
                x: r.x,
                y: r.y,
                z: r.z,
            })
            .collect();
        unsafe { free(ptr, count) };
        out
    }

    fn rebuild_teleport_cache(&self) {
        if let Some(f) = self.cbs.rebuild_teleport_cache {
            unsafe { f() };
        }
    }

    fn query_named_locations(&self) -> Vec<NamedLocationRow> {
        let Some(query) = self.cbs.query_named_locations else {
            return Vec::new();
        };
        let Some(free) = self.cbs.free_named_location_list else {
            return Vec::new();
        };
        let mut ptr: *mut crate::BotNamedLocationRow = core::ptr::null_mut();
        let count = unsafe { query(&raw mut ptr) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
        let out: Vec<NamedLocationRow> = slice
            .iter()
            .map(|r| {
                let name_bytes: [u8; 64] = r.name.map(|b| b as u8);
                NamedLocationRow {
                    name: c_field_to_string(&name_bytes),
                    map_id: r.map_id,
                    x: r.x,
                    y: r.y,
                    z: r.z,
                    orientation: r.orientation,
                }
            })
            .collect();
        unsafe { free(ptr, count) };
        out
    }

    fn query_bg_queue(&self) -> Vec<BgQueueEntry> {
        let Some(query) = self.cbs.query_bg_queue else {
            return Vec::new();
        };
        let Some(free) = self.cbs.free_bg_queue_list else {
            return Vec::new();
        };
        let mut ptr: *mut crate::BotBgQueueEntry = core::ptr::null_mut();
        let count = unsafe { query(&raw mut ptr) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
        let out: Vec<BgQueueEntry> = slice
            .iter()
            .map(|r| BgQueueEntry {
                queue_type: r.queue_type,
                bracket_id: r.bracket_id,
                team_id: r.team_id,
                is_bot: r.is_bot,
                level: r.level,
                map_id: r.map_id,
                arena_rating: r.arena_rating,
                arena_type: r.arena_type,
            })
            .collect();
        unsafe { free(ptr, count) };
        out
    }

    fn query_lfg_queue(&self) -> Vec<LfgQueueEntry> {
        let Some(query) = self.cbs.query_lfg_queue else {
            return Vec::new();
        };
        let Some(free) = self.cbs.free_lfg_queue_list else {
            return Vec::new();
        };
        let mut ptr: *mut crate::BotLfgQueueEntry = core::ptr::null_mut();
        let count = unsafe { query(&raw mut ptr) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
        let out: Vec<LfgQueueEntry> = slice
            .iter()
            .map(|r| LfgQueueEntry {
                team_id: r.team_id,
                dungeon_id: r.dungeon_id,
                role_mask: r.role_mask,
            })
            .collect();
        unsafe { free(ptr, count) };
        out
    }

    fn query_ah_rows(&self) -> Vec<AhMirrorRow> {
        let Some(query) = self.cbs.query_ah_rows else {
            return Vec::new();
        };
        let Some(free) = self.cbs.free_ah_row_list else {
            return Vec::new();
        };
        let mut ptr: *mut crate::BotAhMirrorRow = core::ptr::null_mut();
        let count = unsafe { query(&raw mut ptr) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
        let out: Vec<AhMirrorRow> = slice
            .iter()
            .map(|r| AhMirrorRow {
                item_id: r.item_id,
                buyout: r.buyout,
                count: r.count,
            })
            .collect();
        unsafe { free(ptr, count) };
        out
    }

    fn query_real_player_levels(&self) -> Vec<RealPlayerLevel> {
        let Some(query) = self.cbs.query_real_player_levels else {
            return Vec::new();
        };
        let Some(free) = self.cbs.free_real_player_level_list else {
            return Vec::new();
        };
        let mut ptr: *mut crate::BotRealPlayerLevel = core::ptr::null_mut();
        let count = unsafe { query(&raw mut ptr) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
        let out: Vec<RealPlayerLevel> = slice
            .iter()
            .map(|r| RealPlayerLevel {
                level: r.level,
                total_time: r.total_time,
            })
            .collect();
        unsafe { free(ptr, count) };
        out
    }

    fn query_bot_stats(&self) -> Vec<BotStatsSnapshotRow> {
        let Some(query) = self.cbs.query_bot_stats else {
            return Vec::new();
        };
        let Some(free) = self.cbs.free_bot_stats_list else {
            return Vec::new();
        };
        let mut ptr: *mut crate::BotStatsRow = core::ptr::null_mut();
        let count = unsafe { query(&raw mut ptr) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
        let out: Vec<BotStatsSnapshotRow> = slice
            .iter()
            .map(|r| BotStatsSnapshotRow {
                guid: r.guid,
                race: r.race,
                class_id: r.class_id,
                team: r.team,
                level: r.level,
                flags: BotStatsFlags::from_bits_truncate(r.flags),
            })
            .collect();
        unsafe { free(ptr, count) };
        out
    }

    fn log_info(&self, msg: &str) {
        let Some(f) = self.cbs.log_info else {
            return;
        };
        let buf = make_c_buf::<512>(msg);
        unsafe { f(buf.as_ptr().cast()) };
    }

    fn log_error(&self, msg: &str) {
        let Some(f) = self.cbs.log_error else {
            return;
        };
        let buf = make_c_buf::<512>(msg);
        unsafe { f(buf.as_ptr().cast()) };
    }

    fn log_file(&self, file: &str, line: &str) {
        let Some(f) = self.cbs.log_file else {
            return;
        };
        let file_buf = make_c_buf::<64>(file);
        let line_buf = make_c_buf::<512>(line);
        unsafe { f(file_buf.as_ptr().cast(), line_buf.as_ptr().cast()) };
    }
}

#[allow(unsafe_code)]
fn query_event_rows(
    query: Option<unsafe extern "C" fn(u32, *mut *mut crate::BotEventRow) -> u32>,
    free: Option<unsafe extern "C" fn(*mut crate::BotEventRow, u32)>,
    arg: u32,
) -> Vec<EventRow> {
    let Some(query) = query else {
        return Vec::new();
    };
    let Some(free) = free else {
        return Vec::new();
    };
    let mut ptr: *mut crate::BotEventRow = core::ptr::null_mut();
    let count = unsafe { query(arg, &raw mut ptr) };
    if ptr.is_null() || count == 0 {
        return Vec::new();
    }
    let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
    let out: Vec<EventRow> = slice
        .iter()
        .map(|r| {
            let event_bytes: [u8; 32] = r.event.map(|b| b as u8);
            let data_bytes: [u8; 256] = r.data.map(|b| b as u8);
            EventRow {
                bot: r.bot,
                event: c_field_to_string(&event_bytes),
                value: r.value,
                last_change_time: r.last_change_time,
                valid_in: r.valid_in,
                data: c_field_to_string(&data_bytes),
            }
        })
        .collect();
    unsafe { free(ptr, count) };
    out
}

/// In-memory mock used by the `crates/playerbot` random-mgr tests.
#[cfg(any(test, feature = "mock"))]
pub use mock_impl::{MockRandomMgrEvent, MockRandomMgrWorld};

#[cfg(any(test, feature = "mock"))]
mod mock_impl {
    use super::*;
    use std::sync::Mutex;

    /// One recorded side effect the test can assert against.
    #[derive(Clone, Debug, PartialEq)]
    pub enum MockRandomMgrEvent {
        UpsertEvent(EventRow),
        DeleteEvent(u32, String),
        DeleteAllEventsForBot(u32),
        DeleteAllEvents,
        BumpEventTimes(u32),
        Randomize(u32),
        InstaRandomize(u32),
        RandomizeFirst(u32),
        UpdateGearSpells(u32),
        Refresh(u32),
        RandomTeleportForLevel(u32, bool),
        RandomTeleportForRpg(u32, bool),
        RandomTeleport(u32),
        Revive(u32),
        ChangeStrategy(u32),
        Remove(u32),
        Logout(u32),
        DatabasePing,
        RebuildTeleportCache,
        LogInfo(String),
        LogError(String),
        LogFile(String, String),
    }

    /// Pure-Rust `RandomMgrWorld` impl for tests.
    pub struct MockRandomMgrWorld {
        inner: Mutex<MockState>,
    }

    struct MockState {
        events: Vec<MockRandomMgrEvent>,
        event_rows: Vec<EventRow>,
        teleport_rows: Vec<TeleportRow>,
        named_locations: Vec<NamedLocationRow>,
        bg_queue: Vec<BgQueueEntry>,
        lfg_queue: Vec<LfgQueueEntry>,
        ah_rows: Vec<AhMirrorRow>,
        real_player_levels: Vec<RealPlayerLevel>,
        bot_stats: Vec<BotStatsSnapshotRow>,
        owned_guids: Vec<u32>,
        current_ms_time: u32,
        is_shutting_down: bool,
        world_max_level: u32,
        diff_sample: WorldDiffSample,
        database_delay_ms: u32,
        rng_cursor: u32,
    }

    impl MockRandomMgrWorld {
        #[must_use]
        pub fn new() -> Self {
            Self {
                inner: Mutex::new(MockState {
                    events: Vec::new(),
                    event_rows: Vec::new(),
                    teleport_rows: Vec::new(),
                    named_locations: Vec::new(),
                    bg_queue: Vec::new(),
                    lfg_queue: Vec::new(),
                    ah_rows: Vec::new(),
                    real_player_levels: Vec::new(),
                    bot_stats: Vec::new(),
                    owned_guids: Vec::new(),
                    current_ms_time: 0,
                    is_shutting_down: false,
                    world_max_level: 60,
                    diff_sample: WorldDiffSample::default(),
                    database_delay_ms: 0,
                    rng_cursor: 0,
                }),
            }
        }

        /// Consume and return every recorded side effect. The internal
        /// log is cleared so tests can assert on deltas.
        pub fn events(&self) -> Vec<MockRandomMgrEvent> {
            let mut g = self.inner.lock().unwrap();
            std::mem::take(&mut g.events)
        }

        pub fn set_event_rows(&self, rows: Vec<EventRow>) {
            self.inner.lock().unwrap().event_rows = rows;
        }

        pub fn set_teleport_rows(&self, rows: Vec<TeleportRow>) {
            self.inner.lock().unwrap().teleport_rows = rows;
        }

        pub fn set_named_locations(&self, rows: Vec<NamedLocationRow>) {
            self.inner.lock().unwrap().named_locations = rows;
        }

        pub fn set_bg_queue(&self, rows: Vec<BgQueueEntry>) {
            self.inner.lock().unwrap().bg_queue = rows;
        }

        pub fn set_lfg_queue(&self, rows: Vec<LfgQueueEntry>) {
            self.inner.lock().unwrap().lfg_queue = rows;
        }

        pub fn set_ah_rows(&self, rows: Vec<AhMirrorRow>) {
            self.inner.lock().unwrap().ah_rows = rows;
        }

        pub fn set_real_player_levels(&self, rows: Vec<RealPlayerLevel>) {
            self.inner.lock().unwrap().real_player_levels = rows;
        }

        pub fn set_bot_stats(&self, rows: Vec<BotStatsSnapshotRow>) {
            self.inner.lock().unwrap().bot_stats = rows;
        }

        pub fn set_owned_guids(&self, guids: Vec<u32>) {
            self.inner.lock().unwrap().owned_guids = guids;
        }

        pub fn set_current_ms_time(&self, t: u32) {
            self.inner.lock().unwrap().current_ms_time = t;
        }

        pub fn set_is_shutting_down(&self, v: bool) {
            self.inner.lock().unwrap().is_shutting_down = v;
        }

        pub fn set_world_max_level(&self, l: u32) {
            self.inner.lock().unwrap().world_max_level = l;
        }

        pub fn set_world_diff_sample(&self, sample: WorldDiffSample) {
            self.inner.lock().unwrap().diff_sample = sample;
        }

        pub fn set_database_delay_ms(&self, v: u32) {
            self.inner.lock().unwrap().database_delay_ms = v;
        }
    }

    impl Default for MockRandomMgrWorld {
        fn default() -> Self {
            Self::new()
        }
    }

    impl RandomMgrWorld for MockRandomMgrWorld {
        fn urand_range(&self, min: u32, max: u32) -> u32 {
            if max <= min {
                return min;
            }
            let mut g = self.inner.lock().unwrap();
            let span = max - min + 1;
            let v = min + (g.rng_cursor % span);
            g.rng_cursor = g.rng_cursor.wrapping_add(1);
            v
        }

        fn current_ms_time(&self) -> u32 {
            self.inner.lock().unwrap().current_ms_time
        }

        fn is_shutting_down(&self) -> bool {
            self.inner.lock().unwrap().is_shutting_down
        }

        fn world_max_level(&self) -> u32 {
            self.inner.lock().unwrap().world_max_level
        }

        fn world_diff_sample(&self) -> WorldDiffSample {
            self.inner.lock().unwrap().diff_sample
        }

        fn database_ping(&self) {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::DatabasePing);
        }

        fn database_delay_ms(&self, _db: &str) -> u32 {
            self.inner.lock().unwrap().database_delay_ms
        }

        fn query_all_events(&self) -> Vec<EventRow> {
            self.inner.lock().unwrap().event_rows.clone()
        }

        fn query_events_for_bot(&self, bot: u32) -> Vec<EventRow> {
            self.inner
                .lock()
                .unwrap()
                .event_rows
                .iter()
                .filter(|r| r.bot == bot)
                .cloned()
                .collect()
        }

        fn upsert_event(&self, row: &EventRow) {
            let mut g = self.inner.lock().unwrap();
            g.events
                .push(MockRandomMgrEvent::UpsertEvent(row.clone()));
            if let Some(existing) = g
                .event_rows
                .iter_mut()
                .find(|r| r.bot == row.bot && r.event == row.event)
            {
                *existing = row.clone();
            } else {
                g.event_rows.push(row.clone());
            }
        }

        fn delete_event(&self, bot: u32, event: &str) {
            let mut g = self.inner.lock().unwrap();
            g.events
                .push(MockRandomMgrEvent::DeleteEvent(bot, event.to_string()));
            g.event_rows
                .retain(|r| !(r.bot == bot && r.event == event));
        }

        fn delete_all_events_for_bot(&self, bot: u32) {
            let mut g = self.inner.lock().unwrap();
            g.events
                .push(MockRandomMgrEvent::DeleteAllEventsForBot(bot));
            g.event_rows.retain(|r| r.bot != bot);
        }

        fn delete_all_events(&self) {
            let mut g = self.inner.lock().unwrap();
            g.events.push(MockRandomMgrEvent::DeleteAllEvents);
            g.event_rows.clear();
        }

        fn bump_event_times(&self, delta_seconds: u32) {
            let mut g = self.inner.lock().unwrap();
            g.events
                .push(MockRandomMgrEvent::BumpEventTimes(delta_seconds));
            for r in g.event_rows.iter_mut() {
                r.last_change_time = r.last_change_time.saturating_add(delta_seconds);
            }
        }

        fn owned_bot_guids(&self) -> Vec<u32> {
            self.inner.lock().unwrap().owned_guids.clone()
        }

        fn bot_level(&self, _guid: u32) -> Option<u32> {
            None
        }

        fn has_player_bot(&self, guid: u32) -> bool {
            self.inner.lock().unwrap().owned_guids.contains(&guid)
        }

        fn dispatch_randomize(&self, guid: u32) {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::Randomize(guid));
        }

        fn dispatch_insta_randomize(&self, guid: u32) {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::InstaRandomize(guid));
        }

        fn dispatch_randomize_first(&self, guid: u32) {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::RandomizeFirst(guid));
        }

        fn dispatch_update_gear_spells(&self, guid: u32) {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::UpdateGearSpells(guid));
        }

        fn dispatch_refresh(&self, guid: u32) {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::Refresh(guid));
        }

        fn dispatch_random_teleport_for_level(&self, guid: u32, active_only: bool) -> bool {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::RandomTeleportForLevel(guid, active_only));
            true
        }

        fn dispatch_random_teleport_for_rpg(&self, guid: u32, active_only: bool) -> bool {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::RandomTeleportForRpg(guid, active_only));
            true
        }

        fn dispatch_random_teleport(&self, guid: u32) {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::RandomTeleport(guid));
        }

        fn dispatch_revive(&self, guid: u32) {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::Revive(guid));
        }

        fn dispatch_change_strategy(&self, guid: u32) {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::ChangeStrategy(guid));
        }

        fn dispatch_remove(&self, guid: u32) {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::Remove(guid));
        }

        fn dispatch_logout(&self, guid: u32) {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::Logout(guid));
        }

        fn query_teleport_cache(&self) -> Vec<TeleportRow> {
            self.inner.lock().unwrap().teleport_rows.clone()
        }

        fn rebuild_teleport_cache(&self) {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::RebuildTeleportCache);
        }

        fn query_named_locations(&self) -> Vec<NamedLocationRow> {
            self.inner.lock().unwrap().named_locations.clone()
        }

        fn query_bg_queue(&self) -> Vec<BgQueueEntry> {
            self.inner.lock().unwrap().bg_queue.clone()
        }

        fn query_lfg_queue(&self) -> Vec<LfgQueueEntry> {
            self.inner.lock().unwrap().lfg_queue.clone()
        }

        fn query_ah_rows(&self) -> Vec<AhMirrorRow> {
            self.inner.lock().unwrap().ah_rows.clone()
        }

        fn query_real_player_levels(&self) -> Vec<RealPlayerLevel> {
            self.inner.lock().unwrap().real_player_levels.clone()
        }

        fn query_bot_stats(&self) -> Vec<BotStatsSnapshotRow> {
            self.inner.lock().unwrap().bot_stats.clone()
        }

        fn log_info(&self, msg: &str) {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::LogInfo(msg.to_string()));
        }

        fn log_error(&self, msg: &str) {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::LogError(msg.to_string()));
        }

        fn log_file(&self, file: &str, line: &str) {
            self.inner
                .lock()
                .unwrap()
                .events
                .push(MockRandomMgrEvent::LogFile(
                    file.to_string(),
                    line.to_string(),
                ));
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn upsert_event_records_and_updates_rows() {
            let w = MockRandomMgrWorld::new();
            let row = EventRow {
                bot: 42,
                event: "add".into(),
                value: 1,
                last_change_time: 100,
                valid_in: 60,
                data: "".into(),
            };
            w.upsert_event(&row);
            let rows = w.query_events_for_bot(42);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].value, 1);

            let updated = EventRow {
                value: 2,
                ..row.clone()
            };
            w.upsert_event(&updated);
            assert_eq!(w.query_events_for_bot(42)[0].value, 2);

            assert_eq!(w.query_all_events().len(), 1);
            assert_eq!(w.events().len(), 2);
        }

        #[test]
        fn delete_event_removes_matching_row() {
            let w = MockRandomMgrWorld::new();
            w.set_event_rows(vec![EventRow {
                bot: 1,
                event: "add".into(),
                value: 0,
                last_change_time: 0,
                valid_in: 0,
                data: "".into(),
            }]);
            w.delete_event(1, "add");
            assert!(w.query_events_for_bot(1).is_empty());
        }

        #[test]
        fn dispatch_helpers_record_events() {
            let w = MockRandomMgrWorld::new();
            w.dispatch_randomize(7);
            w.dispatch_revive(7);
            w.dispatch_remove(7);
            let ev = w.events();
            assert_eq!(ev.len(), 3);
            assert!(matches!(ev[0], MockRandomMgrEvent::Randomize(7)));
            assert!(matches!(ev[1], MockRandomMgrEvent::Revive(7)));
            assert!(matches!(ev[2], MockRandomMgrEvent::Remove(7)));
        }
    }
}
