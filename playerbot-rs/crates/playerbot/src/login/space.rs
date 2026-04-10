//! `LoginSpace` — per-tick bookkeeping for the login queue.
//!
//! Mirrors the C++ `LoginSpace` struct plus the helper methods
//! `PlayerBotLoginMgr::FillLoginSpace`, `FillLoginSpace(pool,…)`,
//! `GetClassRaceBucketSize`, `GetLevelBucketSize`, and
//! `PlayerLoginInfo::FillLoginSpace/EmptyLoginSpace`. Every method is
//! pure; no globals, no I/O.

use crate::config::BotConfig;
use crate::login::state::{FillStep, LevelEnv, LoginState, PlayerLoginInfo, Position};
use cmangos::BotRealPlayerInfo;

/// `MAX_CLASSES × MAX_RACES` upper bounds, copied from the existing
/// config module so we don't reach outside the crate for them.
pub const MAX_CLASSES: usize = crate::config::MAX_CLASSES;
pub const MAX_RACES: usize = crate::config::MAX_RACES;

/// Maximum level probability slot — matches the core's
/// `DEFAULT_MAX_LEVEL + 1`. The config already uses `MAX_LEVEL_PROBABILITY_SLOTS`
/// for the same array.
pub const MAX_LEVEL_SLOTS: usize = 81;

/// Stripped-down view of a real (non-bot) player, passed to the login
/// queue each tick. Mirrors the fields the C++ `PlayerLoginInfo(Player*)`
/// constructor copied out of `Player*`.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct RealPlayerSummary {
    pub guid: u32,
    pub position: Position,
    pub group_id: u32,
    pub guild_id: u32,
}

impl From<&BotRealPlayerInfo> for RealPlayerSummary {
    fn from(raw: &BotRealPlayerInfo) -> Self {
        Self {
            guid: raw.guid,
            position: Position {
                map_id: raw.map_id,
                x: raw.x,
                y: raw.y,
                z: raw.z,
                o: 0.0,
            },
            group_id: raw.group_id,
            guild_id: raw.guild_id,
        }
    }
}

/// Per-tick capacity state. Updated in place as candidates are accepted
/// or rejected.
#[derive(Clone, Debug)]
pub struct LoginSpace {
    /// Slots remaining for candidates that would add a net +1 to the
    /// online bot count (login candidates). Starts at
    /// `max_online_bot_count - currently_online` and ticks down as
    /// logins are accepted and up as logouts are accepted.
    pub current_space: i32,
    /// Slots remaining in the "next-step" view — the number of bots
    /// that can still be committed to online status this tick before
    /// the population cap is hit. Starts at `max_online_bot_count`.
    pub total_space: i32,
    /// Per-class-race buckets. `class_race_bucket[cls][race]` is how
    /// many *additional* bots of that pairing may still come online.
    pub class_race_bucket: Box<[[i32; MAX_RACES]; MAX_CLASSES]>,
    /// Per-level buckets, indexed `[1..=80]`.
    pub level_bucket: Box<[i32; MAX_LEVEL_SLOTS]>,
    /// Snapshot of real players this tick. Borrowed by the criterion
    /// evaluator.
    pub real_players: Vec<RealPlayerSummary>,
}

