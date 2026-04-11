/**
 * RandomItemMgr.cpp — thin forwarding layer over the Rust item-pool FFI.
 *
 * Every method here resolves to a single `playerbot_itempool_*` call
 * declared in `cpp_wrapper/botffi.h`. The legacy implementation that
 * used to live here (caches, stat-weight scoring, equip selection,
 * enchant rolls, consumable pools) has been ported to
 * `playerbot-rs/crates/playerbot/src/itempool/`. Keep this file
 * allocation-free apart from the `std::vector` copies required to
 * reshape Rust's `(ptr, len)` tuples into the C++ return type.
 */

#include "botpch.h"
#include "RandomItemMgr.h"

#include "botffi.h"

namespace
{
    /// Resolve a `Player*` to the `guid_low` field that the Rust FFI
    /// expects. Returns 0 when `bot` is null, which the item-pool
    /// treats as "unknown player" (and degrades gracefully).
    inline uint32 ResolveGuidLow(Player* bot)
    {
        return bot ? bot->GetObjectGuid().GetCounter() : 0u;
    }

    /// Drain a `(ptr, len)` pair returned by
    /// `playerbot_itempool_query` / `_get_gems` into an owning
    /// `std::vector<uint32>` and release the Rust-side allocation.
    inline std::vector<uint32> DrainU32List(uint32_t* ptr, size_t len)
    {
        std::vector<uint32> out;
        if (ptr && len > 0)
        {
            out.assign(ptr, ptr + len);
        }
        if (ptr)
        {
            playerbot_itempool_free_u32_list(ptr, len);
        }
        return out;
    }
} // namespace

// ── Item metadata ────────────────────────────────────────────────────

uint32 RandomItemMgr::GetMinLevelFromCache(uint32 itemId)
{
    return playerbot_itempool_get_min_level(itemId);
}

float RandomItemMgr::GetItemRarity(uint32 itemId)
{
    return playerbot_itempool_get_item_rarity(itemId);
}

// ── Spec / stat weight ──────────────────────────────────────────────

uint32 RandomItemMgr::GetPlayerSpecId(Player* bot)
{
    return playerbot_itempool_get_player_spec_id(ResolveGuidLow(bot));
}

uint32 RandomItemMgr::GetStatWeight(uint32 itemId, uint32 specId)
{
    return playerbot_itempool_get_stat_weight(itemId, specId);
}

uint32 RandomItemMgr::GetLiveStatWeight(Player* bot, uint32 itemId, uint32 specId)
{
    return playerbot_itempool_get_live_stat_weight(ResolveGuidLow(bot), itemId, specId);
}

uint32 RandomItemMgr::GetBestRandomEnchantStatWeight(uint32 itemId, uint32 specId)
{
    return playerbot_itempool_get_best_random_enchant_stat_weight(itemId, specId);
}

// ── Equip pool / gem list ───────────────────────────────────────────

std::vector<uint32> RandomItemMgr::Query(uint32 level, uint8 clazz, uint8 specId, uint8 slot, uint32 quality)
{
    uint32_t* ptr = nullptr;
    size_t len = 0;
    if (!playerbot_itempool_query(level, clazz, specId, slot, quality, &ptr, &len))
    {
        return {};
    }
    return DrainU32List(ptr, len);
}

std::vector<uint32> RandomItemMgr::GetGemsList()
{
    uint32_t* ptr = nullptr;
    size_t len = 0;
    if (!playerbot_itempool_get_gems(&ptr, &len))
    {
        return {};
    }
    return DrainU32List(ptr, len);
}

// ── Quest / enchant helpers ─────────────────────────────────────────

bool RandomItemMgr::HasSameQuestRewards(Player* bot, uint32 itemId)
{
    return playerbot_itempool_has_same_quest_rewards(ResolveGuidLow(bot), itemId);
}

uint32 RandomItemMgr::CalculateBestRandomEnchantId(uint8 playerclass, uint8 spec, uint32 itemId)
{
    return playerbot_itempool_calculate_best_random_enchant_id(playerclass, spec, itemId);
}

uint32 RandomItemMgr::CalculateEnchantWeight(uint8 playerclass, uint8 spec, uint32 enchantId)
{
    return playerbot_itempool_calculate_enchant_weight(playerclass, spec, enchantId);
}

// ── Consumable / trade picks ────────────────────────────────────────

uint32 RandomItemMgr::GetAmmo(uint32 level, uint32 subClass)
{
    return playerbot_itempool_get_ammo(level, subClass);
}

uint32 RandomItemMgr::GetRandomPotion(uint32 level, uint32 effect)
{
    return playerbot_itempool_get_random_potion(level, effect);
}

uint32 RandomItemMgr::GetFood(uint32 level, uint32 category)
{
    return playerbot_itempool_get_food(level, category);
}

uint32 RandomItemMgr::GetRandomTrade(uint32 level)
{
    return playerbot_itempool_get_random_trade(level);
}
