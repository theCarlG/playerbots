//! State types for a single candidate bot in the login queue.
//!
//! Ports `PlayerLoginInfo` + `FillStep` + `LoginState` from
//! `playerbot/PlayerbotLoginMgr.cpp`. Every method here is pure — no
//! logging, no global access, no mutation beyond the argument. The
//! manager layer owns side-effects like holder dispatch and login/logout
//! command emission.

use cmangos::BotCandidateInfo;

/// Fill-step selector used when crediting/debiting a `LoginSpace` for a
/// given candidate. Mirrors `PlayerbotLoginMgr.cpp`'s `FillStep` enum.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FillStep {
    /// "Right now" — applies to candidates currently online.
    Now,
    /// "After this attempt resolves" — applies to candidates whose
    /// next-queued state is online.
    NextStep,
    /// Every candidate, regardless of state.
    AllPotential,
}

/// Running state of a single candidate bot.
///
/// The transitions form the standard login FSM:
///
/// ```text
///   Offline ─┬─ ON_LOGINQUEUE ─> Online ─┬─ ON_LOGOUTQUEUE ─> Offline
///            │                           │
///            └──────────────┄┄┄┄┄┄┄┄┄┄┄┄┄┘
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum LoginState {
    #[default]
    Offline,
    OnLoginQueue,
    Online,
    OnLogoutQueue,
}

impl LoginState {
    /// True when the candidate is currently in the game world
    /// (`ONLINE` or `ON_LOGOUTQUEUE`). Mirrors `PlayerLoginInfo::IsOnline`.
    #[must_use]
    pub const fn is_online(self) -> bool {
        matches!(self, Self::Online | Self::OnLogoutQueue)
    }

    /// True when the candidate is in either queue.
    #[must_use]
    pub const fn is_queued(self) -> bool {
        matches!(self, Self::OnLoginQueue | Self::OnLogoutQueue)
    }
}

/// Pure data describing a single candidate. Mirrors
/// `PlayerLoginInfo` with the runtime holder handle removed — holders
/// live C++-side and are polled via the `LoginWorld` trait.
#[derive(Clone, Debug)]
pub struct PlayerLoginInfo {
    pub account: u32,
    pub guid: u32,
    pub race: u8,
    pub cls: u8,
    /// Raw level from the DB row. The *effective* level used for
    /// bucket accounting comes from [`effective_level`] because "new"
    /// bots take a midpoint random level.
    pub level: u32,
    pub is_new: bool,
    pub position: Position,
    pub group_id: u32,
    pub guild_id: u32,
    pub login_state: LoginState,
}

/// Stripped-down `WorldPosition`: just enough to evaluate distance /
/// map gates. Mirrors the fork's `WorldPosition` data members (the
/// fork's `isBg()/isArena()/isOverworld()` are stubs so only the raw
/// coordinates matter for this phase).
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Position {
    pub map_id: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub o: f32,
}

impl Position {
    /// Squared horizontal distance to `other`, matching the
    /// `WorldPosition::sqDistance` used by `IsNearPlayer`.
    #[must_use]
    pub fn sq_distance(self, other: Position) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx * dx + dy * dy + dz * dz
    }
}

impl PlayerLoginInfo {
    /// Construct from a DB candidate row, defaulting to `Offline` with
    /// an empty holder.
    #[must_use]
    pub fn from_candidate(row: &BotCandidateInfo) -> Self {
        Self {
            account: row.account_id,
            guid: row.guid,
            race: row.race,
            cls: row.cls,
            level: row.level,
            // `instant_randomize` semantics (handled at config-read
            // time): if enabled, the row's `total_played_time == 0`
            // marks a brand new bot; otherwise level==1 is enough.
            is_new: false, // filled in by caller; depends on config
            position: Position {
                map_id: row.map_id,
                x: row.x,
                y: row.y,
                z: row.z,
                o: row.o,
            },
            group_id: row.group_id,
            guild_id: row.guild_id,
            login_state: if row.is_online {
                LoginState::Online
            } else {
                LoginState::Offline
            },
        }
    }

    /// Effective level used for per-level bucket accounting. For brand
    /// new bots with random levels enabled, this returns the midpoint
    /// of the allowed random range, mirroring
    /// `PlayerLoginInfo::GetLevel`.
    #[must_use]
    pub fn effective_level(&self, env: &LevelEnv) -> u32 {
        if !self.is_new || env.disable_random_levels {
            return self.level;
        }

        let min = env.random_bot_min_level;
        let sum = env.players_level.saturating_add(env.sync_level_max_above);
        let max = min.max(sum.min(env.world_max_level));
        (min + max) / 2
    }

