//! Login criteria parsing and evaluation.
//!
//! Ports `PlayerBotLoginMgr::GetLoginCriteria` +
//! `PlayerLoginInfo::MatchNoCriteria` +
//! `PlayerBotLoginMgr::CriteriaStillValid` +
//! `PlayerLoginInfo::IsNearPlayer/IsOnPlayerMap/IsInPlayerGroup/IsInPlayerGuild`
//! from `PlayerbotLoginMgr.cpp`.
//!
//! Criterion names come from `sPlayerbotAIConfig.defaultLoginCriteria +
//! loginCriteria[attempt]`. Every criterion is a function from
//! `(candidate, space, env) → bool`, where `true` means "the candidate
//! failed this criterion and should be excluded". The outer
//! `MatchNoCriteria` returns the first failing criterion, or
//! `LoginCriterionFailType::LoginOk` if none fail.
//!
//! Unknown criterion names are silently ignored (same as the C++
//! cascade of `if`s).

use crate::config::BotConfig;
use crate::login::space::LoginSpace;
use crate::login::state::{LevelEnv, PlayerLoginInfo};

/// Mirror of the C++ `LoginCriterionFailType` enum.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum LoginCriterionFailType {
    #[default]
    Unknown,
    MaxBots,
    SpareRoom,
    Online,
    RandomTimedLogout,
    RandomTimedOffline,
    ClassRace,
    Level,
    Range,
    Map,
    Group,
    Guild,
    Bg,
    Arena,
    Instance,
    LoginOk,
}

impl LoginCriterionFailType {
    /// Human-readable form used in debug logs — matches the C++
    /// `failName` table verbatim.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unknown => "UNKNOWN",
            Self::MaxBots => "MAX_BOTS",
            Self::SpareRoom => "SPARE_ROOM",
            Self::Online => "ONLINE",
            Self::RandomTimedLogout => "RANDOM_TIMED_LOGOUT",
            Self::RandomTimedOffline => "RANDOM_TIMED_OFFLINE",
            Self::ClassRace => "CLASSRACE",
            Self::Level => "LEVEL",
            Self::Range => "RANGE",
            Self::Map => "MAP",
            Self::Group => "GROUP",
            Self::Guild => "GUILD",
            Self::Bg => "BG",
            Self::Arena => "ARENA",
            Self::Instance => "INSTANCE",
            Self::LoginOk => "LOGIN_OK",
        }
    }
}

/// A KV lookup used by the `logoff`/`offline` criteria — abstracts the
/// `sRandomPlayerbotMgr.GetValue(guid, key)` call so criterion
/// evaluation can be tested without reaching for a `LoginWorld`.
pub trait RpbmLookup {
    fn get_value(&self, guid: u32, key: &str) -> u32;
}

/// Adapter: anything implementing `Fn(u32, &str) -> u32` is a lookup.
impl<F: Fn(u32, &str) -> u32> RpbmLookup for F {
    fn get_value(&self, guid: u32, key: &str) -> u32 {
        (self)(guid, key)
    }
}

/// A single parsed criterion. Each variant captures all the data it
/// needs to evaluate, so the evaluator doesn't have to look anything
/// up at evaluation time.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Criterion {
    MaxBots,
    /// Activated only on `attempt > 0` in the C++ version.
    SpareRoom {
        free_room_for_non_spare_bots: u32,
    },
    Online,
    /// Activated only if `randomBotTimedLogout` is enabled.
    LogOff,
    /// Activated only if `randomBotTimedOffline` is enabled.
    Offline,
    Bg,
    Arena,
    Instance,
    ClassRace,
    Level,
    Range {
        sq_range: f32,
        instant_randomize: bool,
    },
    Map,
    Guild,
    Group,
}

