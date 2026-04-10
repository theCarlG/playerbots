//! `BuildRandomItemCache` + `Query(level, type, predicate)` — Rust port
//! of `RandomItemMgr::BuildRandomItemCache` / `::Query` / `::GetRandomItem`
//! (`RandomItemMgr.cpp` lines 193-306).
//!
//! This is the small "generic pool" cache — independent of class/spec —
//! that backs the guild-task random-item picker. It buckets every
//! "sensible" prototype by `item_level / 10`, one list per
//! [`RandomItemType`]. At runtime, callers ask for a random item at
//! (level, type) with an optional post-filter predicate.
//!
//! # Dead code caveat
//!
//! In the current playerbot codebase, `GetRandomItem(level, type, ..)`
//! and `Query(level, type, ..)` are not called from anywhere. The
//! class-targeted `Query(level, clazz, spec, slot, quality)` — ported
//! in [`super::equip_cache`] — is used exclusively. The random-item
//! pool predates the class cache and was left behind as scaffolding.
//! Per the project "port everything from PB2" directive, the port
//! stays complete so downstream subsystems can wire up guild-task
//! rewards without reaching back into the C++.
//!
//! # Two code paths
//!
//! * **DB-loaded** — when the `ai_playerbot_rnditem_cache` table is
//!   populated the legacy code streams the rows into
//!   `randomItemCache[level][type]`. [`load_random_cache_rows`] mirrors
//!   that.
//! * **Built fresh** — [`build_random_cache`] iterates prototypes and
//!   applies the duration / name / `item_level` / `sell_price` filters,
//!   then inserts into the decade bucket.
//!
//! # Predicate
//!
//! The legacy `predicates[rit]` map is *declared* but never populated
//! in the C++ (nothing pushes into it). The Rust port keeps a
//! [`RandomCachePredicate`] trait so callers with per-type filtering
//! can supply their own table.

use std::collections::BTreeMap;

use cmangos::{BotItemPrototype, ItemWorld};

use super::types::RandomItemType;

// ── Constants ─────────────────────────────────────────────────────────────

/// Every [`RandomItemType`] variant in iteration order. Mirrors the
/// legacy `for (uint32 type = RANDOM_ITEM_GUILD_TASK; type <= ...; ..)`
/// loop.
pub const RANDOM_ITEM_TYPES: [RandomItemType; 5] = [
    RandomItemType::GuildTask,
    RandomItemType::GuildTaskRewardEquipBlue,
    RandomItemType::GuildTaskRewardEquipGreen,
    RandomItemType::GuildTaskRewardTrade,
    RandomItemType::GuildTaskRewardTradeRare,
];

// ── Name filter ────────────────────────────────────────────────────────────

/// Case-insensitive substrings the legacy `strstri` chain rejects
/// (line 248). The Rust side lower-cases the name once per proto.
const BAD_NAME_SUBSTRINGS: &[&str] = &["qa", "test", "deprecated"];

/// True when `proto.name` contains "qa", "test", or "deprecated" (any
/// case). Mirrors the legacy `strstri` chain.
#[must_use]
pub fn has_bad_name(proto: &BotItemPrototype) -> bool {
    let name = proto_name(proto).to_lowercase();
    BAD_NAME_SUBSTRINGS
        .iter()
        .any(|needle| name.contains(needle))
}

fn proto_name(proto: &BotItemPrototype) -> &str {
    let bytes = c_char_slice_as_bytes(&proto.name);
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..end]).unwrap_or("")
}

#[allow(unsafe_code)]
fn c_char_slice_as_bytes(buf: &[core::ffi::c_char]) -> &[u8] {
    // Safety: `c_char` is either i8 or u8 — both 1-byte, aligned to 1.
    // The lifetime is preserved.
    unsafe { core::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), buf.len()) }
}

// ── Predicate trait ────────────────────────────────────────────────────────

/// Per-type prototype filter the legacy code applies during build. The
/// trait is generic over the item type so callers can supply different
/// predicates for blue-vs-green equipment rewards without plumbing an
/// enum into the trait.
pub trait RandomCachePredicate {
    /// True if `proto` should be included in the cache for this type.
    fn apply(&self, proto: &BotItemPrototype) -> bool;
}

