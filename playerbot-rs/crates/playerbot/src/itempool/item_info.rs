//! `BuildItemInfoCache` — Rust port of `RandomItemMgr::BuildItemInfoCache`
//! (`RandomItemMgr.cpp` lines 855-1484).
//!
//! This is the big per-item pre-computation loop the legacy module runs
//! during `Init`. For every prototype in `sItemStorage` it:
//!
//! 1. Skips non-gear classes (must be weapon/armor/container/projectile).
//! 2. Skips items whose name contains "(Test)", "Deprecated ", "[PH]", …
//! 3. Resolves the item's equipment slot via [`slots::slot_accepts`].
//! 4. Determines the item's faction (race-mask ↔ `BaseRepRaceMask` + the
//!    `ITEM_FLAG2_FACTION_*` bits on `WotLK`).
//! 5. Resolves the item's source: `PvP` reward → quest → vendor → drop.
//! 6. Records reputation / honor-rank / required-skill gates.
//! 7. For every `(class, spec)` that can use the item, runs
//!    [`stat_weight::calculate_stat_weight`] and applies a long list of
//!    post-processing rules (legendary x2, plate ≥ 40 filter, trinket
//!    fallback, wand fallback, random-property fallback).
//!
//! The result is [`ItemInfoCache`] — a `HashMap<u32, ItemInfoEntry>`
//! that downstream passes (`BuildEquipCache`, `GetUpgrade`,
//! `GetLiveStatWeight`) consume.
//!
//! # Differences from the C++
//!
//! * The legacy `BuildItemInfoCache` also writes every row back into
//!   `ai_playerbot_item_info_cache`. That DB write belongs to the C++
//!   bridge and is not part of this module — callers are expected to
//!   persist the result themselves if they want the loaded path.
//! * The C++ drop-source logic runs against a `dropMap` built from
//!   `LootTemplateAccess`, which was removed when the C++ strategy
//!   engine was ripped out. The Rust port accepts an optional
//!   pre-computed drop map; empty means "no drop-source data" which
//!   matches the current C++ behaviour (`dropMap->size() == 0`).
//! * Vendor-team detection in the legacy code uses a mix of the
//!   `creature_template.faction` raw id and a set of hardcoded
//!   alliance/horde vendor templates. The Rust port uses the
//!   `BotVendorItemRow::vendor_faction` field directly when present.

use std::collections::{HashMap, HashSet};

use cmangos::{BotItemPrototype, BotQuestItemRow, BotVendorItemRow, ItemWorld};

use super::equip_filter::{
    armor_sub, can_equip_item_new, class, item_class, should_equip_armor_for_spec,
    should_equip_weapon_for_spec,
};
use super::slots::{self, slot};
use super::stat_weight::{calculate_stat_weight, StatWeightCtx};
use super::types::{ItemInfoEntry, ItemSource, ItemSpecType, WeightScale};

// ── Constants mirroring the C++ flags ─────────────────────────────────────

/// `ITEM_FLAG_NO_DISENCHANT` — the PvP-reward marker the legacy loop
/// inspects at line 1110.
const ITEM_FLAG_NO_DISENCHANT: u32 = 0x0000_2000;

/// WotLK-only `ITEM_FLAG2_FACTION_HORDE`.
#[cfg(feature = "wotlk")]
const ITEM_FLAG2_FACTION_HORDE: u32 = 0x0000_0001;
/// WotLK-only `ITEM_FLAG2_FACTION_ALLIANCE`.
#[cfg(feature = "wotlk")]
const ITEM_FLAG2_FACTION_ALLIANCE: u32 = 0x0000_0002;

/// `ItemQuality::ITEM_QUALITY_UNCOMMON` — lower quality items get a
/// stat-weight floor of 1 at low levels.
const ITEM_QUALITY_UNCOMMON: u32 = 2;
/// `ItemQuality::ITEM_QUALITY_LEGENDARY` — qualifies for the 2× bonus.
const ITEM_QUALITY_LEGENDARY: u32 = 5;
/// `ItemQuality::ITEM_QUALITY_HEIRLOOM` — WotLK-only skip.
#[cfg(feature = "wotlk")]
const ITEM_QUALITY_HEIRLOOM: u32 = 7;

/// Matches the legacy `MAX_STAT_SCALES` — the weight-scale id range is
/// `1..=32`.
pub const MAX_STAT_SCALES: u32 = 32;

/// Simple team tag used by the info cache. The C++ uses the cmangos
/// `Team` enum (`ALLIANCE = 469`, `HORDE = 67`, `TEAM_BOTH_ALLOWED = 3`)
/// but the values are opaque — they only need to round-trip within the
/// Rust port, so we use small sequential ids here.
pub mod team {
    pub const NONE: u32 = 0;
    pub const ALLIANCE: u32 = 1;
    pub const HORDE: u32 = 2;
    pub const BOTH: u32 = 3;
}

