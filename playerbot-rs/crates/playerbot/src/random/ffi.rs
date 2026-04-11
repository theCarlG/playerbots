//! `extern "C"` surface for the Rust random-bot factory.
//!
//! The C++ shim at `cpp_wrapper/RandomFactoryBridge.{h,cpp}` owns the
//! `RandomFactoryCallbacks` vtable and calls into this module via:
//!
//! * [`playerbot_random_factory_init`] — install the vtable.
//! * [`playerbot_random_factory_shutdown`] — drop the cached state.
//! * [`playerbot_random_factory_create_bots`] — port of
//!   `RandomPlayerbotFactory::CreateRandomBots()`.
//! * [`playerbot_random_factory_create_guilds`] — port of
//!   `RandomPlayerbotFactory::CreateRandomGuilds()`.
//! * [`playerbot_random_factory_create_arena_teams`] — port of
//!   `RandomPlayerbotFactory::CreateRandomArenaTeams()` (TBC/WotLK only).
//! * [`playerbot_random_factory_get_accounts`],
//!   [`playerbot_random_factory_get_guilds`],
//!   [`playerbot_random_factory_get_arena_teams`] — expose the runtime
//!   tracking lists back to C++ so `sPlayerbotAIConfig` can keep its
//!   legacy fields in sync.
//!
//! State is a single `OnceLock<Mutex<Option<RandomFactoryFfiState>>>`.
//! The option is `None` until `init`, `Some` until `shutdown`. Each
//! entry point acquires the mutex, clones out the vtable wrapper via
//! `Arc`, releases the lock before touching the game state, then
//! reacquires it to commit any `FactoryState` changes. This matches
//! how `login/ffi.rs` and `itempool/ffi.rs` handle their vtables.

use std::sync::{Arc, Mutex, OnceLock};

use cmangos::{RandomFactoryCallbacks, RandomFactoryWorld, VtableRandomFactoryWorld};

use crate::random::factory::{
    FactoryState, create_random_arena_teams, create_random_bots, create_random_guilds,
};

/// Singleton state owned by the FFI layer. `None` until
/// [`playerbot_random_factory_init`] runs.
struct RandomFactoryFfiState {
    /// Live vtable wrapper. Arc so we can hand a `&dyn
    /// RandomFactoryWorld` to the orchestration functions without
    /// holding the outer mutex across the call.
    world: Arc<dyn RandomFactoryWorld>,
    /// Runtime tracking state — account / guild / arena-team ids plus
    /// the two "already ran the delete pass" latches.
    state: FactoryState,
}

fn state() -> &'static Mutex<Option<RandomFactoryFfiState>> {
    static STATE: OnceLock<Mutex<Option<RandomFactoryFfiState>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

// ── Lifecycle ─────────────────────────────────────────────────────────────

/// Install the random-factory vtable. A null pointer is a safe no-op.
/// A second call tears down the previous state before installing the
/// new one.
///
/// # Safety
///
/// `cbs` must either be null or a valid, fully-initialised
/// `RandomFactoryCallbacks` whose function pointers remain valid until
/// [`playerbot_random_factory_shutdown`] returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_random_factory_init(cbs: *const RandomFactoryCallbacks) {
    if cbs.is_null() {
        return;
    }

    // Copy the vtable by value — it's a PoD struct of fn pointers.
    // Safety: caller guarantees `cbs` points to a fully-initialised
    // `RandomFactoryCallbacks`.
    let cbs_copy = unsafe { *cbs };

    // Safety: the caller promises the function pointers stay valid
    // until shutdown; the wrapper is `Send + Sync` because every
    // underlying C call is thread-safe for our use (see
    // `VtableRandomFactoryWorld` doc comment).
    let world: Arc<dyn RandomFactoryWorld> =
        Arc::new(unsafe { VtableRandomFactoryWorld::new(cbs_copy) });

    let mut guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    *guard = Some(RandomFactoryFfiState {
        world,
        state: FactoryState::new(),
    });
}

/// Drop the cached state. Safe to call more than once; the second call
/// is a no-op.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_factory_shutdown() {
    let mut guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    drop(guard.take());
}

// ── Orchestration entry points ────────────────────────────────────────────

/// Port of `RandomPlayerbotFactory::CreateRandomBots()`. Safe no-op
/// when [`playerbot_random_factory_init`] has not been called.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_factory_create_bots() {
    let Some((world, mut factory_state)) = take_world_and_state() else {
        return;
    };

    let cfg = crate::config::get();
    create_random_bots(&cfg, world.as_ref(), &mut factory_state);

    put_back_state(factory_state);
}

