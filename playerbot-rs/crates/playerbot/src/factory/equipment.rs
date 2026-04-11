//! Equipment initialization — port of `PlayerbotFactory::InitEquipment`
//! (playerbot/PlayerbotFactory.cpp:1732-2365).
//!
//! Re-rolls the bot's equipped slots from the itempool cache. Runs in
//! four modes:
//!
//!   * Fresh re-roll (`incremental=false, progressive=false`) — strip every
//!     slot and hand out best-score items at the current level.
//!   * Fresh progressive (`incremental=false, progressive=true`) — same as
//!     above but the quality floor follows a level table so a level-20 bot
//!     doesn't walk out in Epic gear.
//!   * Incremental (`incremental=true`) — keep existing gear but try to
//!     upgrade each slot. Sorts the candidate pool *descending* by score
//!     so the first match beats the old item.
//!   * Incremental partial (`incremental=true, partial_upgrade=true`) —
//!     same as incremental but only touches 1-4 of the worst slots so a
//!     refresh doesn't rebuild the whole kit.
//!
//! A `sync_with_master` flag narrows the item level window to `masterGS +
//! randomGearMaxDiff` and forces the search level to the bot's current
//! level (the C++ special case for a bot matching a player master's gear).
//!
//! Ported line-by-line from `PlayerbotFactory.cpp:1732-2365`. The only
//! structural change is that the ~7 KB `unavailableItemIDs` hash set per
//! expansion now lives in `factory/data/unavailable_items_*.txt` and is
//! loaded via `include_str!` + a lazy `HashSet` instead of an inline
//! brace initializer.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use cmangos::{BotItemPrototype, BotWorldSnapshot};

use super::FactoryTransaction;
use crate::itempool::equip_filter::{armor_sub, class as class_id, item_class};
use crate::itempool::ffi as itempool;
use crate::itempool::rarity::quality;
use crate::itempool::slots::{inv, slot};

/* ── Constants (verbatim from C++) ─────────────────────────────────────── */

/// `ITEM_FLAG_UNIQUE_EQUIPPABLE` — `ItemPrototype.h` in the Karatefylla
/// fork. The factory skips items carrying this flag when the bot already
/// has one in inventory.
const ITEM_FLAG_UNIQUE_EQUIPPABLE: u32 = 0x0008_0000;

/// `EQUIPMENT_SLOT_END` in `Player.h`. Iteration upper bound for every
/// equipped-gear loop in `InitEquipment`.
const EQUIPMENT_SLOT_END: u8 = 19;

/// Number of damage rows on `ItemPrototype::Damage[]`. Used by the
/// off-hand DPS comparison helper. Matches `MAX_ITEM_PROTO_DAMAGES` in
/// `ItemPrototype.h`.
const MAX_ITEM_PROTO_DAMAGES: usize = 5;

/// Cached bot guid-low / master guid are implicit through the
/// `FactoryTransaction` for the single-bot calls here — no constant.

/// Hard-coded legendary / artifact blacklist. Mirrors the `lockedItems`
/// push-back sequence at `PlayerbotFactory.cpp:1829-1844`. The comment
/// on each id is preserved from the C++ source so `grep` stays useful.
const LOCKED_ITEM_IDS: &[u32] = &[
    30311, // Warp Slicer
    30312, // Infinity Blade
    30313, // Staff of Disentagration
    30314, // Phaseshift Bulwark
    30316, // Devastation
    30317, // Cosmic Infuser
    30318, // Netherstrand Longbow
    18582, // Twin Blades of Azzinoth
    18583, // Right Blade
    18584, // Left Blade
    22736, // Andonisus, Reaper of Souls
    23051, // Glaive of the Defender
    13262, // Ashbringer
    17142, // Shard of the Defiler
    17782, // Talisman of Binding Shard
    12947, // Alex's Ring of Audacity
];

/* ── Expansion-specific unavailable-id tables ──────────────────────────── */

#[cfg(feature = "vanilla")]
const UNAVAILABLE_ITEMS_SRC: &str = include_str!("data/unavailable_items_vanilla.txt");
#[cfg(feature = "tbc")]
const UNAVAILABLE_ITEMS_SRC: &str = include_str!("data/unavailable_items_tbc.txt");
#[cfg(feature = "wotlk")]
const UNAVAILABLE_ITEMS_SRC: &str = include_str!("data/unavailable_items_wotlk.txt");
#[cfg(not(any(feature = "vanilla", feature = "tbc", feature = "wotlk")))]
const UNAVAILABLE_ITEMS_SRC: &str = "";

/// Lazily-parsed view over [`UNAVAILABLE_ITEMS_SRC`]. One-shot build on
/// first call; subsequent calls are a plain HashSet lookup. The data
/// files are one id per line — blank lines are skipped so the embedded
/// files don't need a strict format.
fn unavailable_item_ids() -> &'static HashSet<u32> {
    static IDS: OnceLock<HashSet<u32>> = OnceLock::new();
    IDS.get_or_init(|| {
        UNAVAILABLE_ITEMS_SRC
            .lines()
            .filter_map(|line| line.trim().parse::<u32>().ok())
            .collect()
    })
}

/* ── Entry point ───────────────────────────────────────────────────────── */