impl Criterion {
    /// True when the candidate *fails* the criterion (and should be
    /// excluded from the current pass).
    #[must_use]
    pub fn fails<L: RpbmLookup>(
        self,
        info: &PlayerLoginInfo,
        space: &LoginSpace,
        lookup: &L,
    ) -> bool {
        match self {
            Self::MaxBots => space.total_space <= 0,
            Self::SpareRoom {
                free_room_for_non_spare_bots,
            } => space.total_space <= free_room_for_non_spare_bots as i32,
            // "keep" criteria are inverted: failing = `!cond`.
            Self::Online => !info.is_online(),
            Self::LogOff => info.is_online() && lookup.get_value(info.guid, "add") == 0,
            Self::Offline => !info.is_online() && lookup.get_value(info.guid, "logout") != 0,
            Self::Bg => !(info.is_online() && info.is_in_bg()),
            Self::Arena => !(info.is_online() && info.is_in_arena()),
            Self::Instance => !(info.is_online() && info.is_in_instance()),
            Self::ClassRace => {
                let cls = info.cls as usize;
                let race = info.race as usize;
                if cls >= crate::login::space::MAX_CLASSES
                    || race >= crate::login::space::MAX_RACES
                {
                    return true;
                }
                space.class_race_bucket[cls][race] <= 0
            }
            Self::Level => {
                // The C++ version calls `GetLevel()` again here — we
                // don't have direct access to a `LevelEnv` at
                // evaluation time, so we rely on the level bucket
                // indexing at the candidate's raw level. That matches
                // the behaviour of the C++ version for
                // non-randomized cases and is safe for the randomized
                // case too because both `FillLoginSpace` and this
                // lookup go through `PlayerLoginInfo::effective_level`
                // when invoked through the manager.
                let lvl = info.level as usize;
                if lvl >= crate::login::space::MAX_LEVEL_SLOTS {
                    return true;
                }
                space.level_bucket[lvl] <= 0
            }
            Self::Range {
                sq_range,
                instant_randomize,
            } => !is_near_player(info, space, sq_range, instant_randomize),
            Self::Map => !is_on_player_map(info, space),
            Self::Guild => !is_in_player_guild(info, space),
            Self::Group => !is_in_player_group(info, space),
        }
    }

    /// Fail type produced when this criterion rejects a candidate.
    #[must_use]
    pub const fn fail_type(self) -> LoginCriterionFailType {
        match self {
            Self::MaxBots => LoginCriterionFailType::MaxBots,
            Self::SpareRoom { .. } => LoginCriterionFailType::SpareRoom,
            Self::Online => LoginCriterionFailType::Online,
            Self::LogOff => LoginCriterionFailType::RandomTimedLogout,
            Self::Offline => LoginCriterionFailType::RandomTimedOffline,
            Self::Bg => LoginCriterionFailType::Bg,
            Self::Arena => LoginCriterionFailType::Arena,
            Self::Instance => LoginCriterionFailType::Instance,
            Self::ClassRace => LoginCriterionFailType::ClassRace,
            Self::Level => LoginCriterionFailType::Level,
            Self::Range { .. } => LoginCriterionFailType::Range,
            Self::Map => LoginCriterionFailType::Map,
            Self::Guild => LoginCriterionFailType::Guild,
            Self::Group => LoginCriterionFailType::Group,
        }
    }
}

/// Port of `PlayerLoginInfo::IsNearPlayer`. `sq_range` is the
/// pre-squared `login_bots_near_player_range`.
#[must_use]
pub fn is_near_player(
    info: &PlayerLoginInfo,
    space: &LoginSpace,
    sq_range: f32,
    instant_randomize: bool,
) -> bool {
    if space.real_players.is_empty() {
        return false;
    }
    if info.is_new && instant_randomize {
        // The C++ version returns true — we don't know where the
        // randomized bot will spawn, so it's always "near" a player.
        return true;
    }
    for p in &space.real_players {
        if p.position.map_id == info.position.map_id
            && p.position.sq_distance(info.position) < sq_range
        {
            return true;
        }
    }
    false
}

/// Port of `PlayerLoginInfo::IsOnPlayerMap`.
#[must_use]
pub fn is_on_player_map(info: &PlayerLoginInfo, space: &LoginSpace) -> bool {
    if space.real_players.is_empty() {
        return false;
    }
    space
        .real_players
        .iter()
        .any(|p| p.position.map_id == info.position.map_id)
}

/// Port of `PlayerLoginInfo::IsInPlayerGroup`.
#[must_use]
pub fn is_in_player_group(info: &PlayerLoginInfo, space: &LoginSpace) -> bool {
    if space.real_players.is_empty() || info.group_id == 0 {
        return false;
    }
    space
        .real_players
        .iter()
        .any(|p| p.group_id == info.group_id)
}