    /// True when this candidate is "currently online" from the queue's
    /// perspective — mirrors `PlayerLoginInfo::IsOnline`.
    #[must_use]
    pub const fn is_online(&self) -> bool {
        self.login_state.is_online()
    }

    /// Reset the login state back to `Offline`/`Online` after a failed
    /// pass, mirroring `PlayerLoginInfo::ResetLoginState`.
    pub fn reset_login_state(&mut self) {
        self.login_state = match self.login_state {
            LoginState::OnLoginQueue => LoginState::Offline,
            LoginState::OnLogoutQueue => LoginState::Online,
            other => other,
        };
    }

    /// The C++ version defined `IsInBG`, `IsInArena`, `IsInInstance` in
    /// terms of `WorldPosition::isBg()/isArena()/isOverworld()`. In
    /// this fork, all three of those are stubs (see `playerbotDefs.h`),
    /// so every candidate is reported as "overworld": `is_in_bg` and
    /// `is_in_arena` always return false, and `is_in_instance` is
    /// likewise false until the fork un-stubs `WorldPosition`.
    #[must_use]
    #[inline]
    pub const fn is_in_bg(&self) -> bool {
        false
    }

    #[must_use]
    #[inline]
    pub const fn is_in_arena(&self) -> bool {
        false
    }

    #[must_use]
    #[inline]
    pub const fn is_in_instance(&self) -> bool {
        false
    }
}

/// Environment used by [`PlayerLoginInfo::effective_level`] — pulled
/// once per tick from the `LoginWorld` and config.
#[derive(Copy, Clone, Debug)]
pub struct LevelEnv {
    pub disable_random_levels: bool,
    pub random_bot_min_level: u32,
    pub sync_level_max_above: u32,
    pub players_level: u32,
    pub world_max_level: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> LevelEnv {
        LevelEnv {
            disable_random_levels: false,
            random_bot_min_level: 1,
            sync_level_max_above: 5,
            players_level: 40,
            world_max_level: 80,
        }
    }

    fn info(level: u32, is_new: bool) -> PlayerLoginInfo {
        PlayerLoginInfo {
            account: 1,
            guid: 42,
            race: 1,
            cls: 1,
            level,
            is_new,
            position: Position::default(),
            group_id: 0,
            guild_id: 0,
            login_state: LoginState::Offline,
        }
    }

    #[test]
    fn effective_level_returns_raw_when_not_new() {
        let i = info(20, false);
        assert_eq!(i.effective_level(&env()), 20);
    }

    #[test]
    fn effective_level_returns_raw_when_disabled() {
        let i = info(20, true);
        let mut e = env();
        e.disable_random_levels = true;
        assert_eq!(i.effective_level(&e), 20);
    }

    #[test]
    fn effective_level_midpoint_for_new_bot() {
        let i = info(1, true);
        // min=1, max = max(1, min(40+5, 80)) = 45; midpoint = 23
        assert_eq!(i.effective_level(&env()), 23);
    }

    #[test]
    fn effective_level_caps_at_world_max() {
        let i = info(1, true);
        let mut e = env();
        e.players_level = 120;
        e.world_max_level = 60;
        // max = max(1, min(125, 60)) = 60; midpoint = (1+60)/2 = 30
        assert_eq!(i.effective_level(&e), 30);
    }

    #[test]
    fn effective_level_floor_is_random_bot_min_level() {
        let i = info(1, true);
        let mut e = env();
        e.players_level = 1;
        e.random_bot_min_level = 20;
        // min=20, max = max(20, min(6, 80)) = 20; midpoint = 20
        assert_eq!(i.effective_level(&e), 20);
    }

    #[test]
    fn reset_login_state_transitions() {
        let mut i = info(20, false);
        i.login_state = LoginState::OnLoginQueue;
        i.reset_login_state();
        assert_eq!(i.login_state, LoginState::Offline);

        i.login_state = LoginState::OnLogoutQueue;
        i.reset_login_state();
        assert_eq!(i.login_state, LoginState::Online);

        i.login_state = LoginState::Online;
        i.reset_login_state();
        assert_eq!(i.login_state, LoginState::Online);
    }

    #[test]
    fn is_online_matches_cpp_semantics() {
        let mut i = info(20, false);
        i.login_state = LoginState::Offline;
        assert!(!i.is_online());
        i.login_state = LoginState::OnLoginQueue;
        assert!(!i.is_online());
        i.login_state = LoginState::Online;
        assert!(i.is_online());
        i.login_state = LoginState::OnLogoutQueue;
        assert!(i.is_online());
    }
}
