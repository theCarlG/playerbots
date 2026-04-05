/**
 * BotBridge.h — Implements the BotCallbacks vtable using CMaNGOS APIs.
 *
 * Each callback function takes a BotHandle (= ObjectGuid value), resolves it to
 * the live Player* (or Unit*) via ObjectAccessor, and calls the appropriate
 * CMaNGOS API.  All functions are standalone — no bridge object state is needed
 * because the bot is always looked up from its handle.
 *
 * These are registered once in PlayerbotRust::Init() and stored in the
 * BotCallbacks struct that gets passed to playerbot_create().
 */

#pragma once

#include "botffi.h"

// Forward-declare so callers don't need to pull in all of CMaNGOS.
class Player;
class Unit;

namespace BotBridge
{
    /**
     * Fill a BotCallbacks struct with all callback function pointers.
     * Call this once per bot (in PlayerbotRust constructor).
     */
    BotCallbacks MakeCallbacks();

    // ── Snapshot ────────────────────────────────────────────────────────
    BotWorldSnapshot CB_GetSnapshot(BotHandle bot);
    BotUnitSnapshot  CB_GetUnitSnapshot(BotHandle bot, UnitHandle target);

    // ── Aura queries ─────────────────────────────────────────────────────
    bool         CB_HasAura(BotHandle bot, UnitHandle target, uint32_t spell_id);
    BotAuraInfo  CB_GetAura(BotHandle bot, UnitHandle target, uint32_t spell_id);
    BotAuraInfo* CB_GetAuras(BotHandle bot, UnitHandle target, uint32_t* out_count);
    void         CB_FreeAuraList(BotAuraInfo* list);

    // ── Threat queries ────────────────────────────────────────────────────
    BotThreatEntry* CB_GetThreatList(BotHandle bot, UnitHandle target_unit, uint32_t* out_count);
    void            CB_FreeThreatList(BotThreatEntry* list);
    float           CB_GetUnitThreat(BotHandle bot, UnitHandle target_unit, UnitHandle from_unit);

    // ── Unit queries ───────────────────────────────────────────────────────
    float       CB_UnitDistance(BotHandle bot, UnitHandle target);
    bool        CB_CanCast(BotHandle bot, uint32_t spell_id, UnitHandle target);
    bool        CB_SpellOnCooldown(BotHandle bot, uint32_t spell_id);
    uint32_t    CB_SpellCooldownMs(BotHandle bot, uint32_t spell_id);
    bool        CB_HasLos(BotHandle bot, UnitHandle target);
    UnitHandle* CB_GetNearbyUnits(BotHandle bot, float range, bool hostile, uint32_t* out_count);
    void        CB_FreeUnitList(UnitHandle* list);
    bool        CB_BotIsBehind(BotHandle bot, UnitHandle target);
    uint32_t    CB_BotEquippedWeaponSubclass(BotHandle bot, uint8_t slot);
    uint32_t    CB_BotItemCount(BotHandle bot, uint32_t item_id);
    uint8_t     CB_BotActiveTotemMask(BotHandle bot);
    bool        CB_BotWeaponEnchanted(BotHandle bot, uint8_t slot);
    uint8_t     CB_BotRunesReadyMask(BotHandle bot);
    bool        CB_BotKnowsSpell(BotHandle bot, uint32_t spell_id);

    // ── Pathfinding / positioning ──────────────────────────────────────────
    BotPosition     CB_GetBehindPosition(BotHandle bot, UnitHandle target, float distance);
    BotSafePosition CB_GetSafePosition(BotHandle bot, float search_radius);
    BotPosition     CB_GetSpreadPosition(BotHandle bot, UnitHandle center, float radius,
                                          uint8_t idx, uint8_t total);
    bool            CB_CanReach(BotHandle bot, float x, float y, float z);