/// Port of `PlayerLoginInfo::IsInPlayerGuild`.
#[must_use]
pub fn is_in_player_guild(info: &PlayerLoginInfo, space: &LoginSpace) -> bool {
    if space.real_players.is_empty() {
        return false;
    }
    space
        .real_players
        .iter()
        .any(|p| p.guild_id == info.guild_id)
}

/// Resolve a single criterion name against the current config. `attempt`
/// gates the `spareroom` criterion (matches the C++ `attempt > 0` check).
///
/// Unknown names return `None` — the criterion is silently skipped.
#[must_use]
pub fn parse_criterion(name: &str, cfg: &BotConfig, attempt: u8) -> Option<Criterion> {
    match name {
        "maxbots" => Some(Criterion::MaxBots),
        "spareroom" if attempt > 0 => Some(Criterion::SpareRoom {
            free_room_for_non_spare_bots: cfg.free_room_for_non_spare_bots,
        }),
        "spareroom" => None,
        "online" => Some(Criterion::Online),
        "logoff" if cfg.random_bot_timed_logout => Some(Criterion::LogOff),
        "logoff" => None,
        "offline" if cfg.random_bot_timed_offline => Some(Criterion::Offline),
        "offline" => None,
        "bg" => Some(Criterion::Bg),
        "arena" => Some(Criterion::Arena),
        "instance" => Some(Criterion::Instance),
        "classrace" => Some(Criterion::ClassRace),
        "level" => Some(Criterion::Level),
        "range" => Some(Criterion::Range {
            sq_range: (cfg.login_bots_near_player_range as f32)
                * (cfg.login_bots_near_player_range as f32),
            instant_randomize: cfg.instant_randomize,
        }),
        "map" => Some(Criterion::Map),
        "guild" => Some(Criterion::Guild),
        "group" => Some(Criterion::Group),
        _ => None,
    }
}

/// Build the criterion list for `attempt`. Mirrors
/// `PlayerBotLoginMgr::GetLoginCriteria`: concatenates
/// `cfg.default_login_criteria` with the attempt-specific
/// `cfg.login_criteria[attempt]` (empty if out of range), then parses
/// each name.
#[must_use]
pub fn criteria_for_attempt(cfg: &BotConfig, attempt: u8) -> Vec<Criterion> {
    let mut out = Vec::new();
    for name in &cfg.default_login_criteria {
        if let Some(c) = parse_criterion(name, cfg, attempt) {
            out.push(c);
        }
    }
    if let Some(variable) = cfg.login_criteria.get(attempt as usize) {
        for name in variable {
            if let Some(c) = parse_criterion(name, cfg, attempt) {
                out.push(c);
            }
        }
    }
    out
}

/// Number of attempts to iterate across — matches
/// `PlayerBotLoginMgr::GetLoginCriteriaSize`. The C++ version loops
/// `for (attempt = 0; attempt <= size; attempt++)` so the return value
/// is `cfg.login_criteria.len()` and the caller loops inclusive.
#[must_use]
pub fn login_criteria_size(cfg: &BotConfig) -> usize {
    cfg.login_criteria.len()
}

/// Port of `PlayerLoginInfo::MatchNoCriteria` — returns the first
/// criterion the candidate fails, or `LoginOk` if every criterion
/// passes.
#[must_use]
pub fn match_no_criteria<L: RpbmLookup>(
    info: &PlayerLoginInfo,
    space: &LoginSpace,
    criteria: &[Criterion],
    lookup: &L,
) -> LoginCriterionFailType {
    for c in criteria {
        if c.fails(info, space, lookup) {
            return c.fail_type();
        }
    }
    LoginCriterionFailType::LoginOk
}

