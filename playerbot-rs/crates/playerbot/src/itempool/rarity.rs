//! Item rarity cache — Rust port of `RandomItemMgr::BuildRarityCache`
//! and `GetItemRarity` (RandomItemMgr.cpp 3829-3966, 4016-4018).
//!
//! The legacy C++ has two build paths:
//!
//! 1. **Loaded**: if `ai_playerbot_rarity_cache` rows exist in the
//!    `characters` DB they are read verbatim.
//! 2. **Computed**: otherwise it walks every item in `sItemStorage`,
//!    runs a multi-table SQL query joining `creature_loot_template`,
//!    `gameobject_loot_template`, `disenchant_loot_template`,
//!    `fishing_loot_template`, and `skinning_loot_template`, and
//!    stores the maximum averaged chance back into the DB cache.
//!
//! The SQL path is owned by the C++ bridge — Rust cannot reach
//! `WorldDatabase` directly and the computed value is persisted so the
//! slow scan only ever runs once per server. The bridge surfaces the
//! result through [`cmangos::ItemWorld::query_rarity_rows`]; on Rust
//! side we simply store whatever rows arrive and expose a lookup.
//!
//! When the table is missing and the bridge returns an empty vec, the
//! lookup falls back to [`quality_heuristic`] which maps the four
//! item-quality tiers to hand-picked constants (green/blue/purple/
//! legendary). That mirrors the effective behavior of the legacy code
//! on a freshly-initialised server: `operator[]` returns `0.0f` for
//! unknown ids, which the gear generator then treats as "not rare" and
//! falls back to quality-based heuristics elsewhere.

use std::collections::HashMap;

use cmangos::BotItemRarityRow;

/// `ItemQuality` values we care about for the fallback heuristic.
pub mod quality {
    pub const POOR: u32 = 0;
    pub const NORMAL: u32 = 1;
    pub const UNCOMMON: u32 = 2;
    pub const RARE: u32 = 3;
    pub const EPIC: u32 = 4;
    pub const LEGENDARY: u32 = 5;
    pub const ARTIFACT: u32 = 6;
    pub const HEIRLOOM: u32 = 7;
}

/// Cached rarity values keyed by item id. Missing entries resolve to
/// `0.0` in [`RarityCache::get`], matching the legacy C++ `operator[]`
/// default-initialisation behavior.
#[derive(Default, Debug, Clone)]
pub struct RarityCache {
    rows: HashMap<u32, f32>,
}

impl RarityCache {
    /// Fresh empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the cache contents with the rows returned by the
    /// bridge's `ai_playerbot_rarity_cache` query. Values ≤ 0 or NaN
    /// are skipped because the C++ builder only persists rows with
    /// `rarity > 0.01` and we want to preserve that guarantee on
    /// reload.
    pub fn load(&mut self, rows: &[BotItemRarityRow]) {
        self.rows.clear();
        self.rows.reserve(rows.len());
        for row in rows {
            if row.rarity.is_finite() && row.rarity > 0.01 {
                self.rows.insert(row.item_id, row.rarity);
            }
        }
    }

    /// Number of items with a cached rarity value.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True when no rows were ever loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Lookup rarity for `item_id`. Returns `0.0` when the id is not
    /// cached — mirrors the legacy `operator[]` default.
    #[must_use]
    pub fn get(&self, item_id: u32) -> f32 {
        self.rows.get(&item_id).copied().unwrap_or(0.0)
    }

    /// Convenience wrapper: look up `item_id`, falling back to
    /// [`quality_heuristic`] when the cache has no entry. Use this
    /// from code paths that need _some_ rarity signal even on a
    /// freshly-initialised server.
    #[must_use]
    pub fn get_or_heuristic(&self, item_id: u32, quality: u32) -> f32 {
        let cached = self.get(item_id);
        if cached > 0.0 {
            cached
        } else {
            quality_heuristic(quality)
        }
    }
}

