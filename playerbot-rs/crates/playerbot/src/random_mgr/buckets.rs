//! BG / Arena / LFG bucket matrices — Rust port of the per-queue
//! counters that `CheckBgQueue`, `CheckLfgQueue`, and `AddRandomBots`
//! consume on the legacy C++ side.
//!
//! Layout:
//!
//! * [`BgBuckets::bg_players`] — count of real players in each
//!   `(bg_type, bracket, team)` cell. Mirrors the C++ `BgPlayers`
//!   triple-nested map.
//! * [`BgBuckets::bg_bots`] — count of bots in each cell. Mirrors
//!   `BgBots`.
//! * [`BgBuckets::visual_bots`] — count of "visual bots" (cosmetic
//!   queue fillers). Mirrors `VisualBots`.
//! * [`BgBuckets::need_bots`] — boolean "this queue needs more bots".
//!   Mirrors `NeedBots`.
//! * [`BgBuckets::arena_bots`] — arena team counts keyed by
//!   `(bg_type, bracket, team, arena_type)`. Mirrors `ArenaBots`.
//! * [`BgBuckets::rating`] — arena-team rating snapshot per cell.
//! * [`BgBuckets::supporters`] — supporter slot counts per cell.
//! * [`BgBuckets::battle_masters`] — `team → bg_type → creature
//!   entries` map used by `GetBattleMasterEntry`.
//! * [`BgBuckets::lfg_dungeons`] — `team → dungeon ids` queue. Used
//!   only on `WotLK`.
//!
//! Access pattern mirrors the C++ verbatim but uses tuple keys
//! (`BgKey`, `ArenaKey`) instead of triple-nested `HashMap`s to avoid
//! redundant allocation on every cell write.

use std::collections::HashMap;

/// `Alliance = 0, Horde = 1`. Matches the C++ `TeamId` pattern used by
/// the queue code (note: distinct from the `Team` enum — the queue
/// counters use 0/1 indexing directly).
pub const TEAM_ALLIANCE: u32 = 0;
/// See [`TEAM_ALLIANCE`].
pub const TEAM_HORDE: u32 = 1;
/// Pseudo-team used by `BattleMastersCache` to mean "either faction
/// can use this trainer" — matches `TEAM_BOTH_ALLOWED` in the C++
/// `object_mgr` enum.
pub const TEAM_BOTH_ALLOWED: u32 = 2;

/// `(bg_type, bracket_id, team_id)` index into the BG bucket matrix.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct BgKey {
    pub bg_type: u32,
    pub bracket_id: u32,
    pub team_id: u32,
}

impl BgKey {
    #[must_use]
    pub const fn new(bg_type: u32, bracket_id: u32, team_id: u32) -> Self {
        Self {
            bg_type,
            bracket_id,
            team_id,
        }
    }
}

/// `(bg_type, bracket_id, team_id, arena_type)` index into the arena
/// bucket matrix. `arena_type` matches `CMaNGOS` `ARENA_TYPE_*` (2/3/5
/// for 2v2/3v3/5v5).
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct ArenaKey {
    pub bg_type: u32,
    pub bracket_id: u32,
    pub team_id: u32,
    pub arena_type: u32,
}

impl ArenaKey {
    #[must_use]
    pub const fn new(bg_type: u32, bracket_id: u32, team_id: u32, arena_type: u32) -> Self {
        Self {
            bg_type,
            bracket_id,
            team_id,
            arena_type,
        }
    }
}

/// Combined BG/Arena/LFG bucket state. Owned by the top-level
/// `RandomMgrState` container.
#[derive(Clone, Debug, Default)]
pub struct BgBuckets {
    pub bg_players: HashMap<BgKey, u32>,
    pub bg_bots: HashMap<BgKey, u32>,
    pub visual_bots: HashMap<BgKey, u32>,
    pub need_bots: HashMap<BgKey, bool>,
    pub arena_bots: HashMap<ArenaKey, u32>,
    pub rating: HashMap<BgKey, u32>,
    pub supporters: HashMap<BgKey, u32>,
    /// `team → bg_type → creature entries`. Populated by
    /// `LoadBattleMastersCache` at startup.
    pub battle_masters: HashMap<u32, HashMap<u32, Vec<u32>>>,
    /// `team → dungeon ids queued`. Populated by `CheckLfgQueue`.
    pub lfg_dungeons: HashMap<u32, Vec<u32>>,
}