/// Port of `PlayerbotFactory::InitEquipment`. See module docs for the
/// flag matrix.
///
/// `item_quality` mirrors the C++ method's `itemQuality` local (set by
/// `.py` commands like `equip rare`): when non-zero it pins the quality
/// tier and disables the fresh-reroll quality-table branch.
pub fn init_equipment(
    tx: &mut FactoryTransaction<'_>,
    incremental: bool,
    sync_with_master: bool,
    progressive: bool,
    partial_upgrade: bool,
    item_quality: u32,
) {
    // Snapshot-first pattern. One FFI call per factory entry point.
    let snap = tx.get_snapshot();
    let level = u32::from(snap.self_.level);
    let bot_class = snap.self_.class_id;
    let bot_guild_id = snap.guild_id;
    let guid_low = tx.factory_bot_guid_low();

    // GS saved for the optional sync-with-master announce tail.
    let old_gs = snap.equip_gear_score;

    // Master GS — only meaningful under `sync_with_master`.
    let master_gs = if sync_with_master {
        tx.factory_master_equip_gear_score().unwrap_or(0)
    } else {
        0
    };

    // Fresh re-roll starts by stripping every equipped slot.
    if !incremental {
        tx.factory_destroy_all_equipped_items();
    }

    // Spec id — derived from the active talent tree. A zero result is
    // the C++ "no spec selected, abort" sentinel.
    let spec_id = itempool::player_spec_id(guid_low);
    if spec_id == 0 {
        return;
    }

    // Caster / wand weapon type — classes whose one- vs two-hand
    // preference is persisted in the per-bot KV store. The predicate is
    // slightly different between vanilla and TBC+ (vanilla skips Enhance
    // shaman spec 21), so we preserve the `#ifdef` as a feature gate.
    let weapon_type_candidate = is_caster_weapon_class(bot_class, spec_id);
    let mut weapon_type: u32 = 0;
    if level > 40 && weapon_type_candidate {
        weapon_type = tx.factory_kv_get_u32("weaponType");
        if weapon_type == 0 || !incremental {
            // `urand(0, 1) ? INVTYPE_WEAPON : INVTYPE_2HWEAPON`
            weapon_type = if tx.random_u32(0, 1) != 0 {
                u32::from(inv::WEAPON)
            } else {
                u32::from(inv::TWO_HAND_WEAPON)
            };
            tx.factory_kv_set_u32("weaponType", weapon_type);
        }
    }

    // Config references — pulled once so the inner loops don't re-lock
    // the config singleton.
    let cfg = crate::config::get();
    let random_gear_max_level = cfg.random_gear_max_level;
    let random_gear_max_diff = cfg.random_gear_max_diff;
    let random_gear_lowering_chance = cfg.random_gear_lowering_chance;
    let random_gear_tabards = cfg.random_gear_tabards;
    let random_gear_tabards_chance = cfg.random_gear_tabards_chance;
    let random_gear_tabards_replace_guild = cfg.random_gear_tabards_replace_guild;
    // Blacklist is cheap to copy (usually <20 ids) and keeps the inner
    // filter loop free of config-singleton locking.
    let random_gear_blacklist: Vec<u32> = cfg.random_gear_blacklist.clone();
    drop(cfg);

    // Partial-upgrade slot selection — picks 1..=4 random slots to
    // touch, preferring empty slots then the worst (quality*ilvl) items.
    let upgrade_slots: HashMap<u8, bool> = if incremental && partial_upgrade {
        select_partial_upgrade_slots(tx, random_gear_max_level)
    } else {
        HashMap::new()
    };

    // ── Main per-slot loop ────────────────────────────────────────────
    for slot_idx in 0..EQUIPMENT_SLOT_END {
        // Tabard: gated on the tabard-replace config. Same-team guild
        // tabards are preserved when `random_gear_tabards_replace_guild`
        // is false.
        if slot_idx == slot::TABARD {
            // urand(0, 100) >= 100 * chance  ⇒ skip
            let roll = tx.random_u32(0, 100);
            let gate = (100.0f32 * random_gear_tabards_chance) as u32;
            if !random_gear_tabards || roll < gate {
                continue;
            }
            if bot_guild_id != 0 && !random_gear_tabards_replace_guild {
                continue;
            }
        }

        // Incremental partial: skip slots not on the upgrade list except
        // trinkets (which the C++ always walks).
        if incremental
            && !upgrade_slots.is_empty()
            && upgrade_slots.get(&slot_idx) != Some(&true)
            && slot_idx != slot::TRINKET1
            && slot_idx != slot::TRINKET2
        {
            continue;
        }

        // Per-slot quality floor + search ceiling. The sync-with-master
        // branch narrows the ilvl window to `masterGS +
        // random_gear_max_diff` and pins the search level to the bot's
        // actual level.
        let mut search_level = level;
        let mut quality_cur = quality::POOR;
        let mut max_item_level = random_gear_max_level;
        let mut progressive_gear = progressive;

        if sync_with_master && master_gs != 0 {
            max_item_level = master_gs + random_gear_max_diff;
            progressive_gear = false;
            // `bot->GetLevel()` already equals `level`, so the C++
            // branch that "resets" searchLevel is a no-op here.
            search_level = level;
        } else if progressive_gear {
            quality_cur = progressive_quality_floor(level, incremental, tx);
            if !incremental {
                let roll = tx.random_u32(0, 100);
                let gate = (100.0f32 * random_gear_lowering_chance) as u32;
                if roll < gate && quality_cur > quality::NORMAL {
                    quality_cur -= 1;
                }
            }
        }

        // Explicit quality pin from the command line — overrides both
        // branches above.
        let set_quality = item_quality > 0;
        if set_quality {
            quality_cur = item_quality;
        }

        // ── Retry loop: decrement quality on each failed attempt ──────
        let mut attempts = 0u32;
        loop {
            let found = if slot_idx == slot::BODY || slot_idx == slot::TABARD {
                try_shirt_or_tabard(
                    tx,
                    &snap,
                    slot_idx,
                    spec_id,
                    max_item_level,
                )
            } else {
                try_equipment_slot(
                    tx,
                    &snap,
                    guid_low,
                    bot_class,
                    slot_idx,
                    spec_id,
                    quality_cur,
                    set_quality,
                    search_level,
                    level,
                    max_item_level,
                    random_gear_max_diff,
                    incremental,
                    progressive_gear,
                    weapon_type,
                    &random_gear_blacklist,
                )
            };

            // C++ `if (!found && quality > POOR) { quality--; }` — apply
            // *before* incrementing the attempt counter so the outer
            // check matches PB2's layout.
            if !found && quality_cur > quality::POOR {
                quality_cur -= 1;
            }
            attempts += 1;
            if found || attempts >= 3 || quality_cur == quality::POOR {
                break;
            }
        }

        // PB2 logs a "no items for slot" warning here for non-trinket
        // slots; the Rust bridge can forward that through `log_detail!`
        // if a test wants to assert on it. Skipping for now to avoid
        // log noise in the common "slot has no candidates" case.
    }

    // Tail: recompute stats + optional master sync announce.
    tx.factory_init_stats_for_level_and_update();

    if sync_with_master {
        if let Some(master_gs_val) = tx.factory_master_equip_gear_score() {
            let new_snap = tx.get_snapshot();
            let new_gs = new_snap.equip_gear_score;
            tx.factory_tell_master(&format!(
                "Synced gear with master. Old GS: {} New GS: {} Master GS: {}",
                old_gs, new_gs, master_gs_val
            ));
        }
    }
}