impl LoginSpace {
    /// Build an empty space with every bucket zeroed. Use
    /// [`populate_from_config`](Self::populate_from_config) to fill in
    /// capacities.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            current_space: 0,
            total_space: 0,
            class_race_bucket: Box::new([[0; MAX_RACES]; MAX_CLASSES]),
            level_bucket: Box::new([0; MAX_LEVEL_SLOTS]),
            real_players: Vec::new(),
        }
    }

    /// Zero every bucket + real-player list in place.
    pub fn reset(&mut self) {
        self.current_space = 0;
        self.total_space = 0;
        for row in self.class_race_bucket.iter_mut() {
            for cell in row.iter_mut() {
                *cell = 0;
            }
        }
        for cell in self.level_bucket.iter_mut() {
            *cell = 0;
        }
        self.real_players.clear();
    }

    /// Populate the buckets from config + live `LoginWorld` state.
    /// Mirrors `PlayerBotLoginMgr::FillLoginSpace(pool, step)` on the
    /// *initial* pass (before any candidate has been debited).
    ///
    /// Arguments are:
    ///
    /// * `cfg` — current config snapshot
    /// * `env` — live env for `effective_level` (`players_level` etc.)
    /// * `max_online_bot_count` — from
    ///   `LoginWorld::max_online_bot_count()`
    pub fn populate_from_config(
        &mut self,
        cfg: &BotConfig,
        env: &LevelEnv,
        max_online_bot_count: u32,
    ) {
        let max = max_online_bot_count as i32;
        self.current_space = max;
        self.total_space = max;

        // Per-level buckets — every level slot indexed 1..=max_level
        // gets `max * prob / total_prob`, with prob==0 slots zeroed.
        let max_level = get_max_level(env);
        let mut level_prob_total: u64 = 0;
        for lvl in 1..max_level {
            level_prob_total = level_prob_total
                .saturating_add(u64::from(cfg.level_probability[lvl as usize]));
        }
        for lvl in 1..MAX_LEVEL_SLOTS {
            self.level_bucket[lvl] =
                get_level_bucket_size(cfg, env, max_online_bot_count, lvl as u32, level_prob_total)
                    as i32;
        }

        // Per-class-race buckets.
        for race in 1..MAX_RACES {
            for cls in 1..MAX_CLASSES {
                self.class_race_bucket[cls][race] =
                    get_class_race_bucket_size(cfg, max_online_bot_count, cls as u8, race as u8)
                        as i32;
            }
        }
    }

    /// Apply `fill_login_space(info, step)` to every candidate in
    /// `pool` — also decrements `current_space` for every
    /// already-online candidate, mirroring the C++ manager.
    pub fn fill_from_pool(&mut self, pool: &[PlayerLoginInfo], env: &LevelEnv, step: FillStep) {
        for info in pool {
            self.fill_login_space(info, env, step);
            if matches!(
                info.login_state,
                LoginState::Online | LoginState::OnLogoutQueue
            ) {
                self.current_space -= 1;
            }
        }
    }

    /// Credit a candidate towards the space — matches
    /// `PlayerLoginInfo::FillLoginSpace`.
    pub fn fill_login_space(&mut self, info: &PlayerLoginInfo, env: &LevelEnv, step: FillStep) {
        if !should_apply(info, step) {
            return;
        }
        self.total_space -= 1;
        let cls = info.cls as usize;
        let race = info.race as usize;
        if cls < MAX_CLASSES && race < MAX_RACES {
            self.class_race_bucket[cls][race] -= 1;
        }
        let lvl = info.effective_level(env) as usize;
        if lvl < MAX_LEVEL_SLOTS {
            self.level_bucket[lvl] -= 1;
        }
    }

    /// Debit a candidate from the space — matches
    /// `PlayerLoginInfo::EmptyLoginSpace`.
    pub fn empty_login_space(&mut self, info: &PlayerLoginInfo, env: &LevelEnv, step: FillStep) {
        if !should_apply(info, step) {
            return;
        }
        self.total_space += 1;
        let cls = info.cls as usize;
        let race = info.race as usize;
        if cls < MAX_CLASSES && race < MAX_RACES {
            self.class_race_bucket[cls][race] += 1;
        }
        let lvl = info.effective_level(env) as usize;
        if lvl < MAX_LEVEL_SLOTS {
            self.level_bucket[lvl] += 1;
        }
    }
}

