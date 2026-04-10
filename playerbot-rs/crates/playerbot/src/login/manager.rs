//! `LoginManager` — ports `PlayerBotLoginMgr::Update/FillLoginLogoutQueue/
//! LoginLogoutBots/SendHolders/UpdateOnlineBots` into pure Rust.
//!
//! The manager owns the bot pool, performs a full tick against a
//! provided `LoginWorld`, and emits a queue of `QueuedCommand`s that
//! the main thread drains. It is driven either synchronously (the
//! tests do this) or asynchronously from the background login worker
//! thread (see `super::worker`).

use crate::config::BotConfig;
use crate::login::criteria::{
    LoginCriterionFailType, RpbmLookup, criteria_for_attempt, criteria_still_valid,
    login_criteria_size, match_no_criteria,
};
use crate::login::space::{LoginSpace, RealPlayerSummary};
use crate::login::state::{FillStep, LevelEnv, LoginState, PlayerLoginInfo};
use cmangos::{BotCandidateInfo, HolderState, LoginWorld};
use std::collections::{BTreeMap, HashMap};

/// Command emitted by the worker and executed on the main thread.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum QueuedCommand {
    Login(u32),
    Logout(u32),
}

/// Per-tick output — emitted back to the main thread via mpsc.
#[derive(Clone, Debug, Default)]
pub struct TickOutput {
    pub commands: Vec<QueuedCommand>,
    /// Number of logins actually accepted this tick (used by the
    /// debug log line). Distinct from `commands.len()` because the
    /// queue also contains logouts.
    pub logins: u32,
    pub logouts: u32,
}

/// Top-level login/logout orchestrator.
///
/// Constructed once per worker (tests reuse a single manager across
/// multiple `tick()` calls to mirror the long-lived C++ singleton).
pub struct LoginManager {
    /// All candidate bots keyed by low guid. Mirrors `botPool`.
    pool: BTreeMap<u32, PlayerLoginInfo>,
    /// Set to true after the first `load_pool` — mirrors the C++ guard
    /// that skips the main loop until the pool is populated.
    pool_loaded: bool,
    /// Debug logging flag — mirrors `PlayerBotLoginMgr::debug`.
    pub debug: bool,
}

