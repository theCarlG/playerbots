//! BG / LFG queue accounting — ports `CheckBgQueue` and `CheckLfgQueue`.
//!
//! The C++ originals at `RandomPlayerbotMgr.cpp:1132` and `1478` both:
//!
//! 1. Skip early if less than 30 seconds have elapsed since the last
//!    pass (`BgCheckTimer + 30 > now`).
//! 2. Clear the current bucket matrices.
//! 3. Walk the real-player list + the bot roster, bumping the matching
//!    cell for each queued entry and flipping `need_bots` / pushing
//!    dungeon ids where appropriate.
//!
//! The Rust versions defer the walk-and-classify step to the world
//! trait. `world.query_bg_queue()` yields a pre-flattened
//! [`BgQueueEntry`] per queued player/bot, and `world.query_lfg_queue()`
//! yields one [`LfgQueueEntry`] per dungeon a player is queued for.
//! We just rebuild `state.buckets` from those rows.

use cmangos::{BgQueueEntry, LfgQueueEntry, RandomMgrWorld};

use super::buckets::{ArenaKey, BgKey};
use super::state::RandomMgrState;

/// Minimum seconds between successive [`check_bg_queue`] passes.
/// Mirrors the literal `30` in the C++ `CheckBgQueue` prologue.
pub const BG_CHECK_INTERVAL_S: u32 = 30;

/// Minimum seconds between successive [`check_lfg_queue`] passes.
/// Matches the same `30`-second cadence on the LFG side.
pub const LFG_CHECK_INTERVAL_S: u32 = 30;

/// Run one `CheckBgQueue` pass if the cadence allows. Returns `true`
/// if the bucket matrices were rebuilt this tick.
///
/// The rebuild itself is idempotent — if the cadence hits twice at the
/// same `now_epoch_s`, the second call rewrites the same counters.
pub fn check_bg_queue(
    state: &mut RandomMgrState,
    world: &dyn RandomMgrWorld,
    now_epoch_s: u32,
) -> bool {
    // The C++ seeds the timer to `now` on the first tick and returns
    // without doing any work. We replicate that so the first 30-second
    // window stays empty instead of immediately rebuilding.
    if state.timers.bg_check == 0 {
        state.timers.bg_check = now_epoch_s;
        return false;
    }
    if now_epoch_s < state.timers.bg_check + BG_CHECK_INTERVAL_S {
        return false;
    }
    state.timers.bg_check = now_epoch_s;

    // Clear everything — the C++ does this before walking the players
    // map so stale cells from the previous pass don't leak in.
    state.buckets.reset_bg_counters();

    let rows = world.query_bg_queue();
    for row in rows {
        apply_bg_row(state, &row);
    }

    true
}

/// Run one `CheckLfgQueue` pass if the cadence allows.
pub fn check_lfg_queue(
    state: &mut RandomMgrState,
    world: &dyn RandomMgrWorld,
    now_epoch_s: u32,
) -> bool {
    if state.timers.lfg_check == 0 {
        state.timers.lfg_check = now_epoch_s;
        return false;
    }
    if now_epoch_s < state.timers.lfg_check + LFG_CHECK_INTERVAL_S {
        return false;
    }
    state.timers.lfg_check = now_epoch_s;

    state.buckets.clear_lfg_dungeons();
    let rows = world.query_lfg_queue();
    for row in rows {
        apply_lfg_row(state, &row);
    }

    true
}

/// Fold a single [`BgQueueEntry`] into the bucket matrices. Exposed
/// separately so tests can drive the classifier without stubbing a
/// full queue query.
pub fn apply_bg_row(state: &mut RandomMgrState, row: &BgQueueEntry) {
    let key = BgKey::new(row.queue_type, row.bracket_id, row.team_id);
    if row.is_bot {
        state.buckets.inc_bg_bots(key);
    } else {
        state.buckets.inc_bg_players(key);
    }

    // Arena rows also bump the arena counter. The C++ keyed this on
    // `(queueType, bracket, team, rated-or-not)`; we use the incoming
    // arena_type field directly (2/3/5) so the caller can distinguish
    // 2v2 / 3v3 / 5v5 without an extra lookup.
    if row.arena_type != 0 {
        let ak = ArenaKey::new(
            row.queue_type,
            row.bracket_id,
            row.team_id,
            u32::from(row.arena_type),
        );
        state.buckets.inc_arena_bots(ak);
        if row.arena_rating > 0 {
            state.buckets.rating.insert(key, row.arena_rating);
        }
    }

    // The C++ only flips `need_bots` for rows where the player is
    // still waiting (not yet invited, not yet in-world). Our trait
    // flattens that into the row list — every real-player row is
    // treated as "needs more bots on its side". Bot rows never flip
    // the flag; they're the filler.
    if !row.is_bot {
        if row.arena_type != 0 {
            state.buckets.set_need_bots(key, true);
        } else {
            state.buckets.set_need_bots(
                BgKey::new(row.queue_type, row.bracket_id, 0),
                true,
            );
            state.buckets.set_need_bots(
                BgKey::new(row.queue_type, row.bracket_id, 1),
                true,
            );
        }
    }
}