/// Race-mask constants used by the faction-determination logic.
/// Values come from `cmangos/src/game/SharedDefines.h`.
pub mod race_mask {
    const fn bit(race: u32) -> u32 {
        1 << (race - 1)
    }
    const HUMAN: u32 = bit(1);
    const ORC: u32 = bit(2);
    const DWARF: u32 = bit(3);
    const NIGHTELF: u32 = bit(4);
    const UNDEAD: u32 = bit(5);
    const TAUREN: u32 = bit(6);
    const GNOME: u32 = bit(7);
    const TROLL: u32 = bit(8);
    #[cfg(not(feature = "vanilla"))]
    const BLOODELF: u32 = bit(10);
    #[cfg(not(feature = "vanilla"))]
    const DRAENEI: u32 = bit(11);

    #[cfg(feature = "vanilla")]
    pub const ALLIANCE: u32 = HUMAN | DWARF | NIGHTELF | GNOME;
    #[cfg(feature = "vanilla")]
    pub const HORDE: u32 = ORC | UNDEAD | TAUREN | TROLL;

    #[cfg(not(feature = "vanilla"))]
    pub const ALLIANCE: u32 = HUMAN | DWARF | NIGHTELF | GNOME | DRAENEI;
    #[cfg(not(feature = "vanilla"))]
    pub const HORDE: u32 = ORC | UNDEAD | TAUREN | TROLL | BLOODELF;
}

/// Bitmask the legacy C++ uses in `CLASSMASK_ALL_PLAYABLE`. Classic
/// excludes death knight; TBC+ still excludes DK (added in `WotLK`).
#[cfg(feature = "wotlk")]
const CLASSMASK_ALL_PLAYABLE: u32 = 0x5FF;
#[cfg(not(feature = "wotlk"))]
const CLASSMASK_ALL_PLAYABLE: u32 = 0x5DF; // bit 6 (death knight) cleared

// ── Name filter ───────────────────────────────────────────────────────────

/// Substrings the legacy loop matches at line 998-1015 against
/// `proto.Name1` to reject test/placeholder/deprecated prototypes.
const SKIP_NAME_SUBSTRINGS: &[&str] = &[
    "(Test)",
    "(TEST)",
    "(test)",
    "(JEFFTEST)",
    "Test ",
    "Test",
    "TEST",
    "TEST ",
    " TEST",
    "2200 ",
    "Deprecated ",
    "Unused ",
    "Monster ",
    "[PH]",
    "(OLD)",
    "zzz",
    "ZZZ",
];

/// True when `proto.name` contains any of the [`SKIP_NAME_SUBSTRINGS`].
/// Matches the legacy `strstr` chain at lines 998-1015.
#[must_use]
pub fn is_test_or_placeholder(proto: &BotItemPrototype) -> bool {
    let name = proto_name(proto);
    SKIP_NAME_SUBSTRINGS
        .iter()
        .any(|needle| name.contains(needle))
}

/// Decode `proto.name` (nul-terminated `c_char` buffer) into a borrowed
/// `&str`. Invalid UTF-8 is replaced with an empty string, matching the
/// legacy `strstr` which would also fail to find anything useful in a
/// mangled name.
fn proto_name(proto: &BotItemPrototype) -> &str {
    let bytes = c_char_slice_as_bytes(&proto.name);
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..end]).unwrap_or("")
}

/// View a `[c_char; N]` as `&[u8]`. `c_char` is either i8 or u8
/// depending on target; both share size/alignment so the reinterpret is
/// always sound for a POD input.
#[allow(unsafe_code)]
fn c_char_slice_as_bytes(buf: &[core::ffi::c_char]) -> &[u8] {
    // Safety: `c_char` is either i8 or u8 — both 1-byte, aligned to 1.
    // The lifetime is preserved.
    unsafe { core::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), buf.len()) }
}

// ── Team classification ───────────────────────────────────────────────────

/// Determine an item's faction gating from the prototype's
/// `allowable_race` bitmask and (on `WotLK`) the `flags2` faction bits.
///
/// Returns [`team::NONE`] when the item has no faction preference.
///
/// # Semantics
///
/// * WotLK-only `ITEM_FLAG2_FACTION_HORDE` / `..._ALLIANCE` win over
///   the race-mask check.
/// * A race-mask value of `0` or `>= 8_388_607` (≈`0x7F_FFFF`) is
///   treated as "any race" — matches the `> 1 && < 8388607` gate in
///   the legacy code.
/// * Otherwise any overlap with `RACEMASK_ALLIANCE` / `RACEMASK_HORDE`
///   picks the corresponding team. Both hit → `team::BOTH`.
#[must_use]
pub fn classify_team(proto: &BotItemPrototype) -> u32 {
    #[cfg(feature = "wotlk")]
    {
        if (proto.flags2 & ITEM_FLAG2_FACTION_HORDE) != 0 {
            return team::HORDE;
        }
        if (proto.flags2 & ITEM_FLAG2_FACTION_ALLIANCE) != 0 {
            return team::ALLIANCE;
        }
    }

    let race_mask = proto.allowable_race as u32;
    if race_mask <= 1 || race_mask >= 8_388_607 {
        return team::NONE;
    }

    let hits_ally = (race_mask & race_mask::ALLIANCE) != 0;
    let hits_horde = (race_mask & race_mask::HORDE) != 0;

    match (hits_ally, hits_horde) {
        (true, true) => team::BOTH,
        (true, false) => team::ALLIANCE,
        (false, true) => team::HORDE,
        (false, false) => team::NONE,
    }
}

