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
    UnitHandle* CB_GetAttackers(BotHandle bot, uint32_t* out_count);
    void        CB_FreeUnitList(UnitHandle* list);
    bool        CB_BotIsBehind(BotHandle bot, UnitHandle target);
    uint32_t    CB_BotEquippedWeaponSubclass(BotHandle bot, uint8_t slot);
    uint32_t    CB_BotItemCount(BotHandle bot, uint32_t item_id);
    uint8_t     CB_BotActiveTotemMask(BotHandle bot);
    bool        CB_BotWeaponEnchanted(BotHandle bot, uint8_t slot);
    uint8_t     CB_BotRunesReadyMask(BotHandle bot);
    bool        CB_BotKnowsSpell(BotHandle bot, uint32_t spell_id);
    uint32_t    CB_GetSpellName(uint32_t spell_id, char* buf, uint32_t buf_len);

    // ── Pathfinding / positioning ──────────────────────────────────────────
    BotPosition     CB_GetBehindPosition(BotHandle bot, UnitHandle target, float distance);
    BotSafePosition CB_GetSafePosition(BotHandle bot, float search_radius);
    BotPosition     CB_GetSpreadPosition(BotHandle bot, UnitHandle center, float radius,
                                          uint8_t idx, uint8_t total);
    bool            CB_CanReach(BotHandle bot, float x, float y, float z);
    bool            CB_CanPathfindTo(BotHandle bot, float x, float y, float z);

    // ── Commands ───────────────────────────────────────────────────────────
    bool CB_CastSpell(BotHandle bot, uint32_t spell_id, UnitHandle target);
    bool CB_CastSpellPos(BotHandle bot, uint32_t spell_id, float x, float y, float z);
    bool CB_MoveTo(BotHandle bot, float x, float y, float z);
    bool CB_Follow(BotHandle bot, UnitHandle target, float dist, float angle);
    bool CB_Chase(BotHandle bot, UnitHandle target, float dist, float angle);
    bool CB_StopMoving(BotHandle bot);
    void CB_SetFacing(BotHandle bot, float angle);
    bool CB_Attack(BotHandle bot, UnitHandle target);
    bool CB_AutoAttack(BotHandle bot, bool enable);
    bool CB_AutoShoot(BotHandle bot, UnitHandle target);
    bool CB_Say(BotHandle bot, const char* msg, uint32_t lang);
    bool CB_Whisper(BotHandle bot, uint64_t target_guid, const char* msg);
    bool CB_TellPlayer(BotHandle bot, uint64_t target_guid, const char* msg);
    bool CB_TellAddon(BotHandle bot, uint64_t target_guid, const char* msg);
    bool CB_SendGroupAddonMessage(BotHandle bot, const char* prefix, const char* msg);
    bool CB_UseItem(BotHandle bot, uint32_t item_id, UnitHandle target);
    uint32_t CB_FindFoodDrinkInBags(BotHandle bot, uint32_t category);
    bool CB_Taunt(BotHandle bot, UnitHandle target);
    bool CB_TeleportTo(BotHandle bot, uint32_t map_id, float x, float y, float z, float o);
    bool CB_GetPlayerPosition(BotHandle bot, uint64_t player_guid, BotPosition* out_pos);
    bool CB_SummonToPlayer(BotHandle bot, uint64_t requester_guid);

    /// Clear the per-bot movement dedup cache. Called from HandleTeleportAck
    /// so the next follow/chase after teleport isn't falsely deduped.
    void ClearMoveState(BotHandle bot);

    // ── Group / raid ────────────────────────────────────────────────────────
    UnitHandle CB_GroupGetTank(BotHandle bot);
    UnitHandle CB_GroupGetHealer(BotHandle bot);
    uint8_t    CB_GroupGetRole(BotHandle bot, UnitHandle member);
    UnitHandle CB_GetUnitWithRaidIcon(BotHandle bot, uint8_t icon);
    bool       CB_GroupSetTargetIcon(BotHandle bot, UnitHandle target, uint8_t icon);

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
    uint8_t CB_UnitKind(BotHandle bot, UnitHandle target);

    // ── Pet management ─────────────────────────────────────────────────────
    bool    CB_HasPet(BotHandle bot);
    bool    CB_PetIsAlive(BotHandle bot);
    uint8_t CB_PetHappiness(BotHandle bot);
    uint8_t CB_PetHealthPct(BotHandle bot);
    bool    CB_SummonPet(BotHandle bot);
    bool    CB_RevivePet(BotHandle bot);
    bool    CB_FeedPet(BotHandle bot);

    // ── Dispel / party queries ─────────────────────────────────────────────
    BotDispelTarget CB_FindDispellableTarget(BotHandle bot, uint8_t dispel_mask);
    UnitHandle      CB_FindDeadPartyMember(BotHandle bot);

    // ── Consumables: potion query ──────────────────────────────────────────
    uint32_t CB_FindPotionInBags(BotHandle bot, uint8_t category);
    bool     CB_PotionCooldownReady(BotHandle bot);

    // ── Consumables: trinket activation (11h) ──────────────────────────────
    bool     CB_UseTrinket(BotHandle bot, uint8_t slot);

    // ── Social / group actions (11i) ────────────────────────────────────────
    bool CB_AcceptGroupInvite(BotHandle bot);
    bool CB_LeaveGroup(BotHandle bot);
    bool CB_AcceptReadyCheck(BotHandle bot);
    bool CB_AcceptTrade(BotHandle bot);
    bool CB_AcceptDuel(BotHandle bot);
    bool CB_DeclineDuel(BotHandle bot);

    // ── PvP / duel / faction (11d) ─────────────────────────────────────────
    bool    CB_IsPvpFlagged(BotHandle bot);
    uint8_t CB_DuelState(BotHandle bot);
    uint8_t CB_ReputationRank(BotHandle bot, uint32_t faction_id);

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
    void CB_BotRemoveAuraById(BotHandle bot, uint32_t spell_id);
    bool CB_BotHasSkill(BotHandle bot, uint32_t skill_id);
    void CB_BotLearnSpell(BotHandle bot, uint32_t spell_id);
    void CB_BotRemoveSpell(BotHandle bot, uint32_t spell_id);
    void CB_BotLearnDefaultSpells(BotHandle bot);
    void CB_BotLearnClassLevelSpells(BotHandle bot, bool include_quest_rewards);

    // ── Spell store queries ────────────────────────────────────────────────
    BotSpellInfo CB_GetSpellInfo(BotHandle bot, uint32_t spell_id);
    uint32_t*    CB_GetBotSpells(BotHandle bot, uint32_t* out_count);
    void         CB_FreeBotSpells(uint32_t* list);
    uint32_t     CB_ResolveSpellByName(BotHandle bot, const char* name);
    uint32_t     CB_ResolveItemByName(BotHandle bot, const char* name);
    bool         CB_EquipItem(BotHandle bot, uint32_t item_id);
    bool         CB_GiveLeader(BotHandle bot, uint64_t target_guid);
    uint64_t     CB_ResolvePlayerByName(BotHandle bot, const char* name);
    bool         CB_UnequipItem(BotHandle bot, uint32_t item_id);
    bool         CB_InviteToGroup(BotHandle bot, uint64_t target_guid);
    bool         CB_DestroyItem(BotHandle bot, uint32_t item_id);
    bool         CB_ShareQuest(BotHandle bot, uint32_t quest_id);
    bool         CB_DoTextEmote(BotHandle bot, uint32_t text_emote_id);

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

    // ── Factory: refresh (cheat mask + DB save) ────────────────────────────
    uint32_t CB_BotCheatMask(BotHandle bot);
    void     CB_BotSaveToDbIfNotBusy(BotHandle bot);

    // ── Factory: prepare ───────────────────────────────────────────────────
    void CB_BotResurrectFull(BotHandle bot);
    void CB_BotCombatStop(BotHandle bot);
    void CB_BotSetLevelAndResetXp(BotHandle bot, uint32_t level);
    void CB_BotSetPlayerFlag(BotHandle bot, uint32_t flag, bool set);
    bool CB_FactoryConfigDisableRandomLevels(BotHandle bot);
    bool CB_FactoryConfigRandomBotShowHelmet(BotHandle bot);
    bool CB_FactoryConfigRandomBotShowCloak(BotHandle bot);

    // ── Factory: Randomize orchestration ───────────────────────────────────
    bool     CB_FactoryIsRandomBot(BotHandle bot);
    bool     CB_FactoryHasRealPlayerMaster(BotHandle bot);
    bool     CB_FactoryIsInRealGuild(BotHandle bot);
    uint32_t CB_FactoryConfigMinEnchantingBotLevel(BotHandle bot);
    void     CB_FactoryLoadEnchantContainer(BotHandle bot);
    void     CB_BotResetTalents(BotHandle bot);
    void     CB_BotLearnQuestRewardedSpells(BotHandle bot);
    uint32_t CB_BotGetMoney(BotHandle bot);
    void     CB_BotSetMoney(BotHandle bot, uint32_t amount);
    void     CB_FactoryInitAllGems(BotHandle bot);
    void     CB_FactoryEnchantAllEquipment(BotHandle bot);

    // ── Factory: quests ────────────────────────────────────────────────────
    bool CB_QuestIsEligibleForBot(BotHandle bot, uint32_t quest_id);
    void CB_BotRewardQuestComplete(BotHandle bot, uint32_t quest_id);

    // ── Factory: arena team ────────────────────────────────────────────────
    uint32_t CB_BotGetAccountId(BotHandle bot);

    // ── Factory: guild ─────────────────────────────────────────────────────
    bool     CB_FactoryQueryGuildSummary(BotHandle bot,
                                         uint32_t guild_id,
                                         uint8_t* out_leader_team,
                                         uint32_t* out_member_size,
                                         uint32_t* out_max_members_hint,
                                         char* out_name,
                                         uint32_t name_buf_len);
    bool     CB_FactoryGuildAddMember(BotHandle bot, uint32_t guild_id, uint32_t rank_id);
    bool     CB_FactoryGetGuildRankName(BotHandle bot,
                                        uint32_t guild_id,
                                        uint32_t rank_id,
                                        char* out_name,
                                        uint32_t name_buf_len);
    uint32_t CB_FactoryBotGuildId(BotHandle bot);

    // ── Factory: per-bot KV store ──────────────────────────────────────────
    uint32_t CB_FactoryKvGetU32(BotHandle bot, const char* key);
    void     CB_FactoryKvSetU32(BotHandle bot, const char* key, uint32_t value);
    void     CB_FactoryLearnTradeskillRecipes(BotHandle bot);

    // ── Factory: equipment ────────────────────────────────────────────────
    uint32_t CB_FactoryBotGuidLow(BotHandle bot);
    uint32_t CB_FactoryBotEquippedItemInSlot(BotHandle bot, uint8_t slot);
    void     CB_FactoryDestroyAllEquippedItems(BotHandle bot);
    bool     CB_FactoryEquipNewItemInSlot(BotHandle bot,
                                          uint8_t slot,
                                          uint32_t item_id,
                                          uint32_t random_enchant_id,
                                          bool apply_enchants);
    void     CB_FactoryInitStatsForLevelAndUpdate(BotHandle bot);
    bool     CB_FactoryMasterEquipGearScore(BotHandle bot, uint32_t* out_gs);
    void     CB_FactoryTellMaster(BotHandle bot, const char* msg);

    // ── Factory: pet ──────────────────────────────────────────────────────
    bool      CB_FactoryBotHasPet(BotHandle bot);
    uint32_t  CB_FactoryPetEntry(BotHandle bot);
    uint32_t  CB_FactoryPetFamily(BotHandle bot);
    uint32_t  CB_FactoryPetLevel(BotHandle bot);
    bool      CB_FactoryPetHasSpell(BotHandle bot, uint32_t spell_id);
    uint32_t* CB_FactoryPetAutocastCandidateSpells(BotHandle bot, uint32_t* out_count);
    uint32_t* CB_FactoryTameableCreaturesForBotLevel(BotHandle bot, uint32_t* out_count);
    bool      CB_FactoryCreateHunterPet(BotHandle bot, uint32_t creature_entry);
    void      CB_FactoryPetRefreshStats(BotHandle bot);
    void      CB_FactoryPetLearnSpell(BotHandle bot, uint32_t spell_id);
    void      CB_FactoryPetToggleAutocast(BotHandle bot, uint32_t spell_id, bool enable);
    void      CB_FactoryPetForceDismiss(BotHandle bot);

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
    uint32_t        CB_BotGetSpecTab(BotHandle bot);

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

    // ── RTSC / file I/O helpers ────────────────────────────────────────────
    void CB_BotSummonMarkerCreature(BotHandle bot, uint32_t entry,
                                    float x, float y, float z, float o,
                                    uint32_t despawn_ms, float scale);
    bool CB_BotWriteLogFile(BotHandle bot, const char* name, const char* body);
    bool CB_BotReadLogFile(BotHandle bot, const char* name, char** out_body);
    void CB_BotFreeString(char* s);
    bool CB_BotAppendLogFile(BotHandle bot, const char* name, const char* line);


    // ── Travel destination queries ────────────────────────────────────────
    BotTravelDest*  CB_BotFindTravelDests(BotHandle bot, uint32_t purpose_flags,
                                           float max_range, uint32_t max_results,
                                           uint32_t* out_count);
    void            CB_BotFreeTravelDests(BotTravelDest* list);

    // ── World buffs ───────────────────────────────────────────────────────
    bool     CB_AddAura(BotHandle bot, uint32_t spell_id);
    uint32_t CB_GetNeededWorldBuffs(BotHandle bot, uint32_t* out_spells, uint32_t max_out);

    // ── Heal interrupt ────────────────────────────────────────────────────
    bool CB_InterruptOwnCast(BotHandle bot);

    // ── NPC interaction ───────────────────────────────────────────────────
    bool CB_GossipHello(BotHandle bot, uint32_t npc_entry);
    bool CB_BuyFromVendor(BotHandle bot, uint32_t item_id, uint32_t qty);

    // ── Mail ──────────────────────────────────────────────────────────────
    bool CB_MailItemToMaster(BotHandle bot);

    // ── Bank ──────────────────────────────────────────────────────────────
    bool CB_BankDeposit(BotHandle bot);
    bool CB_BankWithdraw(BotHandle bot);

    // ── Auction house ─────────────────────────────────────────────────────
    bool CB_AhPost(BotHandle bot);
    bool CB_AhBid(BotHandle bot);

    // ── Outfit ────────────────────────────────────────────────────────────
    bool CB_ApplyOutfit(BotHandle bot);

    // ── Fishing ───────────────────────────────────────────────────────────
    bool CB_StartFishing(BotHandle bot);

    // ── BG/Arena ──────────────────────────────────────────────────────────
    bool        CB_QueueBg(BotHandle bot);
    bool        CB_AcceptBgInvite(BotHandle bot);
    BotPosition CB_GetBgObjectivePos(BotHandle bot, uint8_t objective_type);

    // ── LFG ───────────────────────────────────────────────────────────────
    bool CB_LfgJoin(BotHandle bot);
    bool CB_LfgAccept(BotHandle bot);

    // ── Dungeon awareness ─────────────────────────────────────────────────
    BotPosition CB_GetTankPosition(BotHandle bot);
    bool        CB_IsUnitCc(BotHandle bot, UnitHandle target);

    // ── Debug ─────────────────────────────────────────────────────────────
    bool CB_DebugDumpState(BotHandle bot, uint8_t kind);

    // EngBags: inventory enumeration
    BotInventoryItem* CB_BotGetInventory(BotHandle bot, uint32_t* out_count);
    BotInventoryItem* CB_BotGetEquipped(BotHandle bot, uint32_t* out_count);
    BotInventoryItem* CB_BotGetBankItems(BotHandle bot, uint32_t* out_count);
    BotInventoryItem* CB_BotGetMailItems(BotHandle bot, uint32_t* out_count);
    void              CB_BotFreeInventoryList(BotInventoryItem* list);

    // EngBags: item actions
    bool CB_SellItem(BotHandle bot, uint32_t item_id);
    bool CB_BankDepositItem(BotHandle bot, uint32_t item_id);
    bool CB_BankWithdrawItem(BotHandle bot, uint32_t item_id);
    bool CB_BotMailTakeIndex(BotHandle bot, uint32_t mail_index);
    bool CB_SendMailItem(BotHandle bot, uint32_t item_id);
    bool CB_TradeAddItem(BotHandle bot, uint32_t item_id, uint32_t count);
    static uint32_t CB_GetSpellCraftItem(uint32_t spell_id);
    static bool CB_GetItemInfo(uint32_t item_id, char* name_buf, uint32_t name_buf_len, uint8_t* out_quality);

    // ── Internal helpers ────────────────────────────────────────────────────
    Player* FindBot(BotHandle bot);
    Unit*   FindUnit(BotHandle bot, UnitHandle handle);
    BotUnitSnapshot FillUnitSnapshot(Unit* unit);
} // namespace BotBridge