/// Fold a single [`LfgQueueEntry`] into the LFG dungeon lists.
pub fn apply_lfg_row(state: &mut RandomMgrState, row: &LfgQueueEntry) {
    // `team_id = 2` means "both factions" — push the dungeon id to
    // both buckets. Otherwise push only to the matching team.
    match row.team_id {
        2 => {
            state.buckets.push_lfg_dungeon(0, row.dungeon_id);
            state.buckets.push_lfg_dungeon(1, row.dungeon_id);
        }
        team => {
            state.buckets.push_lfg_dungeon(u32::from(team), row.dungeon_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::MockRandomMgrWorld;

    fn mk_world() -> MockRandomMgrWorld {
        MockRandomMgrWorld::new()
    }

    #[test]
    fn first_pass_seeds_timer_and_does_no_work() {
        let mut state = RandomMgrState::new();
        let world = mk_world();
        let rebuilt = check_bg_queue(&mut state, &world, 1000);
        assert!(!rebuilt);
        assert_eq!(state.timers.bg_check, 1000);
    }

    #[test]
    fn second_pass_within_cadence_is_skipped() {
        let mut state = RandomMgrState::new();
        let world = mk_world();
        let _ = check_bg_queue(&mut state, &world, 1000);
        let rebuilt = check_bg_queue(&mut state, &world, 1020);
        assert!(!rebuilt);
    }

    #[test]
    fn second_pass_after_cadence_rebuilds_matrix() {
        let mut state = RandomMgrState::new();
        let world = mk_world();
        world.set_bg_queue(vec![
            BgQueueEntry {
                queue_type: 1,
                bracket_id: 2,
                team_id: 0,
                is_bot: false,
                level: 40,
                map_id: 30,
                arena_rating: 0,
                arena_type: 0,
            },
            BgQueueEntry {
                queue_type: 1,
                bracket_id: 2,
                team_id: 1,
                is_bot: true,
                level: 40,
                map_id: 30,
                arena_rating: 0,
                arena_type: 0,
            },
        ]);

        let _ = check_bg_queue(&mut state, &world, 1000);
        let rebuilt = check_bg_queue(&mut state, &world, 1040);
        assert!(rebuilt);

        assert_eq!(state.buckets.bg_players(BgKey::new(1, 2, 0)), 1);
        assert_eq!(state.buckets.bg_bots(BgKey::new(1, 2, 1)), 1);
        // Player row flipped need_bots for both teams.
        assert!(state.buckets.need_bots(BgKey::new(1, 2, 0)));
        assert!(state.buckets.need_bots(BgKey::new(1, 2, 1)));
    }

    #[test]
    fn arena_row_populates_arena_bucket_and_rating() {
        let mut state = RandomMgrState::new();
        let world = mk_world();
        world.set_bg_queue(vec![BgQueueEntry {
            queue_type: 6,
            bracket_id: 3,
            team_id: 0,
            is_bot: false,
            level: 70,
            map_id: 559,
            arena_rating: 1550,
            arena_type: 2,
        }]);
        let _ = check_bg_queue(&mut state, &world, 1000);
        let _ = check_bg_queue(&mut state, &world, 1040);

        let ak = ArenaKey::new(6, 3, 0, 2);
        assert_eq!(state.buckets.arena_bots(ak), 1);
        let bk = BgKey::new(6, 3, 0);
        assert_eq!(state.buckets.rating.get(&bk).copied(), Some(1550));
    }

    #[test]
    fn rebuild_clears_previous_counters() {
        let mut state = RandomMgrState::new();
        state.buckets.inc_bg_players(BgKey::new(9, 9, 0));
        let world = mk_world();
        // Empty queue — rebuild should zero the stale cell.
        let _ = check_bg_queue(&mut state, &world, 1000);
        let _ = check_bg_queue(&mut state, &world, 1040);
        assert_eq!(state.buckets.bg_players(BgKey::new(9, 9, 0)), 0);
    }

    #[test]
    fn lfg_first_pass_seeds_timer() {
        let mut state = RandomMgrState::new();
        let world = mk_world();
        let rebuilt = check_lfg_queue(&mut state, &world, 500);
        assert!(!rebuilt);
        assert_eq!(state.timers.lfg_check, 500);
    }

    #[test]
    fn lfg_rebuild_populates_dungeon_lists() {
        let mut state = RandomMgrState::new();
        let world = mk_world();
        world.set_lfg_queue(vec![
            LfgQueueEntry {
                team_id: 0,
                dungeon_id: 100,
                role_mask: 0,
            },
            LfgQueueEntry {
                team_id: 1,
                dungeon_id: 200,
                role_mask: 0,
            },
            LfgQueueEntry {
                team_id: 2,
                dungeon_id: 300,
                role_mask: 0,
            },
        ]);
        let _ = check_lfg_queue(&mut state, &world, 500);
        let _ = check_lfg_queue(&mut state, &world, 540);

        assert_eq!(state.buckets.lfg_dungeons_for_team(0), &[100, 300]);
        assert_eq!(state.buckets.lfg_dungeons_for_team(1), &[200, 300]);
    }

    #[test]
    fn lfg_rebuild_clears_previous_lists() {
        let mut state = RandomMgrState::new();
        state.buckets.push_lfg_dungeon(0, 999);
        let world = mk_world();
        let _ = check_lfg_queue(&mut state, &world, 500);
        let _ = check_lfg_queue(&mut state, &world, 540);
        assert!(state.buckets.lfg_dungeons_for_team(0).is_empty());
    }
}