/// `PlayerLoginInfo::FillLoginSpace/EmptyLoginSpace` apply-gate — the
/// state/step combinations under which a candidate should NOT be
/// counted.
#[inline]
fn should_apply(info: &PlayerLoginInfo, step: FillStep) -> bool {
    match step {
        FillStep::Now => !matches!(
            info.login_state,
            LoginState::Offline | LoginState::OnLoginQueue
        ),
        FillStep::NextStep => !matches!(
            info.login_state,
            LoginState::Offline | LoginState::OnLogoutQueue
        ),
        FillStep::AllPotential => true,
    }
}

/// Port of `PlayerBotLoginMgr::GetMaxLevel`. Returns the current
/// capped max level for random bots.
#[must_use]
pub fn get_max_level(env: &LevelEnv) -> u32 {
    let sum = env.players_level.saturating_add(env.sync_level_max_above);
    env.random_bot_min_level
        .max(sum.min(env.world_max_level))
}

/// Port of `PlayerBotLoginMgr::GetClassRaceBucketSize`.
#[must_use]
pub fn get_class_race_bucket_size(
    cfg: &BotConfig,
    max_online: u32,
    cls: u8,
    race: u8,
) -> u32 {
    let cls = cls as usize;
    let race = race as usize;
    if cls >= MAX_CLASSES || race >= MAX_RACES {
        return 0;
    }
    let prob = cfg.class_race_probability[cls][race];
    if prob == 0 {
        return 0;
    }
    if cfg.use_fixed_class_race_counts {
        return prob;
    }
    if cfg.class_race_probability_total == 0 {
        return 0;
    }
    u64::from(max_online)
        .saturating_mul(u64::from(prob))
        .checked_div(u64::from(cfg.class_race_probability_total))
        .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX))
}