impl BgBuckets {
    /// Construct an empty bucket matrix.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Zero every BG-related bucket. Mirrors the loop the C++
    /// constructor runs over every `(bg_type, bracket)` pair to set
    /// the counters to `0` and the `need_bots` flags to `false`. The
    /// battle-master and LFG tables are untouched.
    pub fn reset_bg_counters(&mut self) {
        self.bg_players.clear();
        self.bg_bots.clear();
        self.visual_bots.clear();
        self.need_bots.clear();
        self.arena_bots.clear();
        self.rating.clear();
        self.supporters.clear();
    }

    /// Drop every entry, including the battle-master cache and LFG
    /// dungeon lists.
    pub fn clear_all(&mut self) {
        self.reset_bg_counters();
        self.battle_masters.clear();
        self.lfg_dungeons.clear();
    }

    /// Bump the bot count for a `(bg_type, bracket, team)` cell.
    pub fn inc_bg_bots(&mut self, key: BgKey) {
        *self.bg_bots.entry(key).or_insert(0) += 1;
    }

    /// Bump the player count for a cell.
    pub fn inc_bg_players(&mut self, key: BgKey) {
        *self.bg_players.entry(key).or_insert(0) += 1;
    }

    /// Bump the visual-bot count for a cell.
    pub fn inc_visual_bots(&mut self, key: BgKey) {
        *self.visual_bots.entry(key).or_insert(0) += 1;
    }

    /// Bump an arena team counter for a `(bg_type, bracket, team,
    /// arena_type)` cell.
    pub fn inc_arena_bots(&mut self, key: ArenaKey) {
        *self.arena_bots.entry(key).or_insert(0) += 1;
    }

    /// Read the bot count for a cell (0 if absent).
    #[must_use]
    pub fn bg_bots(&self, key: BgKey) -> u32 {
        self.bg_bots.get(&key).copied().unwrap_or(0)
    }

    /// Read the player count for a cell (0 if absent).
    #[must_use]
    pub fn bg_players(&self, key: BgKey) -> u32 {
        self.bg_players.get(&key).copied().unwrap_or(0)
    }

    /// Read the visual-bot count for a cell.
    #[must_use]
    pub fn visual_bots(&self, key: BgKey) -> u32 {
        self.visual_bots.get(&key).copied().unwrap_or(0)
    }

    /// Read the arena team counter for a cell.
    #[must_use]
    pub fn arena_bots(&self, key: ArenaKey) -> u32 {
        self.arena_bots.get(&key).copied().unwrap_or(0)
    }

    /// Read the `need_bots` flag for a cell (`false` if absent).
    #[must_use]
    pub fn need_bots(&self, key: BgKey) -> bool {
        self.need_bots.get(&key).copied().unwrap_or(false)
    }

    /// Set the `need_bots` flag for a cell.
    pub fn set_need_bots(&mut self, key: BgKey, value: bool) {
        self.need_bots.insert(key, value);
    }

    /// Register a battlemaster NPC entry for the given team + BG type.
    /// Mirrors the `BattleMastersCache[team][bgTypeId].push_back(entry)`
    /// pattern in `LoadBattleMastersCache`.
    pub fn add_battle_master(&mut self, team: u32, bg_type: u32, entry: u32) {
        self.battle_masters
            .entry(team)
            .or_default()
            .entry(bg_type)
            .or_default()
            .push(entry);
    }

    /// Look up battlemaster NPC entries for the given team + BG type.
    #[must_use]
    pub fn battle_masters(&self, team: u32, bg_type: u32) -> &[u32] {
        self.battle_masters
            .get(&team)
            .and_then(|by_type| by_type.get(&bg_type))
            .map_or(&[] as &[u32], Vec::as_slice)
    }