/// Quality-tier → rarity fallback. Matches the ordering the legacy C++
/// uses when it scores gear quality (higher tiers are rarer). The
/// constants sit in the same 0..=100 space the SQL-computed values
/// live in (loot-chance percent), so downstream code can compare cache
/// hits and heuristic fallbacks without rescaling.
#[must_use]
pub fn quality_heuristic(item_quality: u32) -> f32 {
    match item_quality {
        quality::POOR | quality::NORMAL => 0.0,
        quality::UNCOMMON => 25.0,
        quality::RARE => 10.0,
        quality::EPIC => 2.5,
        quality::LEGENDARY => 0.5,
        quality::ARTIFACT => 0.1,
        quality::HEIRLOOM => 1.0,
        _ => 0.0,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn row(item_id: u32, rarity: f32) -> BotItemRarityRow {
        BotItemRarityRow { item_id, rarity }
    }

    #[test]
    fn new_cache_is_empty() {
        let c = RarityCache::new();
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
        assert_eq!(c.get(42), 0.0);
    }

    #[test]
    fn load_populates_cache() {
        let mut c = RarityCache::new();
        c.load(&[row(1, 5.0), row(2, 0.5), row(3, 12.0)]);
        assert_eq!(c.len(), 3);
        assert_eq!(c.get(1), 5.0);
        assert_eq!(c.get(2), 0.5);
        assert_eq!(c.get(3), 12.0);
    }

    #[test]
    fn load_drops_non_positive_rows() {
        let mut c = RarityCache::new();
        c.load(&[
            row(1, 5.0),
            row(2, 0.0),    // legacy builder never persists ≤ 0.01
            row(3, -1.0),   // defensive: should be discarded
            row(4, f32::NAN),
        ]);
        assert_eq!(c.len(), 1);
        assert_eq!(c.get(1), 5.0);
        assert_eq!(c.get(2), 0.0);
        assert_eq!(c.get(3), 0.0);
        assert_eq!(c.get(4), 0.0);
    }

    #[test]
    fn load_drops_rows_exactly_at_threshold() {
        // The legacy builder uses `rarity > 0.01`, so the threshold is
        // strict — 0.01 itself must not make it into the cache.
        let mut c = RarityCache::new();
        c.load(&[row(1, 0.01)]);
        assert_eq!(c.get(1), 0.0);
    }

    #[test]
    fn get_unknown_id_returns_zero() {
        let mut c = RarityCache::new();
        c.load(&[row(1, 5.0)]);
        assert_eq!(c.get(999), 0.0);
    }

    #[test]
    fn load_replaces_previous_contents() {
        let mut c = RarityCache::new();
        c.load(&[row(1, 5.0)]);
        c.load(&[row(2, 10.0)]);
        assert_eq!(c.get(1), 0.0);
        assert_eq!(c.get(2), 10.0);
    }

    #[test]
    fn heuristic_maps_each_quality_tier() {
        assert_eq!(quality_heuristic(quality::POOR), 0.0);
        assert_eq!(quality_heuristic(quality::NORMAL), 0.0);
        assert_eq!(quality_heuristic(quality::UNCOMMON), 25.0);
        assert_eq!(quality_heuristic(quality::RARE), 10.0);
        assert_eq!(quality_heuristic(quality::EPIC), 2.5);
        assert_eq!(quality_heuristic(quality::LEGENDARY), 0.5);
        assert_eq!(quality_heuristic(quality::ARTIFACT), 0.1);
        assert_eq!(quality_heuristic(quality::HEIRLOOM), 1.0);
        assert_eq!(quality_heuristic(99), 0.0);
    }

    #[test]
    fn get_or_heuristic_prefers_cache() {
        let mut c = RarityCache::new();
        c.load(&[row(1, 5.0)]);
        // Cache hit wins over the heuristic.
        assert_eq!(c.get_or_heuristic(1, quality::EPIC), 5.0);
        // Cache miss falls back to the quality heuristic.
        assert_eq!(c.get_or_heuristic(999, quality::EPIC), 2.5);
        assert_eq!(c.get_or_heuristic(999, quality::POOR), 0.0);
    }
}