    // ── Commands ───────────────────────────────────────────────────────────
    bool CB_CastSpell(BotHandle bot, uint32_t spell_id, UnitHandle target);
    bool CB_CastSpellPos(BotHandle bot, uint32_t spell_id, float x, float y, float z);
    bool CB_MoveTo(BotHandle bot, float x, float y, float z);
    bool CB_Follow(BotHandle bot, UnitHandle target, float dist, float angle);
    bool CB_StopMoving(BotHandle bot);
    bool CB_Attack(BotHandle bot, UnitHandle target);
    bool CB_AutoAttack(BotHandle bot, bool enable);
    bool CB_Say(BotHandle bot, const char* msg, uint32_t lang);
    bool CB_Whisper(BotHandle bot, uint64_t target_guid, const char* msg);
    bool CB_TellPlayer(BotHandle bot, uint64_t target_guid, const char* msg);
    bool CB_UseItem(BotHandle bot, uint32_t item_id, UnitHandle target);
    bool CB_Taunt(BotHandle bot, UnitHandle target);
    bool CB_TeleportTo(BotHandle bot, uint32_t map_id, float x, float y, float z, float o);
    bool CB_GetPlayerPosition(BotHandle bot, uint64_t player_guid, BotPosition* out_pos);
    bool CB_SummonToPlayer(BotHandle bot, uint64_t requester_guid);

    // ── Group / raid ────────────────────────────────────────────────────────
    UnitHandle CB_GroupGetTank(BotHandle bot);
    UnitHandle CB_GroupGetHealer(BotHandle bot);
    uint8_t    CB_GroupGetRole(BotHandle bot, UnitHandle member);
    UnitHandle CB_GetUnitWithRaidIcon(BotHandle bot, uint8_t icon);

    // ── Death / resurrection ───────────────────────────────────────────────
    bool        CB_AcceptResurrect(BotHandle bot);
    BotPosition CB_GetCorpsePosition(BotHandle bot);
    bool        CB_UseSpiritHealer(BotHandle bot);
    bool        CB_ResurrectSelf(BotHandle bot);

    // ── Mount ──────────────────────────────────────────────────────────────
    bool CB_IsMounted(BotHandle bot);
    bool CB_MountUp(BotHandle bot);
    bool CB_Dismount(BotHandle bot);
    bool CB_IsIndoor(BotHandle bot);

    // ── Loot ───────────────────────────────────────────────────────────────
    UnitHandle* CB_GetNearbyLootable(BotHandle bot, float range, uint32_t* out_count);
    bool        CB_OpenLoot(BotHandle bot, UnitHandle target);
    bool        CB_TakeAllLoot(BotHandle bot);

    // ── NPC interaction ────────────────────────────────────────────────────
    UnitHandle* CB_GetNearbyNpcs(BotHandle bot, float range, uint32_t npc_flags,
                                  uint32_t* out_count);
    bool        CB_InteractNpc(BotHandle bot, UnitHandle npc);
    bool        CB_RepairAll(BotHandle bot);
    bool        CB_SellGreyItems(BotHandle bot);
    bool        CB_HasSellableItems(BotHandle bot);
    float       CB_GetDurabilityPct(BotHandle bot);

    // ── Quest ──────────────────────────────────────────────────────────────
    BotQuestInfo* CB_GetQuestLog(BotHandle bot, uint32_t* out_count);
    void          CB_FreeQuestLog(BotQuestInfo* list);
    bool          CB_AcceptAllQuests(BotHandle bot, UnitHandle npc);
    bool          CB_TurnInQuest(BotHandle bot, UnitHandle npc, uint32_t quest_id);

    // ── Unit queries (extended) ────────────────────────────────────────────
    bool    CB_IsAttackable(BotHandle bot, UnitHandle target);
    uint8_t CB_GetUnitLevel(BotHandle bot, UnitHandle target);
    bool    CB_IsCastingInterruptible(BotHandle bot, UnitHandle target);