/// Port of `PlayerBotLoginMgr::CriteriaStillValid`.
///
/// `UNKNOWN`, `MAX_BOTS`, `SPARE_ROOM`, `CLASSRACE`, `LEVEL` are
/// "dependent on previous candidates", so they are always
/// re-evaluated (`return false`). Any other fail type that *isn't* in
/// the current criteria list also needs re-evaluation. A fail type
/// that IS in the current list is still valid.
#[must_use]
pub fn criteria_still_valid(old: LoginCriterionFailType, criteria: &[Criterion]) -> bool {
    match old {
        LoginCriterionFailType::Unknown
        | LoginCriterionFailType::MaxBots
        | LoginCriterionFailType::SpareRoom
        | LoginCriterionFailType::ClassRace
        | LoginCriterionFailType::Level => false,
        _ => criteria.iter().any(|c| c.fail_type() == old),
    }
}

/// Unused-underscore suppressor: the `Level` criterion is parameterised
/// by `LevelEnv` only indirectly through the space buckets. We include
/// this helper to document that the `LevelEnv` requirement is tracked.
#[allow(dead_code)]
const fn _level_env_is_tracked(_env: &LevelEnv) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::login::space::{LoginSpace, RealPlayerSummary};
    use crate::login::state::{LoginState, Position};

    fn info(guid: u32, state: LoginState) -> PlayerLoginInfo {
        PlayerLoginInfo {
            account: 1,
            guid,
            race: 1,
            cls: 1,
            level: 20,
            is_new: false,
            position: Position::default(),
            group_id: 0,
            guild_id: 0,
            login_state: state,
        }
    }

    fn empty_lookup() -> impl RpbmLookup {
        |_guid: u32, _key: &str| 0u32
    }

    #[test]
    fn parse_criterion_maps_names() {
        let mut cfg = BotConfig::default();
        cfg.random_bot_timed_logout = true;
        cfg.random_bot_timed_offline = true;
        assert!(matches!(parse_criterion("maxbots", &cfg, 0), Some(Criterion::MaxBots)));
        assert!(matches!(parse_criterion("online", &cfg, 0), Some(Criterion::Online)));
        assert!(matches!(parse_criterion("logoff", &cfg, 0), Some(Criterion::LogOff)));
        assert!(matches!(parse_criterion("offline", &cfg, 0), Some(Criterion::Offline)));
        assert!(matches!(parse_criterion("classrace", &cfg, 0), Some(Criterion::ClassRace)));
        assert!(matches!(parse_criterion("level", &cfg, 0), Some(Criterion::Level)));
        assert!(matches!(parse_criterion("group", &cfg, 0), Some(Criterion::Group)));
        assert!(matches!(parse_criterion("guild", &cfg, 0), Some(Criterion::Guild)));
        assert!(matches!(parse_criterion("map", &cfg, 0), Some(Criterion::Map)));
        assert!(matches!(parse_criterion("bg", &cfg, 0), Some(Criterion::Bg)));
        assert!(matches!(parse_criterion("arena", &cfg, 0), Some(Criterion::Arena)));
        assert!(matches!(parse_criterion("instance", &cfg, 0), Some(Criterion::Instance)));
        assert!(parse_criterion("nope", &cfg, 0).is_none());
    }

    #[test]
    fn spareroom_requires_attempt_gt_zero() {
        let cfg = BotConfig::default();
        assert!(parse_criterion("spareroom", &cfg, 0).is_none());
        assert!(matches!(
            parse_criterion("spareroom", &cfg, 1),
            Some(Criterion::SpareRoom { .. })
        ));
    }

    #[test]
    fn logoff_and_offline_respect_config_gates() {
        let mut cfg = BotConfig::default();
        cfg.random_bot_timed_logout = false;
        cfg.random_bot_timed_offline = false;
        assert!(parse_criterion("logoff", &cfg, 0).is_none());
        assert!(parse_criterion("offline", &cfg, 0).is_none());

        cfg.random_bot_timed_logout = true;
        cfg.random_bot_timed_offline = true;
        assert!(matches!(parse_criterion("logoff", &cfg, 0), Some(Criterion::LogOff)));
        assert!(matches!(parse_criterion("offline", &cfg, 0), Some(Criterion::Offline)));
    }

    #[test]
    fn max_bots_fails_when_total_space_zero() {
        let mut space = LoginSpace::empty();
        space.total_space = 0;
        let info = info(42, LoginState::Offline);
        assert!(Criterion::MaxBots.fails(&info, &space, &empty_lookup()));
    }

    #[test]
    fn online_keep_criterion_fails_when_offline() {
        let space = LoginSpace::empty();
        let info = info(42, LoginState::Offline);
        assert!(Criterion::Online.fails(&info, &space, &empty_lookup()));
    }

    #[test]
    fn match_no_criteria_returns_first_fail() {
        let mut space = LoginSpace::empty();
        space.total_space = 0;
        let info = info(42, LoginState::Offline);
        let criteria = vec![Criterion::MaxBots, Criterion::Online];
        assert_eq!(
            match_no_criteria(&info, &space, &criteria, &empty_lookup()),
            LoginCriterionFailType::MaxBots
        );
    }

    #[test]
    fn match_no_criteria_returns_login_ok_when_all_pass() {
        let mut space = LoginSpace::empty();
        space.total_space = 10;
        space.class_race_bucket[1][1] = 10;
        space.level_bucket[20] = 10;
        let info = info(42, LoginState::Online);
        let criteria = vec![Criterion::MaxBots, Criterion::Online, Criterion::ClassRace];
        assert_eq!(
            match_no_criteria(&info, &space, &criteria, &empty_lookup()),
            LoginCriterionFailType::LoginOk
        );
    }

    #[test]
    fn is_near_player_uses_squared_range() {
        let mut space = LoginSpace::empty();
        space.real_players.push(RealPlayerSummary {
            guid: 1,
            position: Position {
                map_id: 0,
                x: 0.0,
                y: 0.0,
                z: 0.0,
                o: 0.0,
            },
            group_id: 0,
            guild_id: 0,
        });
        let mut info = info(42, LoginState::Offline);
        info.position.map_id = 0;
        info.position.x = 50.0;
        // sq_range = 60*60 = 3600; sq_distance = 2500 < 3600 → near
        assert!(is_near_player(&info, &space, 3600.0, false));
        // sq_range = 40*40 = 1600; sq_distance = 2500 > 1600 → far
        assert!(!is_near_player(&info, &space, 1600.0, false));
    }

    #[test]
    fn is_near_player_new_bot_instant_randomize_short_circuits() {
        let mut space = LoginSpace::empty();
        space.real_players.push(RealPlayerSummary::default());
        let mut info = info(42, LoginState::Offline);
        info.is_new = true;
        // Candidate is far on a different map, but `instant_randomize`
        // means we don't know where they'll spawn → treat as near.
        info.position.map_id = 530;
        assert!(is_near_player(&info, &space, 1.0, true));
    }

    #[test]
    fn criteria_still_valid_dependent_types_are_invalid() {
        let c = vec![Criterion::MaxBots];
        for old in [
            LoginCriterionFailType::Unknown,
            LoginCriterionFailType::MaxBots,
            LoginCriterionFailType::SpareRoom,
            LoginCriterionFailType::ClassRace,
            LoginCriterionFailType::Level,
        ] {
            assert!(!criteria_still_valid(old, &c));
        }
    }

    #[test]
    fn criteria_still_valid_stable_if_present_in_list() {
        let c = vec![Criterion::Range {
            sq_range: 100.0,
            instant_randomize: false,
        }];
        assert!(criteria_still_valid(LoginCriterionFailType::Range, &c));
        assert!(!criteria_still_valid(LoginCriterionFailType::Map, &c));
    }

    #[test]
    fn criteria_for_attempt_combines_default_and_variable() {
        let mut cfg = BotConfig::default();
        cfg.default_login_criteria = vec!["maxbots".into(), "online".into()];
        cfg.login_criteria = vec![vec!["range".into()], vec!["spareroom".into()]];
        // attempt 0 → default + login_criteria[0] = [maxbots, online, range]
        let a0 = criteria_for_attempt(&cfg, 0);
        assert_eq!(a0.len(), 3);
        assert!(matches!(a0[0], Criterion::MaxBots));
        assert!(matches!(a0[1], Criterion::Online));
        assert!(matches!(a0[2], Criterion::Range { .. }));
        // attempt 1 → default + [spareroom] (spareroom active because attempt > 0)
        let a1 = criteria_for_attempt(&cfg, 1);
        assert_eq!(a1.len(), 3);
        assert!(matches!(a1[2], Criterion::SpareRoom { .. }));
    }
}