impl Default for LoginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            pool: BTreeMap::new(),
            pool_loaded: false,
            debug: false,
        }
    }

    /// Read-only pool accessor — tests use this to inspect state.
    #[must_use]
    pub fn pool(&self) -> &BTreeMap<u32, PlayerLoginInfo> {
        &self.pool
    }

    /// Mutable pool accessor — tests (and the worker thread) use this
    /// to seed state without re-loading from the DB.
    pub fn pool_mut(&mut self) -> &mut BTreeMap<u32, PlayerLoginInfo> {
        &mut self.pool
    }

    /// Toggle the debug flag. Exposed as a standalone method to
    /// mirror `PlayerBotLoginMgr::ToggleDebug`.
    pub fn toggle_debug(&mut self) {
        self.debug = !self.debug;
    }

    /// Populate the pool from a freshly-queried candidate row set.
    /// Mirrors `PlayerBotLoginMgr::LoadBotsFromDb` + the
    /// `instant_randomize` correction applied at each row.
    pub fn load_pool(&mut self, rows: &[BotCandidateInfo], cfg: &BotConfig) {
        self.pool.clear();
        for row in rows {
            let mut info = PlayerLoginInfo::from_candidate(row);
            // C++ `isNew` computation:
            //   instant_randomize ? totaltime == 0 : level == 1
            info.is_new = if cfg.instant_randomize {
                row.total_played_time == 0
            } else {
                row.level == 1
            };
            self.pool.insert(row.guid, info);
        }
        self.pool_loaded = true;
    }

    /// True once `load_pool` has been called.
    #[must_use]
    pub const fn pool_loaded(&self) -> bool {
        self.pool_loaded
    }

    /// One full tick of the manager against a live `LoginWorld`.
    ///
    /// The C++ `PlayerBotLoginMgr::Update` does:
    /// 1. `UpdateOnlineBots()` — snapshot online bots' current state.
    ///    We skip this because the Rust port doesn't cache
    ///    `Player*` pointers; the `LoadBotsFromDb` pass already
    ///    captures `is_online` via the `characters.online` column.
    /// 2. If pool empty, kick off `LoadBotsFromDb` via a future and
    ///    return. We model this by the `pool_loaded` flag — the
    ///    worker is responsible for calling `load_pool` before the
    ///    first `tick`.
    /// 3. `FillLoginLogoutQueue` → returns a vector of candidate
    ///    pointers; iterate over attempts evaluating criteria.
    /// 4. `LoginLogoutBots(queue)` → dispatch `LoginBot`/`LogoutBot`.
    ///    We defer actual dispatch to the main thread by returning a
    ///    `TickOutput { commands }` vector.
    pub fn tick(
        &mut self,
        cfg: &BotConfig,
        world: &dyn LoginWorld,
        real_players: &[RealPlayerSummary],
    ) -> TickOutput {
        if !self.pool_loaded {
            return TickOutput::default();
        }

        let env = LevelEnv {
            disable_random_levels: cfg.disable_random_levels,
            random_bot_min_level: cfg.random_bot_min_level,
            sync_level_max_above: cfg.sync_level_max_above,
            players_level: world.players_level(),
            world_max_level: world.world_max_level(),
        };

        let max_online = world.max_online_bot_count();
        let mut space = LoginSpace::empty();
        space.populate_from_config(cfg, &env, max_online);
        space.real_players = real_players.to_vec();

        // Build initial space by debiting every "next-step" online
        // candidate. Mirrors `FillLoginSpace(pool, space, NEXT_STEP)`
        // + the inline `currentSpace--` loop.
        for info in self.pool.values() {
            space.fill_login_space(info, &env, FillStep::NextStep);
            if matches!(
                info.login_state,
                LoginState::Online | LoginState::OnLogoutQueue
            ) {
                space.current_space -= 1;
            }
        }

        if self.debug {
            world.log_debug(&format!(
                "PlayerbotLoginMgr: Initial space {}",
                space.total_space
            ));
        }

        let lookup = RpbmLookupWorld { world };
        let mut login_fails: HashMap<u32, LoginCriterionFailType> = HashMap::new();
        let mut potential_queue: Vec<u32> = Vec::new();

        // Attempt loop — exactly matches the C++ `for (attempt = 0;
        // attempt <= GetLoginCriteriaSize(); attempt++)` that runs
        // one extra iteration for the "default only" baseline.
        let attempts = login_criteria_size(cfg) as u8;
        for attempt in 0..=attempts {
            let criteria = criteria_for_attempt(cfg, attempt);

            // Walk every candidate and evaluate the criteria for this
            // attempt. Uses a stable key order so the queue ordering
            // is deterministic across runs.
            let guids: Vec<u32> = self.pool.keys().copied().collect();
            for guid in guids {
                let prev_fail = login_fails.get(&guid).copied().unwrap_or_default();
                if criteria_still_valid(prev_fail, &criteria) {
                    continue;
                }

                // Temporarily pretend the bot isn't logged in — the C++
                // version calls `EmptyLoginSpace(NOW) → MatchNoCriteria
                // → FillLoginSpace(NOW)` so the criteria see the
                // hypothetical space.
                let info_snapshot = self.pool[&guid].clone();
                space.empty_login_space(&info_snapshot, &env, FillStep::Now);
                let new_fail = match_no_criteria(&info_snapshot, &space, &criteria, &lookup);
                space.fill_login_space(&info_snapshot, &env, FillStep::Now);
                login_fails.insert(guid, new_fail);

                let is_wanted = new_fail == LoginCriterionFailType::LoginOk;
                if !is_wanted && attempt > 0 {
                    continue;
                }

                // `SetQueue` — now look up the mutable reference and
                // apply the state transition + matching bucket update
                // on the *live* state.
                let info = self
                    .pool
                    .get_mut(&guid)
                    .expect("guid was taken from pool keys");
                apply_set_queue(info, is_wanted, &mut space, &env);

                if info.login_state.is_queued() && !potential_queue.contains(&guid) {
                    potential_queue.push(guid);
                }

                if attempt > 0 && space.total_space <= 0 {
                    break;
                }
            }

            if self.debug {
                let variable = cfg
                    .login_criteria
                    .get(attempt as usize)
                    .cloned()
                    .unwrap_or_default();
                let joined = variable.join(",");
                world.log_debug(&format!(
                    "PlayerbotLoginMgr: Attempt {} ({}), space left {}",
                    attempt, joined, space.total_space
                ));
            }

            if space.total_space <= 0 {
                break;
            }
        }

        // Build the command queue — logins first (capped at
        // `random_bots_max_logins_per_interval`), then logouts capped
        // by the free-room-for-non-spare-bots target.
        let mut out = TickOutput::default();
        let mut logins: u32 = 0;
        for &guid in &potential_queue {
            let info = &self.pool[&guid];
            if info.login_state == LoginState::OnLoginQueue {
                out.commands.push(QueuedCommand::Login(guid));
                logins += 1;
                if logins >= cfg.random_bots_max_logins_per_interval {
                    break;
                }
            }
        }
        out.logins = logins;
        space.current_space -= logins as i32;

        let max_logouts: i32 = if space.current_space < cfg.free_room_for_non_spare_bots as i32 {
            cfg.free_room_for_non_spare_bots as i32 - space.current_space
        } else {
            0
        };

        let mut logouts: u32 = 0;
        if max_logouts > 0 {
            for &guid in &potential_queue {
                let info = &self.pool[&guid];
                if info.login_state == LoginState::OnLogoutQueue {
                    if logouts as i32 >= max_logouts {
                        break;
                    }
                    out.commands.push(QueuedCommand::Logout(guid));
                    logouts += 1;
                }
            }
        }
        out.logouts = logouts;

        if self.debug {
            world.log_debug(&format!(
                "PlayerbotLoginMgr: Queued to log in: {}, out: {}",
                logins, logouts
            ));
        }

        // Holder dispatch — mirrors `SendHolders(queue)` /
        // `SendHolders(pool)`. The async pattern is preserved: every
        // candidate that will be processed this tick goes through
        // `send_holder` to kick off its DB holder. On the next tick
        // the C++ side will have marked the holder as `Received` and
        // the candidate can be actually logged in.
        self.dispatch_holders(cfg, world, &potential_queue);

        // After the queue is built, run `LoginLogoutBots` — in the
        // Rust port that just transitions the candidates and hands
        // the command list back to the caller. The main thread
        // executes `perform_login`/`perform_logout`.
        self.apply_commands(&out.commands);

        out
    }

    /// Port of the C++ `SendHolders` dispatch logic. If
    /// `preload_holders` is true the entire pool is polled; otherwise
    /// only the `potential_queue` is. Respects the
    /// `database_delay_ms` back-off.
    fn dispatch_holders(
        &self,
        cfg: &BotConfig,
        world: &dyn LoginWorld,
        potential_queue: &[u32],
    ) {
        world.database_ping();

        if cfg.preload_holders {
            for (guid, info) in &self.pool {
                if world.database_delay_ms() > 100 {
                    break;
                }
                Self::maybe_send_holder(world, *guid, info);
            }
        } else {
            for &guid in potential_queue {
                if world.database_delay_ms() > 100 {
                    break;
                }
                if let Some(info) = self.pool.get(&guid) {
                    Self::maybe_send_holder(world, guid, info);
                }
            }
        }
    }

    /// Equivalent of `PlayerLoginInfo::SendHolder` gating + dispatch.
    fn maybe_send_holder(world: &dyn LoginWorld, guid: u32, info: &PlayerLoginInfo) {
        let state = world.holder_state(guid);
        match state {
            HolderState::Sent | HolderState::Received => return,
            HolderState::Empty => {}
        }
        // `SendHolder` returned false for ONLINE / OFFLINE /
        // ON_LOGOUTQUEUE in the C++ version — only ON_LOGINQUEUE
        // candidates actually kicked off a holder.
        if info.login_state != LoginState::OnLoginQueue {
            return;
        }
        // Mirrors `sRandomPlayerbotMgr.GetValue(guid, "logout")` line
        // in the original SendHolder; it was purely for a side-effect
        // of extending the KV entry's TTL, so we reproduce the call.
        let _ = world.random_mgr_get_value(guid, "logout");
        let _ = world.send_holder(info.account, guid);
    }

    /// Apply the command queue to the local pool state. The main
    /// thread's `perform_login`/`perform_logout` dispatch still
    /// happens separately; this is the local-state transition that
    /// mirrors `LoginLogoutBots`.
    fn apply_commands(&mut self, commands: &[QueuedCommand]) {
        for cmd in commands {
            match cmd {
                QueuedCommand::Login(guid) => {
                    if let Some(info) = self.pool.get_mut(guid)
                        && info.login_state == LoginState::OnLoginQueue
                    {
                        info.login_state = LoginState::Online;
                    }
                }
                QueuedCommand::Logout(guid) => {
                    if let Some(info) = self.pool.get_mut(guid)
                        && info.login_state == LoginState::OnLogoutQueue
                    {
                        info.login_state = LoginState::Offline;
                    }
                }
            }
        }
    }
}