/* ── Helpers ───────────────────────────────────────────────────────────── */

/// PB2's `Shuffle(vec)` helper — five passes of random-swap pairs. Used
/// for every "no incremental, no progressive" candidate order.
fn shuffle(tx: &mut FactoryTransaction<'_>, items: &mut [u32]) {
    let n = items.len();
    if n < 2 {
        return;
    }
    let max = (n - 1) as u32;
    for _ in 0..(n * 5) {
        let i1 = tx.random_u32(0, max) as usize;
        let i2 = tx.random_u32(0, max) as usize;
        items.swap(i1, i2);
    }
}

/// Predicate from `PlayerbotFactory.cpp:1754-1758`: true for the
/// class/spec combos that consult the `weaponType` KV store.
fn is_caster_weapon_class(class: u8, spec: u32) -> bool {
    // Vanilla excludes spec 21 (Enhance TBC+).
    #[cfg(feature = "vanilla")]
    let enhance_gate = false;
    #[cfg(not(feature = "vanilla"))]
    let enhance_gate = spec == 21;

    class == class_id::PRIEST
        || class == class_id::MAGE
        || class == class_id::WARLOCK
        || spec == 20
        || enhance_gate
        || spec == 22
        || spec == 29
        || spec == 31
}

/// Progressive quality floor — port of the `level < 10 / < 20 / …`
/// ladder at `PlayerbotFactory.cpp:1880-1937`. Uses feature flags in
/// place of `MANGOSBOT_ZERO` / `ONE` / `TWO`.
fn progressive_quality_floor(level: u32, incremental: bool, tx: &mut FactoryTransaction<'_>) -> u32 {
    if !incremental {
        // Fresh re-roll: quality is a random range per level bracket.
        if level < 10 {
            return quality::POOR;
        }
        if level < 20 {
            return tx.random_u32(quality::NORMAL, quality::UNCOMMON);
        }
        if level < 40 {
            return tx.random_u32(quality::UNCOMMON, quality::RARE);
        }

        #[cfg(feature = "vanilla")]
        {
            if level < 60 {
                return tx.random_u32(quality::UNCOMMON, quality::RARE);
            }
            // Vanilla caps at 60 so anything higher picks the level-60
            // branch below.
            return tx.random_u32(quality::RARE, quality::EPIC);
        }
        #[cfg(feature = "tbc")]
        {
            if level < 60 {
                return tx.random_u32(quality::UNCOMMON, quality::RARE);
            }
            if level < 70 {
                return tx.random_u32(quality::UNCOMMON, quality::RARE);
            }
            return tx.random_u32(quality::RARE, quality::EPIC);
        }
        #[cfg(feature = "wotlk")]
        {
            if level < 60 {
                return tx.random_u32(quality::UNCOMMON, quality::RARE);
            }
            if level < 70 {
                return tx.random_u32(quality::UNCOMMON, quality::RARE);
            }
            if level < 80 {
                return tx.random_u32(quality::UNCOMMON, quality::RARE);
            }
            return tx.random_u32(quality::RARE, quality::EPIC);
        }
        // Fallback for when no feature is enabled (mock / cargo check).
        #[cfg(not(any(feature = "vanilla", feature = "tbc", feature = "wotlk")))]
        return tx.random_u32(quality::UNCOMMON, quality::RARE);
    }

    // Incremental: quality is deterministic (no urand).
    if level < 10 {
        return quality::POOR;
    }
    if level < 20 {
        return quality::NORMAL;
    }
    if level < 40 {
        return quality::UNCOMMON;
    }

    #[cfg(feature = "vanilla")]
    {
        if level < 60 {
            return quality::UNCOMMON;
        }
        return quality::RARE;
    }
    #[cfg(feature = "tbc")]
    {
        if level < 60 {
            return quality::UNCOMMON;
        }
        if level < 70 {
            return quality::UNCOMMON;
        }
        return quality::RARE;
    }
    #[cfg(feature = "wotlk")]
    {
        if level < 60 {
            return quality::UNCOMMON;
        }
        if level < 70 {
            return quality::UNCOMMON;
        }
        if level < 80 {
            return quality::UNCOMMON;
        }
        return quality::RARE;
    }
    #[cfg(not(any(feature = "vanilla", feature = "tbc", feature = "wotlk")))]
    quality::UNCOMMON
}