/// Identity predicate — accept every prototype. Useful as the default
/// when no per-type filter is configured.
pub struct AlwaysAccept;

impl RandomCachePredicate for AlwaysAccept {
    fn apply(&self, _proto: &BotItemPrototype) -> bool {
        true
    }
}

// ── Cache types ────────────────────────────────────────────────────────────

/// Composite key used by [`RandomItemCache`]. The legacy code uses a
/// nested `map<u32, map<RandomItemType, ...>>`; flattening the keys
/// into a tuple makes iteration order deterministic and avoids a
/// double-lookup on the hot path.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RandomCacheKey {
    /// `level / 10` decade bucket.
    pub level_bucket: u32,
    pub kind: RandomItemType,
}

impl RandomCacheKey {
    #[must_use]
    pub const fn new(level_bucket: u32, kind: RandomItemType) -> Self {
        Self { level_bucket, kind }
    }
}

// Derive Ord on the enum to satisfy the RandomCacheKey derive.
impl PartialOrd for RandomItemType {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RandomItemType {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (*self as u32).cmp(&(*other as u32))
    }
}

/// Top-level random-item cache. `BTreeMap` gives deterministic
/// iteration order for snapshot testing.
pub type RandomItemCache = BTreeMap<RandomCacheKey, Vec<u32>>;

// ── DB-loaded path ─────────────────────────────────────────────────────────

/// One row from `ai_playerbot_rnditem_cache`. Mirrors the three-column
/// `SELECT lvl, type, item FROM ai_playerbot_rnditem_cache`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct RandomCacheRow {
    pub level_bucket: u32,
    pub kind_raw: u32,
    pub item_id: u32,
}

/// Load the random-item cache from DB rows. Rows with an unknown
/// `kind_raw` are silently skipped (matches the C++ `(RandomItemType)
/// type` cast which yields an out-of-range enum value but a
/// well-behaved `std::map` insertion).
#[must_use]
pub fn load_random_cache_rows(rows: &[RandomCacheRow]) -> RandomItemCache {
    let mut cache: RandomItemCache = BTreeMap::new();
    for row in rows {
        let Some(kind) = RandomItemType::from_raw(row.kind_raw) else {
            continue;
        };
        let key = RandomCacheKey::new(row.level_bucket, kind);
        cache.entry(key).or_default().push(row.item_id);
    }
    cache
}

// ── Build path ─────────────────────────────────────────────────────────────

/// Per-type predicate table. Look-up returns `None` for "no predicate
/// configured, accept everything" (matches the legacy `predicates[rit]
/// == nullptr` check).
#[derive(Default)]
pub struct RandomCachePredicates<'a> {
    entries: [Option<&'a dyn RandomCachePredicate>; 5],
}

impl<'a> RandomCachePredicates<'a> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: [None; 5],
        }
    }

    /// Install a predicate for `kind`. Later calls overwrite the
    /// previous entry.
    pub fn set(&mut self, kind: RandomItemType, predicate: &'a dyn RandomCachePredicate) {
        self.entries[kind as usize] = Some(predicate);
    }

    /// Retrieve the predicate for `kind`, or `None` if no predicate is
    /// configured (meaning "accept every prototype").
    #[must_use]
    pub fn get(&self, kind: RandomItemType) -> Option<&'a dyn RandomCachePredicate> {
        self.entries[kind as usize]
    }

    /// True if `proto` is accepted for `kind` — i.e. no predicate is
    /// installed, or the installed predicate returns `true`. Mirrors
    /// `!(predicates[rit] && !predicates[rit]->Apply(proto))` in the
    /// legacy build loop.
    #[must_use]
    pub fn accepts(&self, kind: RandomItemType, proto: &BotItemPrototype) -> bool {
        self.get(kind).is_none_or(|p| p.apply(proto))
    }
}

/// Inputs to [`build_random_cache`]. The struct shape matches the other
/// itempool builders (`ItemInfoBuildCtx`, `EquipCacheBuildCtx`).
pub struct RandomCacheBuildCtx<'a> {
    pub prototypes: &'a [BotItemPrototype],
    pub predicates: &'a RandomCachePredicates<'a>,
}

