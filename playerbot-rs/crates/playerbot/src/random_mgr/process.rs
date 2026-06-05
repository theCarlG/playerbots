//! `ProcessBot` port — Rust implementation of the event-driven
//! per-bot scheduler that fires `randomize`, `change_strategy`, and
//! `teleport` actions once per tick per bot.
//!
//! Split from the C++ `RandomPlayerbotMgr::ProcessBot(uint32 bot)`
//! and `ProcessBot(Player* player)` overloads:
//!
//! * The first overload walked the manager's `players` map, figured
//!   out whether the bot was in limbo, and logged out / logged in
//!   bots that had tipped over their `add` / `login` timers. That
//!   logic is split across:
//!   - [`step_bot_lifecycle`] — the "do we still want this bot in
//!     world?" check (and the logout handling).
//!   - The login-in half is delegated to Phase E's login worker
//!     (not touched here).
//! * The second overload ran the idle-bot decision: randomize, then
//!   `change_strategy`, then teleport. That maps to [`step_bot_actions`].
//!
//! The Rust version operates on `&mut EventCache` rather than the
//! whole `RandomMgrState` so callers can share the cache between the
//! worker tick and the scheduler helpers without borrow conflicts.

use cmangos::RandomMgrWorld;

use super::events::EventCache;
use super::scheduler::{
    self, ScheduleBounds,
};

/// Decisions a single `ProcessBot` pass can emit. The worker tick
/// folds these into counters for the log line.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ProcessOutcome {
    /// Nothing to do this tick — bot is between scheduled actions.
    Idle,
    /// The `randomize` event fired — `dispatch_randomize` was
    /// invoked and the event rescheduled.
    Randomized,
    /// The `change_strategy` event fired.
    ChangedStrategy,
    /// The `teleport` event fired.
    Teleported,
    /// The bot tipped over its `add` timer and was handed a `logout`
    /// event.
    LoggedOut,
    /// The bot was skipped — typically "still in limbo, wait a tick".
    Skipped,
}

/// Flags the worker tick passes down so we don't have to drag the
/// full config through a long call chain. All fields map 1:1 to
/// `sPlayerbotAIConfig` bool / numeric fields.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ProcessContext {
    /// `sPlayerbotAIConfig.enableRandomTeleports` — when false, the
    /// `change_strategy`/teleport branches log but don't dispatch.
    pub enable_random_teleports: bool,
    /// `sPlayerbotAIConfig.RandomBotRpgChance` — fraction of random teleports
    /// that go to an RPG/inn location (towns and capitals) rather than a grind
    /// spot. Keeps cities populated; the rest go to level-appropriate mobs.
    pub rpg_chance: f32,
    /// `sPlayerbotAIConfig.disableRandomLevels` — when true, the
    /// randomize branch short-circuits.
    pub disable_random_levels: bool,
    /// `true` iff at least one real player is online. The C++ gates
    /// the teleport branch on `players.size() > 0`.
    pub has_real_players: bool,
    /// `!randomBotLoginWithPlayer || has_real_players`. When false,
    /// every bot is force-logged-out regardless of its `add` TTL. The
    /// C++ calls this `botsAllowedInWorld`.
    pub bots_allowed_in_world: bool,
    /// `true` iff `randomBotTimedLogout` is active. When false, the
    /// `add` event never fires the logout path.
    pub timed_logout: bool,
    /// `true` iff `randomBotTimedOffline` is active. Drives the
    /// `logout` event write during the logout path.
    pub timed_offline: bool,
    /// `true` iff `asyncBotLogin` is active. When true, the `add`
    /// expiry check is skipped — the login queue owns lifecycle.
    pub async_bot_login: bool,
    /// Scheduling bounds for the randomize/teleport/change-strategy
    /// events.
    pub bounds: ScheduleBounds,
    /// Bounds `(min, max)` for the `add` event TTL used when writing
    /// "log out in X seconds" on the logout path.
    pub in_world_time_bounds: (u32, u32),
}

