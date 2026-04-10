//! `LoginWorld` — the module-level interface the bot login queue worker
//! uses to talk to `CMaNGOS`.
//!
//! Unlike [`World`](crate::World) which is per-bot, `LoginWorld` is shared
//! across all candidate bots: it owns the random-bot candidate enumeration,
//! the async holder lifecycle, the `RandomPlayerbotMgr` KV store, and the
//! actual login/logout dispatch. Separating it from `World` keeps the
//! per-bot trait focused on per-bot semantics and lets the login worker
//! thread carry a single `LoginWorld` handle instead of a per-bot vtable.
//!
//! Two implementations exist, mirroring the `World` split:
//!
//! * [`VtableLoginWorld`] — production impl, wraps a `*const LoginCallbacks`
//!   from the C++ side. Built once at module init on the worker thread.
//! * [`MockLoginWorld`] — feature-gated under `mock`; fixture-backed pure
//!   Rust impl used by the `crates/playerbot` login tests.

use crate::{BotCandidateInfo, BotRealPlayerInfo, LoginCallbacks};

/// Mirrors the C `PLAYERBOT_HOLDER_*` constants with named variants.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum HolderState {
    #[default]
    Empty,
    Sent,
    Received,
}

impl HolderState {
    /// Decode from the raw u8 returned by `get_holder_state`. Unknown
    /// values map to [`HolderState::Empty`] — the safest fallback since
    /// `Empty` causes the login queue to try again later.
    #[must_use]
    pub const fn from_raw(raw: u8) -> Self {
        match raw {
            1 => Self::Sent,
            2 => Self::Received,
            _ => Self::Empty,
        }
    }
}

/// Abstract interface the login worker uses to reach `CMaNGOS`.
///
/// Every method is callable from the worker thread except
/// [`perform_login`](LoginWorld::perform_login) and
/// [`perform_logout`](LoginWorld::perform_logout), which mutate map state
/// and must be dispatched on the main thread. The worker forwards those
/// as commands via an `mpsc::channel`; the main thread's
/// `playerbot_login_update` entry point drains the channel and calls them
/// on a `LoginWorld` it owns directly.
pub trait LoginWorld: Send + Sync {
    /// Scan random-bot accounts whose username begins with `account_prefix`
    /// (case-insensitively) and return every matching character row.
    ///
    /// Called once after startup and then at most once per worker loop
    /// iteration when the pool needs refreshing.
    fn query_candidates(&self, account_prefix: &str) -> Vec<BotCandidateInfo>;

    /// Kick off the async `DelayQueryHolder` for `(account, guid)`.
    /// Returns `true` if the request was queued, `false` on immediate
    /// failure (e.g. the database is not reachable). The bridge tracks
    /// holders internally keyed on `guid`; the worker polls
    /// [`holder_state`](LoginWorld::holder_state) until the holder
    /// arrives.
    fn send_holder(&self, account: u32, guid: u32) -> bool;

    /// Read the current holder state for `guid`.
    fn holder_state(&self, guid: u32) -> HolderState;

    /// Discard the holder for `guid` — used when the login queue resets
    /// a bot's state or the holder has been consumed by `perform_login`.
    fn clear_holder(&self, guid: u32);

    /// `RandomPlayerbotMgr::GetValue` — thin FFI passthrough.
    fn random_mgr_get_value(&self, guid: u32, key: &str) -> u32;

    /// `RandomPlayerbotMgr::SetValue` — thin FFI passthrough. `valid_in_s`
    /// matches the C++ overload; pass `-1` for "use default".
    fn random_mgr_set_value(&self, guid: u32, key: &str, value: u32, valid_in_s: i32);

    /// Current average level of online real players (used to cap the
    /// random bot level).
    fn players_level(&self) -> u32;

    /// Current target bot population (from
    /// `RandomPlayerbotMgr::GetValue(0, "bot_count")`).
    fn max_online_bot_count(&self) -> u32;

    /// `sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL)`. Exposed
    /// through the vtable because we can't reach the `CMaNGOS` global
    /// directly from Rust.
    fn world_max_level(&self) -> u32;

