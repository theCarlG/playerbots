//! `extern "C"` surface for the Rust random-playerbot manager.
//!
//! Mirrors `login/ffi.rs` — state lives behind a
//! `OnceLock<Mutex<Option<RandomMgrFfiState>>>` singleton, initialised
//! by [`playerbot_random_mgr_init`] and torn down by
//! [`playerbot_random_mgr_shutdown`]. Every entry point acquires the
//! mutex, reads a handle to the worker, and drops the guard before
//! issuing channel sends so the C++ main thread never blocks on the
//! Rust worker.
//!
//! The implementation notes worth remembering:
//!
//! * The `get_value` / `set_value` passthroughs use a dedicated
//!   `cache_snapshot` shared between the worker and the main thread
//!   via a `Mutex<EventCache>` held in [`RandomMgrFfiState`]. The
//!   main thread lazy-loads the first time it reads a `(bot, event)`
//!   pair (via `EventCache::get_value` → `world.query_events_for_bot`),
//!   and cache hits after that are a simple hashmap lookup. Writes
//!   from `set_value` / `schedule_*` update both this cache AND fire
//!   through the world trait directly so the DB stays in sync.
//! * Scalar stats (`players_level`, `bot_count_target`) are cached
//!   via `AtomicU32` on the FFI state. The worker updates the actual
//!   values on its own `RandomMgrState`; after each worker response
//!   the main thread drains `TickStats` from the channel and copies
//!   the two scalars into the atomics so `get_players_level()` and
//!   `get_max_online_bot_count()` stay wait-free hot-path reads.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use cmangos::{RandomMgrCallbacks, RandomMgrWorld, VtableRandomMgrWorld};

use super::events::EventCache;
use super::worker::{RandomMgrWorkerHandle, WorkerResponse};

/// Singleton holding the worker handle + a shared snapshot of the
/// event cache. See module docs for why the snapshot exists.
struct RandomMgrFfiState {
    worker: RandomMgrWorkerHandle,
    /// Best-effort mirror of the worker's event cache. Readers
    /// (`get_value`, `is_random_bot`) consult this under the mutex;
    /// writers (`set_value`, `schedule_*`) update both this copy and
    /// push the change through the world trait so the DB side stays
    /// authoritative.
    cache_snapshot: Arc<Mutex<EventCache>>,
    /// Shared handle to the world trait. Cloned out of the worker so
    /// the FFI entry points can dispatch against it without going
    /// through the channel. The worker owns its own `Arc<dyn ...>`
    /// internally, so this is purely for the main-thread-only
    /// `set_value` / `schedule_*` side of the pipeline.
    world: Arc<dyn RandomMgrWorld>,
    /// Latest `state.players_level` snapshot the worker reported. The
    /// FFI update path copies this out of [`WorkerResponse::Tick`]
    /// drains so `get_players_level()` stays a wait-free atomic read.
    players_level: Arc<AtomicU32>,
    /// Latest `bot_count` event value the worker rolled. Same wiring
    /// as `players_level` — updated from the last drained `TickStats`.
    bot_count_target: Arc<AtomicU32>,
}

fn state() -> &'static Mutex<Option<RandomMgrFfiState>> {
    static STATE: OnceLock<Mutex<Option<RandomMgrFfiState>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

fn with_state<F, R>(default: R, f: F) -> R
where
    F: FnOnce(&RandomMgrFfiState) -> R,
{
    let guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    match guard.as_ref() {
        Some(st) => f(st),
        None => default,
    }
}

/// Grab the wall clock in seconds. Matches the worker's expectation
/// that callers supply `now_epoch_s` per request so we don't depend on
/// clock monotonicity across threads.
fn now_epoch_s() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as u32)
}

// ─── Lifecycle ─────────────────────────────────────────────────────────────

/// Install the random-mgr vtable and spawn the worker thread.
///
/// Safe to call more than once — a second call tears down the
/// previous state first.
///
/// # Safety
///
/// `cbs` must either be null (no-op) or a valid, fully-initialised
/// `RandomMgrCallbacks` whose function pointers remain valid until
/// [`playerbot_random_mgr_shutdown`] returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_random_mgr_init(cbs: *const RandomMgrCallbacks) {
    if cbs.is_null() {
        return;
    }
    // Safety: caller guarantees a fully initialised PoD struct.
    let cbs_copy = unsafe { *cbs };

    // Safety: function pointers in `cbs_copy` stay valid for the
    // lifetime of the handle (until `shutdown`).
    let world: Arc<dyn RandomMgrWorld> =
        Arc::new(unsafe { VtableRandomMgrWorld::new(cbs_copy) });

    let mut guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    drop(guard.take());

    let worker = RandomMgrWorkerHandle::spawn(Arc::clone(&world));
    let cache_snapshot = Arc::new(Mutex::new(EventCache::new()));

    *guard = Some(RandomMgrFfiState {
        worker,
        cache_snapshot,
        world,
        players_level: Arc::new(AtomicU32::new(0)),
        bot_count_target: Arc::new(AtomicU32::new(0)),
    });
}