// ── Source classification ────────────────────────────────────────────────

/// Result of determining an item's provenance. The C++ stores these on
/// `ItemInfoEntry`; we return them as a struct so callers can thread
/// them through without relying on an exact field layout.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceTeamInfo {
    pub source: ItemSource,
    pub source_ids: Vec<u32>,
    pub team: u32,
    pub min_level: u32,
    pub rep_faction: u32,
    pub rep_rank: u32,
}

/// Inputs to [`classify_source`]. A small struct so the caller doesn't
/// need to re-build the per-item quest/vendor slice on every iteration
/// (the manager builds both lookups once up front).
pub struct SourceClassifier<'a> {
    pub quests_by_item: &'a HashMap<u32, Vec<&'a BotQuestItemRow>>,
    pub vendors_by_item: &'a HashMap<u32, Vec<&'a BotVendorItemRow>>,
    /// Classic-only alliance PvP-vendor item list. Empty elsewhere.
    pub alliance_pvp_items: &'a HashSet<u32>,
    /// Classic-only horde PvP-vendor item list. Empty elsewhere.
    pub horde_pvp_items: &'a HashSet<u32>,
    /// Creature drop list per item id. Empty means the drop-source
    /// pass is skipped (matches the current C++ which stubs the drop
    /// map out entirely).
    pub creature_drops: &'a HashMap<u32, Vec<u32>>,
    /// Gameobject drop list per item id.
    pub gameobject_drops: &'a HashMap<u32, Vec<u32>>,
}

impl<'a> SourceClassifier<'a> {
    /// Empty classifier — all lookups will miss. Useful for tests and
    /// for the stubbed drop-map case.
    #[must_use]
    pub fn empty() -> Self {
        // Leaking a zero-sized box would complicate the lifetime story;
        // callers that want a trivial classifier can build their own.
        // This stub is only for documentation / unit tests.
        Self {
            quests_by_item: Box::leak(Box::default()),
            vendors_by_item: Box::leak(Box::default()),
            alliance_pvp_items: Box::leak(Box::default()),
            horde_pvp_items: Box::leak(Box::default()),
            creature_drops: Box::leak(Box::default()),
            gameobject_drops: Box::leak(Box::default()),
        }
    }
}