    /// Current `CharacterDatabase` delay in milliseconds — the legacy
    /// queue uses this to back off from `SendHolder` bursts.
    fn database_delay_ms(&self) -> u32;

    /// Send an async `select 1` ping — the legacy queue issues this
    /// before its holder burst so `GetDatabaseDelay` sees an up-to-date
    /// value.
    fn database_ping(&self);

    /// Dispatch the actual login for `guid`. Main-thread only.
    fn perform_login(&self, guid: u32) -> bool;

    /// Dispatch the actual logout for `guid`. Main-thread only.
    fn perform_logout(&self, guid: u32) -> bool;

    /// Forward a debug line to the C++ log sink (used when
    /// `PlayerBotLoginMgr::debug` is on). The default implementation
    /// does nothing so mocks don't have to care.
    fn log_debug(&self, _msg: &str) {}
}

/// Production impl — wraps the C `LoginCallbacks` vtable.
///
/// The C pointer is stored as a raw function pointer because
/// `LoginCallbacks` is `Copy`; the struct is filled once on the C++ side
/// and never mutated again. `VtableLoginWorld` is `Send + Sync` because
/// the underlying pointers refer to global C++ state that is thread-safe
/// for the operations exposed here (the random bot manager guards its
/// KV map, `CharacterDatabase.DelayQueryHolder` is documented-safe to
/// call from any thread, and `perform_login`/`perform_logout` are only
/// called on the main thread anyway).
pub struct VtableLoginWorld {
    cbs: LoginCallbacks,
}

#[allow(unsafe_code)]
// Safety: every field on LoginCallbacks is a plain function pointer
// whose implementation is thread-safe per the invariants above.
unsafe impl Send for VtableLoginWorld {}
#[allow(unsafe_code)]
unsafe impl Sync for VtableLoginWorld {}

impl VtableLoginWorld {
    /// # Safety
    /// `cbs` must be a fully initialised `LoginCallbacks` whose function
    /// pointers remain valid for the entire lifetime of the returned
    /// object (in practice: until server shutdown).
    #[must_use]
    #[allow(unsafe_code)]
    pub unsafe fn new(cbs: LoginCallbacks) -> Self {
        Self { cbs }
    }
}

#[allow(unsafe_code)]
impl LoginWorld for VtableLoginWorld {
    fn query_candidates(&self, account_prefix: &str) -> Vec<BotCandidateInfo> {
        let Some(query) = self.cbs.query_candidates else {
            return Vec::new();
        };
        let Some(free) = self.cbs.free_candidate_list else {
            return Vec::new();
        };

        // Stack-local nul-terminated copy of the prefix. The C++ side
        // only reads `strlen(prefix)` bytes so a small buffer is enough
        // for any random-bot account prefix.
        let mut buf = [0u8; 64];
        let bytes = account_prefix.as_bytes();
        let n = bytes.len().min(buf.len() - 1);
        buf[..n].copy_from_slice(&bytes[..n]);
        buf[n] = 0;

        let mut ptr: *mut BotCandidateInfo = core::ptr::null_mut();
        // Safety: `query_candidates` owns the allocation, we free it
        // below via the matching `free_candidate_list`.
        let count = unsafe { query(buf.as_ptr().cast(), &raw mut ptr) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }

        // Safety: the C++ side guarantees `ptr..ptr+count` is readable.
        let slice = unsafe { core::slice::from_raw_parts(ptr, count as usize) };
        let out = slice.to_vec();
        // Safety: paired free.
        unsafe { free(ptr, count) };
        out
    }

    fn send_holder(&self, account: u32, guid: u32) -> bool {
        self.cbs
            .send_holder
            .is_some_and(|f| unsafe { f(account, guid) })
    }

    fn holder_state(&self, guid: u32) -> HolderState {
        HolderState::from_raw(
            self.cbs
                .get_holder_state
                .map_or(0, |f| unsafe { f(guid) }),
        )
    }

    fn clear_holder(&self, guid: u32) {
        if let Some(f) = self.cbs.clear_holder {
            unsafe { f(guid) };
        }
    }