/// Port of the partial-upgrade slot-selection loop at
/// `PlayerbotFactory.cpp:1770-1826`. Returns a per-slot map marking the
/// ones to touch.
fn select_partial_upgrade_slots(
    tx: &mut FactoryTransaction<'_>,
    random_gear_max_level: u32,
) -> HashMap<u8, bool> {
    let mut upgrade_slots: HashMap<u8, bool> = HashMap::new();
    let mut empty_slots: Vec<u8> = Vec::new();
    let mut item_ids: Vec<u32> = Vec::new();
    let mut item_slots: HashMap<u32, u8> = HashMap::new();

    let max_slots = tx.random_u32(1, 4);

    for slot_idx in 0..EQUIPMENT_SLOT_END {
        upgrade_slots.insert(slot_idx, false);
    }

    for slot_idx in 0..EQUIPMENT_SLOT_END {
        if slot_idx == slot::TABARD
            || slot_idx == slot::BODY
            || slot_idx == slot::TRINKET1
            || slot_idx == slot::TRINKET2
        {
            continue;
        }
        let old_item = tx.factory_bot_equipped_item_in_slot(slot_idx);
        if old_item == 0 {
            empty_slots.push(slot_idx);
            continue;
        }
        if let Some(proto) = itempool::prototype(old_item) {
            if proto.item_level > random_gear_max_level {
                continue;
            }
            item_ids.push(proto.item_id);
            item_slots.insert(proto.item_id, slot_idx);
        }
    }

    // Sort ascending by `quality * ilvl` (PB2 lambda sort).
    item_ids.sort_by(|a, b| {
        let weight_a = itempool::prototype(*a)
            .map(|p| p.quality * p.item_level)
            .unwrap_or(0);
        let weight_b = itempool::prototype(*b)
            .map(|p| p.quality * p.item_level)
            .unwrap_or(0);
        weight_a.cmp(&weight_b)
    });

    // Fill empties first, then worst-score items.
    let mut counter: u32 = 0;
    for empty in &empty_slots {
        if counter > max_slots {
            break;
        }
        upgrade_slots.insert(*empty, true);
        counter += 1;
    }
    for id in &item_ids {
        if counter > max_slots {
            break;
        }
        if let Some(s) = item_slots.get(id) {
            upgrade_slots.insert(*s, true);
            counter += 1;
        }
    }

    upgrade_slots
}

/// Shirt / tabard equip path — port of
/// `PlayerbotFactory.cpp:1968-2022`. A stripped-down version of the
/// main loop: common quality only, level-60 fixed query, no caster /
/// weapon / quest-reward filtering.
fn try_shirt_or_tabard(
    tx: &mut FactoryTransaction<'_>,
    snap: &BotWorldSnapshot,
    slot_idx: u8,
    spec_id: u32,
    max_item_level: u32,
) -> bool {
    let mut ids = itempool::query_equip(60, 1, 1, slot_idx, 1);
    if ids.is_empty() {
        return false;
    }
    shuffle(tx, &mut ids);

    let guid_low = snap.self_.class_id; // placeholder; guid_low unused below
    let _ = guid_low;

    for new_item_id in ids {
        let Some(proto) = itempool::prototype(new_item_id) else {
            continue;
        };
        if proto.quality > quality::UNCOMMON {
            continue;
        }
        if proto.flags & ITEM_FLAG_UNIQUE_EQUIPPABLE != 0
            && bot_has_item_count(tx, proto.item_id, 1)
        {
            continue;
        }
        if proto.max_count != 0 && bot_has_item_count(tx, proto.item_id, proto.max_count) {
            continue;
        }
        if proto.item_level > max_item_level {
            continue;
        }

        let new_stat_value = itempool::live_stat_weight(
            tx.factory_bot_guid_low(),
            new_item_id,
            spec_id,
        );
        if new_stat_value == 0 {
            continue;
        }

        let old_item = tx.factory_bot_equipped_item_in_slot(slot_idx);
        if old_item == new_item_id {
            continue;
        }

        // C++ consults `CanEquipUnseenItem`; shirts / tabards always
        // fit, so the Rust path just swaps directly.
        if tx.factory_equip_new_item_in_slot(slot_idx, new_item_id, 0, true) {
            return true;
        }
    }

    false
}

/// `bot->HasItemCount(id, n)` equivalent. Counts *only* what the bot
/// currently has in backpack + bags, which matches the legacy call.
fn bot_has_item_count(tx: &FactoryTransaction<'_>, item_id: u32, n: u32) -> bool {
    tx.item_count_in_bags(cmangos::ItemId(item_id)) >= n
}

