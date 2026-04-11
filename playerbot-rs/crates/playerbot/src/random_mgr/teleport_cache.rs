//! Level-bucketed teleport caches + the named-location table.
//!
//! Port of the C++ `locsPerLevelCache`, `rpgLocsCacheLevel`, `innCacheLevel`
//! and `namedLocations` fields on `RandomPlayerbotMgr`.
//!
//! Layout mirrors the C++ exactly:
//!
//! * [`TeleportCache::locs_per_level`] — `map<level, vec<WorldLocation>>`
//!   used by `RandomTeleport(bot)` (the level-appropriate random spawn
//!   pool pulled from creature scanning).
//! * [`TeleportCache::rpg_locs_per_race_level`] — `map<race, map<level,
//!   vec<WorldLocation>>>` used by `RandomTeleportForRpg`.
//! * [`TeleportCache::inn_per_race_level`] — `map<race, map<level,
//!   vec<(guid, WorldLocation)>>>` used for logout-to-inn handling.
//! * [`TeleportCache::named_locations`] — `map<name, WorldLocation>`
//!   looked up by the `LocationTrigger`.
//!
//! The RPG + inn caches are still TODO on the trait side — the C++
//! builder scans the creature table, which needs its own FFI query
//! pass (see Phase H.2). For now they exist as empty containers so
//! the scaffold can reach `state.rs` without blocking.

use std::collections::HashMap;

use cmangos::{NamedLocationRow, RandomMgrWorld, TeleportRow};

/// A point on the world — the Rust analogue of `CMaNGOS`
/// `WorldLocation(map_id, x, y, z, o)`.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct WorldLocation {
    pub map_id: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub orientation: f32,
}

impl WorldLocation {
    #[must_use]
    pub const fn new(map_id: u32, x: f32, y: f32, z: f32, orientation: f32) -> Self {
        Self {
            map_id,
            x,
            y,
            z,
            orientation,
        }
    }
}

impl From<TeleportRow> for WorldLocation {
    fn from(row: TeleportRow) -> Self {
        Self::new(row.map_id, row.x, row.y, row.z, 0.0)
    }
}

impl From<&NamedLocationRow> for WorldLocation {
    fn from(row: &NamedLocationRow) -> Self {
        Self::new(row.map_id, row.x, row.y, row.z, row.orientation)
    }
}

/// One inn spawn point — the legacy C++ stored `(ObjectGuid, WorldLocation)`
/// pairs so `RandomTeleport` could hand the innkeeper guid off to the
/// AI for binding purposes.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct InnLocation {
    /// Innkeeper guid (0 if the cache entry is a fallback).
    pub guid: u64,
    pub location: WorldLocation,
}

/// Combined teleport cache. Owned by the top-level `RandomMgrState`.
#[derive(Clone, Debug, Default)]
pub struct TeleportCache {
    /// Level → candidate world spawns. Populated by
    /// [`load_from_world`](Self::load_from_world).
    pub locs_per_level: HashMap<u32, Vec<WorldLocation>>,

    /// Race → level → RPG spawn candidates. Unpopulated in H.1 — the
    /// builder pass that hits the creature table lives in Phase H.2.
    pub rpg_locs_per_race_level: HashMap<u32, HashMap<u32, Vec<WorldLocation>>>,

    /// Race → level → inn spawn candidates. Same status as the RPG
    /// cache: placeholder for Phase H.2.
    pub inn_per_race_level: HashMap<u32, HashMap<u32, Vec<InnLocation>>>,

    /// Name → world location. Mirrors `namedLocations` verbatim.
    pub named_locations: HashMap<String, WorldLocation>,
}

impl TeleportCache {
    /// Construct an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop every cached entry.
    pub fn clear(&mut self) {
        self.locs_per_level.clear();
        self.rpg_locs_per_race_level.clear();
        self.inn_per_race_level.clear();
        self.named_locations.clear();
    }

    /// Pull the level-bucketed world locations + named locations from
    /// the world trait. Mirrors the first half of the C++
    /// `PrepareTeleportCache()` function (the bit that hits
    /// `ai_playerbot_tele_cache`) plus `LoadNamedLocations()`.
    ///
    /// If the teleport cache query returns an empty vec, the C++ side
    /// is asked to rebuild the table and the result is re-queried.
    pub fn load_from_world(&mut self, world: &dyn RandomMgrWorld) {
        self.clear();

        let mut rows = world.query_teleport_cache();
        if rows.is_empty() {
            world.rebuild_teleport_cache();
            rows = world.query_teleport_cache();
        }
        for row in rows {
            self.locs_per_level
                .entry(row.level)
                .or_default()
                .push(row.into());
        }

        for row in world.query_named_locations() {
            let loc = WorldLocation::from(&row);
            self.named_locations.insert(row.name, loc);
        }
    }