/// Determine an item's provenance — `PvP` → Quest → Vendor → Drop, in
/// priority order matching the legacy `BuildItemInfoCache` flow.
///
/// `initial_team` is the faction pre-classification from
/// [`classify_team`]; the function may refine it (e.g. a quest that
/// requires Alliance races, or a vendor in Orgrimmar) but never
/// weakens an existing team tag.
///
/// `required_rep_*` come from `proto.required_reputation_*`; they are
/// used as fallbacks when neither the quest nor the vendor supplies a
/// reputation gate.
#[must_use]
pub fn classify_source(
    proto: &BotItemPrototype,
    classifier: &SourceClassifier<'_>,
    initial_team: u32,
    initial_min_level: u32,
) -> SourceTeamInfo {
    let item_id = proto.item_id;
    let mut info = SourceTeamInfo {
        source: ItemSource::None,
        source_ids: Vec::new(),
        team: initial_team,
        min_level: initial_min_level,
        rep_faction: 0,
        rep_rank: 0,
    };

    // Vanilla PvP-vendor hardcodes: entries on the classic Arathi Basin
    // / Warsong Gulch / Alterac Valley alliance+horde vendors are
    // faction-locked.
    if classifier.alliance_pvp_items.contains(&item_id) {
        info.team = team::ALLIANCE;
    }
    if classifier.horde_pvp_items.contains(&item_id) {
        info.team = team::HORDE;
    }

    // 1. PvP reward: the `NO_DISENCHANT` flag short-circuits source.
    if (proto.flags & ITEM_FLAG_NO_DISENCHANT) != 0 {
        info.source = ItemSource::Pvp;
    }

    // 2. Quest rewards.
    if matches!(info.source, ItemSource::None | ItemSource::Pvp)
        && let Some(rows) = classifier.quests_by_item.get(&item_id)
        && !rows.is_empty()
    {
        info.source = ItemSource::Quest;
        let mut is_ally = false;
        let mut is_horde = false;
        for row in rows {
            info.source_ids.push(row.quest_id);
            if info.min_level == 0 && row.min_level > 0 {
                info.min_level = row.min_level;
            }
            if info.team == team::NONE {
                let rr = row.required_races;
                if (rr & race_mask::ALLIANCE) != 0 {
                    is_ally = true;
                }
                if (rr & race_mask::HORDE) != 0 {
                    is_horde = true;
                }
            }
            if row.reward_rep_faction != 0 {
                info.rep_faction = row.reward_rep_faction;
                // reward_rep_value is the raw rep award amount;
                // the legacy code maps it back to a rank by
                // walking `PointsInRank[r]` which the bridge
                // pre-computes into the row. Use the absolute
                // value when positive (a negative reward is a
                // loss and should not gate equipment).
                if row.reward_rep_value > 0 {
                    info.rep_rank = row.reward_rep_value as u32;
                }
            }
        }
        if info.team == team::NONE {
            info.team = match (is_ally, is_horde) {
                (true, true) => team::BOTH,
                (true, false) => team::ALLIANCE,
                (false, true) => team::HORDE,
                (false, false) => team::NONE,
            };
        }
    }

    // 3. Vendors.
    if matches!(info.source, ItemSource::None | ItemSource::Pvp)
        && let Some(rows) = classifier.vendors_by_item.get(&item_id)
        && !rows.is_empty()
    {
        info.source = ItemSource::Vendor;
        let mut is_ally = false;
        let mut is_horde = false;
        for row in rows {
            info.source_ids.push(row.vendor_entry);
            // `vendor_faction` is the cmangos `FactionTemplate`
            // id. The legacy C++ hardcodes a few classic
            // faction ids (12/55/79/84 = alliance,
            // 29/68/71/85 = horde). Match that table verbatim.
            match row.vendor_faction {
                12 | 55 | 79 | 84 => is_ally = true,
                29 | 68 | 71 | 85 => is_horde = true,
                0 => { /* no faction data */ }
                _ => {
                    is_ally = true;
                    is_horde = true;
                }
            }
            if row.required_reputation_faction != 0 {
                info.rep_faction = row.required_reputation_faction;
                info.rep_rank = row.required_reputation_rank;
            }
        }
        if info.team == team::NONE {
            info.team = match (is_ally, is_horde) {
                (true, true) => team::BOTH,
                (true, false) => team::ALLIANCE,
                (false, true) => team::HORDE,
                (false, false) => team::NONE,
            };
        }
    }

    // 4. Drops — both creature and gameobject. The legacy code prefers
    //    creature over gameobject when both exist, and records a single
    //    source id when there's exactly one drop source.
    if matches!(info.source, ItemSource::None)
        && let Some(creatures) = classifier.creature_drops.get(&item_id)
        && !creatures.is_empty()
    {
        info.source = ItemSource::Drop;
        if creatures.len() == 1 {
            info.source_ids.push(creatures[0]);
        }
    }
    if matches!(info.source, ItemSource::None)
        && let Some(gameobjects) = classifier.gameobject_drops.get(&item_id)
        && !gameobjects.is_empty()
    {
        info.source = ItemSource::Drop;
        if gameobjects.len() == 1 {
            info.source_ids.push(gameobjects[0]);
        }
    }

    // Proto-level reputation / honor gates carry over.
    if proto.required_reputation_faction > 0
        && proto.required_reputation_faction != 35
        && proto.required_reputation_rank < 15
    {
        info.rep_faction = proto.required_reputation_faction;
        info.rep_rank = proto.required_reputation_rank;
    }

    info
}

// ── Per-spec stat weight with fallbacks ──────────────────────────────────

/// Apply the long list of post-`CalculateStatWeight` fallbacks to a raw
/// weight. Mirrors the section at lines 1398-1435 in the C++.
///
/// `slot_id` is the [`slots::slot`] value the item was assigned to;
/// `player_class` and `spec` come from the outer loop.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn apply_weight_fallbacks(
    mut stat_w: u32,
    proto: &BotItemPrototype,
    slot_id: u8,
    player_class: u8,
    spec: u32,
    quality: u32,
    min_level: u32,
) -> u32 {
    // Low-level greens/whites get a weight of 1 to avoid being culled.
    if stat_w == 0 && quality < ITEM_QUALITY_UNCOMMON && min_level <= 10 {
        stat_w = 1;
    }

    // Legendary bonus — only when already positive (legacy skips 0/1).
    if proto.quality >= ITEM_QUALITY_LEGENDARY && stat_w > 1 {
        stat_w = stat_w.saturating_mul(2);
    }

    // Body/tabard always keep a minimum weight of 1.
    if slot_id == slot::BODY && stat_w == 0 {
        stat_w = 1;
    }
    if slot_id == slot::TABARD && stat_w == 0 {
        stat_w = 1;
    }

    // Warriors never want mail at level 40+ (they get plate).
    if proto.sub_class == armor_sub::MAIL
        && min_level >= 40
        && player_class == class::WARRIOR
    {
        stat_w = 0;
    }
    // Paladin tank/dps spec filter: spec 4 is holy (keeps mail for
    // leveling); every other paladin spec gets plate-only at 40+.
    if proto.sub_class == armor_sub::MAIL
        && min_level >= 40
        && player_class == class::PALADIN
        && spec != 4
    {
        stat_w = 0;
    }

    // Trinkets with no stats but an equip spell keep their itemLevel as
    // an opinion — better than nothing.
    if (slot_id == slot::TRINKET1 || slot_id == slot::TRINKET2)
        && stat_w == 0
        && proto.allowable_class as u32 == u32::from(player_class)
        && proto.spells[0].spell_id != 0
    {
        stat_w = proto.quality + proto.item_level;
    }

    // Wands deserve any amount of love for their casters.
    if stat_w == 0
        && slot_id == slot::RANGED
        && proto.sub_class == super::equip_filter::weapon_sub::WAND
        && matches!(
            player_class,
            class::PRIEST | class::MAGE | class::WARLOCK
        )
    {
        stat_w = 1;
    }

    // Items with random enchants have a non-deterministic stat profile;
    // keep them in the pool with a minimum weight of 1.
    if stat_w == 0 && proto.random_property != 0 {
        stat_w = 1;
    }

    stat_w
}

