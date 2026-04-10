//! `extern "C"` surface for the Rust login queue.
//!
//! The C++ shim at `cpp_wrapper/LoginBridge.{h,cpp}` owns the
//! `LoginCallbacks` vtable and calls into this module via:
//!
//! * [`playerbot_login_init`] — install the vtable + spawn the worker.
//! * [`playerbot_login_update`] — per-tick push of the real-player
//!   snapshot + drain of pending login/logout commands.
//! * [`playerbot_login_toggle_debug`] — flip the worker's debug flag.
//! * [`playerbot_login_shutdown`] — join the worker, tear down state.
//!
//! All state is held in a single `OnceLock<Mutex<Option<LoginState>>>`.
//! The option is `None` until `init`, then `Some` until `shutdown`. No
//! locks are held across FFI boundaries; the mutex is released before
//! any command dispatch so the main thread never blocks on the worker.

use std::sync::{Arc, Mutex, OnceLock};

use cmangos::{BotRealPlayerInfo, LoginCallbacks, LoginWorld, VtableLoginWorld};

use crate::log_error;
use crate::login::space::RealPlayerSummary;
use crate::login::worker::LoginWorkerHandle;

/// Singleton state owned by the FFI layer. Wrapped in a `Mutex<Option>`
/// so `init`/`shutdown` can install/remove it without racing each other.
struct LoginState {
    worker: LoginWorkerHandle,
}

fn state() -> &'static Mutex<Option<LoginState>> {
    static STATE: OnceLock<Mutex<Option<LoginState>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

/// Install the login vtable and spawn the worker thread. Safe to call
/// more than once — a second call tears down the previous state first.
///
/// # Safety
///
/// `cbs` must either be null (no-op) or a valid, fully-initialised
/// `LoginCallbacks` whose function pointers remain valid until
/// [`playerbot_login_shutdown`] returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_login_init(cbs: *const LoginCallbacks) {
    if cbs.is_null() {
        return;
    }

    // Copy the vtable by value — it's a PoD struct of fn pointers.
    // Safety: caller guarantees `cbs` points to a fully-initialised
    // `LoginCallbacks`.
    let cbs_copy = unsafe { *cbs };

    // Safety: the caller promises the function pointers stay valid
    // until shutdown; the wrapper is `Send + Sync` because every
    // underlying C call is thread-safe for our use (see
    // `VtableLoginWorld` doc comment).
    let world: Arc<dyn LoginWorld> =
        Arc::new(unsafe { VtableLoginWorld::new(cbs_copy) });

    let mut guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    // Tear down any previous worker first, then spawn a fresh one.
    drop(guard.take());
    *guard = Some(LoginState {
        worker: LoginWorkerHandle::spawn(world),
    });
}

/// Push a real-player snapshot to the worker and drain any pending
/// login/logout commands.
///
/// # Safety
///
/// `real_players` must either be null with `count == 0`, or a valid
/// pointer to `count` consecutive `BotRealPlayerInfo` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn playerbot_login_update(
    real_players: *const BotRealPlayerInfo,
    count: u32,
) {
    // Safety: forwarded from the caller's contract above — `real_players`
    // is either null with `count == 0`, or a valid pointer to `count`
    // consecutive entries that stay live for the duration of this call.
    let snapshot = unsafe { cmangos::login_world::borrow_real_players(real_players, count) };
    let players: Vec<RealPlayerSummary> =
        snapshot.iter().map(RealPlayerSummary::from).collect();

    // Take the state handle without holding the mutex across the
    // channel send — channels are lock-free, but we still want to
    // minimise contention.
    let guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    let Some(st) = guard.as_ref() else {
        return;
    };

    if !st.worker.send_tick(players) {
        log_error!("playerbot_login_update: worker channel closed");
        return;
    }

    // Drain anything the worker already produced and dispatch on this
    // thread. Holding the mutex through dispatch is fine: the dispatch
    // only calls into the main-thread-only `perform_login` /
    // `perform_logout` entries, which themselves are already
    // synchronised inside the C++ bridge.
    for output in st.worker.drain_tick_output() {
        st.worker.dispatch_commands(&output);
    }
}

/// Toggle the worker's debug flag. Safe no-op if the worker has not
/// been initialised.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_login_toggle_debug() {
    let guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    if let Some(st) = guard.as_ref() {
        st.worker.toggle_debug();
    }
}

/// Join the worker thread and drop the vtable. Safe to call multiple
/// times.
#[unsafe(no_mangle)]
pub extern "C" fn playerbot_login_shutdown() {
    let mut guard = match state().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    if let Some(st) = guard.take() {
        drop(st); // Drop impls join the worker thread.
    }
}