    fn random_mgr_get_value(&self, guid: u32, key: &str) -> u32 {
        let Some(f) = self.cbs.random_mgr_get_value else {
            return 0;
        };
        let mut buf = [0u8; 64];
        let bytes = key.as_bytes();
        let n = bytes.len().min(buf.len() - 1);
        buf[..n].copy_from_slice(&bytes[..n]);
        buf[n] = 0;
        unsafe { f(guid, buf.as_ptr().cast()) }
    }

    fn random_mgr_set_value(&self, guid: u32, key: &str, value: u32, valid_in_s: i32) {
        let Some(f) = self.cbs.random_mgr_set_value else {
            return;
        };
        let mut buf = [0u8; 64];
        let bytes = key.as_bytes();
        let n = bytes.len().min(buf.len() - 1);
        buf[..n].copy_from_slice(&bytes[..n]);
        buf[n] = 0;
        unsafe { f(guid, buf.as_ptr().cast(), value, valid_in_s) };
    }

    fn players_level(&self) -> u32 {
        self.cbs
            .random_mgr_get_players_level
            .map_or(0, |f| unsafe { f() })
    }

    fn max_online_bot_count(&self) -> u32 {
        self.cbs
            .random_mgr_get_max_online_bot_count
            .map_or(0, |f| unsafe { f() })
    }

    fn world_max_level(&self) -> u32 {
        self.cbs
            .random_mgr_get_world_max_level
            .map_or(0, |f| unsafe { f() })
    }

    fn database_delay_ms(&self) -> u32 {
        self.cbs
            .random_mgr_database_delay_ms
            .map_or(0, |f| unsafe { f() })
    }

    fn database_ping(&self) {
        if let Some(f) = self.cbs.random_mgr_database_ping {
            unsafe { f() };
        }
    }

    fn perform_login(&self, guid: u32) -> bool {
        self.cbs
            .perform_login
            .is_some_and(|f| unsafe { f(guid) })
    }

    fn perform_logout(&self, guid: u32) -> bool {
        self.cbs
            .perform_logout
            .is_some_and(|f| unsafe { f(guid) })
    }

    fn log_debug(&self, msg: &str) {
        let Some(f) = self.cbs.log_debug else {
            return;
        };
        let mut buf = [0u8; 512];
        let bytes = msg.as_bytes();
        let n = bytes.len().min(buf.len() - 1);
        buf[..n].copy_from_slice(&bytes[..n]);
        buf[n] = 0;
        unsafe { f(buf.as_ptr().cast()) };
    }
}

/// The raw pointer into the per-tick `BotRealPlayerInfo` array is
/// borrowed back into a slice for callers. This shim lets the bridge's
/// `playerbot_login_update` FFI entry point hand the slice over without
/// any copies.
///
/// # Safety
/// `ptr` must be either null or a pointer to a valid `BotRealPlayerInfo`
/// array of length `count` that remains live for the duration of the
/// returned borrow `'a`. The caller is responsible for choosing `'a`
/// such that no aliasing `&mut` exists during that span.
#[must_use]
#[allow(unsafe_code)]
pub unsafe fn borrow_real_players<'a>(
    ptr: *const BotRealPlayerInfo,
    count: u32,
) -> &'a [BotRealPlayerInfo] {
    if ptr.is_null() || count == 0 {
        &[]
    } else {
        // Safety: callers (the `playerbot_login_update` FFI shim) pass a
        // pointer that is valid for `count` elements for the duration of
        // the call.
        unsafe { core::slice::from_raw_parts(ptr, count as usize) }
    }
}

/// In-memory mock used by the `crates/playerbot` login tests.
///
/// `MockLoginWorld` is feature-gated under `mock` (same as
/// [`MockWorld`](crate::MockWorld)) and every method is fully
/// implemented — no `unimplemented!` paths. Tests drive it by
/// pre-seeding candidate rows, holder states, and RPBM key/value pairs,
/// then assert on the recorded event log after running the login queue.
#[cfg(any(test, feature = "mock"))]
pub use mock_impl::{MockLoginEvent, MockLoginWorld};

