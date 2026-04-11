//! In-memory `CachedEvent` KV store — port of the
//! `eventCache`/`GetEventValue`/`SetEventValue` trio from the legacy C++
//! `RandomPlayerbotMgr`.
//!
//! Layout: `HashMap<bot_guid, HashMap<event_name, CachedEvent>>`, loaded
//! lazily — the first `get_value(bot, event)` call for a bot triggers a
//! full row fetch through the world trait, mirroring the C++ behaviour
//! where `eventCache[bot]` was populated on the first access.
//!
//! TTL semantics: every cached entry carries a `last_change_time` and
//! a `valid_in` window. Once `(now - last_change_time) >= valid_in`
//! the entry is considered expired and `get_value` returns 0 — except
//! for the six "sticky" event names the C++ code whitelists
//! (`specNo`, `specLink`, `init`, `current_time`, `always`, `selfbot`),
//! which always return their stored value regardless of TTL.
//!
//! Writes always go through [`set_value`](`EventCache::set_value`),
//! which deletes + reinserts the row via the trait (the C++ `DELETE`
//! + `INSERT` pair) and updates the in-memory cache under a single
//! logical operation.
//!
//! This module stays free of `RandomMgrState` references — it operates
//! against `&dyn RandomMgrWorld` so both the worker thread and the
//! main-thread `get_value` FFI can reuse it without owning the whole
//! manager state container.

use std::collections::HashMap;

use cmangos::{EventRow, RandomMgrWorld};

/// Event names whose cached value is NOT subject to the TTL check.
/// Mirrors the C++ whitelist in `RandomPlayerbotMgr::GetEventValue`.
const STICKY_EVENTS: &[&str] = &[
    "specNo",
    "specLink",
    "init",
    "current_time",
    "always",
    "selfbot",
];

/// Default TTL applied by `SetValue` when the caller passes `-1`
/// (meaning "use default"). Matches the C++ constant
/// `15 * 24 * 3600` seconds = 15 days.
pub const DEFAULT_VALUE_TTL_S: u32 = 15 * 24 * 3600;

/// One cached event row — the Rust analogue of the C++ `CachedEvent`
/// struct.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CachedEvent {
    pub value: u32,
    pub last_change_time: u32,
    pub valid_in: u32,
    pub data: String,
}

impl CachedEvent {
    /// True iff the cached entry is "uninitialised" — no timestamp
    /// was ever written. Mirrors the C++ `CachedEvent::IsEmpty`.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.last_change_time == 0
    }
}

/// In-memory event cache. `HashMap<bot, HashMap<event, CachedEvent>>`.
///
/// Populated lazily — the first `get_value(bot, _)` call for a given
/// bot triggers a `query_events_for_bot` pass through the world trait,
/// mirroring the C++ `eventCache[bot].empty()` guard.
#[derive(Clone, Debug, Default)]
pub struct EventCache {
    bots: HashMap<u32, HashMap<String, CachedEvent>>,
    /// Tracks bots whose per-bot map has been hydrated from the DB.
    /// Distinct from "has any entry" because `SetEventValue` can
    /// insert into `bots[guid]` before the initial fetch has run.
    loaded: std::collections::HashSet<u32>,
}

impl EventCache {
    /// Construct an empty cache. Caller must call
    /// [`hydrate_all`](Self::hydrate_all) or allow lazy hydration via
    /// `get_value`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Eagerly load every `ai_playerbot_random_bots` row (owner = 0).
    /// Used once at startup by the worker so `SyncEventTimers` can
    /// operate on the full cache without a lazy-load race.
    pub fn hydrate_all(&mut self, world: &dyn RandomMgrWorld) {
        let rows = world.query_all_events();
        self.bots.clear();
        self.loaded.clear();
        for row in rows {
            self.bots
                .entry(row.bot)
                .or_default()
                .insert(row.event, CachedEvent {
                    value: row.value,
                    last_change_time: row.last_change_time,
                    valid_in: row.valid_in,
                    data: row.data,
                });
            self.loaded.insert(row.bot);
        }
    }

    /// Drop every cached entry. Mirrors the console `reset` command.
    pub fn clear(&mut self) {
        self.bots.clear();
        self.loaded.clear();
    }

    /// True iff `(bot, event)` has an entry in the in-memory cache.
    /// Does not trigger a lazy load.
    #[must_use]
    pub fn contains(&self, bot: u32, event: &str) -> bool {
        self.bots
            .get(&bot)
            .is_some_and(|m| m.contains_key(event))
    }