/// Compute the stat weight for a single `(class, spec)` × item pair,
/// including all fallbacks. Returns `None` when the item is filtered
/// out of the spec entirely (e.g. plate on a rogue).
#[allow(clippy::too_many_arguments)]
pub fn weight_for_spec(
    world: &dyn ItemWorld,
    proto: &BotItemPrototype,
    slot_id: u8,
    player_class: u8,
    spec: u32,
    scale: &WeightScale,
    whitelist: &[u32],
    min_level: u32,
) -> Option<(u32, ItemSpecType)> {
    // Resolve the spec *name* once — the equip-filter helpers key on the
    // name (`"arms"`, `"holy"`, …) because they pre-date the numeric
    // weight-scale id table.
    let spec_name: &str = scale.info.name.as_str();

    // Armor slots: the spec must allow this armor subclass at all.
    let is_armor_slot = matches!(
        slot_id,
        slot::HEAD
            | slot::SHOULDERS
            | slot::CHEST
            | slot::WAIST
            | slot::LEGS
            | slot::FEET
            | slot::WRISTS
            | slot::HANDS
    );
    if proto.class_id == item_class::ARMOR
        && is_armor_slot
        && !should_equip_armor_for_spec(player_class, spec_name, true, proto)
    {
        return None;
    }

    // Weapon/shield/holdable: same spec gate.
    let is_weaponish = proto.class_id == item_class::WEAPON
        || (proto.class_id == item_class::ARMOR
            && (proto.sub_class == armor_sub::SHIELD
                || (proto.sub_class == armor_sub::MISC
                    && proto.inventory_type == u32::from(slots::inv::HOLDABLE))));
    if is_weaponish && !should_equip_weapon_for_spec(player_class, spec_name, true, proto) {
        return None;
    }

    let ctx = StatWeightCtx {
        player_class,
        spec_id: spec,
        scale,
        whitelist,
        world,
    };
    let result = calculate_stat_weight(&ctx, proto);
    let adjusted = apply_weight_fallbacks(
        result.weight,
        proto,
        slot_id,
        player_class,
        spec,
        proto.quality,
        min_level,
    );

    Some((adjusted, result.item_spec))
}

// ── build_item_info_cache ────────────────────────────────────────────────

/// Bundle of inputs the manager threads into the cache builder. Keeping
/// it as a struct means the function signature doesn't balloon when
/// future passes (glyph cache, ammo cache) need more context.
pub struct ItemInfoBuildCtx<'a> {
    pub world: &'a dyn ItemWorld,
    pub prototypes: &'a [BotItemPrototype],
    pub scales: &'a [WeightScale],
    pub classifier: &'a SourceClassifier<'a>,
    pub whitelist: &'a [u32],
}

/// Alias for the top-level cache type so callers don't re-type the
/// generics everywhere.
pub type ItemInfoCache = HashMap<u32, ItemInfoEntry>;