/// The real equipment slot loop — port of
/// `PlayerbotFactory.cpp:2024-2320`. This is where most of the filters
/// live: blacklist, unavailable-id set, unique-equippable, ilvl cap,
/// min-level delta, weapon shape, caster-weapon rules, off-hand vs
/// main-hand DPS comparison, quest rewards.
#[allow(clippy::too_many_arguments)]
fn try_equipment_slot(
    tx: &mut FactoryTransaction<'_>,
    snap: &BotWorldSnapshot,
    guid_low: u32,
    bot_class: u8,
    slot_idx: u8,
    spec_id: u32,
    quality_cur: u32,
    set_quality: bool,
    search_level: u32,
    level: u32,
    max_item_level: u32,
    random_gear_max_diff: u32,
    incremental: bool,
    progressive_gear: bool,
    weapon_type: u32,
    blacklist: &[u32],
) -> bool {
    // ── Build the candidate pool ─────────────────────────────────────
    let mut ids: Vec<u32> = Vec::new();

    for q in quality_cur..quality::ARTIFACT {
        if set_quality && q != quality_cur {
            continue;
        }

        let mut curr_search_level = search_level;
        let mut has_proper_level = false;
        while !has_proper_level && curr_search_level > 0 {
            let bucket = itempool::query_equip(
                curr_search_level,
                bot_class,
                spec_id as u8,
                slot_idx,
                q,
            );
            if !bucket.is_empty() {
                // PB2 inserts at `begin()` — not a prepend optimization,
                // just "put new rows first". We preserve that ordering.
                let mut merged = bucket;
                merged.extend_from_slice(&ids);
                ids = merged;
            }
            for id in &ids {
                if let Some(proto) = itempool::prototype(*id) {
                    if proto.item_level > max_item_level {
                        continue;
                    }
                    has_proper_level = true;
                    break;
                }
            }
            if !has_proper_level {
                ids.clear();
                curr_search_level -= 1;
            }
        }

        // Tank main-hand: also consider one-handers from the OFFHAND
        // pool (spec 3 = Prot warrior, 5 = Prot paladin).
        if (spec_id == 3 || spec_id == 5) && slot_idx == slot::MAINHAND {
            let one_handed = itempool::query_equip(level, bot_class, spec_id as u8, slot::OFFHAND, q);
            if !one_handed.is_empty() {
                let mut merged = one_handed;
                merged.extend_from_slice(&ids);
                ids = merged;
            }
        }

        // Caster main-hand: same trick for casters + resto druids etc.
        let caster_main_hand = (spec_id == 4
            || bot_class == class_id::DRUID
            || bot_class == class_id::PRIEST
            || bot_class == class_id::MAGE
            || bot_class == class_id::WARLOCK
            || spec_id == 20
            || spec_id == 22)
            && slot_idx == slot::MAINHAND;
        if caster_main_hand {
            let one_handed = itempool::query_equip(level, bot_class, spec_id as u8, slot::OFFHAND, q);
            if !one_handed.is_empty() {
                let mut merged = one_handed;
                merged.extend_from_slice(&ids);
                ids = merged;
            }
        }

        // Dual wield (rogue / fury / TBC+ enhance).
        #[cfg(feature = "vanilla")]
        let dual_wield = slot_idx == slot::MAINHAND
            && (bot_class == class_id::ROGUE || spec_id == 2);
        #[cfg(not(feature = "vanilla"))]
        let dual_wield = slot_idx == slot::MAINHAND
            && (bot_class == class_id::ROGUE || spec_id == 2 || spec_id == 21);
        if dual_wield {
            let one_handed = itempool::query_equip(level, bot_class, spec_id as u8, slot::OFFHAND, q);
            if !one_handed.is_empty() {
                let mut merged = one_handed;
                merged.extend_from_slice(&ids);
                ids = merged;
            }
        }
    }

    // ── Order the pool ────────────────────────────────────────────────
    if incremental || !progressive_gear {
        // Sort ascending by (base_weight + enchant_weight) * 1000 +
        // quality * ilvl, then reverse so best is first. Matches the
        // PB2 sort lambda + `std::reverse` tail.
        ids.sort_by(|a, b| {
            let a_weight = compute_sort_key(*a, spec_id);
            let b_weight = compute_sort_key(*b, spec_id);
            a_weight.cmp(&b_weight)
        });
        if !progressive_gear {
            ids.reverse();
        }
    } else if !ids.is_empty() {
        shuffle(tx, &mut ids);
    }

    // ── Walk the pool and try to equip the first valid candidate ──────
    let unavailable = unavailable_item_ids();
    let bot_level = level;

    for new_item_id in &ids {
        let new_item_id = *new_item_id;
        let Some(proto) = itempool::prototype(new_item_id) else {
            continue;
        };

        if LOCKED_ITEM_IDS.contains(&proto.item_id) {
            continue;
        }
        if unavailable.contains(&proto.item_id) {
            continue;
        }
        if blacklist.contains(&proto.item_id) {
            continue;
        }
        if proto.flags & ITEM_FLAG_UNIQUE_EQUIPPABLE != 0
            && bot_has_item_count(tx, proto.item_id, 1)
        {
            continue;
        }
        if proto.max_count != 0 && bot_has_item_count(tx, proto.item_id, proto.max_count) {
            continue;
        }
        if proto.item_level > max_item_level {
            continue;
        }

        // Min-level delta check.
        let req_level = itempool::min_level(new_item_id);
        if req_level != 0
            && proto.quality < quality::LEGENDARY
            && (bot_level as i32 - req_level as i32).abs() > random_gear_max_diff as i32
        {
            continue;
        }

        // Tank weapons + shield gating.
        if slot_idx == slot::OFFHAND
            && (spec_id == 3 || spec_id == 5)
            && !(proto.class_id == item_class::ARMOR && proto.sub_class == armor_sub::SHIELD)
        {
            continue;
        }
        if slot_idx == slot::MAINHAND
            && proto.class_id == item_class::ARMOR
            && proto.sub_class == armor_sub::SHIELD
        {
            continue;
        }
        if slot_idx == slot::MAINHAND && proto.inventory_type == u32::from(inv::HOLDABLE) {
            continue;
        }
        if slot_idx == slot::MAINHAND
            && (spec_id == 3 || spec_id == 5)
            && !(proto.class_id == item_class::WEAPON
                && proto.inventory_type != u32::from(inv::HOLDABLE))
        {
            continue;
        }

        // Fury warrior: prefer slow main-hand (>=2.0s speed).
        if slot_idx == slot::MAINHAND
            && spec_id == 2
            && proto.class_id == item_class::WEAPON
            && proto.delay < 2000
        {
            continue;
        }

        // TBC enhancement shaman: both hands 2.0+ speed.
        #[cfg(feature = "tbc")]
        {
            if (slot_idx == slot::MAINHAND || slot_idx == slot::OFFHAND)
                && bot_level >= 60
                && spec_id == 21
                && proto.class_id == item_class::WEAPON
                && proto.inventory_type != u32::from(inv::TWO_HAND_WEAPON)
                && proto.delay < 2000
            {
                continue;
            }
        }

        // Classic / TBC 60+ enhance shaman + retri paladin: 2H only,
        // 3.0+ speed.
        if slot_idx == slot::MAINHAND
            && (spec_id == 6 || spec_id == 21)
            && proto.class_id == item_class::WEAPON
            && bot_level >= 60
            && proto.inventory_type == u32::from(inv::TWO_HAND_WEAPON)
            && proto.delay < 3000
        {
            continue;
        }

        // Caster weapon shape gate.
        if weapon_type != 0 {
            if slot_idx == slot::MAINHAND {
                if proto.class_id == item_class::ARMOR && proto.sub_class == armor_sub::SHIELD {
                    continue;
                }
                if weapon_type == u32::from(inv::TWO_HAND_WEAPON)
                    && proto.class_id == item_class::WEAPON
                    && proto.inventory_type != u32::from(inv::TWO_HAND_WEAPON)
                {
                    continue;
                }
                if weapon_type != u32::from(inv::TWO_HAND_WEAPON)
                    && proto.class_id == item_class::WEAPON
                    && proto.inventory_type == u32::from(inv::TWO_HAND_WEAPON)
                {
                    continue;
                }
                if proto.inventory_type == u32::from(inv::HOLDABLE) {
                    continue;
                }
            }
            if slot_idx == slot::OFFHAND {
                let is_caster_class = bot_class == class_id::PRIEST
                    || bot_class == class_id::MAGE
                    || bot_class == class_id::WARLOCK
                    || (bot_class == class_id::DRUID && (spec_id == 29 || spec_id == 31));
                if weapon_type != u32::from(inv::TWO_HAND_WEAPON)
                    && is_caster_class
                    && proto.inventory_type != u32::from(inv::HOLDABLE)
                {
                    continue;
                }
                if weapon_type == u32::from(inv::TWO_HAND_WEAPON) && is_caster_class {
                    continue;
                }
                if weapon_type != u32::from(inv::TWO_HAND_WEAPON)
                    && proto.class_id == item_class::WEAPON
                    && proto.inventory_type == u32::from(inv::TWO_HAND_WEAPON)
                {
                    continue;
                }
            }
        }

        // Old item score — for the "don't downgrade" gate.
        let old_item = tx.factory_bot_equipped_item_in_slot(slot_idx);
        let old_proto = if old_item != 0 {
            itempool::prototype(old_item)
        } else {
            None
        };
        let old_stat_value = if old_item != 0 {
            itempool::live_stat_weight(guid_low, old_item, spec_id)
        } else {
            0
        };

        if old_item != 0 && old_item == new_item_id {
            continue;
        }

        // Legendary rolls.
        if proto.quality == quality::LEGENDARY && tx.random_u32(0, 100) > 20 {
            continue;
        }
        if incremental
            && old_proto.is_some()
            && old_proto.unwrap().quality == quality::LEGENDARY
            && tx.random_u32(0, 100) > 50
        {
            continue;
        }

        let mut new_stat_value = itempool::live_stat_weight(guid_low, new_item_id, spec_id);
        if new_stat_value == 0 {
            continue;
        }

        if itempool::has_same_quest_rewards(guid_low, new_item_id) {
            continue;
        }

        // Random enchant selection + weight top-up.
        let mut random_ench_best_id: u32 = 0;
        if proto.random_property != 0 {
            random_ench_best_id =
                itempool::calculate_best_random_enchant_id(bot_class, spec_id, new_item_id);
            let random_ench_best_value =
                itempool::calculate_enchant_weight(bot_class, spec_id, random_ench_best_id);
            new_stat_value += random_ench_best_value;
        }

        // Off-hand vs main-hand weapon gate — reject a new off-hand
        // that is worse than the main-hand in both DPS and damage.
        let is_weapon = proto.class_id == item_class::WEAPON;
        #[cfg(feature = "vanilla")]
        let off_hand_weapon_gate = is_weapon
            && slot_idx == slot::OFFHAND
            && (bot_class == class_id::ROGUE || spec_id == 2);
        #[cfg(feature = "tbc")]
        let off_hand_weapon_gate = is_weapon
            && slot_idx == slot::OFFHAND
            && (bot_class == class_id::ROGUE || spec_id == 2 || spec_id == 21);
        #[cfg(feature = "wotlk")]
        let off_hand_weapon_gate = is_weapon
            && slot_idx == slot::OFFHAND
            && (bot_class == class_id::ROGUE
                || spec_id == 2
                || spec_id == 21
                || bot_class == class_id::DEATH_KNIGHT);
        #[cfg(not(any(feature = "vanilla", feature = "tbc", feature = "wotlk")))]
        let off_hand_weapon_gate = is_weapon
            && slot_idx == slot::OFFHAND
            && bot_class == class_id::ROGUE;

        if off_hand_weapon_gate {
            let mh_item = tx.factory_bot_equipped_item_in_slot(slot::MAINHAND);
            if mh_item != 0 {
                if let Some(mh_proto) = itempool::prototype(mh_item) {
                    let mh_stat = itempool::live_stat_weight(guid_low, mh_item, spec_id);
                    let better_value = new_stat_value > mh_stat;

                    let (mh_damage, mh_dps) = compute_weapon_damage_and_dps(&mh_proto);
                    let (oh_damage, oh_dps) = compute_weapon_damage_and_dps(&proto);

                    let better_dps = oh_dps > mh_dps;
                    let better_damage = oh_damage > mh_damage;
                    if better_dps || (better_damage && better_value) {
                        continue;
                    }
                }
            }
        }

        // Incremental: don't swap to a worse item.
        if incremental && old_item != 0 && old_stat_value >= new_stat_value && old_stat_value > 1 {
            continue;
        }

        // Replace grey with grey? No — keep the old one.
        if (incremental || progressive_gear)
            && old_proto.is_some()
            && old_proto.unwrap().quality < quality::NORMAL
            && proto.quality < quality::NORMAL
            && level > 5
        {
            continue;
        }

        // Attempt the swap. The C++ `CanEquipUnseenItem` is handled on
        // the C++ side through `factory_equip_new_item_in_slot` — it
        // returns false if the equip check fails.
        if tx.factory_equip_new_item_in_slot(slot_idx, new_item_id, random_ench_best_id, true) {
            return true;
        }
    }

    let _ = snap;
    false
}