/// Port of `RandomPlayerbotFactory::CreateRandomGuilds()`. Safe no-op
/// when [`playerbot_random_factory_init`] has not been called.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_factory_create_guilds() {
    let Some((world, mut factory_state)) = take_world_and_state() else {
        return;
    };

    let cfg = crate::config::get();
    create_random_guilds(&cfg, world.as_ref(), &mut factory_state);

    put_back_state(factory_state);
}

/// Port of `RandomPlayerbotFactory::CreateRandomArenaTeams()`. Safe
/// no-op when [`playerbot_random_factory_init`] has not been called.
/// Compiled to an empty stub on Classic.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_random_factory_create_arena_teams() {
    let Some((world, mut factory_state)) = take_world_and_state() else {
        return;
    };

    let cfg = crate::config::get();
    create_random_arena_teams(&cfg, world.as_ref(), &mut factory_state);

    put_back_state(factory_state);
}

// ── Runtime tracking list accessors ───────────────────────────────────────

/// Copy the currently tracked random-bot account ids into
/// `out_ids` (up to `max_out` entries) and return the total count of
/// tracked ids (which may exceed `max_out`). Returns 0 when the module
/// is uninitialised.
///
/// # Safety
///
/// `out_ids` must be either null with `max_out == 0` or a valid pointer
/// to `max_out` consecutive `u32` slots.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_random_factory_get_accounts(
    out_ids: *mut u32,
    max_out: u32,
) -> u32 {
    // Safety: forwarded from the caller's contract above.
    unsafe { copy_list(out_ids, max_out, |st| &st.random_bot_accounts) }
}

/// Copy the currently tracked random-bot guild ids into `out_ids` (up
/// to `max_out` entries) and return the total count of tracked ids.
///
/// # Safety
///
/// `out_ids` must be either null with `max_out == 0` or a valid pointer
/// to `max_out` consecutive `u32` slots.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_random_factory_get_guilds(
    out_ids: *mut u32,
    max_out: u32,
) -> u32 {
    // Safety: forwarded from the caller's contract above.
    unsafe { copy_list(out_ids, max_out, |st| &st.random_bot_guilds) }
}

/// Copy the currently tracked random-bot arena team ids into `out_ids`
/// (up to `max_out` entries) and return the total count of tracked
/// ids. Always 0 on Classic.
///
/// # Safety
///
/// `out_ids` must be either null with `max_out == 0` or a valid pointer
/// to `max_out` consecutive `u32` slots.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_random_factory_get_arena_teams(
    out_ids: *mut u32,
    max_out: u32,
) -> u32 {
    // Safety: forwarded from the caller's contract above.
    unsafe { copy_list(out_ids, max_out, |st| &st.random_bot_arena_teams) }
}

// ── Internal helpers ──────────────────────────────────────────────────────

/// Grab an Arc-clone of the vtable wrapper and move the runtime state
/// out of the singleton. Leaves the singleton with a default
/// `FactoryState` so concurrent accessors never observe stale data.
fn take_world_and_state() -> Option<(Arc<dyn RandomFactoryWorld>, FactoryState)> {
    let mut guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    let st = guard.as_mut()?;
    let world = Arc::clone(&st.world);
    let factory_state = std::mem::take(&mut st.state);
    Some((world, factory_state))
}

/// Commit a mutated `FactoryState` back into the singleton. Silently
/// discards the state when the module has been shut down between
/// `take_world_and_state` and this call — which should be impossible
/// under normal use but must not panic.
fn put_back_state(factory_state: FactoryState) {
    let mut guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    if let Some(st) = guard.as_mut() {
        st.state = factory_state;
    }
}

/// Shared helper for the three `get_*` FFI entry points. Reads the
/// requested list from the singleton under the mutex and copies up to
/// `max_out` entries into the caller's buffer.
///
/// # Safety
///
/// `out_ids` must be either null with `max_out == 0` or a valid pointer
/// to `max_out` consecutive `u32` slots.
unsafe fn copy_list(
    out_ids: *mut u32,
    max_out: u32,
    selector: fn(&FactoryState) -> &Vec<u32>,
) -> u32 {
    let guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    let Some(st) = guard.as_ref() else {
        return 0;
    };

    let list = selector(&st.state);
    let total = list.len() as u32;
    if out_ids.is_null() || max_out == 0 {
        return total;
    }

    let copy_n = total.min(max_out) as usize;
    // Safety: caller guarantees `out_ids` points to `max_out` writable
    // `u32` slots; `copy_n <= max_out` by construction.
    unsafe {
        std::ptr::copy_nonoverlapping(list.as_ptr(), out_ids, copy_n);
    }
    total
}