impl Default for ProcessContext {
    fn default() -> Self {
        Self {
            enable_random_teleports: true,
            rpg_chance: 0.35,
            disable_random_levels: false,
            has_real_players: true,
            bots_allowed_in_world: true,
            timed_logout: true,
            timed_offline: false,
            async_bot_login: false,
            bounds: ScheduleBounds::default(),
            in_world_time_bounds: (2 * 3600, 14 * 3600),
        }
    }
}

/// First half of the C++ `ProcessBot(uint32 bot)` overload: the
/// "is this bot still valid in-world?" decision.
///
/// Returns:
/// * `Some(ProcessOutcome::LoggedOut)` — bot should log out; the
///   world has been told via `dispatch_logout`, and the `add` event
///   has been cleared.
/// * `Some(ProcessOutcome::Skipped)` — bot is in limbo (teleporting,
///   logging out, has no in-world representation, etc).
/// * `None` — the bot is alive and well; proceed to
///   [`step_bot_actions`].
pub fn step_bot_lifecycle(
    events: &mut EventCache,
    world: &dyn RandomMgrWorld,
    bot: u32,
    ctx: ProcessContext,
    now_epoch_s: u32,
) -> Option<ProcessOutcome> {
    if !world.has_player_bot(bot) {
        return Some(ProcessOutcome::Skipped);
    }

    // Two separate reasons to force a logout, matching the C++
    // `isValid` chain at RandomPlayerbotMgr.cpp:1891+:
    //
    //   isValid = !(timedLogout && !asyncBotLogin && !GetEventValue(bot, "add"))
    //             && botsAllowedInWorld
    //
    // Either branch triggers the same cleanup (dispatch_logout + clear
    // `add` + optional offline cooldown).
    let add_expired = ctx.timed_logout
        && !ctx.async_bot_login
        && events.get_value(bot, "add", now_epoch_s, world) == 0;
    let force_logout = !ctx.bots_allowed_in_world;

    if add_expired || force_logout {
        world.dispatch_logout(bot);
        // Clear the add event so we don't try logging out again next
        // tick — matches `SetEventValue(bot, "add", 0, 0)` in C++.
        events.set_value(bot, "add", 0, 0, "", now_epoch_s, world);

        if ctx.timed_offline {
            // Schedule an "offline period" before the bot can be
            // logged back in.
            let (min_s, max_s) = ctx.in_world_time_bounds;
            let ttl = world.urand_range(min_s, max_s);
            events.set_value(bot, "logout", 1, ttl, "", now_epoch_s, world);
        }
        return Some(ProcessOutcome::LoggedOut);
    }

    None
}

/// Second half of `ProcessBot(Player*)`: randomize → `change_strategy` →
/// teleport. Returns the first action that fired, or
/// [`ProcessOutcome::Idle`] if none of the event windows were due.
pub fn step_bot_actions(
    events: &mut EventCache,
    world: &dyn RandomMgrWorld,
    bot: u32,
    ctx: ProcessContext,
    now_epoch_s: u32,
) -> ProcessOutcome {
    if !ctx.disable_random_levels
        && events.get_value(bot, "randomize", now_epoch_s, world) == 0
    {
        world.dispatch_randomize(bot);
        scheduler::schedule_randomize(events, world, bot, ctx.bounds, None, now_epoch_s);
        return ProcessOutcome::Randomized;
    }

    if events.get_value(bot, "change_strategy", now_epoch_s, world) == 0 {
        if ctx.enable_random_teleports {
            world.dispatch_change_strategy(bot);
            scheduler::schedule_change_strategy(
                events, world, bot, ctx.bounds, None, now_epoch_s,
            );
        }
        return ProcessOutcome::ChangedStrategy;
    }

    if ctx.has_real_players && events.get_value(bot, "teleport", now_epoch_s, world) == 0 {
        if ctx.enable_random_teleports {
            // Mirror PB2 `Refresh`: `rpg_chance` of teleports go to an RPG/inn
            // location (towns and capitals), the rest to a level-appropriate
            // grind spot. The RPG branch is what keeps cities populated — it
            // was dropped in the Phase H port (only ForLevel was kept), which
            // left bots forever sent to grind spots and capitals empty.
            let threshold = (100.0 * ctx.rpg_chance) as u32;
            if world.urand_range(0, 100) <= threshold {
                world.dispatch_random_teleport_for_rpg(bot, true);
            } else {
                world.dispatch_random_teleport_for_level(bot, true);
            }
            scheduler::schedule_teleport(events, world, bot, ctx.bounds, None, now_epoch_s);
        }
        return ProcessOutcome::Teleported;
    }

    ProcessOutcome::Idle
}