/// Build the random-item cache from scratch. Mirrors the `else` branch
/// of `BuildRandomItemCache` (lines 236-294).
#[must_use]
pub fn build_random_cache(ctx: &RandomCacheBuildCtx<'_>) -> RandomItemCache {
    let mut cache: RandomItemCache = BTreeMap::new();

    for proto in ctx.prototypes {
        // Sign bit set on `duration` marks a "temporary" item — same
        // filter the C++ uses verbatim.
        if (proto.duration & 0x8000_0000) != 0 {
            continue;
        }
        if has_bad_name(proto) {
            continue;
        }
        if proto.item_level == 0 {
            continue;
        }
        if proto.sell_price == 0 {
            continue;
        }

        let bucket = proto.item_level / 10;
        for kind in RANDOM_ITEM_TYPES {
            if !ctx.predicates.accepts(kind, proto) {
                continue;
            }
            let key = RandomCacheKey::new(bucket, kind);
            cache.entry(key).or_default().push(proto.item_id);
        }
    }

    cache
}

// ── Query ──────────────────────────────────────────────────────────────────

/// Mirrors `RandomItemMgr::Query(level, type, predicate)`. The level
/// bucket is `(level - 1) / 10` (note: the legacy code uses `level - 1`
/// here but `level / 10` during build — a one-off in the C++ we
/// preserve verbatim). An optional runtime predicate post-filters the
/// bucket contents.
#[must_use]
pub fn query_random_cache(
    cache: &RandomItemCache,
    level: u32,
    kind: RandomItemType,
    predicate: Option<&dyn RandomCachePredicate>,
    prototypes_by_id: &std::collections::HashMap<u32, &BotItemPrototype>,
) -> Vec<u32> {
    // Legacy `(level - 1) / 10` with a saturating subtraction so
    // level 0 doesn't underflow.
    let level_bucket = level.saturating_sub(1) / 10;
    let key = RandomCacheKey::new(level_bucket, kind);
    let Some(list) = cache.get(&key) else {
        return Vec::new();
    };

    let mut result = Vec::with_capacity(list.len());
    for &item_id in list {
        let Some(proto) = prototypes_by_id.get(&item_id) else {
            continue;
        };
        if let Some(pred) = predicate
            && !pred.apply(proto)
        {
            continue;
        }
        result.push(item_id);
    }
    result
}

