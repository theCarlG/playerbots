//! Per-bot scheduling helpers — the Rust equivalents of the C++
//! `ScheduleRandomize` / `ScheduleTeleport` / `ScheduleChangeStrategy`
//! private methods on `RandomPlayerbotMgr`.
//!
//! These helpers all follow the same pattern: pick a duration (either
//! an explicit override from the caller or a random value between
//! configured min/max bounds) and write a KV-store event that the
//! next `ProcessBot` pass will read back. The `valid_in` field on the
//! event row encodes the TTL — once it expires, `GetEventValue`
//! returns 0 and the scheduler re-fires.

use cmangos::RandomMgrWorld;

use super::events::EventCache;

/// Configuration slice pulled from `BotConfig` for scheduler-driven
/// events. Having a dedicated struct keeps the scheduler unit-testable
/// without dragging the full config into the call site.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ScheduleBounds {
    pub teleport_min_s: u32,
    pub teleport_max_s: u32,
    pub change_strategy_min_s: u32,
    pub change_strategy_max_s: u32,
    pub randomize_min_s: u32,
    pub randomize_max_s: u32,
}

impl Default for ScheduleBounds {
    fn default() -> Self {
        // Defaults mirror the legacy playerbot config values.
        Self {
            teleport_min_s: 600,
            teleport_max_s: 1200,
            change_strategy_min_s: 300,
            change_strategy_max_s: 600,
            randomize_min_s: 600,
            randomize_max_s: 3600,
        }
    }
}

/// Set the `randomize` event for `bot`. If `override_ttl_s` is `Some`,
/// uses that TTL directly; otherwise picks a random value in
/// `[randomize_min_s, randomize_max_s]`.
pub fn schedule_randomize(
    events: &mut EventCache,
    world: &dyn RandomMgrWorld,
    bot: u32,
    bounds: ScheduleBounds,
    override_ttl_s: Option<u32>,
    now_epoch_s: u32,
) {
    let ttl = override_ttl_s
        .unwrap_or_else(|| world.urand_range(bounds.randomize_min_s, bounds.randomize_max_s));
    events.set_value(bot, "randomize", 1, ttl, "", now_epoch_s, world);
}

/// Set the `teleport` event for `bot`. Matches the C++ body:
/// ```cpp
/// if (!time)
///     time = 60 + urand(teleMin, teleMax);
/// SetEventValue(bot, "teleport", 1, time);
/// ```
pub fn schedule_teleport(
    events: &mut EventCache,
    world: &dyn RandomMgrWorld,
    bot: u32,
    bounds: ScheduleBounds,
    override_ttl_s: Option<u32>,
    now_epoch_s: u32,
) {
    let ttl = override_ttl_s
        .unwrap_or_else(|| 60 + world.urand_range(bounds.teleport_min_s, bounds.teleport_max_s));
    events.set_value(bot, "teleport", 1, ttl, "", now_epoch_s, world);
}

/// Set the `change_strategy` event for `bot`. Mirrors the C++
/// `ScheduleChangeStrategy` helper verbatim.
pub fn schedule_change_strategy(
    events: &mut EventCache,
    world: &dyn RandomMgrWorld,
    bot: u32,
    bounds: ScheduleBounds,
    override_ttl_s: Option<u32>,
    now_epoch_s: u32,
) {
    let ttl = override_ttl_s.unwrap_or_else(|| {
        world.urand_range(
            bounds.change_strategy_min_s,
            bounds.change_strategy_max_s,
        )
    });
    events.set_value(bot, "change_strategy", 1, ttl, "", now_epoch_s, world);
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::MockRandomMgrWorld;

    fn bounds() -> ScheduleBounds {
        ScheduleBounds {
            teleport_min_s: 100,
            teleport_max_s: 200,
            change_strategy_min_s: 30,
            change_strategy_max_s: 60,
            randomize_min_s: 10,
            randomize_max_s: 20,
        }
    }

    #[test]
    fn schedule_randomize_writes_event_with_random_ttl() {
        let world = MockRandomMgrWorld::new();
        let mut events = EventCache::new();
        schedule_randomize(&mut events, &world, 7, bounds(), None, 100);

        // The mock's `urand_range` starts at min and steps up — first
        // call returns exactly `randomize_min_s`.
        let valid = events.get_value_valid_time(7, "randomize", 100);
        assert_eq!(valid, 10);
    }

    #[test]
    fn schedule_teleport_prepends_60s_buffer() {
        let world = MockRandomMgrWorld::new();
        let mut events = EventCache::new();
        schedule_teleport(&mut events, &world, 1, bounds(), None, 100);

        // `60 + randomish(100, 200)`; first mock urand returns 100 so
        // TTL should be 160.
        let valid = events.get_value_valid_time(1, "teleport", 100);
        assert_eq!(valid, 160);
    }

    #[test]
    fn override_ttl_bypasses_random_bounds() {
        let world = MockRandomMgrWorld::new();
        let mut events = EventCache::new();
        schedule_change_strategy(&mut events, &world, 2, bounds(), Some(42), 100);
        let valid = events.get_value_valid_time(2, "change_strategy", 100);
        assert_eq!(valid, 42);
    }

    #[test]
    fn scheduled_event_is_readable_by_event_cache() {
        let world = MockRandomMgrWorld::new();
        let mut events = EventCache::new();
        schedule_randomize(&mut events, &world, 9, bounds(), Some(100), 1000);
        assert_eq!(events.get_value(9, "randomize", 1050, &world), 1);
        // Past TTL → 0
        assert_eq!(events.get_value(9, "randomize", 1200, &world), 0);
    }
}