    /// True iff the entire per-bot map for `bot` is empty. Used by
    /// `UpdateAIInternal` to gate the "log in cached bot" fast path.
    #[must_use]
    pub fn per_bot_empty(&self, bot: u32) -> bool {
        self.bots.get(&bot).is_none_or(|m| m.is_empty())
    }

    /// Port of `RandomPlayerbotMgr::GetEventValue(bot, event)`.
    ///
    /// On first access for a given `bot`, lazily triggers a
    /// `query_events_for_bot` pass and populates the cache. After
    /// that, hits the in-memory map and applies the TTL rule to
    /// produce the "0 on expiry" semantics — with the `STICKY_EVENTS`
    /// whitelist exempted.
    #[must_use]
    pub fn get_value(
        &mut self,
        bot: u32,
        event: &str,
        now_epoch_s: u32,
        world: &dyn RandomMgrWorld,
    ) -> u32 {
        self.ensure_loaded(bot, world);
        let Some(m) = self.bots.get(&bot) else {
            return 0;
        };
        let Some(e) = m.get(event) else {
            return 0;
        };
        if STICKY_EVENTS.contains(&event) {
            return e.value;
        }
        if now_epoch_s.saturating_sub(e.last_change_time) >= e.valid_in {
            0
        } else {
            e.value
        }
    }

    /// Port of `GetValueValidTime`. Returns the remaining seconds on
    /// the entry, or 0 if the entry is missing. Negative values (past
    /// the expiry) are reported as-is to preserve the C++ behaviour.
    #[must_use]
    pub fn get_value_valid_time(&self, bot: u32, event: &str, now_epoch_s: u32) -> i64 {
        let Some(m) = self.bots.get(&bot) else {
            return 0;
        };
        let Some(e) = m.get(event) else {
            return 0;
        };
        i64::from(e.valid_in) - i64::from(now_epoch_s.saturating_sub(e.last_change_time))
    }

    /// Port of `GetEventData(bot, event)`. Returns an empty string if
    /// the entry is missing or expired.
    #[must_use]
    pub fn get_data(
        &mut self,
        bot: u32,
        event: &str,
        now_epoch_s: u32,
        world: &dyn RandomMgrWorld,
    ) -> String {
        if self.get_value(bot, event, now_epoch_s, world) == 0 {
            return String::new();
        }
        self.bots
            .get(&bot)
            .and_then(|m| m.get(event))
            .map(|e| e.data.clone())
            .unwrap_or_default()
    }

    /// Port of `SetEventValue`. `valid_in_s == 0` means "no TTL".
    /// `value == 0` means "delete the row" — mirrors the `if (value)`
    /// branch in the C++ version.
    #[allow(clippy::too_many_arguments)] // mirrors the C++ SetEventValue signature 1:1
    pub fn set_value(
        &mut self,
        bot: u32,
        event: &str,
        value: u32,
        valid_in_s: u32,
        data: &str,
        now_epoch_s: u32,
        world: &dyn RandomMgrWorld,
    ) {
        // Always delete the old row first, matching the C++
        // `DELETE ... WHERE bot = ? AND event = ?` step.
        world.delete_event(bot, event);

        if value != 0 {
            let row = EventRow {
                bot,
                event: event.to_string(),
                value,
                last_change_time: now_epoch_s,
                valid_in: valid_in_s,
                data: data.to_string(),
            };
            world.upsert_event(&row);
        }

        // Update the in-memory cache. Deleting a row yields an empty
        // `CachedEvent` — the C++ code writes `CachedEvent(value=0,…)`
        // in both branches, so we mirror that.
        let cached = CachedEvent {
            value,
            last_change_time: now_epoch_s,
            valid_in: valid_in_s,
            data: data.to_string(),
        };
        self.bots
            .entry(bot)
            .or_default()
            .insert(event.to_string(), cached);
        self.loaded.insert(bot);
    }

    /// Port of the top-level `SetValue(bot, type, value, data,
    /// validIn = -1)`. `valid_in_s = None` means "use
    /// `DEFAULT_VALUE_TTL_S`"; `Some(0)` means "no TTL".
    #[allow(clippy::too_many_arguments)] // mirrors the C++ SetValue signature 1:1
    pub fn set_value_default_ttl(
        &mut self,
        bot: u32,
        event: &str,
        value: u32,
        data: &str,
        valid_in_s: Option<u32>,
        now_epoch_s: u32,
        world: &dyn RandomMgrWorld,
    ) {
        let ttl = valid_in_s.unwrap_or(DEFAULT_VALUE_TTL_S);
        self.set_value(bot, event, value, ttl, data, now_epoch_s, world);
    }