#[cfg(any(test, feature = "mock"))]
mod mock_impl {
    use super::{BotCandidateInfo, HolderState, LoginWorld};
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// An event recorded by [`MockLoginWorld`]. Tests assert on the
    /// event list after running the login queue to verify transitions.
    #[derive(Clone, Debug, PartialEq)]
    pub enum MockLoginEvent {
        SendHolder { account: u32, guid: u32 },
        ClearHolder(u32),
        SetValue { guid: u32, key: String, value: u32, valid_in_s: i32 },
        DatabasePing,
        PerformLogin(u32),
        PerformLogout(u32),
        Debug(String),
    }

    #[derive(Default)]
    struct MockState {
        candidates: Vec<BotCandidateInfo>,
        holder_states: HashMap<u32, HolderState>,
        values: HashMap<(u32, String), u32>,
        players_level: u32,
        max_online_bot_count: u32,
        world_max_level: u32,
        database_delay_ms: u32,
        logins: HashMap<u32, bool>,
        logouts: HashMap<u32, bool>,
        events: Vec<MockLoginEvent>,
    }

    /// Fixture-backed [`LoginWorld`] impl for unit/integration tests.
    ///
    /// Thread-safe (`Send + Sync`) by internal `Mutex`; tests that spawn
    /// the login worker thread can share a single `Arc<MockLoginWorld>`.
    #[derive(Default)]
    pub struct MockLoginWorld {
        state: Mutex<MockState>,
    }

    impl MockLoginWorld {
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Replace the fixture candidate list.
        pub fn set_candidates(&self, rows: Vec<BotCandidateInfo>) {
            self.state.lock().unwrap().candidates = rows;
        }

        /// Seed a holder state for `guid`. Unknown guids return
        /// [`HolderState::Empty`] on query.
        pub fn set_holder_state(&self, guid: u32, state: HolderState) {
            self.state.lock().unwrap().holder_states.insert(guid, state);
        }

        /// Seed a KV entry.
        pub fn set_value(&self, guid: u32, key: &str, value: u32) {
            self.state
                .lock()
                .unwrap()
                .values
                .insert((guid, key.to_string()), value);
        }

        /// Set the fake `players_level()` return value.
        pub fn set_players_level(&self, lvl: u32) {
            self.state.lock().unwrap().players_level = lvl;
        }

        /// Set the fake `max_online_bot_count()` return value.
        pub fn set_max_online_bot_count(&self, n: u32) {
            self.state.lock().unwrap().max_online_bot_count = n;
        }

        /// Set the fake `world_max_level()` return value.
        pub fn set_world_max_level(&self, lvl: u32) {
            self.state.lock().unwrap().world_max_level = lvl;
        }

        /// Set the fake `database_delay_ms()` return value.
        pub fn set_database_delay_ms(&self, ms: u32) {
            self.state.lock().unwrap().database_delay_ms = ms;
        }

        /// Control what `perform_login(guid)` returns.
        pub fn set_login_result(&self, guid: u32, ok: bool) {
            self.state.lock().unwrap().logins.insert(guid, ok);
        }

        /// Control what `perform_logout(guid)` returns.
        pub fn set_logout_result(&self, guid: u32, ok: bool) {
            self.state.lock().unwrap().logouts.insert(guid, ok);
        }

        /// Take the recorded event log, clearing it.
        pub fn take_events(&self) -> Vec<MockLoginEvent> {
            std::mem::take(&mut self.state.lock().unwrap().events)
        }

        /// Peek at the event log without clearing it.
        pub fn events(&self) -> Vec<MockLoginEvent> {
            self.state.lock().unwrap().events.clone()
        }
    }

    impl LoginWorld for MockLoginWorld {
        fn query_candidates(&self, _account_prefix: &str) -> Vec<BotCandidateInfo> {
            self.state.lock().unwrap().candidates.clone()
        }

        fn send_holder(&self, account: u32, guid: u32) -> bool {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockLoginEvent::SendHolder { account, guid });
            // In the mock, the holder is immediately "sent". Tests can
            // seed a later state via `set_holder_state` between ticks
            // to simulate the async callback.
            s.holder_states
                .entry(guid)
                .and_modify(|st| {
                    if *st == HolderState::Empty {
                        *st = HolderState::Sent;
                    }
                })
                .or_insert(HolderState::Sent);
            true
        }