/// Port of the PB2 sort lambda at `PlayerbotFactory.cpp:2092-2108`.
/// Encodes `(base_weight + enchant_weight) * 1000 + quality * ilvl`.
fn compute_sort_key(item_id: u32, spec_id: u32) -> u64 {
    let base = itempool::stat_weight(item_id, spec_id) as u64;
    let ench = itempool::best_random_enchant_stat_weight(item_id, spec_id) as u64;
    let mut score = (base + ench) * 1000;
    if let Some(proto) = itempool::prototype(item_id) {
        score += (proto.quality as u64) * (proto.item_level as u64);
    }
    score
}

/// Compute (max damage, dps) for the last populated damage row —
/// matches the PB2 loop that keeps rewriting `mhDamage` / `mhDps` until
/// the first empty row. The final values are whatever the *last*
/// non-empty row held.
fn compute_weapon_damage_and_dps(proto: &BotItemPrototype) -> (u32, u32) {
    let mut damage: u32 = 0;
    let mut dps: u32 = 0;
    for i in 0..MAX_ITEM_PROTO_DAMAGES {
        let d = &proto.damages[i];
        if d.damage_max == 0.0 {
            break;
        }
        damage = d.damage_max as u32;
        if proto.delay > 0 {
            dps = ((d.damage_min + d.damage_max) / (proto.delay as f32 / 1000.0) / 2.0) as u32;
        }
    }
    (damage, dps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{self, ConfigState, TEST_LOCK};
    use crate::factory::with_tx;
    use crate::itempool::ffi::install_state_for_test;
    use crate::itempool::item_info::ItemInfoCache;
    use crate::itempool::manager::ItemPoolManager;
    use crate::itempool::types::{
        BotEquipKey, ItemInfoEntry, WeightScale, WeightScaleInfo,
    };
    use cmangos::{
        BotItemPrototype, BotPlayerItemCtx, BotUnitSnapshot, MockEvent, MockItemWorld, MockWorld,
        TalentTabInfo,
    };
    use std::sync::{Arc, Mutex};

    /// Serialise every test that touches the itempool singleton. The
    /// state is process-wide so parallel tests race `install_state_for_test`.
    static SINGLETON_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn locked_items_matches_pb2() {
        // Guard against accidental edits to the hard-coded legendary
        // blacklist. Matches the exact push_back sequence at
        // PlayerbotFactory.cpp:1829-1844.
        assert_eq!(LOCKED_ITEM_IDS.len(), 16);
        assert!(LOCKED_ITEM_IDS.contains(&30311)); // Warp Slicer
        assert!(LOCKED_ITEM_IDS.contains(&13262)); // Ashbringer
        assert!(LOCKED_ITEM_IDS.contains(&12947)); // Alex's Ring of Audacity
    }

    #[test]
    fn unavailable_ids_not_empty_when_expansion_enabled() {
        // Only assert the set is populated when the build actually
        // compiled in one of the expansion data files. Under `cargo
        // check` with no features, the file is empty and the set is
        // zero — that's a legit path too.
        if !UNAVAILABLE_ITEMS_SRC.is_empty() {
            let ids = unavailable_item_ids();
            assert!(!ids.is_empty());
        }
    }

    #[test]
    fn item_flag_unique_equippable_matches_source() {
        assert_eq!(ITEM_FLAG_UNIQUE_EQUIPPABLE, 0x0008_0000);
    }

    #[test]
    fn equipment_slot_end_matches_player_h() {
        // Player.h in the Karatefylla fork defines EQUIPMENT_SLOT_END = 19.
        assert_eq!(EQUIPMENT_SLOT_END, 19);
    }

    /* ── Integration test fixtures ─────────────────────────────────── */

    const GUID: u32 = 42;
    // Use a high id that's guaranteed to be absent from every
    // `unavailable_items_*.txt` — those files cap out well below this.
    const CHEST_ITEM: u32 = 999_999;

    /// Build an `ItemPoolManager` pre-populated with:
    ///   * one arms-warrior weight scale (spec id 1)
    ///   * one chest-slot candidate prototype (`CHEST_ITEM`)
    ///   * an `item_info` entry with a non-zero stat weight
    ///   * an `equip_cache` row at (60, warrior, arms, CHEST, NORMAL)
    fn warrior_arms_manager() -> ItemPoolManager {
        let mut mgr = ItemPoolManager::new();

        // Weight scale — spec 1 = warrior arms in PB2's scale table.
        mgr.scales.push(WeightScale {
            info: WeightScaleInfo {
                id: 1,
                name: "arms".into(),
                class_id: 1,
            },
            stats: Vec::new(),
        });

        // A single candidate chest item — plate, quality "normal", ilvl 60.
        // Keep `required_level` within `random_gear_max_diff` (default 9)
        // of the level-60 bot so the factory's min-level gate accepts it.
        let proto = BotItemPrototype {
            item_id: CHEST_ITEM,
            class_id: item_class::ARMOR,
            sub_class: armor_sub::PLATE,
            inventory_type: u32::from(inv::CHEST),
            quality: quality::NORMAL,
            item_level: 60,
            required_level: 60,
            ..Default::default()
        };
        mgr.prototypes.push(proto);

        // `item_info` entry — live_stat_weight reads `weights` by spec id.
        let mut info = ItemInfoEntry::new(CHEST_ITEM);
        info.weights.insert(1, 100);
        info.item_level = 60;
        info.min_level = 60;
        info.quality = quality::NORMAL;
        info.slot = u32::from(slot::CHEST);
        let mut item_info = ItemInfoCache::new();
        item_info.insert(CHEST_ITEM, info);
        mgr.item_info = item_info;

        // Equip cache bucket — (level, class, spec, slot, quality).
        let key = BotEquipKey::new(60, 1, 1, slot::CHEST, quality::NORMAL);
        mgr.equip_cache.insert(key, vec![CHEST_ITEM]);

        mgr
    }

    /// Build a `MockItemWorld` seeded with the warrior player ctx + the
    /// arms talent tab so `player_spec_id` returns spec id 1.
    fn warrior_arms_item_world() -> Arc<MockItemWorld> {
        let w = MockItemWorld::new();
        w.set_player_ctx(
            GUID,
            BotPlayerItemCtx {
                guid_low: GUID,
                level: 60,
                class_id: 1,
                race_id: 1,
                team: 0,
                pvp_rank: 0,
                map_id: 0,
                in_world: true,
            },
        );
        w.set_player_talent_tab(
            GUID,
            TalentTabInfo {
                best_tab: 0, // arms
                meditation_rank: 0,
            },
        );
        // Silence any random rolls — the equipment path uses urand for
        // legendary / progressive rolls even on non-legendary items.
        w.push_urand_sequence(std::iter::repeat(0).take(64));
        Arc::new(w)
    }

    /// Build a `MockWorld` for a level-60 warrior bot.
    fn warrior_world() -> MockWorld {
        let self_snap = BotUnitSnapshot {
            class_id: 1,
            level: 60,
            ..Default::default()
        };
        MockWorld::builder()
            .self_snap(self_snap)
            .bot_guid_low(GUID)
            .build()
    }

    /// Install a fresh default config so `random_gear_max_level` and
    /// friends are defined. Keep a guard on `TEST_LOCK` because the
    /// config singleton is also process-wide.
    fn install_default_config() {
        config::install(ConfigState::default());
    }

    #[test]
    fn fresh_reroll_strips_then_equips_and_recomputes_stats() {
        let _s = SINGLETON_LOCK.lock().unwrap();
        let _g = TEST_LOCK.lock().unwrap();
        install_default_config();

        let mgr = warrior_arms_manager();
        let item_world = warrior_arms_item_world();
        install_state_for_test(mgr, item_world.clone());

        let mut world = warrior_world();
        with_tx(&mut world, |tx| {
            super::init_equipment(
                tx, false, /* incremental   */
                false, /* sync_with_master */
                false, /* progressive      */
                false, /* partial_upgrade  */
                0,     /* item_quality     */
            );
        });

        let events = world.events();
        // A fresh re-roll must destroy every slot first.
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MockEvent::DestroyAllEquippedItems)),
            "fresh re-roll should destroy all equipped items first; events: {events:?}"
        );
        // Chest item should have been handed out.
        assert!(
            events.iter().any(|e| matches!(
                e,
                MockEvent::EquipNewItemInSlot { slot, item, .. }
                    if *slot == slot::CHEST && *item == CHEST_ITEM
            )),
            "expected CHEST_ITEM ({CHEST_ITEM}) to be equipped in CHEST slot; events: {events:?}"
        );
        // Tail always recomputes stats.
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MockEvent::InitStatsForLevelAndUpdate)),
            "fresh re-roll should recompute stats; events: {events:?}"
        );
    }

    #[test]
    fn incremental_skips_destroy_all_equipped() {
        let _s = SINGLETON_LOCK.lock().unwrap();
        let _g = TEST_LOCK.lock().unwrap();
        install_default_config();

        let mgr = warrior_arms_manager();
        let item_world = warrior_arms_item_world();
        install_state_for_test(mgr, item_world.clone());

        let mut world = warrior_world();
        with_tx(&mut world, |tx| {
            super::init_equipment(
                tx, true,  /* incremental     */
                false, /* sync_with_master */
                false, /* progressive      */
                false, /* partial_upgrade  */
                0,     /* item_quality     */
            );
        });

        let events = world.events();
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, MockEvent::DestroyAllEquippedItems)),
            "incremental path must NOT destroy all equipped items; events: {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MockEvent::InitStatsForLevelAndUpdate)),
            "incremental path should still recompute stats; events: {events:?}"
        );
    }

    #[test]
    fn zero_spec_id_aborts_without_touching_slots() {
        let _s = SINGLETON_LOCK.lock().unwrap();
        let _g = TEST_LOCK.lock().unwrap();
        install_default_config();

        // Empty manager — no scales → `player_spec_id` returns 0 →
        // PB2 bails before iterating slots. We only expect the fresh
        // re-roll strip + the final stats recompute.
        let mgr = ItemPoolManager::new();
        let item_world = Arc::new(MockItemWorld::new());
        install_state_for_test(mgr, item_world);

        let mut world = warrior_world();
        with_tx(&mut world, |tx| {
            super::init_equipment(tx, false, false, false, false, 0);
        });

        let events = world.events();
        // Pre-spec destroy still runs (matches the C++ order).
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MockEvent::DestroyAllEquippedItems)),
            "fresh re-roll still strips the old gear before the spec check; events: {events:?}"
        );
        // No equip events — the zero spec id aborts the slot loop.
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, MockEvent::EquipNewItemInSlot { .. })),
            "zero spec id must skip every slot; events: {events:?}"
        );
        // No stats recompute either — PB2 returns before the tail.
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, MockEvent::InitStatsForLevelAndUpdate)),
            "zero spec id must NOT recompute stats; events: {events:?}"
        );
    }

    #[test]
    fn sync_with_master_announces_result() {
        let _s = SINGLETON_LOCK.lock().unwrap();
        let _g = TEST_LOCK.lock().unwrap();
        install_default_config();

        let mgr = warrior_arms_manager();
        let item_world = warrior_arms_item_world();
        install_state_for_test(mgr, item_world.clone());

        let mut world = MockWorld::builder()
            .self_snap(BotUnitSnapshot {
                class_id: 1,
                level: 60,
                ..Default::default()
            })
            .bot_guid_low(GUID)
            .master_equip_gear_score(80)
            .build();

        with_tx(&mut world, |tx| {
            super::init_equipment(
                tx, true,  /* incremental     */
                true,  /* sync_with_master */
                false, /* progressive      */
                false, /* partial_upgrade  */
                0,     /* item_quality     */
            );
        });

        let events = world.events();
        let told_master = events.iter().any(|e| matches!(
            e,
            MockEvent::TellMaster(msg) if msg.contains("Synced gear with master")
        ));
        assert!(
            told_master,
            "sync_with_master should announce the old/new/master GS to the master; events: {events:?}"
        );
    }
}