/// `PlayerLoginInfo::SetQueue` — the state transition happens against
/// the *candidate*, but the space bookkeeping updates the same
/// `LoginSpace` we're evaluating the criteria against.
fn apply_set_queue(
    info: &mut PlayerLoginInfo,
    is_wanted: bool,
    space: &mut LoginSpace,
    env: &LevelEnv,
) {
    if is_wanted {
        match info.login_state {
            LoginState::Offline => {
                info.login_state = LoginState::OnLoginQueue;
                space.fill_login_space(info, env, FillStep::NextStep);
            }
            LoginState::OnLogoutQueue => {
                info.login_state = LoginState::Online;
                space.fill_login_space(info, env, FillStep::NextStep);
            }
            _ => {}
        }
    } else {
        match info.login_state {
            LoginState::Online => {
                space.empty_login_space(info, env, FillStep::NextStep);
                info.login_state = LoginState::OnLogoutQueue;
            }
            LoginState::OnLoginQueue => {
                space.empty_login_space(info, env, FillStep::NextStep);
                info.login_state = LoginState::Offline;
            }
            _ => {}
        }
    }
}

/// Plumbing to let `match_no_criteria` reach the `LoginWorld`'s KV
/// store without exposing the whole trait to `criteria.rs`.
struct RpbmLookupWorld<'a> {
    world: &'a dyn LoginWorld,
}