    /// Clear the LFG dungeon list for both factions — called at the
    /// top of every `CheckLfgQueue` pass.
    pub fn clear_lfg_dungeons(&mut self) {
        self.lfg_dungeons.clear();
    }

    /// Append a dungeon id to the LFG queue list for a team.
    pub fn push_lfg_dungeon(&mut self, team: u32, dungeon_id: u32) {
        self.lfg_dungeons.entry(team).or_default().push(dungeon_id);
    }

    /// Read the LFG dungeon list for a team.
    #[must_use]
    pub fn lfg_dungeons_for_team(&self, team: u32) -> &[u32] {
        self.lfg_dungeons
            .get(&team)
            .map_or(&[] as &[u32], Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_counters_increment_and_reset() {
        let mut b = BgBuckets::new();
        let k = BgKey::new(1, 2, TEAM_ALLIANCE);
        b.inc_bg_bots(k);
        b.inc_bg_bots(k);
        b.inc_bg_players(k);
        assert_eq!(b.bg_bots(k), 2);
        assert_eq!(b.bg_players(k), 1);
        assert_eq!(b.bg_bots(BgKey::new(1, 2, TEAM_HORDE)), 0);

        b.reset_bg_counters();
        assert_eq!(b.bg_bots(k), 0);
        assert_eq!(b.bg_players(k), 0);
    }

    #[test]
    fn arena_counters_use_four_dim_key() {
        let mut b = BgBuckets::new();
        let k = ArenaKey::new(9, 3, TEAM_ALLIANCE, 2);
        b.inc_arena_bots(k);
        b.inc_arena_bots(k);
        assert_eq!(b.arena_bots(k), 2);
        assert_eq!(b.arena_bots(ArenaKey::new(9, 3, TEAM_ALLIANCE, 3)), 0);
    }

    #[test]
    fn need_bots_default_false() {
        let mut b = BgBuckets::new();
        let k = BgKey::new(1, 2, TEAM_ALLIANCE);
        assert!(!b.need_bots(k));
        b.set_need_bots(k, true);
        assert!(b.need_bots(k));
    }

    #[test]
    fn battle_masters_cache_by_team_and_type() {
        let mut b = BgBuckets::new();
        b.add_battle_master(TEAM_ALLIANCE, 1, 14981);
        b.add_battle_master(TEAM_ALLIANCE, 1, 14982);
        b.add_battle_master(TEAM_HORDE, 2, 14983);
        assert_eq!(b.battle_masters(TEAM_ALLIANCE, 1), &[14981, 14982]);
        assert_eq!(b.battle_masters(TEAM_HORDE, 2), &[14983]);
        assert!(b.battle_masters(TEAM_ALLIANCE, 99).is_empty());
    }

    #[test]
    fn lfg_dungeons_clear_and_append() {
        let mut b = BgBuckets::new();
        b.push_lfg_dungeon(TEAM_ALLIANCE, 100);
        b.push_lfg_dungeon(TEAM_ALLIANCE, 200);
        b.push_lfg_dungeon(TEAM_HORDE, 300);
        assert_eq!(b.lfg_dungeons_for_team(TEAM_ALLIANCE), &[100, 200]);
        assert_eq!(b.lfg_dungeons_for_team(TEAM_HORDE), &[300]);

        b.clear_lfg_dungeons();
        assert!(b.lfg_dungeons_for_team(TEAM_ALLIANCE).is_empty());
    }

    #[test]
    fn clear_all_drops_everything() {
        let mut b = BgBuckets::new();
        b.inc_bg_bots(BgKey::new(1, 2, TEAM_ALLIANCE));
        b.add_battle_master(TEAM_ALLIANCE, 1, 14981);
        b.push_lfg_dungeon(TEAM_ALLIANCE, 100);

        b.clear_all();
        assert_eq!(b.bg_bots(BgKey::new(1, 2, TEAM_ALLIANCE)), 0);
        assert!(b.battle_masters(TEAM_ALLIANCE, 1).is_empty());
        assert!(b.lfg_dungeons_for_team(TEAM_ALLIANCE).is_empty());
    }
}