    // ── Pet management ─────────────────────────────────────────────────────
    bool    CB_HasPet(BotHandle bot);
    bool    CB_PetIsAlive(BotHandle bot);
    uint8_t CB_PetHappiness(BotHandle bot);
    bool    CB_SummonPet(BotHandle bot);
    bool    CB_RevivePet(BotHandle bot);
    bool    CB_FeedPet(BotHandle bot);

    // ── Dispel / party queries ─────────────────────────────────────────────
    BotDispelTarget CB_FindDispellableTarget(BotHandle bot);
    UnitHandle      CB_FindDeadPartyMember(BotHandle bot);

    // ── Battleground ───────────────────────────────────────────────────────
    bool            CB_IsInBattleground(BotHandle bot);
    uint8_t         CB_BattlegroundType(BotHandle bot);
    BotSafePosition CB_GetBgObjective(BotHandle bot);
    bool            CB_CaptureBgObjective(BotHandle bot);
    UnitHandle*     CB_GetNearbyEnemies(BotHandle bot, float range, uint32_t* out_count);

    // ── RPG / social ───────────────────────────────────────────────────────
    BotSafePosition CB_GetRandomPointNearby(BotHandle bot, float range);
    bool            CB_Emote(BotHandle bot, uint32_t emote_id);
    UnitHandle*     CB_GetNearbyGossipNpcs(BotHandle bot, float range, uint32_t* out_count);

    // ── Gathering ──────────────────────────────────────────────────────────
    bool        CB_HasGatheringSkill(BotHandle bot);
    uint64_t*   CB_GetNearbyGatherables(BotHandle bot, float range, uint32_t* out_count);
    void        CB_FreeGatherableList(uint64_t* list);
    bool        CB_GatherNode(BotHandle bot, uint64_t handle);
    float       CB_GameobjectDistance(BotHandle bot, uint64_t handle);
    BotPosition CB_GameobjectPosition(BotHandle bot, uint64_t handle);
    uint64_t    CB_NearbyGameObjectByEntry(BotHandle bot, uint32_t entry, float range);
    bool        CB_UseGameObject(BotHandle bot, uint64_t handle);

    // ── Factory: inventory mutation ────────────────────────────────────────
    void     CB_InventoryDestroyEquippedAndBags(BotHandle bot);
    void     CB_InventoryDestroyAll(BotHandle bot);
    uint32_t CB_ItemCountInBags(BotHandle bot, uint32_t item_id);
    uint32_t CB_InventoryAddItem(BotHandle bot, uint32_t item_id, uint32_t count);
    uint32_t CB_ItemMaxStackSize(BotHandle bot, uint32_t item_id);

    // ── Factory: consumable selection ──────────────────────────────────────
    uint32_t CB_FactoryPickPotionForLevel(BotHandle bot, uint32_t level, uint32_t effect);
    uint32_t CB_FactoryPickFoodForLevel(BotHandle bot, uint32_t level, uint32_t category);

    // ── RNG ─────────────────────────────────────────────────────────────────
    uint32_t CB_RandomU32(BotHandle bot, uint32_t min, uint32_t max);

    // ── Factory: progression wipe ──────────────────────────────────────────
    void CB_BotClearSkill(BotHandle bot, uint32_t skill_id);
    void CB_BotResetSpells(BotHandle bot);
    void CB_BotResetAllQuests(BotHandle bot);

    // ── Factory: misc pre/post init ────────────────────────────────────────
    void CB_BotRemoveAllAuras(BotHandle bot);
    bool CB_BotHasSkill(BotHandle bot, uint32_t skill_id);
    void CB_BotLearnSpell(BotHandle bot, uint32_t spell_id);
    void CB_BotLearnDefaultSpells(BotHandle bot);
    void CB_BotLearnClassLevelSpells(BotHandle bot, bool include_quest_rewards);

    // ── Spell store queries ────────────────────────────────────────────────
    BotSpellInfo CB_GetSpellInfo(BotHandle bot, uint32_t spell_id);
    uint32_t*    CB_GetBotSpells(BotHandle bot, uint32_t* out_count);
    void         CB_FreeBotSpells(uint32_t* list);