    /// Drop every cached entry for `bot`. Mirrors the C++ helper used
    /// by `Remove(bot)`.
    pub fn drop_bot(&mut self, bot: u32) {
        self.bots.remove(&bot);
        self.loaded.remove(&bot);
    }

    /// Bump every `last_change_time` in the cache by `delta_seconds`.
    /// Mirrors the C++ `SaveCurTime` path where the manager bumps the
    /// DB and then replays the delta onto the in-memory rows.
    pub fn bump_all_times(&mut self, delta_seconds: u32) {
        for m in self.bots.values_mut() {
            for e in m.values_mut() {
                e.last_change_time = e.last_change_time.saturating_add(delta_seconds);
            }
        }
    }

    fn ensure_loaded(&mut self, bot: u32, world: &dyn RandomMgrWorld) {
        if self.loaded.contains(&bot) {
            return;
        }
        let rows = world.query_events_for_bot(bot);
        let entry = self.bots.entry(bot).or_default();
        for row in rows {
            entry.insert(row.event, CachedEvent {
                value: row.value,
                last_change_time: row.last_change_time,
                valid_in: row.valid_in,
                data: row.data,
            });
        }
        self.loaded.insert(bot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::MockRandomMgrWorld;

    fn mk_row(bot: u32, event: &str, value: u32, last: u32, ttl: u32) -> EventRow {
        EventRow {
            bot,
            event: event.into(),
            value,
            last_change_time: last,
            valid_in: ttl,
            data: String::new(),
        }
    }

    #[test]
    fn lazy_load_on_first_get() {
        let world = MockRandomMgrWorld::new();
        world.set_event_rows(vec![mk_row(1, "add", 42, 100, 600)]);
        let mut cache = EventCache::new();

        assert_eq!(cache.get_value(1, "add", 200, &world), 42);
        assert_eq!(cache.get_value(1, "missing", 200, &world), 0);
    }

    #[test]
    fn ttl_expires_value_after_window() {
        let world = MockRandomMgrWorld::new();
        world.set_event_rows(vec![mk_row(1, "add", 5, 100, 60)]);
        let mut cache = EventCache::new();

        // now - last = 30 < 60 → returns value
        assert_eq!(cache.get_value(1, "add", 130, &world), 5);
        // now - last = 60 >= 60 → expired → returns 0
        assert_eq!(cache.get_value(1, "add", 160, &world), 0);
    }

    #[test]
    fn sticky_events_ignore_ttl() {
        let world = MockRandomMgrWorld::new();
        world.set_event_rows(vec![
            mk_row(1, "specNo", 7, 100, 60),
            mk_row(1, "always", 3, 100, 0),
        ]);
        let mut cache = EventCache::new();
        assert_eq!(cache.get_value(1, "specNo", 10_000, &world), 7);
        assert_eq!(cache.get_value(1, "always", 10_000, &world), 3);
    }

    #[test]
    fn set_value_writes_and_updates_cache() {
        let world = MockRandomMgrWorld::new();
        let mut cache = EventCache::new();

        cache.set_value(1, "add", 9, 120, "", 500, &world);
        assert_eq!(cache.get_value(1, "add", 510, &world), 9);

        // Writing value=0 deletes the in-memory entry via the TTL
        // check on the now-zero `value` field.
        cache.set_value(1, "add", 0, 0, "", 600, &world);
        assert_eq!(cache.get_value(1, "add", 610, &world), 0);
    }

    #[test]
    fn drop_bot_clears_state() {
        let world = MockRandomMgrWorld::new();
        world.set_event_rows(vec![mk_row(1, "add", 1, 100, 60)]);
        let mut cache = EventCache::new();
        let _ = cache.get_value(1, "add", 120, &world);
        cache.drop_bot(1);
        assert!(cache.per_bot_empty(1));
        assert!(!cache.contains(1, "add"));
    }

    #[test]
    fn hydrate_all_populates_cache() {
        let world = MockRandomMgrWorld::new();
        world.set_event_rows(vec![
            mk_row(1, "add", 1, 0, 0),
            mk_row(2, "logout", 2, 0, 0),
        ]);
        let mut cache = EventCache::new();
        cache.hydrate_all(&world);
        assert!(cache.contains(1, "add"));
        assert!(cache.contains(2, "logout"));
    }

    #[test]
    fn get_value_valid_time_returns_remainder() {
        let world = MockRandomMgrWorld::new();
        world.set_event_rows(vec![mk_row(1, "add", 1, 100, 60)]);
        let mut cache = EventCache::new();
        let _ = cache.get_value(1, "add", 130, &world);
        // valid = 60, elapsed = 30 → remaining = 30.
        assert_eq!(cache.get_value_valid_time(1, "add", 130), 30);
    }
}
