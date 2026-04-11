/**
 * RandomItemMgr.h — thin C++ façade over the Rust item-pool FFI.
 *
 * The real item-pool lives in
 * `playerbot-rs/crates/playerbot/src/itempool/`; this header only
 * preserves the legacy call surface used by `PlayerbotFactory.cpp`,
 * `ahbot/PricingStrategy.cpp`, and `cpp_wrapper/BotBridge.cpp` so those
 * call sites keep compiling. Every method forwards directly to a
 * `playerbot_itempool_*` entry point declared in
 * `cpp_wrapper/botffi.h`. No per-instance state is kept on the C++
 * side.
 */
#ifndef _RandomItemMgr_H
#define _RandomItemMgr_H

#include "PlayerbotAIBase.h"
#ifdef CMANGOS
#include "Entities/Player.h"
#endif
#ifdef MANGOS
#include "Object/Player.h"
#endif

#include <vector>

class RandomItemMgr
{
    public:
        RandomItemMgr() = default;
        ~RandomItemMgr() = default;

        static RandomItemMgr& instance()
        {
            static RandomItemMgr instance;
            return instance;
        }

        // ── Item metadata ────────────────────────────────────────────
        uint32 GetMinLevelFromCache(uint32 itemId);
        float  GetItemRarity(uint32 itemId);

        // ── Spec / stat weight ───────────────────────────────────────
        uint32 GetPlayerSpecId(Player* bot);
        uint32 GetStatWeight(uint32 itemId, uint32 specId);
        uint32 GetLiveStatWeight(Player* bot, uint32 itemId, uint32 specId = 0);
        uint32 GetBestRandomEnchantStatWeight(uint32 itemId, uint32 specId);

        // ── Equip pool / gem list ────────────────────────────────────
        std::vector<uint32> Query(uint32 level, uint8 clazz, uint8 specId, uint8 slot, uint32 quality);
        std::vector<uint32> GetGemsList();

        // ── Quest / enchant helpers ──────────────────────────────────
        bool   HasSameQuestRewards(Player* bot, uint32 itemId);
        uint32 CalculateBestRandomEnchantId(uint8 playerclass, uint8 spec, uint32 itemId);
        uint32 CalculateEnchantWeight(uint8 playerclass, uint8 spec, uint32 enchantId);

        // ── Consumable / trade picks (still routed through BotBridge) ─
        uint32 GetAmmo(uint32 level, uint32 subClass);
        uint32 GetRandomPotion(uint32 level, uint32 effect);
        uint32 GetFood(uint32 level, uint32 category);
        uint32 GetRandomTrade(uint32 level);
};

#define sRandomItemMgr RandomItemMgr::instance()

#endif