    // ── Factory: bag slot management ───────────────────────────────────────
    uint32_t CB_BotEmptyBagSlotCount(BotHandle bot);
    bool     CB_BotStoreNewInBestSlots(BotHandle bot, uint32_t item_id, uint32_t count);

    // ── Factory: reputation ────────────────────────────────────────────────
    bool     CB_BotSetReputation(BotHandle bot, uint32_t faction_id, int32_t value);

    // ── Factory: ammo management ───────────────────────────────────────────
    uint32_t CB_BotEquippedRangedSubclass(BotHandle bot);
    uint32_t CB_BotCurrentAmmoId(BotHandle bot);
    uint32_t CB_FactoryPickAmmoForLevel(BotHandle bot, uint32_t level, uint32_t ammo_subclass);
    void     CB_BotSetAmmo(BotHandle bot, uint32_t item_id);

    // ── Factory: skills ────────────────────────────────────────────────────
    uint32_t CB_BotGetSkillValue(BotHandle bot, uint32_t skill_id);
    void     CB_BotSetSkill(BotHandle bot, uint32_t skill_id, uint32_t value, uint32_t max);
    void     CB_BotUpdateSkillsForLevel(BotHandle bot);

    // ── Factory: item prototype queries ────────────────────────────────────
    uint32_t CB_ItemPrototypeQuality(BotHandle bot, uint32_t item_id);

    // ── Factory: random item picks ─────────────────────────────────────────
    uint32_t CB_FactoryPickTradeForLevel(BotHandle bot, uint32_t level);

    // ── Factory: config list queries ───────────────────────────────────────
    uint32_t* CB_GetRandomBotSpellIds(BotHandle bot, uint32_t* out_count);

    // ── Factory: taxi nodes ────────────────────────────────────────────────
    BotTaxiNode* CB_GetOverworldTaxiNodes(BotHandle bot, uint8_t team, uint32_t* out_count);
    void         CB_FreeTaxiNodes(BotTaxiNode* list);
    void         CB_BotSetTaxiNode(BotHandle bot, uint32_t node_index);

    // ── Factory: talents ───────────────────────────────────────────────────
    BotTalentEntry* CB_GetClassTalents(BotHandle bot, uint8_t spec_no, uint32_t* out_count);
    void            CB_FreeClassTalents(BotTalentEntry* list);
    uint32_t        CB_BotFreeTalentPoints(BotHandle bot);
    void            CB_BotUpdateFreeTalentPoints(BotHandle bot);
    uint32_t        CB_BotPickSpecNo(BotHandle bot, bool incremental);

    // ── Chat-command helpers (Wave 2) ──────────────────────────────────────
    bool                CB_BotJump(BotHandle bot);
    bool                CB_BotUseHearthstone(BotHandle bot);
    BotReputationEntry* CB_BotGetReputationList(BotHandle bot, uint32_t* out_count);
    void                CB_BotFreeReputationList(BotReputationEntry* list);
    BotSkillEntry*      CB_BotGetLearnedSkills(BotHandle bot, uint32_t* out_count);
    void                CB_BotFreeSkillList(BotSkillEntry* list);
    bool                CB_BotQuestAcceptFrom(BotHandle bot, UnitHandle npc);
    bool                CB_BotQuestAbandon(BotHandle bot, uint32_t quest_id);

    // ── Chat-command helpers (Wave 3: mail + guild) ────────────────────────
    BotMailSummary      CB_BotMailSummary(BotHandle bot);
    bool                CB_BotMailTakeAll(BotHandle bot);
    bool                CB_BotGuildLeave(BotHandle bot);

    // ── Internal helpers ────────────────────────────────────────────────────
    Player* FindBot(BotHandle bot);
    Unit*   FindUnit(BotHandle bot, UnitHandle handle);
    BotUnitSnapshot FillUnitSnapshot(Unit* unit);
} // namespace BotBridge