impl<'a> RpbmLookup for RpbmLookupWorld<'a> {
    fn get_value(&self, guid: u32, key: &str) -> u32 {
        self.world.random_mgr_get_value(guid, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::MockLoginWorld;

    fn cfg_with_defaults() -> BotConfig {
        let mut cfg = BotConfig::default();
        cfg.random_bots_max_logins_per_interval = 10;
        cfg.free_room_for_non_spare_bots = 0;
        cfg.random_bot_min_level = 1;
        cfg.sync_level_max_above = 5;
        cfg.login_bots_near_player_range = 25;
        cfg.default_login_criteria = vec!["maxbots".into()];
        cfg.login_criteria = vec![vec!["online".into()]];
        cfg.class_race_probability[1][1] = 100;
        cfg.class_race_probability_total = 100;
        for lvl in 1..=80 {
            cfg.level_probability[lvl] = 100;
        }
        cfg
    }

    fn mk_row(guid: u32, level: u32, is_online: bool) -> BotCandidateInfo {
        BotCandidateInfo {
            account_id: 1,
            guid,
            race: 1,
            cls: 1,
            level,
            is_online,
            total_played_time: 100,
            map_id: 0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            o: 0.0,
            guild_id: 0,
            group_id: 0,
        }
    }

    #[test]
    fn tick_without_pool_is_noop() {
        let mut mgr = LoginManager::new();
        let world = MockLoginWorld::new();
        world.set_max_online_bot_count(10);
        world.set_world_max_level(80);
        let out = mgr.tick(&cfg_with_defaults(), &world, &[]);
        assert!(out.commands.is_empty());
        assert!(!mgr.pool_loaded());
    }

    #[test]
    fn load_pool_populates_state() {
        let mut mgr = LoginManager::new();
        let rows = vec![mk_row(1, 10, false), mk_row(2, 20, true)];
        mgr.load_pool(&rows, &cfg_with_defaults());
        assert_eq!(mgr.pool.len(), 2);
        assert_eq!(mgr.pool[&1].login_state, LoginState::Offline);
        assert_eq!(mgr.pool[&2].login_state, LoginState::Online);
    }

    #[test]
    fn first_tick_queues_offline_bots_for_login() {
        let mut mgr = LoginManager::new();
        let cfg = cfg_with_defaults();
        mgr.load_pool(&[mk_row(1, 10, false), mk_row(2, 11, false)], &cfg);

        let world = MockLoginWorld::new();
        world.set_max_online_bot_count(10);
        world.set_world_max_level(80);

        let out = mgr.tick(&cfg, &world, &[]);
        assert_eq!(out.commands.len(), 2);
        assert!(out.commands.contains(&QueuedCommand::Login(1)));
        assert!(out.commands.contains(&QueuedCommand::Login(2)));
        assert_eq!(mgr.pool[&1].login_state, LoginState::Online);
        assert_eq!(mgr.pool[&2].login_state, LoginState::Online);
    }

    #[test]
    fn max_logins_per_interval_is_enforced() {
        let mut mgr = LoginManager::new();
        let mut cfg = cfg_with_defaults();
        cfg.random_bots_max_logins_per_interval = 2;
        mgr.load_pool(
            &[
                mk_row(1, 10, false),
                mk_row(2, 11, false),
                mk_row(3, 12, false),
                mk_row(4, 13, false),
            ],
            &cfg,
        );
        let world = MockLoginWorld::new();
        world.set_max_online_bot_count(20);
        world.set_world_max_level(80);

        let out = mgr.tick(&cfg, &world, &[]);
        assert_eq!(out.logins, 2);
        assert_eq!(out.commands.len(), 2);
    }

    #[test]
    fn cap_hit_stops_further_logins() {
        let mut mgr = LoginManager::new();
        let cfg = cfg_with_defaults();
        mgr.load_pool(
            &[
                mk_row(1, 10, false),
                mk_row(2, 11, false),
                mk_row(3, 12, false),
            ],
            &cfg,
        );
        let world = MockLoginWorld::new();
        // Cap of 1 — only one login should be queued.
        world.set_max_online_bot_count(1);
        world.set_world_max_level(80);
        let out = mgr.tick(&cfg, &world, &[]);
        assert_eq!(out.commands.len(), 1);
    }

    #[test]
    fn second_tick_queues_online_bots_for_logout_when_cap_drops() {
        let mut mgr = LoginManager::new();
        let mut cfg = cfg_with_defaults();
        cfg.free_room_for_non_spare_bots = 1;
        // Start with 2 bots online, then drop the cap so one of them
        // must be logged out.
        mgr.load_pool(&[mk_row(1, 10, true), mk_row(2, 11, true)], &cfg);
        let world = MockLoginWorld::new();
        world.set_max_online_bot_count(1);
        world.set_world_max_level(80);

        let out = mgr.tick(&cfg, &world, &[]);
        // Exactly one of them must be logged out — the other stays
        // online to satisfy the cap.
        assert_eq!(
            out.commands
                .iter()
                .filter(|c| matches!(c, QueuedCommand::Logout(_)))
                .count(),
            1
        );
    }

    #[test]
    fn debug_flag_emits_log_lines() {
        let mut mgr = LoginManager::new();
        mgr.debug = true;
        let cfg = cfg_with_defaults();
        mgr.load_pool(&[mk_row(1, 10, false)], &cfg);
        let world = MockLoginWorld::new();
        world.set_max_online_bot_count(5);
        world.set_world_max_level(80);

        mgr.tick(&cfg, &world, &[]);
        let events = world.events();
        let debug_lines: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                cmangos::MockLoginEvent::Debug(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert!(!debug_lines.is_empty());
        assert!(debug_lines.iter().any(|l| l.contains("Initial space")));
        assert!(debug_lines.iter().any(|l| l.contains("Queued to log in")));
    }
}
