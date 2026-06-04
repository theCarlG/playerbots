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

/// `RandomPlayerbotMgr::SetEventValue(guid, key, value, validIn, data)` —
/// the faithful low-level event write. Unlike
/// [`playerbot_random_mgr_set_value`], `valid_in_s` is the raw `u32` TTL
/// in seconds with **no** `-1` sentinel: `0` means "no TTL" and a large
/// value (e.g. `0xFFFFFFFF`, the C++ `(uint32)-1`) means "effectively
/// never expires". This preserves the exact C++ `SetEventValue` semantics.
///
/// # Safety
///
/// `key` and `data` must be valid null-terminated C strings (`data` may
/// be a pointer to an empty string).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_random_mgr_set_event_value(
    guid: u32,
    key: *const c_char,
    value: u32,
    valid_in_s: u32,
    data: *const c_char,
) {
    let Some(key) = (unsafe { c_str_to_owned(key) }) else {
        return;
    };
    let data = unsafe { c_str_to_owned(data) }.unwrap_or_default();
    with_state((), |st| {
        let mut cache = match st.cache_snapshot.lock() {
            Ok(c) => c,
            Err(e) => e.into_inner(),
        };
        cache.set_value(
            guid,
            &key,
            value,
            valid_in_s,
            &data,
            now_epoch_s(),
            st.world.as_ref(),
        );
    });
}

/// `RandomPlayerbotMgr::Remove(bot)` cache half — drop every cached
/// event row for `guid`. The DB delete is issued separately by the C++
/// caller; this clears the Rust-side mirror.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_mgr_drop_bot_events(guid: u32) {
    with_state((), |st| {
        let mut cache = match st.cache_snapshot.lock() {
            Ok(c) => c,
            Err(e) => e.into_inner(),
        };
        cache.drop_bot(guid);
    });
}

/// `RandomPlayerbotMgr::HandleConsoleReset` cache half — clear the entire
/// event cache. The DB wipe is issued separately by the C++ caller.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_mgr_clear_event_cache() {
    with_state((), |st| {
        let mut cache = match st.cache_snapshot.lock() {
            Ok(c) => c,
            Err(e) => e.into_inner(),
        };
        cache.clear();
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

/// Side-effect flags returned via the `out_flags` out-param of
/// [`playerbot_random_mgr_console_command`]. Rust performs every effect it
/// owns (DB event wipe, in-memory cache clear, worker reset, worker PID
/// retune); these two are genuinely CMaNGOS-side and are signalled back
/// for the C++ caller to perform, plus the login-debug toggle which lives
/// behind the separate login-worker FFI singleton.
pub const CONSOLE_FLAG_UPDATE_TICK: u32 = 1 << 0;
pub const CONSOLE_FLAG_CLEAN_MAP: u32 = 1 << 1;
pub const CONSOLE_FLAG_LOGIN_DEBUG: u32 = 1 << 2;

/// `RandomPlayerbotMgr::HandlePlayerbotConsoleCommand` core. Parses and
/// dispatches a console or per-bot command and returns the newline-joined
/// output lines as a freshly allocated C string (free with
/// `playerbot_free_string`), or NULL when the command is not recognised —
/// in which case the C++ caller falls through to the holder command
/// handler. `*out_flags` receives `CONSOLE_FLAG_*` bits for the side
/// effects the C++ caller must perform (force-tick, clean-map, login-debug
/// toggle); every other side effect (event wipe, worker reset, PID retune)
/// is applied here against the real worker / world.
///
/// # Safety
///
/// `text` must be a valid null-terminated C string. `out_flags` must be a
/// valid pointer to a `uint32_t` (or null).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_random_mgr_console_command(
    text: *const c_char,
    out_flags: *mut u32,
) -> *mut c_char {
    use super::commands::{parse, run_console_command, ConsoleCommand, ParsedCommand};

    if !out_flags.is_null() {
        unsafe { *out_flags = 0 };
    }
    let Some(text) = (unsafe { c_str_to_owned(text) }) else {
        return core::ptr::null_mut();
    };

    with_state(core::ptr::null_mut(), |st| {
        let mut flags: u32 = 0;
        let messages: Vec<String> = match parse(&text) {
            ParsedCommand::Unknown => return core::ptr::null_mut(),
            ParsedCommand::Bot { cmd, name_prefix, params } => {
                let guids = matching_bot_guids(st.world.as_ref(), &name_prefix);
                let mut cache = lock_cache(st);
                super::commands::run_bot_command(cmd, &guids, &params, &mut cache, st.world.as_ref())
                    .messages
            }
            ParsedCommand::Console { cmd, args } => {
                // `stats` renders against the live shared event cache;
                // every other console command's output is independent of
                // worker state, so a scratch state produces the correct
                // lines while the real side effects below hit the worker.
                let out = if cmd == ConsoleCommand::Stats {
                    let rows = st.world.query_bot_stats();
                    let mut cache = lock_cache(st);
                    super::stats::print_stats(
                        &rows,
                        &mut cache,
                        st.world.as_ref(),
                        now_epoch_s(),
                        st.world.world_max_level(),
                    )
                } else {
                    let mut scratch = super::state::RandomMgrState::new();
                    run_console_command(cmd, &args, &mut scratch, st.world.as_ref())
                };

                match cmd {
                    ConsoleCommand::Update => flags |= CONSOLE_FLAG_UPDATE_TICK,
                    ConsoleCommand::CleanMap => flags |= CONSOLE_FLAG_CLEAN_MAP,
                    ConsoleCommand::LoginDebug => flags |= CONSOLE_FLAG_LOGIN_DEBUG,
                    ConsoleCommand::Pid => {
                        // `run_console_command` already produced the echo
                        // message; retune the *worker's* PID for real.
                        let p: Vec<f64> = args
                            .split_ascii_whitespace()
                            .map(|s| s.parse::<f64>().unwrap_or(0.0))
                            .collect();
                        st.worker.adjust_pid(
                            p.first().copied().unwrap_or(0.0),
                            p.get(1).copied().unwrap_or(0.0),
                            p.get(2).copied().unwrap_or(0.0),
                        );
                    }
                    ConsoleCommand::Reset => {
                        // `run_console_command` already fired the DB-side
                        // `world.delete_all_events()`; clear the shared
                        // cache mirror and wipe the worker's in-memory state.
                        lock_cache(st).clear();
                        st.worker.reset();
                    }
                    _ => {}
                }
                out.messages
            }
        };

        if !out_flags.is_null() {
            unsafe { *out_flags = flags };
        }
        match std::ffi::CString::new(messages.join("\n")) {
            Ok(cs) => cs.into_raw(),
            Err(_) => core::ptr::null_mut(),
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

/// Lock the shared event-cache snapshot, recovering from a poisoned mutex.
fn lock_cache(st: &RandomMgrFfiState) -> std::sync::MutexGuard<'_, EventCache> {
    match st.cache_snapshot.lock() {
        Ok(c) => c,
        Err(e) => e.into_inner(),
    }
}

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