/// Port of `PlayerBotLoginMgr::GetLevelBucketSize`. `level_prob_total`
/// is the sum of `cfg.level_probability[1..max_level]` — we hoist it
/// out of the per-bucket call so the whole `populate_from_config` pass
/// is `O(slots)`, not `O(slots²)`.
#[must_use]
pub fn get_level_bucket_size(
    cfg: &BotConfig,
    env: &LevelEnv,
    max_online: u32,
    level: u32,
    level_prob_total: u64,
) -> u32 {
    let level_usize = level as usize;
    if level_usize >= MAX_LEVEL_SLOTS {
        return 0;
    }
    let prob = cfg.level_probability[level_usize];
    if prob == 0 || level > get_max_level(env) {
        return 0;
    }
    if level_prob_total == 0 {
        return 0;
    }
    u64::from(max_online)
        .saturating_mul(u64::from(prob))
        .checked_div(level_prob_total)
        .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BotConfig;

    fn mk_cfg() -> BotConfig {
        let mut cfg = BotConfig::default();
        cfg.random_bot_min_level = 1;
        cfg.sync_level_max_above = 5;
        cfg.use_fixed_class_race_counts = false;
        // Zero the matrix so the defaults from `BotConfig::from_raw`
        // (uniform 100) don't leak into the per-bucket calculation, then
        // seed a few class/race probs: warrior+human=100, mage+human=50.
        cfg.class_race_probability = Box::new([[0; MAX_RACES]; MAX_CLASSES]);
        cfg.class_race_probability[1][1] = 100;
        cfg.class_race_probability[8][1] = 50;
        cfg.class_race_probability_total = 150;
        // Level probability uniform 100 for 1..=80
        for lvl in 1..=80 {
            cfg.level_probability[lvl] = 100;
        }
        cfg
    }

    fn mk_env() -> LevelEnv {
        LevelEnv {
            disable_random_levels: false,
            random_bot_min_level: 1,
            sync_level_max_above: 5,
            players_level: 20,
            world_max_level: 80,
        }
    }

    #[test]
    fn populate_total_and_current_space() {
        let mut space = LoginSpace::empty();
        space.populate_from_config(&mk_cfg(), &mk_env(), 300);
        assert_eq!(space.total_space, 300);
        assert_eq!(space.current_space, 300);
    }

    #[test]
    fn class_race_bucket_scales_with_prob() {
        let mut space = LoginSpace::empty();
        space.populate_from_config(&mk_cfg(), &mk_env(), 300);
        // warrior+human: 300 * 100 / 150 = 200
        assert_eq!(space.class_race_bucket[1][1], 200);
        // mage+human: 300 * 50 / 150 = 100
        assert_eq!(space.class_race_bucket[8][1], 100);
        // druid+human: prob 0 → bucket 0
        assert_eq!(space.class_race_bucket[11][1], 0);
    }

    #[test]
    fn fixed_class_race_counts_short_circuit() {
        let mut cfg = mk_cfg();
        cfg.use_fixed_class_race_counts = true;
        let mut space = LoginSpace::empty();
        space.populate_from_config(&cfg, &mk_env(), 300);
        // With fixed counts, the bucket just equals the prob value.
        assert_eq!(space.class_race_bucket[1][1], 100);
        assert_eq!(space.class_race_bucket[8][1], 50);
    }

    #[test]
    fn level_bucket_zero_past_max_level() {
        // players_level=20, sync_level_max_above=5, min=1, world_max=80
        //   → max_level = max(1, min(25, 80)) = 25
        let mut space = LoginSpace::empty();
        space.populate_from_config(&mk_cfg(), &mk_env(), 300);
        // Uniform prob 100 across 1..=80; only 1..=24 are summed
        // (loop is `level < max_level`), so level_prob_total = 24 * 100 = 2400
        // bucket for level 10 = 300 * 100 / 2400 = 12
        assert_eq!(space.level_bucket[10], 12);
        // level 26 is beyond max_level, bucket must be 0
        assert_eq!(space.level_bucket[26], 0);
    }

    #[test]
    fn fill_and_empty_are_inverse() {
        let cfg = mk_cfg();
        let env = mk_env();
        let mut space = LoginSpace::empty();
        space.populate_from_config(&cfg, &env, 300);
        let before = space.total_space;
        let bucket_before = space.class_race_bucket[1][1];
        let level_before = space.level_bucket[10];

        let info = PlayerLoginInfo {
            account: 1,
            guid: 42,
            race: 1,
            cls: 1,
            level: 10,
            is_new: false,
            position: Position::default(),
            group_id: 0,
            guild_id: 0,
            login_state: LoginState::Online,
        };

        space.fill_login_space(&info, &env, FillStep::Now);
        assert_eq!(space.total_space, before - 1);
        assert_eq!(space.class_race_bucket[1][1], bucket_before - 1);
        assert_eq!(space.level_bucket[10], level_before - 1);

        space.empty_login_space(&info, &env, FillStep::Now);
        assert_eq!(space.total_space, before);
        assert_eq!(space.class_race_bucket[1][1], bucket_before);
        assert_eq!(space.level_bucket[10], level_before);
    }

    #[test]
    fn fill_from_pool_counts_online_bots_against_current_space() {
        let cfg = mk_cfg();
        let env = mk_env();
        let mut space = LoginSpace::empty();
        space.populate_from_config(&cfg, &env, 300);

        let mut pool = vec![
            PlayerLoginInfo {
                account: 1,
                guid: 42,
                race: 1,
                cls: 1,
                level: 10,
                is_new: false,
                position: Position::default(),
                group_id: 0,
                guild_id: 0,
                login_state: LoginState::Online,
            },
            PlayerLoginInfo {
                account: 1,
                guid: 43,
                race: 1,
                cls: 1,
                level: 11,
                is_new: false,
                position: Position::default(),
                group_id: 0,
                guild_id: 0,
                login_state: LoginState::Offline,
            },
        ];
        pool[1].reset_login_state();

        space.fill_from_pool(&pool, &env, FillStep::NextStep);
        // One online → current_space decremented once.
        assert_eq!(space.current_space, 299);
        // One next-step-online candidate (same one) → total_space decremented once.
        assert_eq!(space.total_space, 299);
    }
}