    /// Add or replace a named location — matches C++ `AddNamedLocation`
    /// but *overwrites* silently instead of failing. The C++ version
    /// logged an error and refused to update; we take the simpler
    /// "last write wins" stance for in-memory cache manipulation.
    pub fn insert_named_location(&mut self, name: impl Into<String>, location: WorldLocation) {
        self.named_locations.insert(name.into(), location);
    }

    /// Fetch a named location by name. Returns `None` when missing.
    #[must_use]
    pub fn named_location(&self, name: &str) -> Option<WorldLocation> {
        self.named_locations.get(name).copied()
    }

    /// Return the candidate world locations for a given level — an
    /// empty slice when nothing is cached.
    #[must_use]
    pub fn locs_for_level(&self, level: u32) -> &[WorldLocation] {
        self.locs_per_level
            .get(&level)
            .map_or(&[] as &[WorldLocation], Vec::as_slice)
    }

    /// Return the RPG candidates for a `(race, level)` bucket.
    #[must_use]
    pub fn rpg_locs(&self, race: u32, level: u32) -> &[WorldLocation] {
        self.rpg_locs_per_race_level
            .get(&race)
            .and_then(|levels| levels.get(&level))
            .map_or(&[] as &[WorldLocation], Vec::as_slice)
    }

    /// Return the inn candidates for a `(race, level)` bucket.
    #[must_use]
    pub fn inns(&self, race: u32, level: u32) -> &[InnLocation] {
        self.inn_per_race_level
            .get(&race)
            .and_then(|levels| levels.get(&level))
            .map_or(&[] as &[InnLocation], Vec::as_slice)
    }

    /// Total number of level buckets currently cached. Useful for
    /// "teleport cache loaded" log output.
    #[must_use]
    pub fn level_bucket_count(&self) -> usize {
        self.locs_per_level.len()
    }

    /// Total number of individual world locations across every level
    /// bucket — used in stats output.
    #[must_use]
    pub fn total_locations(&self) -> usize {
        self.locs_per_level.values().map(Vec::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::MockRandomMgrWorld;

    #[test]
    fn load_from_world_populates_level_buckets() {
        let world = MockRandomMgrWorld::new();
        world.set_teleport_rows(vec![
            TeleportRow {
                level: 1,
                map_id: 0,
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            TeleportRow {
                level: 1,
                map_id: 0,
                x: 4.0,
                y: 5.0,
                z: 6.0,
            },
            TeleportRow {
                level: 5,
                map_id: 1,
                x: 7.0,
                y: 8.0,
                z: 9.0,
            },
        ]);
        let mut cache = TeleportCache::new();
        cache.load_from_world(&world);

        assert_eq!(cache.level_bucket_count(), 2);
        assert_eq!(cache.total_locations(), 3);
        assert_eq!(cache.locs_for_level(1).len(), 2);
        assert_eq!(cache.locs_for_level(5).len(), 1);
        assert_eq!(cache.locs_for_level(10).len(), 0);
    }

    #[test]
    fn empty_teleport_cache_triggers_rebuild() {
        let world = MockRandomMgrWorld::new();
        // No rows set → first query returns empty → rebuild dispatched.
        let mut cache = TeleportCache::new();
        cache.load_from_world(&world);

        let events = world.events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, cmangos::MockRandomMgrEvent::RebuildTeleportCache))
        );
        assert_eq!(cache.level_bucket_count(), 0);
    }

    #[test]
    fn load_from_world_populates_named_locations() {
        let world = MockRandomMgrWorld::new();
        world.set_teleport_rows(vec![TeleportRow {
            level: 1,
            map_id: 0,
            x: 1.0,
            y: 2.0,
            z: 3.0,
        }]);
        world.set_named_locations(vec![NamedLocationRow {
            name: "stormwind_bank".into(),
            map_id: 0,
            x: 10.0,
            y: 20.0,
            z: 30.0,
            orientation: 1.5,
        }]);
        let mut cache = TeleportCache::new();
        cache.load_from_world(&world);

        let loc = cache
            .named_location("stormwind_bank")
            .expect("named location should resolve");
        assert_eq!(loc.map_id, 0);
        assert!((loc.orientation - 1.5).abs() < 1e-6);
    }

    #[test]
    fn insert_named_location_last_write_wins() {
        let mut cache = TeleportCache::new();
        cache.insert_named_location("spot", WorldLocation::new(0, 1.0, 2.0, 3.0, 0.0));
        cache.insert_named_location("spot", WorldLocation::new(1, 4.0, 5.0, 6.0, 0.0));
        let loc = cache.named_location("spot").unwrap();
        assert_eq!(loc.map_id, 1);
        assert!((loc.x - 4.0).abs() < 1e-6);
    }
}