/// Mirrors `RandomItemMgr::GetRandomItem(level, type, predicate)`.
/// Returns `0` when the bucket is empty (same sentinel the C++ uses).
#[must_use]
pub fn get_random_item(
    cache: &RandomItemCache,
    world: &dyn ItemWorld,
    level: u32,
    kind: RandomItemType,
    predicate: Option<&dyn RandomCachePredicate>,
    prototypes_by_id: &std::collections::HashMap<u32, &BotItemPrototype>,
) -> u32 {
    let list = query_random_cache(cache, level, kind, predicate, prototypes_by_id);
    if list.is_empty() {
        return 0;
    }
    // urand_range is inclusive on both ends, matching the legacy
    // `urand(0, list.size() - 1)`.
    let idx = world.urand_range(0, (list.len() - 1) as u32) as usize;
    list[idx]
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cmangos::MockItemWorld;
    use std::collections::HashMap;

    fn ascii_name(out: &mut [core::ffi::c_char; 96], s: &str) {
        for (i, b) in s.as_bytes().iter().enumerate().take(out.len() - 1) {
            out[i] = *b as core::ffi::c_char;
        }
    }

    fn sensible_proto(item_id: u32, item_level: u32) -> BotItemPrototype {
        let mut proto = BotItemPrototype::default();
        proto.item_id = item_id;
        proto.item_level = item_level;
        proto.sell_price = 100;
        proto
    }

    fn build_index(protos: &[BotItemPrototype]) -> HashMap<u32, &BotItemPrototype> {
        let mut map: HashMap<u32, &BotItemPrototype> = HashMap::new();
        for p in protos {
            map.insert(p.item_id, p);
        }
        map
    }

    #[test]
    fn has_bad_name_detects_test_substring() {
        let mut proto = BotItemPrototype::default();
        ascii_name(&mut proto.name, "Test Sword");
        assert!(has_bad_name(&proto));

        ascii_name(&mut proto.name, "Deprecated Wand");
        assert!(has_bad_name(&proto));

        ascii_name(&mut proto.name, "QA Robe");
        assert!(has_bad_name(&proto));

        ascii_name(&mut proto.name, "Thunderfury");
        assert!(!has_bad_name(&proto));
    }

    #[test]
    fn random_item_type_ordering() {
        assert!(RandomItemType::GuildTask < RandomItemType::GuildTaskRewardEquipBlue);
        assert!(
            RandomItemType::GuildTaskRewardEquipGreen < RandomItemType::GuildTaskRewardTradeRare
        );
    }

    #[test]
    fn load_random_cache_rows_groups_per_key() {
        let rows = vec![
            RandomCacheRow {
                level_bucket: 2,
                kind_raw: RandomItemType::GuildTask as u32,
                item_id: 11,
            },
            RandomCacheRow {
                level_bucket: 2,
                kind_raw: RandomItemType::GuildTask as u32,
                item_id: 12,
            },
            RandomCacheRow {
                level_bucket: 3,
                kind_raw: RandomItemType::GuildTaskRewardTrade as u32,
                item_id: 21,
            },
            // Out-of-range kind — silently dropped.
            RandomCacheRow {
                level_bucket: 0,
                kind_raw: 99,
                item_id: 99,
            },
        ];
        let cache = load_random_cache_rows(&rows);
        let k1 = RandomCacheKey::new(2, RandomItemType::GuildTask);
        let k2 = RandomCacheKey::new(3, RandomItemType::GuildTaskRewardTrade);
        assert_eq!(cache.get(&k1).unwrap(), &vec![11, 12]);
        assert_eq!(cache.get(&k2).unwrap(), &vec![21]);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn build_skips_duration_sign_bit_items() {
        let mut proto = sensible_proto(100, 25);
        proto.duration = 0x8000_0000_u32;
        let protos = vec![proto];
        let predicates = RandomCachePredicates::new();
        let ctx = RandomCacheBuildCtx {
            prototypes: &protos,
            predicates: &predicates,
        };
        let cache = build_random_cache(&ctx);
        assert!(cache.is_empty());
    }

    #[test]
    fn build_skips_zero_item_level() {
        let proto = sensible_proto(101, 0);
        let protos = vec![proto];
        let predicates = RandomCachePredicates::new();
        let ctx = RandomCacheBuildCtx {
            prototypes: &protos,
            predicates: &predicates,
        };
        let cache = build_random_cache(&ctx);
        assert!(cache.is_empty());
    }

    #[test]
    fn build_skips_zero_sell_price() {
        let mut proto = sensible_proto(102, 25);
        proto.sell_price = 0;
        let protos = vec![proto];
        let predicates = RandomCachePredicates::new();
        let ctx = RandomCacheBuildCtx {
            prototypes: &protos,
            predicates: &predicates,
        };
        let cache = build_random_cache(&ctx);
        assert!(cache.is_empty());
    }

    #[test]
    fn build_skips_bad_name() {
        let mut proto = sensible_proto(103, 25);
        ascii_name(&mut proto.name, "Test Sword of Testing");
        let protos = vec![proto];
        let predicates = RandomCachePredicates::new();
        let ctx = RandomCacheBuildCtx {
            prototypes: &protos,
            predicates: &predicates,
        };
        let cache = build_random_cache(&ctx);
        assert!(cache.is_empty());
    }

    #[test]
    fn build_buckets_by_item_level() {
        let protos = vec![
            sensible_proto(200, 15), // bucket 1
            sensible_proto(201, 25), // bucket 2
            sensible_proto(202, 29), // bucket 2
            sensible_proto(203, 30), // bucket 3
        ];
        let predicates = RandomCachePredicates::new();
        let ctx = RandomCacheBuildCtx {
            prototypes: &protos,
            predicates: &predicates,
        };
        let cache = build_random_cache(&ctx);

        // With no predicates configured, every (bucket, kind) gets
        // the same set of items.
        for kind in RANDOM_ITEM_TYPES {
            let k1 = RandomCacheKey::new(1, kind);
            let k2 = RandomCacheKey::new(2, kind);
            let k3 = RandomCacheKey::new(3, kind);
            assert_eq!(cache.get(&k1).unwrap(), &vec![200]);
            assert_eq!(cache.get(&k2).unwrap(), &vec![201, 202]);
            assert_eq!(cache.get(&k3).unwrap(), &vec![203]);
        }
    }

    struct OnlyEvenIds;
    impl RandomCachePredicate for OnlyEvenIds {
        fn apply(&self, proto: &BotItemPrototype) -> bool {
            proto.item_id % 2 == 0
        }
    }

    #[test]
    fn build_applies_predicate_per_type() {
        let protos = vec![
            sensible_proto(300, 25), // even
            sensible_proto(301, 25), // odd
            sensible_proto(302, 25), // even
        ];
        let even = OnlyEvenIds;
        let mut predicates = RandomCachePredicates::new();
        predicates.set(RandomItemType::GuildTask, &even);

        let ctx = RandomCacheBuildCtx {
            prototypes: &protos,
            predicates: &predicates,
        };
        let cache = build_random_cache(&ctx);

        // GuildTask bucket: only evens (300, 302).
        let k = RandomCacheKey::new(2, RandomItemType::GuildTask);
        assert_eq!(cache.get(&k).unwrap(), &vec![300, 302]);
        // Other kinds: no predicate, accept all.
        let k2 = RandomCacheKey::new(2, RandomItemType::GuildTaskRewardEquipBlue);
        assert_eq!(cache.get(&k2).unwrap(), &vec![300, 301, 302]);
    }

    #[test]
    fn query_uses_level_minus_one_bucket() {
        let proto = sensible_proto(400, 25); // bucket 2 at build time
        let protos = vec![proto];
        let predicates = RandomCachePredicates::new();
        let ctx = RandomCacheBuildCtx {
            prototypes: &protos,
            predicates: &predicates,
        };
        let cache = build_random_cache(&ctx);
        let index = build_index(&protos);

        // Query at level 21 -> (21 - 1) / 10 = 2 → hit.
        let result = query_random_cache(&cache, 21, RandomItemType::GuildTask, None, &index);
        assert_eq!(result, vec![400]);

        // Query at level 20 -> (20 - 1) / 10 = 1 → miss (bucket 1 empty).
        let result = query_random_cache(&cache, 20, RandomItemType::GuildTask, None, &index);
        assert!(result.is_empty());
    }

    #[test]
    fn query_applies_runtime_predicate() {
        let protos = vec![
            sensible_proto(500, 25),
            sensible_proto(501, 25),
            sensible_proto(502, 25),
        ];
        let predicates = RandomCachePredicates::new();
        let ctx = RandomCacheBuildCtx {
            prototypes: &protos,
            predicates: &predicates,
        };
        let cache = build_random_cache(&ctx);
        let index = build_index(&protos);

        let even = OnlyEvenIds;
        let result = query_random_cache(
            &cache,
            21,
            RandomItemType::GuildTask,
            Some(&even),
            &index,
        );
        assert_eq!(result, vec![500, 502]);
    }

    #[test]
    fn get_random_item_returns_zero_for_empty_bucket() {
        let cache: RandomItemCache = BTreeMap::new();
        let index: HashMap<u32, &BotItemPrototype> = HashMap::new();
        let world = MockItemWorld::new();
        let id = get_random_item(
            &cache,
            &world,
            21,
            RandomItemType::GuildTask,
            None,
            &index,
        );
        assert_eq!(id, 0);
    }

    #[test]
    fn get_random_item_picks_via_urand() {
        let protos = vec![
            sensible_proto(600, 25),
            sensible_proto(601, 25),
            sensible_proto(602, 25),
        ];
        let predicates = RandomCachePredicates::new();
        let ctx = RandomCacheBuildCtx {
            prototypes: &protos,
            predicates: &predicates,
        };
        let cache = build_random_cache(&ctx);
        let index = build_index(&protos);
        let world = MockItemWorld::new();
        let id = get_random_item(
            &cache,
            &world,
            21,
            RandomItemType::GuildTask,
            None,
            &index,
        );
        assert!([600, 601, 602].contains(&id), "got {id}");
    }
}