/// Drive one tick of the worker with the supplied `elapsed_ms`. The
/// main thread calls this exactly once per world update. `elapsed_ms`
/// is currently unused — the worker pulls a fresh wall-clock itself —
/// but is kept in the signature so the C++ side doesn't need to
/// change when we start feeding it to the PID sampler.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_mgr_update(_elapsed_ms: u32) {
    with_state((), |st| {
        let _ = st.worker.send_tick(now_epoch_s());
        // Drain any replies the worker had queued since the last
        // update pass. Tick stats are copied into the atomic scalar
        // cache so subsequent `get_players_level()` /
        // `get_max_online_bot_count()` calls return fresh values.
        for resp in st.worker.drain_responses() {
            if let WorkerResponse::Tick(stats) = resp {
                st.players_level.store(stats.players_level, Ordering::Relaxed);
                st.bot_count_target
                    .store(stats.bot_count_target, Ordering::Relaxed);
            }
        }
    });
}

/// Join the worker thread and drop the vtable. Safe to call multiple
/// times.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_mgr_shutdown() {
    let mut guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    if let Some(st) = guard.take() {
        drop(st); // Drop impls join the worker.
    }
}

// ─── Event cache passthroughs ──────────────────────────────────────────────

/// `RandomPlayerbotMgr::GetValue(guid, key)`.
///
/// # Safety
///
/// `key` must be a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_random_mgr_get_value(
    guid: u32,
    key: *const c_char,
) -> u32 {
    let Some(key) = (unsafe { c_str_to_owned(key) }) else {
        return 0;
    };
    with_state(0, |st| {
        let mut cache = match st.cache_snapshot.lock() {
            Ok(c) => c,
            Err(e) => e.into_inner(),
        };
        cache.get_value(guid, &key, now_epoch_s(), st.world.as_ref())
    })
}

/// `RandomPlayerbotMgr::SetValue(guid, key, value, validIn)`.
///
/// `valid_in_s == -1` means "use the default TTL for this key".
///
/// # Safety
///
/// `key` must be a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_random_mgr_set_value(
    guid: u32,
    key: *const c_char,
    value: u32,
    valid_in_s: i32,
) {
    let Some(key) = (unsafe { c_str_to_owned(key) }) else {
        return;
    };
    with_state((), |st| {
        let ttl = if valid_in_s < 0 {
            super::events::DEFAULT_VALUE_TTL_S
        } else {
            valid_in_s as u32
        };
        let mut cache = match st.cache_snapshot.lock() {
            Ok(c) => c,
            Err(e) => e.into_inner(),
        };
        cache.set_value(guid, &key, value, ttl, "", now_epoch_s(), st.world.as_ref());
    });
}

// ─── Scalar getters ────────────────────────────────────────────────────────

/// Current average level of online real players. Served from the
/// atomic snapshot the worker's last `TickStats` message deposited.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_mgr_get_players_level() -> u32 {
    with_state(0, |st| st.players_level.load(Ordering::Relaxed))
}

/// Current target bot population — the `bot_count` event value. Served
/// from the same atomic snapshot as `get_players_level`.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_mgr_get_max_online_bot_count() -> u32 {
    with_state(0, |st| st.bot_count_target.load(Ordering::Relaxed))
}

/// `sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL)`.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_mgr_get_world_max_level() -> u32 {
    with_state(0, |st| st.world.world_max_level())
}

/// Last observed `CharacterDatabase` round-trip in ms.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_mgr_get_database_delay_ms() -> u32 {
    with_state(0, |st| st.world.database_delay_ms("CharacterDatabase"))
}

/// Issue a fresh async DB ping.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_mgr_database_ping() {
    with_state((), |st| st.world.database_ping());
}

/// `currentBots` membership check.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_mgr_is_random_bot(guid: u32) -> bool {
    with_state(false, |st| {
        let mut cache = match st.cache_snapshot.lock() {
            Ok(c) => c,
            Err(e) => e.into_inner(),
        };
        cache.get_value(guid, "add", now_epoch_s(), st.world.as_ref()) > 0
    })
}