/// Combined `step_bot_lifecycle` + `step_bot_actions`. Matches the
/// C++ `bool ProcessBot(uint32 bot)` return value: `true` if any
/// action fired (the worker tick uses this to decrement the
/// `updateBots` budget).
pub fn process_bot(
    events: &mut EventCache,
    world: &dyn RandomMgrWorld,
    bot: u32,
    ctx: ProcessContext,
    now_epoch_s: u32,
) -> ProcessOutcome {
    if let Some(outcome) = step_bot_lifecycle(events, world, bot, ctx, now_epoch_s) {
        return outcome;
    }
    step_bot_actions(events, world, bot, ctx, now_epoch_s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::{MockRandomMgrEvent, MockRandomMgrWorld};

    fn ctx() -> ProcessContext {
        ProcessContext {
            enable_random_teleports: true,
            rpg_chance: 0.35,
            disable_random_levels: false,
            has_real_players: true,
            bots_allowed_in_world: true,
            timed_logout: true,
            timed_offline: true,
            async_bot_login: false,
            bounds: ScheduleBounds {
                teleport_min_s: 100,
                teleport_max_s: 200,
                change_strategy_min_s: 30,
                change_strategy_max_s: 60,
                randomize_min_s: 500,
                randomize_max_s: 1000,
            },
            in_world_time_bounds: (3600, 7200),
        }
    }

    #[test]
    fn missing_bot_is_skipped() {
        let world = MockRandomMgrWorld::new();
        let mut events = EventCache::new();
        let outcome = process_bot(&mut events, &world, 42, ctx(), 100);
        assert_eq!(outcome, ProcessOutcome::Skipped);
    }

    #[test]
    fn expired_add_triggers_logout_flow() {
        let world = MockRandomMgrWorld::new();
        world.set_owned_guids(vec![42]);
        let mut events = EventCache::new();
        // No `add` event at all → TTL check returns 0 → logout path.
        let outcome = process_bot(&mut events, &world, 42, ctx(), 100);
        assert_eq!(outcome, ProcessOutcome::LoggedOut);

        let ev = world.events();
        assert!(
            ev.iter()
                .any(|e| matches!(e, MockRandomMgrEvent::Logout(42))),
            "expected Logout(42)"
        );
        // The `logout` event should now be set so the bot doesn't get
        // re-picked next tick.
        assert!(events.get_value(42, "logout", 100, &world) > 0);
    }

    #[test]
    fn randomize_fires_when_event_expired() {
        let world = MockRandomMgrWorld::new();
        world.set_owned_guids(vec![7]);
        let mut events = EventCache::new();
        // Give the bot a valid `add` event so lifecycle passes.
        events.set_value(7, "add", 1, 10_000, "", 0, &world);

        let outcome = process_bot(&mut events, &world, 7, ctx(), 100);
        assert_eq!(outcome, ProcessOutcome::Randomized);

        // The `randomize` event should now be scheduled.
        assert_eq!(events.get_value(7, "randomize", 200, &world), 1);

        let ev = world.events();
        assert!(
            ev.iter()
                .any(|e| matches!(e, MockRandomMgrEvent::Randomize(7))),
            "expected Randomize(7)"
        );
    }

    #[test]
    fn change_strategy_fires_after_randomize_is_scheduled() {
        let world = MockRandomMgrWorld::new();
        world.set_owned_guids(vec![5]);
        let mut events = EventCache::new();
        // Populate the cache so lifecycle passes and randomize is "set".
        events.set_value(5, "add", 1, 10_000, "", 0, &world);
        events.set_value(5, "randomize", 1, 10_000, "", 0, &world);

        let outcome = process_bot(&mut events, &world, 5, ctx(), 100);
        assert_eq!(outcome, ProcessOutcome::ChangedStrategy);
    }

    #[test]
    fn teleport_fires_after_all_earlier_events_are_scheduled() {
        let world = MockRandomMgrWorld::new();
        world.set_owned_guids(vec![3]);
        let mut events = EventCache::new();
        events.set_value(3, "add", 1, 10_000, "", 0, &world);
        events.set_value(3, "randomize", 1, 10_000, "", 0, &world);
        events.set_value(3, "change_strategy", 1, 10_000, "", 0, &world);

        let outcome = process_bot(&mut events, &world, 3, ctx(), 100);
        assert_eq!(outcome, ProcessOutcome::Teleported);
    }

    #[test]
    fn teleport_routes_to_rpg_when_rpg_chance_high() {
        // The regression: the RPG/city teleport branch was dropped in the port,
        // so bots only ever went to grind spots and capitals emptied out.
        let world = MockRandomMgrWorld::new();
        world.set_owned_guids(vec![3]);
        let mut events = EventCache::new();
        events.set_value(3, "add", 1, 10_000, "", 0, &world);
        events.set_value(3, "randomize", 1, 10_000, "", 0, &world);
        events.set_value(3, "change_strategy", 1, 10_000, "", 0, &world);

        let mut c = ctx();
        c.rpg_chance = 1.0; // always pick the RPG/inn location
        let outcome = process_bot(&mut events, &world, 3, c, 100);
        assert_eq!(outcome, ProcessOutcome::Teleported);
        assert!(
            world
                .events()
                .iter()
                .any(|e| matches!(e, MockRandomMgrEvent::RandomTeleportForRpg(3, _))),
            "rpg_chance=1.0 must route the teleport to an RPG/city location"
        );
    }

    #[test]
    fn teleport_skipped_when_no_real_players() {
        let world = MockRandomMgrWorld::new();
        world.set_owned_guids(vec![3]);
        let mut events = EventCache::new();
        events.set_value(3, "add", 1, 10_000, "", 0, &world);
        events.set_value(3, "randomize", 1, 10_000, "", 0, &world);
        events.set_value(3, "change_strategy", 1, 10_000, "", 0, &world);

        let mut c = ctx();
        c.has_real_players = false;
        let outcome = process_bot(&mut events, &world, 3, c, 100);
        assert_eq!(outcome, ProcessOutcome::Idle);
    }

    #[test]
    fn bots_not_allowed_in_world_forces_logout_even_with_fresh_add() {
        let world = MockRandomMgrWorld::new();
        world.set_owned_guids(vec![9]);
        let mut events = EventCache::new();
        // Fresh `add` event → without the force-logout path we'd go on
        // to the action chain. With `bots_allowed_in_world=false` we
        // must log out regardless.
        events.set_value(9, "add", 1, 10_000, "", 0, &world);

        let mut c = ctx();
        c.bots_allowed_in_world = false;
        let outcome = process_bot(&mut events, &world, 9, c, 100);
        assert_eq!(outcome, ProcessOutcome::LoggedOut);

        let ev = world.events();
        assert!(
            ev.iter()
                .any(|e| matches!(e, MockRandomMgrEvent::Logout(9))),
            "expected Logout(9)"
        );
    }

    #[test]
    fn disable_random_levels_skips_randomize() {
        let world = MockRandomMgrWorld::new();
        world.set_owned_guids(vec![1]);
        let mut events = EventCache::new();
        events.set_value(1, "add", 1, 10_000, "", 0, &world);

        let mut c = ctx();
        c.disable_random_levels = true;
        let outcome = process_bot(&mut events, &world, 1, c, 100);
        // With randomize disabled, change_strategy fires instead.
        assert_eq!(outcome, ProcessOutcome::ChangedStrategy);
    }
}
