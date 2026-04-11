/**
 * ItemBridge.h — C++ side of the Phase F item-pool rewrite.
 *
 * Implements the `ItemCallbacks` vtable declared in `botffi.h`. The Rust
 * `playerbot` crate now owns every cache that used to live in
 * `playerbot/RandomItemMgr.cpp` (item-info, equip pool, consumable
 * pools, stat-weight scoring, random-enchant resolution, rarity cache,
 * gem list). This file is the narrow surface left on the C++ side that
 * still needs to talk to CMaNGOS:
 *
 *   1. Table dumps fed into the Rust `itempool::init()` pass — item
 *      prototypes, spell lookups, weight scale rows, quest rewards,
 *      vendor relationships, item enchantment template, random
 *      property enchant slots, rarity cache, gem properties.
 *   2. Per-player runtime lookups used by the Rust
 *      `GetLiveStatWeight` / `HasSameQuestRewards` path —
 *      `BotPlayerItemCtx`, reputation rank, skill value, quest
 *      status, bag/bank presence, best talent tab.
 *   3. A `urand_range` wrapper and a debug-log sink.
 *
 * The bridge is stateless — every function is a free-standing static
 * that looks things up from the per-call arguments. Memory returned
 * via `query_*` is freshly allocated with `std::malloc`; Rust hands
 * it back through the matching `free_*` callback and the bridge
 * releases it with `std::free`.
 */

#pragma once

#include "botffi.h"

namespace ItemBridge
{
    /// Build the populated `ItemCallbacks` vtable the Rust item-pool
    /// consumes during `playerbot_itempool_init`. Every field points
    /// at an `ItemBridge::CB_*` free function defined in
    /// `ItemBridge.cpp`. Safe to call from any thread; the returned
    /// struct is a PoD value and is passed by value into
    /// `playerbot_itempool_init`.
    ItemCallbacks MakeCallbacks();

    // ── Table dumps (init-time) ─────────────────────────────────────
    uint32_t CB_QueryItemPrototypes(BotItemPrototype** out_rows);
    void     CB_FreeItemPrototypeList(BotItemPrototype* rows, uint32_t count);

    bool     CB_LookupSpellEntry(uint32_t spell_id, BotSpellEntryInfo* out);

    uint32_t CB_QueryWeightScales(BotWeightScaleRow** out_rows);
    void     CB_FreeWeightScaleList(BotWeightScaleRow* rows, uint32_t count);
    uint32_t CB_QueryWeightScaleStats(BotWeightScaleStatRow** out_rows);
    void     CB_FreeWeightScaleStatList(BotWeightScaleStatRow* rows, uint32_t count);

    uint32_t CB_QueryQuestItems(BotQuestItemRow** out_rows);
    void     CB_FreeQuestItemList(BotQuestItemRow* rows, uint32_t count);

    uint32_t CB_QueryVendorItems(BotVendorItemRow** out_rows);
    void     CB_FreeVendorItemList(BotVendorItemRow* rows, uint32_t count);

    uint32_t CB_QueryItemEnchantmentTemplate(BotItemEnchantmentRow** out_rows);
    void     CB_FreeItemEnchantmentList(BotItemEnchantmentRow* rows, uint32_t count);

    bool     CB_LookupEnchantInfo(uint32_t enchant_id, BotEnchantInfo* out);
    bool     CB_LookupRandomPropertyEnchants(int32_t property_id, uint32_t out_enchant_ids[4]);

    uint32_t CB_QueryRarityRows(BotItemRarityRow** out_rows);
    void     CB_FreeRarityList(BotItemRarityRow* rows, uint32_t count);

    uint32_t CB_QueryGemProperties(BotGemPropertiesRow** out_rows);
    void     CB_FreeGemList(BotGemPropertiesRow* rows, uint32_t count);

    // ── Per-player runtime queries ──────────────────────────────────
    bool     CB_GetPlayerItemCtx(uint32_t guid_low, BotPlayerItemCtx* out);
    int32_t  CB_GetPlayerReputationRank(uint32_t guid_low, uint32_t faction_id);
    uint32_t CB_GetPlayerSkillValue(uint32_t guid_low, uint32_t skill_id);
    uint32_t CB_PlayerQuestStatus(uint32_t guid_low, uint32_t quest_id);
    bool     CB_PlayerHasItem(uint32_t guid_low, uint32_t item_id);
    uint32_t CB_GetPlayerTalentTab(uint32_t guid_low, uint32_t* out_meditation_rank);

    // ── RNG + debug logging ─────────────────────────────────────────
    uint32_t CB_UrandRange(uint32_t min, uint32_t max);
    void     CB_LogDebug(const char* msg);
} // namespace ItemBridge