/// Main entry point — mirrors `RandomItemMgr::BuildItemInfoCache` in
/// purpose. Returns the populated cache keyed by item id.
#[must_use]
pub fn build_item_info_cache(ctx: &ItemInfoBuildCtx<'_>) -> ItemInfoCache {
    // Index the scales by id once for O(1) lookups below. The spec id
    // used by the weight-scale table is 1..=32 so we size accordingly.
    let mut scale_by_id: HashMap<u32, &WeightScale> = HashMap::with_capacity(ctx.scales.len());
    for scale in ctx.scales {
        scale_by_id.insert(scale.info.id, scale);
    }

    let mut cache: ItemInfoCache = HashMap::with_capacity(ctx.prototypes.len());

    for proto in ctx.prototypes {
        // Filter non-gear classes.
        if proto.class_id != item_class::WEAPON
            && proto.class_id != item_class::ARMOR
            && proto.class_id != item_class::CONTAINER
            && proto.class_id != item_class::PROJECTILE
        {
            continue;
        }
        if !can_equip_item_new(proto) {
            continue;
        }
        if is_test_or_placeholder(proto) {
            continue;
        }
        #[cfg(feature = "wotlk")]
        {
            if proto.quality == ITEM_QUALITY_HEIRLOOM {
                continue;
            }
        }

        // Resolve equipment slot. The legacy code picks the *last*
        // viable slot in the iteration order of `viableSlots` — an
        // implementation detail of `std::map<EquipmentSlots, ...>`.
        // `slots::for_each_slot_inv` yields slots in sorted order so
        // we always get the same result for identical iteration.
        let mut slot_id: Option<u8> = None;
        slots::for_each_slot_inv(|s, iv| {
            if u32::from(iv) == proto.inventory_type {
                slot_id = Some(s);
            }
        });
        let Some(slot_id) = slot_id else { continue };

        // Base entry.
        let mut entry = ItemInfoEntry::new(proto.item_id);
        entry.quality = proto.quality;
        entry.slot = u32::from(slot_id);
        entry.item_level = proto.item_level;
        if proto.required_level != 0 {
            entry.min_level = proto.required_level;
        }
        if proto.required_honor_rank > 0 {
            entry.pvp_rank = proto.required_honor_rank;
        }
        if proto.required_skill > 0 {
            entry.req_skill = proto.required_skill;
            entry.req_skill_rank = proto.required_skill_rank;
        }

        // Faction + source classification.
        let initial_team = classify_team(proto);
        let src = classify_source(proto, ctx.classifier, initial_team, entry.min_level);
        entry.source = src.source.as_u32();
        entry.source_ids = src.source_ids;
        entry.team = src.team;
        entry.min_level = src.min_level;
        entry.rep_faction = src.rep_faction;
        entry.rep_rank = src.rep_rank;

        // Per-(class, spec) stat weights.
        let mut aggregate_spec = ItemSpecType::NONE;
        for clazz in 1u8..=11u8 {
            let class_bit = 1u32 << (u32::from(clazz).saturating_sub(1));
            if (class_bit & CLASSMASK_ALL_PLAYABLE) == 0 {
                continue;
            }
            if (proto.allowable_class & (class_bit as i32)) == 0 {
                continue;
            }

            for spec_id in 1u32..=MAX_STAT_SCALES {
                let Some(scale) = scale_by_id.get(&spec_id) else {
                    continue;
                };
                if u32::from(scale.info.class_id) != u32::from(clazz) {
                    continue;
                }

                if let Some((weight, item_spec)) = weight_for_spec(
                    ctx.world,
                    proto,
                    slot_id,
                    clazz,
                    spec_id,
                    scale,
                    ctx.whitelist,
                    entry.min_level,
                ) {
                    aggregate_spec |= item_spec;
                    entry.weights.insert(spec_id, weight);
                }
            }
        }
        entry.item_spec = aggregate_spec;

        cache.insert(proto.item_id, entry);
    }

    cache
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itempool::types::{WeightScaleInfo, WeightScaleStat};
    use cmangos::MockItemWorld;

    fn scale(id: u32, name: &str, class_id: u8, stats: &[(&str, u32)]) -> WeightScale {
        WeightScale {
            info: WeightScaleInfo {
                id,
                name: name.to_string(),
                class_id,
            },
            stats: stats
                .iter()
                .map(|(s, w)| WeightScaleStat {
                    stat: (*s).to_string(),
                    weight: *w,
                })
                .collect(),
        }
    }

    fn ascii_name(out: &mut [core::ffi::c_char; 96], s: &str) {
        for (i, b) in s.as_bytes().iter().enumerate().take(out.len() - 1) {
            out[i] = *b as core::ffi::c_char;
        }
    }

    fn empty_classifier() -> SourceClassifier<'static> {
        SourceClassifier::empty()
    }

    #[test]
    fn is_test_or_placeholder_matches_substrings() {
        let mut proto = BotItemPrototype::default();
        ascii_name(&mut proto.name, "Sword of (Test) Doom");
        assert!(is_test_or_placeholder(&proto));

        ascii_name(&mut proto.name, "[PH] Gnomish Gadget");
        assert!(is_test_or_placeholder(&proto));

        ascii_name(&mut proto.name, "Deprecated Wand");
        assert!(is_test_or_placeholder(&proto));

        ascii_name(&mut proto.name, "Thunderfury, Blessed Blade");
        assert!(!is_test_or_placeholder(&proto));
    }

    #[test]
    fn classify_team_race_mask_alliance() {
        let proto = BotItemPrototype {
            allowable_race: race_mask::ALLIANCE as i32,
            ..BotItemPrototype::default()
        };
        assert_eq!(classify_team(&proto), team::ALLIANCE);
    }

    #[test]
    fn classify_team_race_mask_horde() {
        let proto = BotItemPrototype {
            allowable_race: race_mask::HORDE as i32,
            ..BotItemPrototype::default()
        };
        assert_eq!(classify_team(&proto), team::HORDE);
    }

    #[test]
    fn classify_team_race_mask_both() {
        let proto = BotItemPrototype {
            allowable_race: (race_mask::ALLIANCE | race_mask::HORDE) as i32,
            ..BotItemPrototype::default()
        };
        assert_eq!(classify_team(&proto), team::BOTH);
    }

    #[test]
    fn classify_team_unrestricted_returns_none() {
        let proto = BotItemPrototype {
            allowable_race: -1,
            ..BotItemPrototype::default()
        };
        assert_eq!(classify_team(&proto), team::NONE);
    }

    #[cfg(feature = "wotlk")]
    #[test]
    fn classify_team_flags2_wins_over_race_mask() {
        let proto = BotItemPrototype {
            allowable_race: race_mask::ALLIANCE as i32,
            flags2: ITEM_FLAG2_FACTION_HORDE,
            ..BotItemPrototype::default()
        };
        assert_eq!(classify_team(&proto), team::HORDE);
    }

    #[test]
    fn classify_source_pvp_flag_wins() {
        let proto = BotItemPrototype {
            item_id: 42,
            flags: ITEM_FLAG_NO_DISENCHANT,
            ..BotItemPrototype::default()
        };
        let classifier = empty_classifier();
        let info = classify_source(&proto, &classifier, team::NONE, 0);
        assert_eq!(info.source, ItemSource::Pvp);
    }

    #[test]
    fn classify_source_quest_reward_uses_min_level_and_team() {
        let quest_row = BotQuestItemRow {
            item_id: 10,
            quest_id: 9001,
            min_level: 15,
            required_classes: 0,
            required_races: race_mask::HORDE,
            zone_or_sort: 0,
            reward_rep_faction: 0,
            reward_rep_value: 0,
            is_choice: false,
        };
        // Build owned storage and references that live through the call.
        let mut owned_quests: HashMap<u32, Vec<&BotQuestItemRow>> = HashMap::new();
        owned_quests.insert(10, vec![&quest_row]);
        let owned_vendors: HashMap<u32, Vec<&BotVendorItemRow>> = HashMap::new();
        let owned_ally: HashSet<u32> = HashSet::new();
        let owned_horde: HashSet<u32> = HashSet::new();
        let owned_cdrops: HashMap<u32, Vec<u32>> = HashMap::new();
        let owned_gdrops: HashMap<u32, Vec<u32>> = HashMap::new();

        let classifier = SourceClassifier {
            quests_by_item: &owned_quests,
            vendors_by_item: &owned_vendors,
            alliance_pvp_items: &owned_ally,
            horde_pvp_items: &owned_horde,
            creature_drops: &owned_cdrops,
            gameobject_drops: &owned_gdrops,
        };

        let proto = BotItemPrototype {
            item_id: 10,
            ..BotItemPrototype::default()
        };
        let info = classify_source(&proto, &classifier, team::NONE, 0);
        assert_eq!(info.source, ItemSource::Quest);
        assert_eq!(info.source_ids, vec![9001]);
        assert_eq!(info.min_level, 15);
        assert_eq!(info.team, team::HORDE);
    }

    #[test]
    fn classify_source_vendor_faction_resolves_alliance() {
        let vendor_row = BotVendorItemRow {
            vendor_entry: 555,
            item_id: 20,
            vendor_faction_template: 12, // alliance
            vendor_faction: 12,
            required_reputation_rank: 0,
            required_reputation_faction: 0,
            condition_id: 0,
        };
        let owned_quests: HashMap<u32, Vec<&BotQuestItemRow>> = HashMap::new();
        let mut owned_vendors: HashMap<u32, Vec<&BotVendorItemRow>> = HashMap::new();
        owned_vendors.insert(20, vec![&vendor_row]);
        let owned_ally: HashSet<u32> = HashSet::new();
        let owned_horde: HashSet<u32> = HashSet::new();
        let owned_cdrops: HashMap<u32, Vec<u32>> = HashMap::new();
        let owned_gdrops: HashMap<u32, Vec<u32>> = HashMap::new();

        let classifier = SourceClassifier {
            quests_by_item: &owned_quests,
            vendors_by_item: &owned_vendors,
            alliance_pvp_items: &owned_ally,
            horde_pvp_items: &owned_horde,
            creature_drops: &owned_cdrops,
            gameobject_drops: &owned_gdrops,
        };

        let proto = BotItemPrototype {
            item_id: 20,
            ..BotItemPrototype::default()
        };
        let info = classify_source(&proto, &classifier, team::NONE, 0);
        assert_eq!(info.source, ItemSource::Vendor);
        assert_eq!(info.team, team::ALLIANCE);
        assert_eq!(info.source_ids, vec![555]);
    }

    #[test]
    fn classify_source_creature_drop_single_records_id() {
        let owned_quests: HashMap<u32, Vec<&BotQuestItemRow>> = HashMap::new();
        let owned_vendors: HashMap<u32, Vec<&BotVendorItemRow>> = HashMap::new();
        let owned_ally: HashSet<u32> = HashSet::new();
        let owned_horde: HashSet<u32> = HashSet::new();
        let mut owned_cdrops: HashMap<u32, Vec<u32>> = HashMap::new();
        owned_cdrops.insert(33, vec![7777]);
        let owned_gdrops: HashMap<u32, Vec<u32>> = HashMap::new();

        let classifier = SourceClassifier {
            quests_by_item: &owned_quests,
            vendors_by_item: &owned_vendors,
            alliance_pvp_items: &owned_ally,
            horde_pvp_items: &owned_horde,
            creature_drops: &owned_cdrops,
            gameobject_drops: &owned_gdrops,
        };

        let proto = BotItemPrototype {
            item_id: 33,
            ..BotItemPrototype::default()
        };
        let info = classify_source(&proto, &classifier, team::NONE, 0);
        assert_eq!(info.source, ItemSource::Drop);
        assert_eq!(info.source_ids, vec![7777]);
    }

    #[test]
    fn apply_weight_fallbacks_low_quality_low_level_gets_one() {
        let proto = BotItemPrototype::default();
        let w = apply_weight_fallbacks(0, &proto, slot::HEAD, 1, 1, 1, 5);
        assert_eq!(w, 1);
    }

    #[test]
    fn apply_weight_fallbacks_legendary_doubles() {
        let proto = BotItemPrototype {
            quality: ITEM_QUALITY_LEGENDARY,
            ..BotItemPrototype::default()
        };
        let w = apply_weight_fallbacks(50, &proto, slot::MAINHAND, 1, 1, 5, 60);
        assert_eq!(w, 100);
    }

    #[test]
    fn apply_weight_fallbacks_body_gets_floor_of_one() {
        let proto = BotItemPrototype::default();
        let w = apply_weight_fallbacks(0, &proto, slot::BODY, 1, 1, 3, 20);
        assert_eq!(w, 1);
    }

    #[test]
    fn apply_weight_fallbacks_warrior_mail_above_40_zero() {
        let proto = BotItemPrototype {
            sub_class: armor_sub::MAIL,
            ..BotItemPrototype::default()
        };
        let w = apply_weight_fallbacks(100, &proto, slot::CHEST, class::WARRIOR, 1, 3, 45);
        assert_eq!(w, 0);
    }

    #[test]
    fn apply_weight_fallbacks_paladin_holy_keeps_mail() {
        let proto = BotItemPrototype {
            sub_class: armor_sub::MAIL,
            ..BotItemPrototype::default()
        };
        // spec 4 = holy paladin (per legacy filter), keeps mail.
        let w = apply_weight_fallbacks(100, &proto, slot::CHEST, class::PALADIN, 4, 3, 45);
        assert_eq!(w, 100);
        // Any other paladin spec → zero.
        let w = apply_weight_fallbacks(100, &proto, slot::CHEST, class::PALADIN, 3, 3, 45);
        assert_eq!(w, 0);
    }

    #[test]
    fn apply_weight_fallbacks_random_property_gets_floor() {
        let proto = BotItemPrototype {
            random_property: 42,
            ..BotItemPrototype::default()
        };
        let w = apply_weight_fallbacks(0, &proto, slot::CHEST, 1, 1, 3, 60);
        assert_eq!(w, 1);
    }

    #[test]
    fn build_item_info_cache_skips_non_gear() {
        // A consumable prototype — must be filtered out.
        let mut consumable = BotItemPrototype::default();
        consumable.item_id = 1;
        consumable.class_id = 0; // ITEM_CLASS_CONSUMABLE
        ascii_name(&mut consumable.name, "Major Healing Potion");

        let classifier = empty_classifier();
        let scales = vec![scale(1, "arms", class::WARRIOR, &[("str", 3)])];
        let world = MockItemWorld::new();

        let ctx = ItemInfoBuildCtx {
            world: &world,
            prototypes: &[consumable],
            scales: &scales,
            classifier: &classifier,
            whitelist: &[],
        };
        let cache = build_item_info_cache(&ctx);
        assert!(cache.is_empty());
    }

    #[test]
    fn build_item_info_cache_skips_placeholder_names() {
        let mut proto = BotItemPrototype::default();
        proto.item_id = 2;
        proto.class_id = item_class::ARMOR;
        proto.sub_class = armor_sub::CLOTH;
        proto.inventory_type = u32::from(slots::inv::CHEST);
        proto.allowable_class = -1;
        proto.quality = 3;
        ascii_name(&mut proto.name, "[PH] Test Chest");

        let classifier = empty_classifier();
        let scales = vec![scale(1, "arcane", class::MAGE, &[("int", 3)])];
        let world = MockItemWorld::new();

        let ctx = ItemInfoBuildCtx {
            world: &world,
            prototypes: &[proto],
            scales: &scales,
            classifier: &classifier,
            whitelist: &[],
        };
        let cache = build_item_info_cache(&ctx);
        assert!(cache.is_empty());
    }

    #[test]
    fn build_item_info_cache_records_min_level_and_slot() {
        let mut proto = BotItemPrototype::default();
        proto.item_id = 100;
        proto.class_id = item_class::ARMOR;
        proto.sub_class = armor_sub::CLOTH;
        proto.inventory_type = u32::from(slots::inv::CHEST);
        proto.allowable_class = -1;
        proto.required_level = 20;
        proto.item_level = 25;
        proto.quality = 3;
        ascii_name(&mut proto.name, "Plain Cloth Robe");

        let classifier = empty_classifier();
        let scales = vec![scale(1, "arcane", class::MAGE, &[("int", 3)])];
        let world = MockItemWorld::new();

        let ctx = ItemInfoBuildCtx {
            world: &world,
            prototypes: &[proto],
            scales: &scales,
            classifier: &classifier,
            whitelist: &[],
        };
        let cache = build_item_info_cache(&ctx);
        let entry = cache.get(&100).expect("chest should be cached");
        assert_eq!(entry.item_id, 100);
        assert_eq!(entry.min_level, 20);
        assert_eq!(entry.item_level, 25);
        assert_eq!(entry.slot, u32::from(slot::CHEST));
        assert_eq!(entry.quality, 3);
    }
}