        fn holder_state(&self, guid: u32) -> HolderState {
            self.state
                .lock()
                .unwrap()
                .holder_states
                .get(&guid)
                .copied()
                .unwrap_or(HolderState::Empty)
        }

        fn clear_holder(&self, guid: u32) {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockLoginEvent::ClearHolder(guid));
            s.holder_states.remove(&guid);
        }

        fn random_mgr_get_value(&self, guid: u32, key: &str) -> u32 {
            self.state
                .lock()
                .unwrap()
                .values
                .get(&(guid, key.to_string()))
                .copied()
                .unwrap_or(0)
        }

        fn random_mgr_set_value(&self, guid: u32, key: &str, value: u32, valid_in_s: i32) {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockLoginEvent::SetValue {
                guid,
                key: key.to_string(),
                value,
                valid_in_s,
            });
            s.values.insert((guid, key.to_string()), value);
        }

        fn players_level(&self) -> u32 {
            self.state.lock().unwrap().players_level
        }

        fn max_online_bot_count(&self) -> u32 {
            self.state.lock().unwrap().max_online_bot_count
        }

        fn world_max_level(&self) -> u32 {
            self.state.lock().unwrap().world_max_level
        }

        fn database_delay_ms(&self) -> u32 {
            self.state.lock().unwrap().database_delay_ms
        }

        fn database_ping(&self) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockLoginEvent::DatabasePing);
        }

        fn perform_login(&self, guid: u32) -> bool {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockLoginEvent::PerformLogin(guid));
            s.logins.get(&guid).copied().unwrap_or(true)
        }

        fn perform_logout(&self, guid: u32) -> bool {
            let mut s = self.state.lock().unwrap();
            s.events.push(MockLoginEvent::PerformLogout(guid));
            s.logouts.get(&guid).copied().unwrap_or(true)
        }

        fn log_debug(&self, msg: &str) {
            self.state
                .lock()
                .unwrap()
                .events
                .push(MockLoginEvent::Debug(msg.to_string()));
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn holder_state_round_trip() {
            let m = MockLoginWorld::new();
            assert_eq!(m.holder_state(42), HolderState::Empty);
            m.set_holder_state(42, HolderState::Received);
            assert_eq!(m.holder_state(42), HolderState::Received);
        }

        #[test]
        fn send_holder_promotes_empty_to_sent() {
            let m = MockLoginWorld::new();
            assert!(m.send_holder(1, 42));
            assert_eq!(m.holder_state(42), HolderState::Sent);
        }

        #[test]
        fn set_value_writes_and_reads() {
            let m = MockLoginWorld::new();
            m.random_mgr_set_value(42, "add", 1, -1);
            assert_eq!(m.random_mgr_get_value(42, "add"), 1);
        }

        #[test]
        fn candidates_round_trip() {
            let m = MockLoginWorld::new();
            m.set_candidates(vec![BotCandidateInfo {
                account_id: 1,
                guid: 42,
                race: 1,
                cls: 1,
                level: 30,
                is_online: false,
                total_played_time: 0,
                map_id: 0,
                x: 0.0,
                y: 0.0,
                z: 0.0,
                o: 0.0,
                guild_id: 0,
                group_id: 0,
            }]);
            let rows = m.query_candidates("rndbot");
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].guid, 42);
        }

        #[test]
        fn events_track_ordered_calls() {
            let m = MockLoginWorld::new();
            m.send_holder(1, 42);
            m.perform_login(42);
            m.random_mgr_set_value(42, "add", 1, -1);
            let events = m.take_events();
            assert_eq!(events.len(), 3);
            assert!(matches!(events[0], MockLoginEvent::SendHolder { guid: 42, .. }));
            assert!(matches!(events[1], MockLoginEvent::PerformLogin(42)));
            assert!(matches!(
                events[2],
                MockLoginEvent::SetValue { guid: 42, .. }
            ));
            assert!(m.take_events().is_empty());
        }
    }
}