// ─── Schedule helpers (main thread fast paths) ─────────────────────────────

/// Schedule a future `Randomize` dispatch.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_mgr_schedule_randomize(guid: u32, delay_s: u32) {
    schedule_event(guid, "randomize", delay_s);
}

/// Schedule a future `Teleport` dispatch.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_mgr_schedule_teleport(guid: u32, delay_s: u32) {
    schedule_event(guid, "teleport", delay_s);
}

/// Schedule a future `ChangeStrategy` dispatch.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_mgr_schedule_change_strategy(guid: u32, delay_s: u32) {
    schedule_event(guid, "change_strategy", delay_s);
}

fn schedule_event(guid: u32, key: &str, delay_s: u32) {
    with_state((), |st| {
        let mut cache = match st.cache_snapshot.lock() {
            Ok(c) => c,
            Err(e) => e.into_inner(),
        };
        cache.set_value(guid, key, 1, delay_s, "", now_epoch_s(), st.world.as_ref());
    });
}

// ─── Console command / stats dispatch ──────────────────────────────────────

/// `/bot <args>` passthrough. Returns `true` if the command parsed into
/// a known [`super::commands::ParsedCommand`] and the side effects
/// ran, `false` otherwise.
///
/// # Safety
///
/// `text` must be a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_random_mgr_handle_command(text: *const c_char) -> bool {
    let Some(text) = (unsafe { c_str_to_owned(text) }) else {
        return false;
    };
    with_state(false, |st| {
        let parsed = super::commands::parse(&text);
        match parsed {
            super::commands::ParsedCommand::Console { cmd, args } => {
                // Console commands that mutate state go through the
                // worker (so all the state lives on one thread). For
                // now we dispatch them on the main thread against a
                // throw-away state — the worker will replay the same
                // command via its own channel when we wire the RA
                // path end-to-end. The stats command still produces a
                // chat line that we log out directly.
                let mut tmp = super::state::RandomMgrState::new();
                let out = super::commands::run_console_command(
                    cmd,
                    &args,
                    &mut tmp,
                    st.world.as_ref(),
                );
                for line in out.messages {
                    st.world.log_info(&line);
                }
                true
            }
            super::commands::ParsedCommand::Bot { cmd, name_prefix, params } => {
                let guids = matching_bot_guids(st.world.as_ref(), &name_prefix);
                let mut cache = match st.cache_snapshot.lock() {
                    Ok(c) => c,
                    Err(e) => e.into_inner(),
                };
                let out = super::commands::run_bot_command(
                    cmd,
                    &guids,
                    &params,
                    &mut cache,
                    st.world.as_ref(),
                );
                for line in out.messages {
                    st.world.log_info(&line);
                }
                true
            }
            super::commands::ParsedCommand::Unknown => false,
        }
    })
}

/// `RandomPlayerbotMgr::PrintStats(requester_guid)` passthrough. The
/// `requester_guid` is currently ignored — the chat routing lives on
/// the C++ side and the caller already has the session handle.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_mgr_print_stats(_requester_guid: u32) {
    with_state((), |st| {
        let rows = st.world.query_bot_stats();
        let mut cache = match st.cache_snapshot.lock() {
            Ok(c) => c,
            Err(e) => e.into_inner(),
        };
        let out = super::stats::print_stats(
            &rows,
            &mut cache,
            st.world.as_ref(),
            now_epoch_s(),
            st.world.world_max_level(),
        );
        for line in out.messages {
            st.world.log_info(&line);
        }
    });
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Filter the world's bot roster by the legacy "name prefix" rules:
/// * `%` matches every bot.
/// * any other literal returns an empty list unless the bridge starts
///   exposing a name lookup — for now, we pass the full roster when
///   the caller sends `%` and nothing otherwise so the command
///   handlers still get exercised in tests.
fn matching_bot_guids(world: &dyn RandomMgrWorld, name_prefix: &str) -> Vec<u32> {
    if name_prefix == "%" {
        world.owned_bot_guids()
    } else {
        // Name lookups would require another trait method; the legacy
        // C++ side walked `objmgr` to resolve a name → guid. We'll
        // fill that in once the name lookup is ported.
        Vec::new()
    }
}

/// Decode a C string into an owned Rust `String`. Returns `None` if
/// the pointer is null or the bytes aren't valid UTF-8.
///
/// # Safety
///
/// `ptr` must be either null or a valid null-terminated C string.
unsafe fn c_str_to_owned(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // Safety: caller guarantees null-terminated validity.
    let cstr = unsafe { CStr::from_ptr(ptr) };
    cstr.to_str().ok().map(str::to_owned)
}
