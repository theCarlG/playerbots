/**
 * BotBridge.cpp — CMaNGOS implementation of the BotCallbacks vtable.
 *
 * Every callback:
 *   1. Resolves BotHandle → Player* (and UnitHandle → Unit*) via ObjectAccessor.
 *   2. Calls the CMaNGOS API.
 *   3. Returns a plain-data result (no pointers into CMaNGOS memory).
 *
 * Threading: these are called on the map worker thread that owns the bot's Player.
 * CMaNGOS guarantees single-threaded access to each Player's UpdateAI, but
 * bots on different maps run on different threads. Any shared mutable state
 * (like s_moveState) must be thread_local or mutex-protected.
 */

#include "botpch.h"
#include "BotBridge.h"

#include <cstdlib>
#include <unordered_map>
#include <unordered_set>
#include <queue>
#include <algorithm>

#include "Entities/Player.h"
#include "Entities/Unit.h"
#include "Entities/Creature.h"
#include "Entities/Pet.h"
#include "Entities/Corpse.h"
#include "Entities/GameObject.h"
#include "Entities/Bag.h"
#include "Entities/Item.h"
#include "Entities/ItemPrototype.h"
#include "BotConfig.h"
#include "playerbot/PlayerbotAI.h"
// RandomItemMgr façade eliminated in Phase K — calls inlined to
// playerbot_itempool_* FFI entry points.
#include "RandomPlayerbotMgr.h"
#include "Util/Util.h"
#include "Globals/ObjectAccessor.h"
#include "Spells/SpellMgr.h"
#include "Spells/Spell.h"
#include "Spells/SpellAuras.h"
#include "Maps/Map.h"
#include "MotionGenerators/MotionMaster.h"
#include "MotionGenerators/PathFinder.h"
#include "Globals/ObjectMgr.h"
#include "Loot/LootMgr.h"
#include "Grids/GridNotifiers.h"
#include "Grids/GridNotifiersImpl.h"
#include "Grids/CellImpl.h"
#include "Server/SQLStorages.h"
#include "Server/DBCStores.h"
#include "Reputation/ReputationMgr.h"
#include "Mails/Mail.h"
#include "Guilds/Guild.h"
#include "Guilds/GuildMgr.h"
#include "Server/WorldSession.h"
#include "Server/Opcodes.h"
#include "Server/WorldPacket.h"
#include "Entities/Transports.h"
#include "Maps/TransportMgr.h"
#include "AI/ScriptDevAI/base/escort_ai.h"
#include "Chat/ChannelMgr.h"
#include "Chat/Channel.h"
#include "BattleGround/BattleGround.h"
#include "BattleGround/BattleGroundWS.h"
#include "BattleGround/BattleGroundAB.h"
#include "BattleGround/BattleGroundMgr.h"
#include "BattleGround/BattleGroundDefines.h"
#include "AuctionHouse/AuctionHouseMgr.h"
#include "Chat/Chat.h"
#include "Groups/Group.h"
#include "Config/Config.h"
#include "Entities/EntitiesMgr.h"

#include <cstdio>
#include <cstring>
#include <fstream>
#include <sstream>

#ifdef CMANGOS
#include "Combat/ThreatManager.h"
#endif

#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

// ── Platform helpers ──────────────────────────────────────────────────────

static inline ObjectGuid MakeGuid(uint64_t handle)
{
    return ObjectGuid(handle);
}

// ── Internal helpers ──────────────────────────────────────────────────────

Player* BotBridge::FindBot(BotHandle bot)
{
    return sObjectAccessor.FindPlayer(MakeGuid(bot));
}

Unit* BotBridge::FindUnit(BotHandle bot, UnitHandle handle)
{
    if (handle == 0)
        return nullptr;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;
    // Use the bot's map to find any unit (player, creature, pet)
    return b->GetMap()->GetUnit(MakeGuid(handle));
}

BotUnitSnapshot BotBridge::FillUnitSnapshot(Unit* unit)
{
    BotUnitSnapshot s{};
    if (!unit)
        return s;

    s.health     = unit->GetHealth();
    s.max_health = unit->GetMaxHealth();
    s.is_alive   = unit->IsAlive();
    s.is_ghost   = false;
    s.on_taxi    = false;
    if (Player* p = unit->ToPlayer())
    {
        s.is_ghost = p->HasFlag(PLAYER_FLAGS, PLAYER_FLAGS_GHOST);
        s.on_taxi  = p->IsTaxiFlying();
    }
    s.in_combat  = unit->IsInCombat();
    s.is_casting = unit->IsNonMeleeSpellCasted(false, false, true);
    s.is_moving  = unit->IsMoving();
    s.is_channeling = unit->GetCurrentSpell(CURRENT_CHANNELED_SPELL) != nullptr;

    if (s.is_casting)
    {
        Spell* spell = unit->GetCurrentSpell(CURRENT_GENERIC_SPELL);
        if (!spell)
            spell = unit->GetCurrentSpell(CURRENT_CHANNELED_SPELL);
        if (spell && spell->m_spellInfo)
            s.casting_spell_id = spell->m_spellInfo->Id;
    }
    s.casting_progress = 0.0f; // filled below if casting

    // Power (mana, rage, energy, etc.)
    Powers powerType = unit->GetPowerType();
    s.power_type = static_cast<uint8_t>(powerType);
    s.mana       = unit->GetPower(powerType);
    s.max_mana   = unit->GetMaxPower(powerType);

    // Identity
    s.level    = static_cast<uint8_t>(unit->GetLevel());
    s.race_id  = static_cast<uint8_t>(unit->getRace());
    s.class_id = static_cast<uint8_t>(unit->getClass());
    s.team     = 0; // filled below for Players

    // Position
    s.pos.x    = unit->GetPositionX();
    s.pos.y    = unit->GetPositionY();
    s.pos.z    = unit->GetPositionZ();
    s.pos.o    = unit->GetOrientation();
    s.pos.map_id = unit->GetMapId();

    // Target — prefer combat victim, fall back to player's UI selection.
    if (unit->GetVictim())
        s.current_target = unit->GetVictim()->GetObjectGuid().GetRawValue();
    else if (Player* p = unit->ToPlayer())
        s.current_target = p->GetSelectionGuid().GetRawValue();
    else
        s.current_target = 0;

    // Aura state mask
    s.aura_state_mask = unit->GetUInt32Value(UNIT_FIELD_AURASTATE);

    // Combo points live on Unit but are only populated for rogue/feral
    // players. Rust checks are always evaluated in the context of the bot's
    // current target, so the raw stored count is accurate enough for
    // finisher gating.
    s.combo_points = unit->GetComboPoints();
    // Shapeshift form doubles as warrior stance id in this fork — Rust
    // treats it opaquely and matches on raw u8 values.
    s.shapeshift_form = static_cast<uint8_t>(unit->GetShapeshiftForm());

    s.is_mounted = unit->IsMounted();

    // NPC entry for creature identification (boss detection, etc.)
    if (Creature* c = unit->ToCreature())
    {
        s.npc_entry = c->GetEntry();
        s.creature_type = static_cast<uint8_t>(c->GetCreatureType());
    }

    // Player-specific
    if (Player* p = unit->ToPlayer())
    {
        s.team = static_cast<uint8_t>(p->GetTeam() == ALLIANCE ? 0 : 1);
    }

    return s;
}

// ── BotCallbacks factory ──────────────────────────────────────────────────

BotCallbacks BotBridge::MakeCallbacks()
{
    BotCallbacks cbs{};

    // Snapshot
    cbs.get_snapshot        = CB_GetSnapshot;
    cbs.get_unit_snapshot   = CB_GetUnitSnapshot;

    // Aura queries
    cbs.has_aura            = CB_HasAura;
    cbs.get_aura            = CB_GetAura;
    cbs.get_auras           = CB_GetAuras;
    cbs.free_aura_list      = CB_FreeAuraList;

    // Threat
    cbs.get_threat_list     = CB_GetThreatList;
    cbs.free_threat_list    = CB_FreeThreatList;
    cbs.get_unit_threat     = CB_GetUnitThreat;

    // Unit queries
    cbs.unit_distance       = CB_UnitDistance;
    cbs.can_cast            = CB_CanCast;
    cbs.spell_on_cooldown   = CB_SpellOnCooldown;
    cbs.spell_cooldown_ms   = CB_SpellCooldownMs;
    cbs.has_los             = CB_HasLos;
    cbs.get_nearby_units    = CB_GetNearbyUnits;
    cbs.get_attackers       = CB_GetAttackers;
    cbs.free_unit_list      = CB_FreeUnitList;
    cbs.bot_is_behind               = CB_BotIsBehind;
    cbs.bot_is_at_flank             = CB_BotIsAtFlank;
    cbs.bot_equipped_weapon_subclass = CB_BotEquippedWeaponSubclass;
    cbs.bot_item_count              = CB_BotItemCount;
    cbs.bot_active_totem_mask       = CB_BotActiveTotemMask;
    cbs.bot_weapon_enchanted        = CB_BotWeaponEnchanted;
    cbs.bot_runes_ready_mask        = CB_BotRunesReadyMask;
    cbs.bot_knows_spell             = CB_BotKnowsSpell;
    cbs.get_spell_name              = CB_GetSpellName;

    // Pathfinding / positioning
    cbs.get_behind_position = CB_GetBehindPosition;
    cbs.get_safe_position   = CB_GetSafePosition;
    cbs.get_spread_position = CB_GetSpreadPosition;
    cbs.can_reach           = CB_CanReach;
    cbs.can_pathfind_to     = CB_CanPathfindTo;

    // Commands
    cbs.cast_spell          = CB_CastSpell;
    cbs.cast_spell_pos      = CB_CastSpellPos;
    cbs.move_to             = CB_MoveTo;
    cbs.follow              = CB_Follow;
    cbs.chase               = CB_Chase;
    cbs.stop_moving         = CB_StopMoving;
    cbs.set_facing          = CB_SetFacing;
    cbs.attack              = CB_Attack;
    cbs.auto_attack         = CB_AutoAttack;
    cbs.auto_shoot          = CB_AutoShoot;
    cbs.say                 = CB_Say;
    cbs.tell_player         = CB_TellPlayer;
    cbs.whisper             = CB_Whisper;
    cbs.use_item            = CB_UseItem;
    cbs.find_food_drink_in_bags = CB_FindFoodDrinkInBags;
    cbs.taunt               = CB_Taunt;
    cbs.teleport_to         = CB_TeleportTo;
    cbs.get_player_position = CB_GetPlayerPosition;
    cbs.summon_to_player    = CB_SummonToPlayer;

    // Group / raid
    cbs.group_get_tank      = CB_GroupGetTank;
    cbs.group_get_healer    = CB_GroupGetHealer;
    cbs.group_get_role      = CB_GroupGetRole;
    cbs.get_unit_with_raid_icon = CB_GetUnitWithRaidIcon;
    cbs.group_set_target_icon   = CB_GroupSetTargetIcon;

    // Death / resurrection
    cbs.accept_resurrect    = CB_AcceptResurrect;
    cbs.get_corpse_position = CB_GetCorpsePosition;
    cbs.use_spirit_healer   = CB_UseSpiritHealer;
    cbs.resurrect_self      = CB_ResurrectSelf;

    // Mount
    cbs.is_mounted          = CB_IsMounted;
    cbs.mount_up            = CB_MountUp;
    cbs.dismount            = CB_Dismount;
    cbs.is_indoor           = CB_IsIndoor;

    // Loot
    cbs.get_nearby_lootable = CB_GetNearbyLootable;
    cbs.open_loot           = CB_OpenLoot;
    cbs.take_all_loot       = CB_TakeAllLoot;

    // NPC interaction
    cbs.get_nearby_npcs     = CB_GetNearbyNpcs;
    cbs.interact_npc        = CB_InteractNpc;
    cbs.repair_all          = CB_RepairAll;
    cbs.sell_grey_items     = CB_SellGreyItems;
    cbs.has_sellable_items  = CB_HasSellableItems;
    cbs.get_durability_pct  = CB_GetDurabilityPct;

    // Quest
    cbs.get_quest_log       = CB_GetQuestLog;
    cbs.free_quest_log      = CB_FreeQuestLog;
    cbs.accept_all_quests   = CB_AcceptAllQuests;
    cbs.turn_in_quest       = CB_TurnInQuest;

    // Unit queries (extended)
    cbs.is_attackable       = CB_IsAttackable;
    cbs.get_unit_level      = CB_GetUnitLevel;
    cbs.is_casting_interruptible = CB_IsCastingInterruptible;
    cbs.unit_kind           = CB_UnitKind;

    // Pet management
    cbs.has_pet             = CB_HasPet;
    cbs.pet_is_alive        = CB_PetIsAlive;
    cbs.pet_happiness       = CB_PetHappiness;
    cbs.pet_health_pct      = CB_PetHealthPct;
    cbs.summon_pet          = CB_SummonPet;
    cbs.revive_pet          = CB_RevivePet;
    cbs.feed_pet            = CB_FeedPet;
    cbs.pet_attack          = CB_PetAttack;
    cbs.cast_pet_spell      = CB_CastPetSpell;

    // Dispel / party queries
    cbs.find_dispellable_target = CB_FindDispellableTarget;
    cbs.find_potion_in_bags     = CB_FindPotionInBags;
    cbs.potion_cooldown_ready   = CB_PotionCooldownReady;
    cbs.use_trinket             = CB_UseTrinket;
    cbs.accept_group_invite     = CB_AcceptGroupInvite;
    cbs.leave_group             = CB_LeaveGroup;
    cbs.accept_ready_check      = CB_AcceptReadyCheck;
    cbs.accept_trade            = CB_AcceptTrade;
    cbs.accept_duel             = CB_AcceptDuel;
    cbs.decline_duel            = CB_DeclineDuel;
    cbs.accept_summon           = nullptr; // PB2 doesn't handle summon responses
    cbs.use_meeting_stone       = nullptr; // PB2 doesn't handle meeting stones
    cbs.is_pvp_flagged          = CB_IsPvpFlagged;
    cbs.duel_state              = CB_DuelState;
    cbs.reputation_rank         = CB_ReputationRank;
    cbs.find_dead_party_member  = CB_FindDeadPartyMember;

    // Battleground
    cbs.is_in_battleground  = CB_IsInBattleground;
    cbs.battleground_type   = CB_BattlegroundType;
    cbs.get_bg_objective    = CB_GetBgObjective;
    cbs.capture_bg_objective= CB_CaptureBgObjective;
    cbs.get_nearby_enemies  = CB_GetNearbyEnemies;

    // RPG / social
    cbs.get_random_point_nearby = CB_GetRandomPointNearby;
    cbs.emote               = CB_Emote;
    cbs.get_nearby_gossip_npcs  = CB_GetNearbyGossipNpcs;

    // Gathering
    cbs.has_gathering_skill     = CB_HasGatheringSkill;
    cbs.get_nearby_gatherables  = CB_GetNearbyGatherables;
    cbs.free_gatherable_list    = CB_FreeGatherableList;
    cbs.gather_node             = CB_GatherNode;
    cbs.gameobject_distance     = CB_GameobjectDistance;
    cbs.gameobject_position     = CB_GameobjectPosition;
    cbs.nearby_gameobject_by_entry = CB_NearbyGameObjectByEntry;
    cbs.use_gameobject             = CB_UseGameObject;
    cbs.use_nearby_quest_object    = CB_UseNearbyQuestObject;
    cbs.is_quest_objective_creature = CB_IsQuestObjectiveCreature;
    cbs.get_active_escort_npc      = CB_GetActiveEscortNpc;
    cbs.bot_broadcast_random       = CB_BotBroadcastRandom;
    cbs.bot_greet_nearby_player    = CB_BotGreetNearbyPlayer;

    // Factory: inventory mutation
    cbs.inventory_destroy_equipped_and_bags = CB_InventoryDestroyEquippedAndBags;
    cbs.inventory_destroy_all               = CB_InventoryDestroyAll;
    cbs.item_count_in_bags                  = CB_ItemCountInBags;
    cbs.inventory_add_item                  = CB_InventoryAddItem;
    cbs.item_max_stack_size                 = CB_ItemMaxStackSize;

    // Factory: consumable selection
    cbs.factory_pick_potion_for_level       = CB_FactoryPickPotionForLevel;
    cbs.factory_pick_food_for_level         = CB_FactoryPickFoodForLevel;

    // RNG
    cbs.random_u32                          = CB_RandomU32;

    // Factory: progression wipe
    cbs.bot_clear_skill                     = CB_BotClearSkill;
    cbs.bot_reset_spells                    = CB_BotResetSpells;
    cbs.bot_reset_all_quests                = CB_BotResetAllQuests;

    // Factory: misc pre/post init
    cbs.bot_remove_all_auras                = CB_BotRemoveAllAuras;
    cbs.bot_remove_aura_by_id               = CB_BotRemoveAuraById;
    cbs.bot_has_skill                       = CB_BotHasSkill;
    cbs.bot_learn_spell                     = CB_BotLearnSpell;
    cbs.bot_remove_spell                    = CB_BotRemoveSpell;
    cbs.bot_learn_default_spells            = CB_BotLearnDefaultSpells;
    cbs.bot_learn_class_level_spells        = CB_BotLearnClassLevelSpells;

    cbs.get_spell_info                      = CB_GetSpellInfo;
    cbs.get_bot_spells                      = CB_GetBotSpells;
    cbs.free_bot_spells                     = CB_FreeBotSpells;
    cbs.bot_empty_bag_slot_count            = CB_BotEmptyBagSlotCount;
    cbs.bot_store_new_in_best_slots         = CB_BotStoreNewInBestSlots;
    cbs.bot_set_reputation                  = CB_BotSetReputation;

    cbs.bot_equipped_ranged_subclass        = CB_BotEquippedRangedSubclass;
    cbs.bot_current_ammo_id                 = CB_BotCurrentAmmoId;
    cbs.factory_pick_ammo_for_level         = CB_FactoryPickAmmoForLevel;
    cbs.bot_set_ammo                        = CB_BotSetAmmo;

    cbs.bot_get_skill_value                 = CB_BotGetSkillValue;
    cbs.bot_set_skill                       = CB_BotSetSkill;
    cbs.bot_update_skills_for_level         = CB_BotUpdateSkillsForLevel;

    cbs.bot_cheat_mask                      = CB_BotCheatMask;
    cbs.bot_save_to_db_if_not_busy          = CB_BotSaveToDbIfNotBusy;

    cbs.bot_resurrect_full                  = CB_BotResurrectFull;
    cbs.bot_combat_stop                     = CB_BotCombatStop;
    cbs.bot_set_level_and_reset_xp          = CB_BotSetLevelAndResetXp;
    cbs.bot_set_player_flag                 = CB_BotSetPlayerFlag;
    cbs.factory_config_disable_random_levels = CB_FactoryConfigDisableRandomLevels;
    cbs.factory_config_random_bot_show_helmet = CB_FactoryConfigRandomBotShowHelmet;
    cbs.factory_config_random_bot_show_cloak  = CB_FactoryConfigRandomBotShowCloak;

    cbs.factory_is_random_bot                   = CB_FactoryIsRandomBot;
    cbs.factory_has_real_player_master          = CB_FactoryHasRealPlayerMaster;
    cbs.factory_is_in_real_guild                = CB_FactoryIsInRealGuild;
    cbs.factory_config_min_enchanting_bot_level = CB_FactoryConfigMinEnchantingBotLevel;
    cbs.factory_load_enchant_container          = CB_FactoryLoadEnchantContainer;
    cbs.bot_reset_talents                       = CB_BotResetTalents;
    cbs.bot_learn_quest_rewarded_spells         = CB_BotLearnQuestRewardedSpells;
    cbs.bot_get_money                           = CB_BotGetMoney;
    cbs.bot_set_money                           = CB_BotSetMoney;
    cbs.factory_init_all_gems                   = CB_FactoryInitAllGems;
    cbs.factory_enchant_all_equipment           = CB_FactoryEnchantAllEquipment;

    cbs.quest_is_eligible_for_bot           = CB_QuestIsEligibleForBot;
    cbs.bot_reward_quest_complete           = CB_BotRewardQuestComplete;

    cbs.bot_get_account_id                  = CB_BotGetAccountId;

    cbs.factory_query_guild_summary         = CB_FactoryQueryGuildSummary;
    cbs.factory_guild_add_member            = CB_FactoryGuildAddMember;
    cbs.factory_get_guild_rank_name         = CB_FactoryGetGuildRankName;
    cbs.factory_bot_guild_id                = CB_FactoryBotGuildId;

    cbs.factory_kv_get_u32                  = CB_FactoryKvGetU32;
    cbs.factory_kv_set_u32                  = CB_FactoryKvSetU32;
    cbs.factory_learn_tradeskill_recipes    = CB_FactoryLearnTradeskillRecipes;

    cbs.factory_bot_guid_low                = CB_FactoryBotGuidLow;
    cbs.factory_bot_equipped_item_in_slot   = CB_FactoryBotEquippedItemInSlot;
    cbs.factory_destroy_all_equipped_items  = CB_FactoryDestroyAllEquippedItems;
    cbs.factory_equip_new_item_in_slot      = CB_FactoryEquipNewItemInSlot;
    cbs.factory_init_stats_for_level_and_update = CB_FactoryInitStatsForLevelAndUpdate;
    cbs.factory_master_equip_gear_score     = CB_FactoryMasterEquipGearScore;
    cbs.factory_tell_master                 = CB_FactoryTellMaster;

    // Factory: pet
    cbs.factory_bot_has_pet                      = CB_FactoryBotHasPet;
    cbs.factory_pet_entry                        = CB_FactoryPetEntry;
    cbs.factory_pet_family                       = CB_FactoryPetFamily;
    cbs.factory_pet_level                        = CB_FactoryPetLevel;
    cbs.factory_pet_has_spell                    = CB_FactoryPetHasSpell;
    cbs.factory_pet_autocast_candidate_spells    = CB_FactoryPetAutocastCandidateSpells;
    cbs.factory_tameable_creatures_for_bot_level = CB_FactoryTameableCreaturesForBotLevel;
    cbs.factory_create_hunter_pet                = CB_FactoryCreateHunterPet;
    cbs.factory_pet_refresh_stats                = CB_FactoryPetRefreshStats;
    cbs.factory_pet_learn_spell                  = CB_FactoryPetLearnSpell;
    cbs.factory_pet_toggle_autocast              = CB_FactoryPetToggleAutocast;
    cbs.factory_pet_force_dismiss                = CB_FactoryPetForceDismiss;

    cbs.item_prototype_quality              = CB_ItemPrototypeQuality;
    cbs.factory_pick_trade_for_level        = CB_FactoryPickTradeForLevel;
    cbs.get_random_bot_spell_ids            = CB_GetRandomBotSpellIds;

    cbs.get_overworld_taxi_nodes            = CB_GetOverworldTaxiNodes;
    cbs.free_taxi_nodes                     = CB_FreeTaxiNodes;
    cbs.bot_set_taxi_node                   = CB_BotSetTaxiNode;
    cbs.nearest_taxi_node_pos               = CB_NearestTaxiNodePos;
    cbs.take_taxi_toward                    = CB_TakeTaxiToward;
    cbs.cross_continent_travel              = CB_CrossContinentTravel;
    cbs.get_class_talents                   = CB_GetClassTalents;
    cbs.free_class_talents                  = CB_FreeClassTalents;
    cbs.bot_free_talent_points              = CB_BotFreeTalentPoints;
    cbs.bot_update_free_talent_points       = CB_BotUpdateFreeTalentPoints;
    cbs.bot_pick_spec_no                    = CB_BotPickSpecNo;
    cbs.bot_get_spec_tab                    = CB_BotGetSpecTab;

    // Chat-command helpers (Wave 2)
    cbs.bot_jump                            = CB_BotJump;
    cbs.bot_use_hearthstone                 = CB_BotUseHearthstone;
    cbs.bot_get_reputation_list             = CB_BotGetReputationList;
    cbs.bot_free_reputation_list            = CB_BotFreeReputationList;
    cbs.bot_get_learned_skills              = CB_BotGetLearnedSkills;
    cbs.bot_free_skill_list                 = CB_BotFreeSkillList;
    cbs.bot_quest_accept_from               = CB_BotQuestAcceptFrom;
    cbs.bot_quest_abandon                   = CB_BotQuestAbandon;

    // Chat-command helpers (Wave 3: mail + guild)
    cbs.bot_mail_summary                    = CB_BotMailSummary;
    cbs.bot_mail_take_all                   = CB_BotMailTakeAll;
    cbs.bot_guild_leave                     = CB_BotGuildLeave;

    // RTSC / file I/O helpers
    cbs.bot_summon_marker_creature          = CB_BotSummonMarkerCreature;
    cbs.bot_write_log_file                  = CB_BotWriteLogFile;
    cbs.bot_read_log_file                   = CB_BotReadLogFile;
    cbs.bot_free_string                     = CB_BotFreeString;
    cbs.bot_append_log_file                 = CB_BotAppendLogFile;

    // Addon-channel reply routing
    cbs.bot_tell_addon                      = CB_TellAddon;
    cbs.bot_send_group_addon                = CB_SendGroupAddonMessage;

    // Loot rolling — PB2 handles this via packet interception in
    // PlayerbotMgr::HandleMasterIncomingPacket (CMSG_LOOT_ROLL),
    // not through proactive callbacks.
    cbs.get_pending_roll_count              = nullptr;
    cbs.auto_loot_roll                      = nullptr;
    cbs.cast_loot_roll                      = nullptr;

    // Travel destination queries
    cbs.bot_find_travel_dests               = CB_BotFindTravelDests;
    cbs.bot_free_travel_dests               = CB_BotFreeTravelDests;

    // Spell name resolution
    cbs.resolve_spell_by_name               = CB_ResolveSpellByName;

    // Item name resolution + equip
    cbs.resolve_item_by_name                = CB_ResolveItemByName;
    cbs.equip_item                          = CB_EquipItem;

    // Group management
    cbs.give_leader                         = CB_GiveLeader;
    cbs.resolve_player_by_name              = CB_ResolvePlayerByName;
    cbs.unequip_item                        = CB_UnequipItem;
    cbs.invite_to_group                     = CB_InviteToGroup;
    cbs.destroy_item                        = CB_DestroyItem;
    cbs.share_quest                         = CB_ShareQuest;
    cbs.do_text_emote                       = CB_DoTextEmote;

    // World buffs
    cbs.add_aura                            = CB_AddAura;
    cbs.get_needed_world_buffs              = CB_GetNeededWorldBuffs;

    // Heal interrupt
    cbs.interrupt_own_cast                  = CB_InterruptOwnCast;

    // NPC interaction
    cbs.gossip_hello                        = CB_GossipHello;
    cbs.buy_from_vendor                     = CB_BuyFromVendor;

    // Mail
    cbs.mail_item_to_master                 = CB_MailItemToMaster;

    // Bank
    cbs.bank_deposit                        = CB_BankDeposit;
    cbs.bank_withdraw                       = CB_BankWithdraw;

    // Auction house
    cbs.ah_post                             = CB_AhPost;
    cbs.ah_bid                              = CB_AhBid;

    // Fishing
    cbs.start_fishing                       = CB_StartFishing;

    // BG/Arena
    cbs.queue_bg                            = CB_QueueBg;
    cbs.accept_bg_invite                    = CB_AcceptBgInvite;
    cbs.get_bg_objective_pos                = CB_GetBgObjectivePos;

    // LFG
    cbs.lfg_join                            = CB_LfgJoin;
    cbs.lfg_accept                          = CB_LfgAccept;

    // Dungeon awareness
    cbs.get_tank_position                   = CB_GetTankPosition;
    cbs.is_unit_cc                          = CB_IsUnitCc;

    // Debug
    cbs.debug_dump_state                    = CB_DebugDumpState;

    // EngBags: inventory enumeration
    cbs.bot_get_inventory                   = CB_BotGetInventory;
    cbs.bot_get_equipped                    = CB_BotGetEquipped;
    cbs.bot_get_bank_items                  = CB_BotGetBankItems;
    cbs.bot_get_mail_items                  = CB_BotGetMailItems;
    cbs.bot_free_inventory_list             = CB_BotFreeInventoryList;

    // EngBags: item actions
    cbs.sell_item                           = CB_SellItem;
    cbs.bank_deposit_item                   = CB_BankDepositItem;
    cbs.bank_withdraw_item                  = CB_BankWithdrawItem;
    cbs.bot_mail_take_index                 = CB_BotMailTakeIndex;
    cbs.send_mail_item                      = CB_SendMailItem;
    cbs.trade_add_item                      = CB_TradeAddItem;
    cbs.get_spell_craft_item                = CB_GetSpellCraftItem;
    cbs.get_item_info                       = CB_GetItemInfo;

    return cbs;
}

// ── Snapshot ──────────────────────────────────────────────────────────────

BotWorldSnapshot BotBridge::CB_GetSnapshot(BotHandle bot)
{
    BotWorldSnapshot snap{};
    Player* b = FindBot(bot);
    if (!b)
        return snap;

    snap.self = FillUnitSnapshot(b);

    // Level-up detection: flag a single natural level gain (XP), but not a
    // multi-level `randomize` SetLevel jump. Per-bot last level is kept in a
    // static map; the first snapshot for a bot just seeds it (no false ding).
    {
        static std::unordered_map<uint64_t, uint8_t> s_lastLevel;
        uint8_t lvl = b->GetLevel();
        auto it = s_lastLevel.find(bot);
        snap.just_leveled = (it != s_lastLevel.end() && lvl == it->second + 1) ? 1 : 0;
        s_lastLevel[bot] = lvl;
    }

    snap.zone_id     = b->GetZoneId();
    snap.area_id     = b->GetAreaId();
    snap.instance_id = b->GetInstanceId();
    snap.server_time_ms = WorldTimer::getMSTime();
    snap.is_leader   = false; // filled below

    // Group members
    Group* group = b->GetGroup();
    if (group)
    {
        uint8_t count = 0;
        snap.is_leader = (group->GetLeaderGuid() == b->GetObjectGuid());
        snap.is_raid_group = group->IsRaidGroup();
        snap.subgroup = static_cast<uint8_t>(b->GetSubGroup() + 1);

        for (GroupReference* ref = group->GetFirstMember(); ref != nullptr && count < 40;
             ref = ref->next())
        {
            Player* member = ref->getSource();
            // Include all group members, even those mid-teleport (!IsInWorld()).
            // The previous IsInWorld() filter caused group membership to flicker
            // when a bot was summoned/teleported, making the Rust side think the
            // bot left the group and losing raid coordination state.
            if (member)
            {
                snap.group_members[count++] = member->GetGUID();
            }
        }
        snap.group_size = count;
    }
    else
    {
        snap.group_size = 0;
        snap.is_raid_group = false;
        snap.subgroup = 0;
    }

    // Guild
    snap.guild_id = b->GetGuildId();
    snap.in_guild = (snap.guild_id != 0);
    snap.is_guild_leader = false;
    if (snap.guild_id)
    {
        if (Guild* guild = sGuildMgr.GetGuildById(snap.guild_id))
            snap.is_guild_leader = (guild->GetLeaderGuid() == b->GetObjectGuid());
    }

    // Durability — lowest slot, as a 0..100 percentage. Used by the
    // `@needrepair` chat filter and by anyone else who wants to gate on
    // "needs to visit a vendor". We report the minimum across equipped
    // slots rather than the average because PB2's `AI_VALUE("durability")`
    // is the single worst slot (the one that limits the bot first).
    {
        uint32_t worstPct = 100;
        bool any = false;
        for (int i = EQUIPMENT_SLOT_START; i < EQUIPMENT_SLOT_END; ++i)
        {
            Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
            if (!item)
                continue;
            uint32_t maxD = item->GetUInt32Value(ITEM_FIELD_MAXDURABILITY);
            uint32_t curD = item->GetUInt32Value(ITEM_FIELD_DURABILITY);
            if (maxD == 0)
                continue;
            uint32_t pct = (curD * 100) / maxD;
            if (!any || pct < worstPct)
            {
                worstPct = pct;
                any = true;
            }
        }
        snap.durability_pct = any ? static_cast<uint8_t>(worstPct) : 100;
    }

    // Bag space — percent of inventory slots *used* (PB2's "bag space"
    // value is a used-percent, not a free-percent). Counts backpack + all
    // equipped bags. `@bagfull` fires at 100% used, `@bagalmostfull` at ≥80%.
    {
        uint32_t used = 0;
        uint32_t total = 0;
        // Backpack (main bag, 16 slots).
        for (int i = INVENTORY_SLOT_ITEM_START; i < INVENTORY_SLOT_ITEM_END; ++i)
        {
            total++;
            if (b->GetItemByPos(INVENTORY_SLOT_BAG_0, i))
                used++;
        }
        // Equipped bags.
        for (int bag = INVENTORY_SLOT_BAG_START; bag < INVENTORY_SLOT_BAG_END; ++bag)
        {
            Bag* pBag = static_cast<Bag*>(b->GetItemByPos(INVENTORY_SLOT_BAG_0, bag));
            if (!pBag)
                continue;
            for (uint32_t slot = 0; slot < pBag->GetBagSize(); ++slot)
            {
                total++;
                if (b->GetItemByPos(bag, slot))
                    used++;
            }
        }
        snap.bag_space_pct = (total > 0)
            ? static_cast<uint8_t>((used * 100) / total)
            : 0;
    }

    // Average equipped item level (rough gear score for the `@tierN`
    // chat filter). This deliberately mirrors the simple average used by
    // PB2's `PlayerbotAI::GetEquipGearScore(bot, false, false)` — ignore
    // bags/bank, just average non-null equipped slots.
    {
        uint32_t sum = 0;
        uint32_t n = 0;
        for (int i = EQUIPMENT_SLOT_START; i < EQUIPMENT_SLOT_END; ++i)
        {
            Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
            if (!item)
                continue;
            ItemPrototype const* proto = item->GetProto();
            if (!proto)
                continue;
            sum += proto->ItemLevel;
            n++;
        }
        snap.equip_gear_score = (n > 0) ? (sum / n) : 0;
    }

    // Overworld vs. instance. Classic-era overworld maps are Eastern
    // Kingdoms (0) and Kalimdor (1). TBC adds Outland (530); WotLK adds
    // Northrend (571). Everything else is an instance/BG. Matching PB2's
    // `WorldPosition(bot).isOverworld()` exactly is overkill for the chat
    // filter, which only needs to distinguish "inside an instance" from
    // "not".
    {
        uint32_t mapId = b->GetMapId();
        snap.is_overworld = (mapId == 0 || mapId == 1 || mapId == 530 || mapId == 571);
    }

    // ── Rti chat filter state (3b) ─────────────────────────────────
    // Copy raid target icons from the group. PB2's RtiChatFilter calls
    // `group->GetTargetIcon(RtiTargetValue::GetRtiIndex(name))` per
    // lookup (ChatFilter.cpp:457); here we just snapshot all 8 slots.
    // TARGETICONCOUNT is 8 in core (see Group.h:292 usage).
    if (group)
    {
        for (int i = 0; i < 8; ++i)
            snap.group_raid_target_icons[i] = group->GetTargetIcon(i).GetRawValue();
    }
    // else: zero-initialized by {} at the top.

    // ── Location chat filter state (3d) ────────────────────────────
    // Lowercase the map name and the bot's zone-level area name. PB2's
    // `WorldPosition::getAreaName(true, true)` walks up the area parent
    // chain and returns the top-level name; `Player::GetZoneId()` does
    // the same walk inside the core and is cheaper. Truncated into
    // fixed buffers with a NUL terminator.
    auto ToLowerCopy = [](const char* src, char* dst, size_t dst_cap) {
        if (!src || dst_cap == 0)
        {
            if (dst_cap > 0) dst[0] = '\0';
            return;
        }
        size_t i = 0;
        for (; src[i] != '\0' && i + 1 < dst_cap; ++i)
        {
            unsigned char c = static_cast<unsigned char>(src[i]);
            dst[i] = static_cast<char>(
                (c >= 'A' && c <= 'Z') ? (c + ('a' - 'A')) : c);
        }
        dst[i] = '\0';
    };
    if (Map* map = b->GetMap())
        ToLowerCopy(map->GetMapName(), snap.map_name_lower, sizeof(snap.map_name_lower));
    if (uint32 zoneId = b->GetZoneId())
    {
        if (AreaTableEntry const* zone = GetAreaEntryByAreaID(zoneId))
            ToLowerCopy(zone->area_name[0], snap.area_name_lower, sizeof(snap.area_name_lower));
    }

    // ── Guild chat filter state (3f) ───────────────────────────────
    // Raw (not lowercased) guild name and the bot's rank name. PB2
    // compares with `std::string::find` on raw strings
    // (ChatFilter.cpp:679, 731). Empty strings when not in a guild.
    if (snap.guild_id)
    {
        if (Guild* guild = sGuildMgr.GetGuildById(snap.guild_id))
        {
            std::string const& gname = guild->GetName();
            size_t n = std::min(gname.size(), sizeof(snap.guild_name) - 1);
            std::memcpy(snap.guild_name, gname.data(), n);
            snap.guild_name[n] = '\0';

            int32 rankId = guild->GetRank(b->GetObjectGuid());
            if (rankId >= 0)
            {
                std::string rname = guild->GetRankName(static_cast<uint32>(rankId));
                size_t m = std::min(rname.size(), sizeof(snap.guild_rank_name) - 1);
                std::memcpy(snap.guild_rank_name, rname.data(), m);
                snap.guild_rank_name[m] = '\0';
            }
        }
    }

    // ── Quest chat filter state (3e) ───────────────────────────────
    // Snapshot active quest log IDs. Mirrors PB2's
    // `PlayerbotAI::GetAllCurrentQuestIds` loop, capped at the
    // snapshot buffer size (MAX_QUEST_LOG_SIZE = 25).
    {
        uint8_t n = 0;
        const uint16 cap = static_cast<uint16>(
            sizeof(snap.current_quest_ids) / sizeof(snap.current_quest_ids[0]));
        for (uint16 slot = 0; slot < MAX_QUEST_LOG_SIZE && n < cap; ++slot)
        {
            uint32 questId = b->GetQuestSlotQuestId(slot);
            if (!questId)
                continue;
            snap.current_quest_ids[n++] = questId;
        }
        snap.current_quest_count = n;
    }

    return snap;
}

BotUnitSnapshot BotBridge::CB_GetUnitSnapshot(BotHandle bot, UnitHandle target)
{
    Unit* unit = FindUnit(bot, target);
    return FillUnitSnapshot(unit);
}

// ── Aura queries ──────────────────────────────────────────────────────────

bool BotBridge::CB_HasAura(BotHandle bot, UnitHandle target, uint32_t spell_id)
{
    Unit* unit = FindUnit(bot, target);
    if (!unit)
        return false;
    return unit->HasAura(spell_id);
}

BotAuraInfo BotBridge::CB_GetAura(BotHandle bot, UnitHandle target, uint32_t spell_id)
{
    BotAuraInfo info{};
    Unit* unit = FindUnit(bot, target);
    if (!unit)
        return info;

    SpellAuraHolder* holder = unit->GetSpellAuraHolder(spell_id);
    if (!holder)
        return info;

    info.spell_id       = spell_id;
    info.duration_ms    = static_cast<uint32_t>(holder->GetAuraDuration());
    info.max_duration_ms= static_cast<uint32_t>(holder->GetAuraMaxDuration());
    info.stacks         = static_cast<uint8_t>(holder->GetStackAmount());
    info.is_mine        = (holder->GetCasterGuid() == FindBot(bot)->GetObjectGuid());
    info.is_harmful     = !IsPositiveSpell(spell_id);
    info.is_passive     = IsPassiveSpell(spell_id);
    return info;
}

BotAuraInfo* BotBridge::CB_GetAuras(BotHandle bot, UnitHandle target, uint32_t* out_count)
{
    *out_count = 0;
    Unit* unit = FindUnit(bot, target);
    if (!unit)
        return nullptr;

    Player* b = FindBot(bot);
    ObjectGuid botGuid = b ? b->GetObjectGuid() : ObjectGuid();

    // Collect all aura holders
    std::vector<BotAuraInfo> results;
    results.reserve(32);

    Unit::SpellAuraHolderMap const& holders = unit->GetSpellAuraHolderMap();
    for (auto const& pair : holders)
    {
        SpellAuraHolder* holder = pair.second;
        if (!holder)
            continue;
        BotAuraInfo info{};
        info.spell_id        = holder->GetId();
        info.duration_ms     = static_cast<uint32_t>(holder->GetAuraDuration());
        info.max_duration_ms = static_cast<uint32_t>(holder->GetAuraMaxDuration());
        info.stacks          = static_cast<uint8_t>(holder->GetStackAmount());
        info.is_mine         = (holder->GetCasterGuid() == botGuid);
        info.is_harmful      = !IsPositiveSpell(info.spell_id);
        info.is_passive      = IsPassiveSpell(info.spell_id);
        results.push_back(info);
    }

    if (results.empty())
        return nullptr;

    BotAuraInfo* arr = new BotAuraInfo[results.size()];
    std::copy(results.begin(), results.end(), arr);
    *out_count = static_cast<uint32_t>(results.size());
    return arr;
}

void BotBridge::CB_FreeAuraList(BotAuraInfo* list)
{
    delete[] list;
}

// ── Threat queries ────────────────────────────────────────────────────────

BotThreatEntry* BotBridge::CB_GetThreatList(BotHandle bot, UnitHandle target_unit,
                                             uint32_t* out_count)
{
    *out_count = 0;
    Unit* target = FindUnit(bot, target_unit);
    if (!target)
        return nullptr;

    ThreatManager const& tm = target->getThreatManager();
    ThreatList const& list  = tm.getThreatList();
    if (list.empty())
        return nullptr;

    BotThreatEntry* arr = new BotThreatEntry[list.size()];
    uint32_t count = 0;
    for (HostileReference* ref : list)
    {
        arr[count].unit      = ref->getUnitGuid().GetRawValue();
        arr[count].threat    = ref->getThreat();
        arr[count].is_online = ref->isOnline();
        arr[count].is_taunted= false; // CMaNGOS doesn't expose this directly
        ++count;
    }
    *out_count = count;
    return arr;
}

void BotBridge::CB_FreeThreatList(BotThreatEntry* list)
{
    delete[] list;
}

float BotBridge::CB_GetUnitThreat(BotHandle bot, UnitHandle target_unit, UnitHandle from_unit)
{
    Unit* target = FindUnit(bot, target_unit);
    Unit* from   = FindUnit(bot, from_unit);
    if (!target || !from)
        return 0.0f;
    return target->getThreatManager().getThreat(from);
}

// ── Unit queries ──────────────────────────────────────────────────────────

float BotBridge::CB_UnitDistance(BotHandle bot, UnitHandle target)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return 0.0f;
    return b->GetDistance(t);
}

bool BotBridge::CB_CanCast(BotHandle bot, uint32_t spell_id, UnitHandle target)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b)
        return false;

    SpellEntry const* spellInfo = sSpellTemplate.LookupEntry<SpellEntry>(spell_id);
    if (!spellInfo)
        return false;

    // Check if player has the spell
    if (!b->HasSpell(spell_id))
        return false;

    // Check cooldown
    if (!b->IsSpellReady(spell_id))
        return false;

    // Basic castability check (range, power, etc.)
    (void)t;
    if (!b->IsSpellFitByClassAndRace(spell_id))
        return false;

    return true;
}

bool BotBridge::CB_SpellOnCooldown(BotHandle bot, uint32_t spell_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    return !b->IsSpellReady(spell_id);
}

uint32_t BotBridge::CB_SpellCooldownMs(BotHandle bot, uint32_t spell_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    // The cooldown map is not publicly accessible in this fork. Return the
    // cooldown in ms via the public GetSpellCooldownDelay helper (in seconds)
    // when the spell is on cooldown; otherwise 0.
    if (b->IsSpellReady(spell_id))
        return 0;
    return b->GetSpellCooldownDelay(spell_id) * 1000u;
}

bool BotBridge::CB_HasLos(BotHandle bot, UnitHandle target)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return false;
    return b->IsWithinLOSInMap(t);
}

UnitHandle* BotBridge::CB_GetNearbyUnits(BotHandle bot, float range, bool hostile,
                                          uint32_t* out_count)
{
    *out_count = 0;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    UnitList units;
    MaNGOS::AnyUnitInObjectRangeCheck checker(b, range);
    MaNGOS::UnitListSearcher<MaNGOS::AnyUnitInObjectRangeCheck> searcher(units, checker);
    Cell::VisitAllObjects(b, searcher, range);

    std::vector<UnitHandle> handles;
    handles.reserve(units.size());
    for (Unit* u : units)
    {
        if (!u || u == b || !u->IsAlive())
            continue;
        // When hostile=true, return only hostile units. When hostile=false,
        // return ALL units (hostile + friendly) — the Rust side uses this
        // list as "nearby_units" for the full neighbourhood.
        if (hostile && !b->IsHostileTo(u))
            continue;
        handles.push_back(u->GetObjectGuid().GetRawValue());
    }

    if (handles.empty())
        return nullptr;

    UnitHandle* arr = new UnitHandle[handles.size()];
    std::copy(handles.begin(), handles.end(), arr);
    *out_count = static_cast<uint32_t>(handles.size());
    return arr;
}

UnitHandle* BotBridge::CB_GetAttackers(BotHandle bot, uint32_t* out_count)
{
    *out_count = 0;
    Player* b = FindBot(bot);
    if (!b || !b->GetCombatData())
        return nullptr;

    // HostileRefManager tracks units that have the bot on their threat list —
    // i.e., mobs (or players) actively fighting the bot.
    HostileRefManager& hrm = b->getHostileRefManager();
    std::vector<UnitHandle> handles;
    for (HostileReference* ref = hrm.getFirst(); ref; ref = ref->next())
    {
        Unit* attacker = ref->getSource()->getOwner();
        if (!attacker || !attacker->IsAlive())
            continue;
        handles.push_back(attacker->GetObjectGuid().GetRawValue());
    }

    if (handles.empty())
        return nullptr;

    UnitHandle* arr = new UnitHandle[handles.size()];
    std::copy(handles.begin(), handles.end(), arr);
    *out_count = static_cast<uint32_t>(handles.size());
    return arr;
}

void BotBridge::CB_FreeUnitList(UnitHandle* list)
{
    delete[] list;
}

// ── Pathfinding / positioning ─────────────────────────────────────────────

BotPosition BotBridge::CB_GetBehindPosition(BotHandle bot, UnitHandle target, float distance)
{
    BotPosition pos{};
    Unit* t = FindUnit(bot, target);
    if (!t)
        return pos;

    float angle = t->GetOrientation() + static_cast<float>(M_PI); // behind = facing + 180°
    pos.x = t->GetPositionX() + std::cos(angle) * distance;
    pos.y = t->GetPositionY() + std::sin(angle) * distance;
    pos.z = t->GetPositionZ();
    pos.o = angle;
    pos.map_id = t->GetMapId();
    return pos;
}

BotSafePosition BotBridge::CB_GetSafePosition(BotHandle bot, float search_radius)
{
    BotSafePosition result{};
    Player* b = FindBot(bot);
    if (!b)
        return result;

    float bx = b->GetPositionX();
    float by = b->GetPositionY();
    float bz = b->GetPositionZ();

    PathFinder pathfinder(b);

    // Try 8 directions at search_radius
    for (int i = 0; i < 8; ++i)
    {
        float angle = i * (static_cast<float>(M_PI) / 4.0f);
        float cx = bx + std::cos(angle) * search_radius;
        float cy = by + std::sin(angle) * search_radius;
        float cz = bz;
        b->UpdateGroundPositionZ(cx, cy, cz);

        // Validate with navmesh pathfinding — LoS alone passes through
        // thin dungeon walls, causing bots to walk through geometry.
        pathfinder.calculate(cx, cy, cz, false);
        if (pathfinder.getPathType() & PATHFIND_NORMAL)
        {
            result.x     = cx;
            result.y     = cy;
            result.z     = cz;
            result.found = true;
            return result;
        }
    }
    return result; // found = false
}

BotPosition BotBridge::CB_GetSpreadPosition(BotHandle bot, UnitHandle center, float radius,
                                              uint8_t idx, uint8_t total)
{
    BotPosition pos{};
    Unit* c = FindUnit(bot, center);
    if (!c || total == 0)
        return pos;

    float angle = (2.0f * static_cast<float>(M_PI) / total) * idx;
    pos.x    = c->GetPositionX() + std::cos(angle) * radius;
    pos.y    = c->GetPositionY() + std::sin(angle) * radius;
    pos.z    = c->GetPositionZ();
    pos.o    = angle + static_cast<float>(M_PI); // face center
    pos.map_id = c->GetMapId();
    return pos;
}

bool BotBridge::CB_CanReach(BotHandle bot, float x, float y, float z)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    return b->GetMap()->IsInLineOfSight(b->GetPositionX(), b->GetPositionY(),
                                         b->GetPositionZ(), x, y, z, false);
}

bool BotBridge::CB_CanPathfindTo(BotHandle bot, float x, float y, float z)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    PathFinder pathfinder(b);
    pathfinder.calculate(x, y, z, false);
    return (pathfinder.getPathType() & PATHFIND_NORMAL) != 0;
}

// ── Movement de-duplication state (used by CB_CastSpell and movement callbacks) ──
// Defined here before CB_CastSpell which needs to erase entries when stopping to cast.
struct MoveState
{
    enum Kind { NONE, POINT, FOLLOW, CHASE };
    float x = 0, y = 0, z = 0;
    UnitHandle followTarget = 0;
    float followDist = 0;
    float followAngle = 0;
    Kind kind = NONE;
};

// Thread-local: each map-update worker thread gets its own map so there is no
// data race when bots on different maps tick concurrently.
static thread_local std::unordered_map<BotHandle, MoveState> s_moveState;

// ── Commands ──────────────────────────────────────────────────────────────

bool BotBridge::CB_CastSpell(BotHandle bot, uint32_t spell_id, UnitHandle target)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    SpellEntry const* spellInfo = sSpellTemplate.LookupEntry<SpellEntry>(spell_id);
    if (!spellInfo)
        return false;

    Unit* t = FindUnit(bot, target);
    if (!t)
        t = b; // fallback to self

    // Check that the spell is ready and known
    if (!b->HasSpell(spell_id))
    {
        sLog.outDebug("[BotBridge] CastSpell: bot=0x%llX spell=%u NOT KNOWN",
                      (unsigned long long)bot, spell_id);
        return false;
    }
    if (!b->IsSpellReady(spell_id))
    {
        sLog.outDebug("[BotBridge] CastSpell: bot=0x%llX spell=%u NOT READY (server CD)",
                      (unsigned long long)bot, spell_id);
        return false;
    }

    // Stop moving before casting spells that require it. Cast-time and
    // channeled spells obviously need the bot to stand still. Melee
    // instants can be used while moving in WoW, so don't disrupt the
    // bot's approach/positioning for those.
    bool hasCastTime = (GetSpellCastTime(spellInfo, b) > 0)
                     || IsChanneledSpell(spellInfo);
    if (hasCastTime && !b->IsStopped())
    {
        b->StopMoving();
        b->GetMotionMaster()->Clear(false);
        b->GetMotionMaster()->MoveIdle();
        // Invalidate move state so the next chase/follow call isn't deduped.
        s_moveState.erase(bot);
    }

    // Face the target before casting. PB2 always called SetFacingTo before
    // spell casts — without this the server rejects most spells because the
    // bot isn't oriented toward the target (SPELL_FAILED_UNIT_NOT_INFRONT).
    if (t != b && !b->HasInArc(t, M_PI))
    {
        float angle = b->GetAngle(t);
        if (b->IsStopped())
        {
            b->SetOrientation(angle);
            b->SendHeartBeat();
        }
        else
        {
            b->SetFacingTo(angle);
        }
        b->SetInFront(t);
    }

    // Pre-validate with CheckCast so we don't trigger a GCD on the Rust side
    // for a spell that will fail (wrong stance, out of range, not enough power, etc.)
    Spell* spell = new Spell(b, spellInfo, false);
    SpellCastTargets targets;
    targets.setUnitTarget(t);
    spell->m_targets = targets;
    SpellCastResult result = spell->CheckCast(true);
    if (result != SPELL_CAST_OK)
    {
        sLog.outDebug("[BotBridge] CastSpell REJECTED: bot=0x%llX spell=%u target=0x%llX CheckCast=%d",
                      (unsigned long long)bot, spell_id, (unsigned long long)target, (int)result);
        delete spell;
        return false;
    }

    spell->SpellStart(&targets);
    return true;
}

bool BotBridge::CB_CastSpellPos(BotHandle bot, uint32_t spell_id,
                                  float x, float y, float z)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    SpellEntry const* spellInfo = sSpellTemplate.LookupEntry<SpellEntry>(spell_id);
    if (!spellInfo)
        return false;

    if (!b->HasSpell(spell_id) || !b->IsSpellReady(spell_id))
        return false;

    Spell* spell = new Spell(b, spellInfo, false);
    SpellCastTargets targets;
    targets.setDestination(x, y, z);
    spell->m_targets = targets;
    SpellCastResult result = spell->CheckCast(true);
    if (result != SPELL_CAST_OK)
    {
        delete spell;
        return false;
    }

    spell->SpellStart(&targets);
    return true;
}

// ── Movement de-duplication ──────────────────────────────────────────────────
//
// The Rust BT re-calls move_to / follow on every tick while an action returns
// Running (throttle "Running-transparency"). Without de-duplication each call
// creates a *new* MovePoint / MoveFollow generator, which restarts the walk
// animation and produces the visible "walk-stop-walk" stutter.
//
// We track the last-issued destination per bot. If the new request matches the
// previous one (within a small epsilon) and the bot is still moving, we skip
// the MotionMaster call entirely.
//
// NOTE: MoveState and s_moveState are defined above (before CB_CastSpell)
// because CB_CastSpell needs to erase entries when stopping to cast.

static constexpr float MOVE_EPSILON_SQ = 1.5f * 1.5f; // 1.5 yards

static bool sameDest(const MoveState& s, float x, float y, float z)
{
    float dx = s.x - x, dy = s.y - y, dz = s.z - z;
    return (dx * dx + dy * dy + dz * dz) < MOVE_EPSILON_SQ;
}

// Follow/chase params (dist, angle) are re-derived on the Rust side via
// atan2/hypot from the leader's *moving* position every cycle, so the
// float values jitter by ~1e-7 each tick even when the formation slot is
// unchanged. Comparing them with exact `==` therefore fails on every tick
// while the leader walks, re-issuing MoveFollow/MoveChase and restarting
// the chase spline — the visible "stagger". Compare within a tolerance
// instead (angle delta normalised into (-PI, PI] to ignore 2*PI wraps).
static bool sameFollowParams(float d0, float d1, float a0, float a1)
{
    constexpr float PI_F = 3.14159265f;
    constexpr float TWO_PI_F = 6.2831853f;
    float dd = d0 - d1;
    float da = a0 - a1;
    while (da > PI_F) da -= TWO_PI_F;
    while (da < -PI_F) da += TWO_PI_F;
    return dd * dd < 0.05f * 0.05f && da * da < 0.02f * 0.02f;
}

bool BotBridge::CB_MoveTo(BotHandle bot, float x, float y, float z)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // Ensure run mode — bots default to walking without this.
    bool wasWalking = b->IsWalking();
    if (wasWalking)
        b->m_movementInfo.RemoveMovementFlag(MOVEFLAG_WALK_MODE);

    auto& st = s_moveState[bot];
    // Skip if already heading to the same point AND not transitioning
    // from walk to run (the existing spline was started in walk mode
    // and needs to be replaced with a run-speed spline).
    if (st.kind == MoveState::POINT && sameDest(st, x, y, z) && b->IsMoving() && !wasWalking)
        return true; // already heading there at run speed

    b->GetMotionMaster()->MovePoint(0, x, y, z);
    st.x = x; st.y = y; st.z = z;
    st.followTarget = 0; st.followDist = 0; st.followAngle = 0;
    st.kind = MoveState::POINT;
    return true;
}

void BotBridge::CB_SetFacing(BotHandle bot, float angle)
{
    Player* b = FindBot(bot);
    if (!b)
        return;

    // Stop movement so the facing takes effect immediately.
    if (!b->IsStopped())
    {
        b->StopMoving();
        b->GetMotionMaster()->Clear(false);
        b->GetMotionMaster()->MoveIdle();
        s_moveState.erase(bot);
    }

    b->SetOrientation(angle);
    b->SendHeartBeat();
}

bool BotBridge::CB_Follow(BotHandle bot, UnitHandle target, float dist, float angle)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return false;

    // Ensure run mode — bots default to walking without this.
    bool wasWalking = b->IsWalking();
    if (wasWalking)
        b->m_movementInfo.RemoveMovementFlag(MOVEFLAG_WALK_MODE);

    auto& st = s_moveState[bot];
    // Skip if already following the same target with the same params AND
    // not transitioning from walk to run.
    if (st.kind == MoveState::FOLLOW && st.followTarget == target
        && sameFollowParams(st.followDist, dist, st.followAngle, angle) && !wasWalking)
        return true; // already following at run speed

    b->GetMotionMaster()->MoveFollow(t, dist, angle);
    st.x = 0; st.y = 0; st.z = 0;
    st.followTarget = target; st.followDist = dist; st.followAngle = angle;
    st.kind = MoveState::FOLLOW;
    return true;
}

bool BotBridge::CB_Chase(BotHandle bot, UnitHandle target, float dist, float angle)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return false;

    // Ensure run mode.
    if (b->IsWalking())
        b->m_movementInfo.RemoveMovementFlag(MOVEFLAG_WALK_MODE);

    auto& st = s_moveState[bot];
    // Skip if already chasing the same target with the same params.
    if (st.kind == MoveState::CHASE && st.followTarget == target
        && sameFollowParams(st.followDist, dist, st.followAngle, angle))
        return true;

    b->GetMotionMaster()->MoveChase(t, dist, angle);
    st.x = 0; st.y = 0; st.z = 0;
    st.followTarget = target; st.followDist = dist; st.followAngle = angle;
    st.kind = MoveState::CHASE;
    return true;
}

bool BotBridge::CB_StopMoving(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    b->GetMotionMaster()->Clear(false);
    b->StopMoving();
    s_moveState.erase(bot);
    return true;
}

void BotBridge::ClearMoveState(BotHandle bot)
{
    s_moveState.erase(bot);
}

bool BotBridge::CB_Attack(BotHandle bot, UnitHandle target)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return false;

    // Face the target before attacking.
    if (!b->HasInArc(t, M_PI))
    {
        float angle = b->GetAngle(t);
        b->SetOrientation(angle);
        b->SendHeartBeat();
        b->SetInFront(t);
    }

    return b->Attack(t, true);
}

bool BotBridge::CB_AutoAttack(BotHandle bot, bool enable)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    if (!enable)
    {
        b->AttackStop();
    }
    else
    {
        // Enable melee auto-attack on current target if not already swinging.
        // Mirrors PB2's AttackAnythingAction: find a valid victim and engage.
        if (!b->IsInCombat() || b->GetVictim())
            return true; // already attacking or not in combat — skip
        // If in combat but no victim, re-engage via CB_Attack on the current
        // target. This path is rare (mob tab-targeted but not attacked yet).
    }
    return true;
}

// Ranged auto-attack / wand-shoot pull. Picks a spell based on the
// ranged slot's weapon subclass (wand → Shoot 5019; bow/gun/crossbow →
// Auto Shot 75) and fires it at `target`. Returns false if the bot has
// no ranged weapon, doesn't know the appropriate spell, or the cast
// fails. Used by the generic `PullTarget` BT leaf as the "shoot pull"
// branch so strategies don't need to know which class is driving them.
bool BotBridge::CB_AutoShoot(BotHandle bot, UnitHandle target)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return false;

    Item* ranged = b->GetItemByPos(INVENTORY_SLOT_BAG_0, EQUIPMENT_SLOT_RANGED);
    if (!ranged)
        return false;
    ItemPrototype const* proto = ranged->GetProto();
    if (!proto || proto->Class != ITEM_CLASS_WEAPON)
        return false;

    uint32_t spell_id = 0;
    switch (proto->SubClass)
    {
        case ITEM_SUBCLASS_WEAPON_BOW:
            // Hunters use Auto Shot (75); everyone else uses Shoot Bow (2480).
            spell_id = b->HasSpell(75) ? 75 : 2480;
            break;
        case ITEM_SUBCLASS_WEAPON_GUN:
            spell_id = b->HasSpell(75) ? 75 : 7918;  // Shoot Gun
            break;
        case ITEM_SUBCLASS_WEAPON_CROSSBOW:
            spell_id = b->HasSpell(75) ? 75 : 7919;  // Shoot Crossbow
            break;
        case ITEM_SUBCLASS_WEAPON_THROWN:
            spell_id = 2764;  // Throw
            break;
        case ITEM_SUBCLASS_WEAPON_WAND:
            spell_id = 5019;  // Shoot (wand)
            break;
        default:
            return false;
    }

    if (!b->HasSpell(spell_id))
        return false;

    // Check if we're already auto-shooting BEFORE the IsSpellReady gate.
    // Auto Shot (75) has a brief cooldown between each volley; during that
    // window IsSpellReady returns false. If we checked ready-state first,
    // we'd return false and the Rust caller would fall through to melee
    // auto_attack, which cancels the ranged auto-repeat — causing the
    // hunter to toggle between ranged and melee every tick.
    if (Spell* current = b->GetCurrentSpell(CURRENT_AUTOREPEAT_SPELL))
    {
        if (current->m_spellInfo->Id == spell_id)
            return true; // already auto-shooting — idempotent success
    }

    if (!b->IsSpellReady(spell_id))
        return false;

    SpellEntry const* spellInfo = sSpellTemplate.LookupEntry<SpellEntry>(spell_id);
    if (!spellInfo)
        return false;

    // Face target before starting auto-shoot (same reason as CB_CastSpell).
    if (!b->HasInArc(t, M_PI))
    {
        float angle = b->GetAngle(t);
        if (b->IsStopped())
        {
            b->SetOrientation(angle);
            b->SendHeartBeat();
        }
        else
        {
            b->SetFacingTo(angle);
        }
        b->SetInFront(t);
    }

    Spell* spell = new Spell(b, spellInfo, false);
    SpellCastTargets targets;
    targets.setUnitTarget(t);
    spell->SpellStart(&targets);
    return true;
}

bool BotBridge::CB_Say(BotHandle bot, const char* msg, uint32_t lang)
{
    Player* b = FindBot(bot);
    if (!b || !msg)
        return false;
    b->Say(msg, static_cast<uint32_t>(lang));
    return true;
}

bool BotBridge::CB_Whisper(BotHandle bot, uint64_t target_guid, const char* msg)
{
    Player* b = FindBot(bot);
    if (!b || !msg || !target_guid)
        return false;
    ObjectGuid target(target_guid);
    b->Whisper(msg, LANG_UNIVERSAL, target);
    return true;
}

// PB2 TellPlayerNoFacing routing rule: if the bot is in a group, broadcast
// the reply to that group's PARTY/RAID channel (so every group member sees
// it, not just the command sender). If the bot is solo, fall back to a
// whisper so a random player asking a question still gets an answer.
bool BotBridge::CB_TellPlayer(BotHandle bot, uint64_t target_guid, const char* msg)
{
    Player* b = FindBot(bot);
    if (!b || !msg)
        return false;

    Group* group = b->GetGroup();
    if (group)
    {
        const ChatMsg msgType = group->IsRaidGroup() ? CHAT_MSG_RAID : CHAT_MSG_PARTY;
        WorldPacket data;
        ChatHandler::BuildChatPacket(data, msgType, msg, LANG_UNIVERSAL, CHAT_TAG_NONE,
                                     b->GetObjectGuid(), b->GetName());
        group->BroadcastPacket(data, false);
        return true;
    }

    if (target_guid)
    {
        ObjectGuid target(target_guid);
        b->Whisper(msg, LANG_UNIVERSAL, target);
        return true;
    }
    return false;
}

// Addon-channel reply: commands that arrived via `#a` / SendAddonMessage("BOT",…)
// must have their replies routed back over the addon wire rather than whisper,
// so the Mangosbot UI's CHAT_MSG_ADDON handler consumes them. Mirrors PB2
// `PlayerbotAI.cpp:3475-3485`: prepend `BOT\t`, wrap as CHAT_MSG_PARTY +
// LANG_ADDON (1.12 wire form for addon messages), and deliver directly to the
// sender's session so only they receive it.
bool BotBridge::CB_TellAddon(BotHandle bot, uint64_t target_guid, const char* msg)
{
    Player* b = FindBot(bot);
    if (!b || !msg || !target_guid)
        return false;

    Player* target = sObjectAccessor.FindPlayer(ObjectGuid(target_guid));
    if (!target)
        return false;

    std::string payload;
    payload.reserve(4 + strlen(msg));
    payload.append("BOT\t");
    payload.append(msg);

    WorldPacket data;
    ChatHandler::BuildChatPacket(data, CHAT_MSG_PARTY, payload.c_str(), LANG_ADDON,
                                 CHAT_TAG_NONE, b->GetObjectGuid(), b->GetName());
    target->GetSession()->SendPacket(data);
    return true;
}

// Broadcast an addon message to the bot's group/raid. Used for protocols
// like KLHThreatMeter where the bot must appear as a normal addon user.
// Wire format: "PREFIX\tBODY" sent as CHAT_MSG_PARTY/RAID + LANG_ADDON.
bool BotBridge::CB_SendGroupAddonMessage(BotHandle bot, const char* prefix, const char* msg)
{
    Player* b = FindBot(bot);
    if (!b || !prefix || !msg)
        return false;

    Group* group = b->GetGroup();
    if (!group)
        return false;

    std::string payload;
    payload.reserve(strlen(prefix) + 1 + strlen(msg));
    payload.append(prefix);
    payload.push_back('\t');
    payload.append(msg);

    const ChatMsg msgType = group->IsRaidGroup() ? CHAT_MSG_RAID : CHAT_MSG_PARTY;
    WorldPacket data;
    ChatHandler::BuildChatPacket(data, msgType, payload.c_str(), LANG_ADDON,
                                 CHAT_TAG_NONE, b->GetObjectGuid(), b->GetName());
    group->BroadcastPacket(data, false);
    return true;
}

bool BotBridge::CB_UseItem(BotHandle bot, uint32_t item_id, UnitHandle target)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    Item* item = b->GetItemByEntry(item_id);
    if (!item)
        return false;

    // This fork has no Player::UseItem(item, targets) helper — the real path
    // goes through CMSG_USE_ITEM packet handling. For the stripped bridge we
    // cast the item's first OnUse spell directly.
    (void)target;
    ItemPrototype const* proto = item->GetProto();
    if (!proto)
        return false;

    uint32 spellId = 0;
    for (int i = 0; i < MAX_ITEM_PROTO_SPELLS; ++i)
    {
        if (proto->Spells[i].SpellId > 0)
        {
            spellId = proto->Spells[i].SpellId;
            break;
        }
    }
    if (spellId == 0)
        return false;

    SpellEntry const* info = sSpellTemplate.LookupEntry<SpellEntry>(spellId);
    if (!info)
        return false;

    Unit* t = FindUnit(bot, target);
    Spell* spell = new Spell(b, info, false);
    SpellCastTargets targets;
    targets.setUnitTarget(t ? t : static_cast<Unit*>(b));
    spell->SpellStart(&targets);
    return true;
}

uint32_t BotBridge::CB_FindFoodDrinkInBags(BotHandle bot, uint32_t category)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;

    // category: 11 = food (HP), 59 = drink (mana).
    // Walk all bag items and pick the highest-level consumable matching the
    // requested food category. WoW food/drink items have one of their spells
    // with SpellCategory matching the food/drink category constants.
    uint32_t bestItem = 0;
    uint32_t bestLevel = 0;

    for (uint8 bag = INVENTORY_SLOT_ITEM_START; bag < INVENTORY_SLOT_ITEM_END; ++bag)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, bag);
        if (!item)
            continue;
        ItemPrototype const* proto = item->GetProto();
        if (!proto || proto->Class != ITEM_CLASS_CONSUMABLE)
            continue;

        // Check if any item spell matches the requested food/drink category.
        bool matches = false;
        for (int i = 0; i < MAX_ITEM_PROTO_SPELLS; ++i)
        {
            if (proto->Spells[i].SpellId > 0 && proto->Spells[i].SpellCategory == category)
            {
                matches = true;
                break;
            }
        }
        if (!matches)
            continue;

        // Prefer higher item-level (better stat food restores more).
        if (proto->RequiredLevel >= bestLevel)
        {
            bestLevel = proto->RequiredLevel;
            bestItem = proto->ItemId;
        }
    }

    // Also scan additional bag slots.
    for (uint8 bag = INVENTORY_SLOT_BAG_START; bag < INVENTORY_SLOT_BAG_END; ++bag)
    {
        Bag* pBag = reinterpret_cast<Bag*>(b->GetItemByPos(INVENTORY_SLOT_BAG_0, bag));
        if (!pBag || !pBag->IsBag())
            continue;
        for (uint32 slot = 0; slot < pBag->GetBagSize(); ++slot)
        {
            Item* item = pBag->GetItemByPos(slot);
            if (!item)
                continue;
            ItemPrototype const* proto = item->GetProto();
            if (!proto || proto->Class != ITEM_CLASS_CONSUMABLE)
                continue;

            bool matches = false;
            for (int i = 0; i < MAX_ITEM_PROTO_SPELLS; ++i)
            {
                if (proto->Spells[i].SpellId > 0 && proto->Spells[i].SpellCategory == category)
                {
                    matches = true;
                    break;
                }
            }
            if (!matches)
                continue;

            if (proto->RequiredLevel >= bestLevel)
            {
                bestLevel = proto->RequiredLevel;
                bestItem = proto->ItemId;
            }
        }
    }

    return bestItem;
}

bool BotBridge::CB_UseTrinket(BotHandle bot, uint8_t slot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    uint8_t eq_slot;
    switch (slot)
    {
        case 0: eq_slot = EQUIPMENT_SLOT_TRINKET1; break;
        case 1: eq_slot = EQUIPMENT_SLOT_TRINKET2; break;
        default: return false;
    }

    Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, eq_slot);
    if (!item)
        return false;

    ItemPrototype const* proto = item->GetProto();
    if (!proto)
        return false;

    // Walk the OnUse spell list and fire the first one that is both known
    // and ready. Mirrors CB_UseItem's spell pick, with an added cooldown
    // gate so `Bt::UseTrinket` callers don't have to pre-check.
    for (int i = 0; i < MAX_ITEM_PROTO_SPELLS; ++i)
    {
        uint32 spellId = proto->Spells[i].SpellId;
        if (spellId == 0)
            continue;
        if (b->HasSpellCooldown(spellId))
            continue;
        SpellEntry const* info = sSpellTemplate.LookupEntry<SpellEntry>(spellId);
        if (!info)
            continue;
        Spell* spell = new Spell(b, info, false);
        SpellCastTargets targets;
        targets.setUnitTarget(static_cast<Unit*>(b));
        spell->SpellStart(&targets);
        return true;
    }
    return false;
}

bool BotBridge::CB_Taunt(BotHandle bot, UnitHandle target)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return false;

    // Find and cast the bot's taunt spell (class-dependent)
    // Warrior: Taunt (355), Paladin: Righteous Defense (31789),
    // Druid: Growl (6795), DK: Dark Command (56222)
    static const uint32_t tauntSpells[] = {355, 31789, 6795, 56222, 0};
    for (int i = 0; tauntSpells[i] != 0; ++i)
    {
        if (b->HasSpell(tauntSpells[i]) && b->IsSpellReady(tauntSpells[i]))
        {
            SpellEntry const* spellInfo = sSpellTemplate.LookupEntry<SpellEntry>(tauntSpells[i]);
            if (!spellInfo)
                continue;
            Spell* spell = new Spell(b, spellInfo, false);
            SpellCastTargets targets;
            targets.setUnitTarget(t);
            spell->SpellStart(&targets);
            return true;
        }
    }
    return false;
}

bool BotBridge::CB_TeleportTo(BotHandle bot, uint32_t map_id, float x, float y, float z, float o)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // Interrupt any in-progress spell / movement before teleporting, the
    // same way the old PB2 SummonAction did.
    if (b->IsTaxiFlying())
        return false;
    if (b->IsNonMeleeSpellCasted(true))
        b->InterruptNonMeleeSpells(true);

    return b->TeleportTo(map_id, x, y, z, o);
}

bool BotBridge::CB_GetPlayerPosition(BotHandle /*bot*/, uint64_t player_guid, BotPosition* out_pos)
{
    if (!out_pos)
        return false;
    Player* target = sObjectAccessor.FindPlayer(ObjectGuid(player_guid));
    if (!target)
        return false;
    out_pos->x      = target->GetPositionX();
    out_pos->y      = target->GetPositionY();
    out_pos->z      = target->GetPositionZ();
    out_pos->o      = target->GetOrientation();
    out_pos->map_id = target->GetMapId();
    return true;
}

bool BotBridge::CB_SummonToPlayer(BotHandle bot, uint64_t requester_guid)
{
    // Mirrors PB2 `SummonAction::Teleport` exactly — angle search around the
    // requester for a LOS-clear spot offset by the configured follow range,
    // reviving the bot in place if it is dead, then teleporting.
    Player* b = FindBot(bot);
    if (!b)
        return false;
    Player* requester = sObjectAccessor.FindPlayer(ObjectGuid(requester_guid));
    if (!requester || requester->IsBeingTeleported())
        return false;
    if (b->IsBeingTeleported() || b->IsTaxiFlying())
        return false;

    const float followRange = playerbot_config_follow_distance() > 0.0f
                                  ? playerbot_config_follow_distance()
                                  : 3.0f;

    // PB2 iterates angle ± π in π/4 steps starting from the bot's follow
    // angle. We don't have the AI context here so start at 0 — the search
    // still covers the full circle, just rotated.
    for (double angle = -M_PI; angle <= M_PI; angle += M_PI / 4.0)
    {
        uint32_t mapId = requester->GetMapId();
        float x = requester->GetPositionX() + std::cos(angle) * followRange;
        float y = requester->GetPositionY() + std::sin(angle) * followRange;
        float z = requester->GetPositionZ();
        requester->UpdateGroundPositionZ(x, y, z);

        float los_z = z + b->GetCollisionHeight();
        if (!requester->IsWithinLOS(x, y, los_z, true))
        {
            // Fall back to the requester's exact position (guaranteed in LOS).
            x = requester->GetPositionX();
            y = requester->GetPositionY();
            z = requester->GetPositionZ();
        }

        if (requester->IsWithinLOS(x, y, z + b->GetCollisionHeight(), true))
        {
            if (!b->IsAlive() && requester->IsAlive())
            {
                b->ResurrectPlayer(1.0f, false);
                b->SpawnCorpseBones();
            }

            if (b->IsTaxiFlying())
            {
                b->TaxiFlightInterrupt();
                b->GetMotionMaster()->MovementExpired();
            }

            if (b->IsNonMeleeSpellCasted(true))
                b->InterruptNonMeleeSpells(true);

            b->GetMotionMaster()->Clear();
            b->TeleportTo(mapId, x, y, z, 0.0f);

            if (GenericTransport* transport = requester->GetTransport())
                transport->AddPassenger(b, false);

            return true;
        }
    }
    return false;
}

// ── Group / raid ──────────────────────────────────────────────────────────

UnitHandle BotBridge::CB_GroupGetTank(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    Group* group = b->GetGroup();
    if (!group)
        return 0;

    // This fork tracks the main tank via a dedicated guid rather than a
    // per-member flag. Fall back to nothing if no main tank is set.
    return group->GetMainTankGuid().GetRawValue();
}

UnitHandle BotBridge::CB_GroupGetHealer(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    Group* group = b->GetGroup();
    if (!group)
        return 0;

    for (GroupReference* ref = group->GetFirstMember(); ref != nullptr; ref = ref->next())
    {
        Player* member = ref->getSource();
        if (!member)
            continue;
        uint8_t cls = member->getClass();
        // Priest, Paladin, Druid, Shaman as potential healers
        if (cls == CLASS_PRIEST || cls == CLASS_PALADIN ||
            cls == CLASS_DRUID  || cls == CLASS_SHAMAN)
        {
            return member->GetGUID();
        }
    }
    return 0;
}

uint8_t BotBridge::CB_GroupGetRole(BotHandle bot, UnitHandle member)
{
    Player* b     = FindBot(bot);
    Unit*   m_raw = FindUnit(bot, member);
    if (!b || !m_raw)
        return 0;

    Player* m = m_raw->ToPlayer();
    if (!m)
        return 0;

    Group* group = b->GetGroup();
    if (!group)
        return 0;

    // This fork has no per-member role flags. Only main tank is tracked.
    uint8_t role = 0;
    if (group->GetMainTankGuid() == m->GetObjectGuid())
        role |= 1; // TANK
    // No healer / assist flag in vanilla — infer healer from class.
    uint8_t cls = m->getClass();
    if (cls == CLASS_PRIEST || cls == CLASS_PALADIN ||
        cls == CLASS_DRUID  || cls == CLASS_SHAMAN)
        role |= 2; // HEAL

    return role;
}

UnitHandle BotBridge::CB_GetUnitWithRaidIcon(BotHandle bot, uint8_t icon)
{
    // Raid target icons are 0..7 on the wire (0 = star, 7 = skull).
    // The Rust side uses 1..8 (1 = star, 8 = skull) to match the UI.
    if (icon == 0 || icon > 8)
        return 0;
    uint8_t internal = icon - 1;

    Player* b = FindBot(bot);
    if (!b)
        return 0;

    Group* group = b->GetGroup();
    if (!group)
        return 0;

    // NOTE: Depending on CMaNGOS branch this method is named
    // `GetTargetIcon(idx)` or `GetTargetWithIcon(idx)`. If the build fails
    // here, swap to whichever symbol exists in `Group.h`.
    ObjectGuid target = group->GetTargetIcon(internal);
    if (target.IsEmpty())
        return 0;

    // Only return hostile units — raid icons on friendly players would
    // break `pull rti` / `attack rti` semantics.
    Unit* u = sObjectAccessor.GetUnit(*b, target);
    if (!u || !b->IsHostileTo(u))
        return 0;

    return target.GetRawValue();
}

// Assign raid target icon `icon` (0..7, raw `TargetIconList` index — star=0,
// skull=7) to `target`. Broadcasts MSG_RAID_TARGET_UPDATE to the whole group.
// Returns false when the bot is solo, the icon index is out of range, or the
// target handle can't be resolved. Passing `target == 0` clears the icon.
// Used by the 11g `MarkRti`/`MarkRtiCc` BT leaves. Note: this callback uses
// 0..7 indexing to match `BotWorldSnapshot::group_raid_target_icons` and
// `Group::SetTargetIcon`, whereas `CB_GetUnitWithRaidIcon` uses 1..8 — the
// inconsistency is intentional (both mirror their respective call sites)
// and the BT handlers translate at the caller.
bool BotBridge::CB_GroupSetTargetIcon(BotHandle bot, UnitHandle target, uint8_t icon)
{
    if (icon >= 8)
        return false;

    Player* b = FindBot(bot);
    if (!b)
        return false;

    Group* group = b->GetGroup();
    if (!group)
        return false;

    ObjectGuid guid = ObjectGuid();
    if (target != 0)
    {
        Unit* u = FindUnit(bot, target);
        if (!u)
            return false;
        guid = u->GetObjectGuid();
    }

    group->SetTargetIcon(icon, guid);
    return true;
}

// ═══════════════════════════════════════════════════════════════════════════
// NEW CALLBACKS — Phase 3-8 behavior
// ═══════════════════════════════════════════════════════════════════════════

// ── Death / resurrection ──────────────────────────────────────────────────

bool BotBridge::CB_AcceptResurrect(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // If there's a pending resurrect request, accept it
    if (!b->isRessurectRequested())
        return false;

    b->ResurrectUsingRequestDataInit();
    return true;
}

BotPosition BotBridge::CB_GetCorpsePosition(BotHandle bot)
{
    BotPosition pos{};
    Player* b = FindBot(bot);
    if (!b)
        return pos;

    Corpse* corpse = b->GetCorpse();
    if (!corpse)
        return pos; // {0,0,0} = no corpse

    pos.x      = corpse->GetPositionX();
    pos.y      = corpse->GetPositionY();
    pos.z      = corpse->GetPositionZ();
    pos.o      = corpse->GetOrientation();
    pos.map_id = corpse->GetMapId();
    return pos;
}

bool BotBridge::CB_UseSpiritHealer(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b || b->IsAlive())
        return false;

    // Repop at graveyard with durability loss
    b->RepopAtGraveyard();
    return true;
}

bool BotBridge::CB_ResurrectSelf(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b || b->IsAlive())
        return false;

    // Mirror PB2 `SummonAction::Teleport`: full-HP in-place revive and
    // clean up the corpse. Caller (Rust summon handler) teleports
    // immediately after so position is irrelevant here.
    b->ResurrectPlayer(1.0f, false);
    b->SpawnCorpseBones();
    return true;
}

// ── Mount ─────────────────────────────────────────────────────────────────

bool BotBridge::CB_IsMounted(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    return b->IsMounted();
}

bool BotBridge::CB_MountUp(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b || b->IsMounted() || b->IsInCombat())
        return false;

    // Find the best mount spell the bot knows.
    // Epic mounts (level 60 riding): 23228 (Swift Palomino), etc.
    // Regular mounts (level 40 riding).
    // We look for any mount aura the bot can cast.
    static const uint32_t mountSpells[] = {
        // Epic mounts (60% → 100% speed)
        23228, 23229, 23227, 23338, 23219, 23221, 23246, 23247, 23248, 23249,
        23250, 23251, 23252, 23338, 23509, 23510,
        // Regular mounts (40 → 60% speed)
        6898, 6899, 6648, 458, 470, 580, 8394, 8395, 10793, 10796, 10969,
        17229, 17450, 17459, 17460, 17461, 17462, 17463, 17464, 17465,
        // Catch-all: Summon Charger / Warhorse (Paladin), Felsteed (Warlock)
        13819, 23214, 34767, 34769, 5784, 23161,
        0
    };

    for (int i = 0; mountSpells[i] != 0; ++i)
    {
        if (b->HasSpell(mountSpells[i]) && b->IsSpellReady(mountSpells[i]))
        {
            SpellEntry const* info = sSpellTemplate.LookupEntry<SpellEntry>(mountSpells[i]);
            if (!info)
                continue;
            Spell* spell = new Spell(b, info, false);
            SpellCastTargets targets;
            targets.setUnitTarget(b);
            spell->SpellStart(&targets);
            return true;
        }
    }
    return false;
}

bool BotBridge::CB_Dismount(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b || !b->IsMounted())
        return false;
    b->RemoveSpellsCausingAura(SPELL_AURA_MOUNTED);
    return true;
}

bool BotBridge::CB_IsIndoor(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    // This fork has no Player::IsIndoors — WMO group detection is not
    // exposed. Callers only use this to gate mount-up, which also checks
    // combat, so returning false here is a safe conservative answer.
    return false;
}

// ── Loot ──────────────────────────────────────────────────────────────────

UnitHandle* BotBridge::CB_GetNearbyLootable(BotHandle bot, float range, uint32_t* out_count)
{
    *out_count = 0;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    std::vector<UnitHandle> handles;

    // Find dead creatures that have loot
    CreatureList creatures;
    MaNGOS::AllCreaturesOfEntryInRangeCheck anyCreatureCheck(b, 0, range);
    MaNGOS::CreatureListSearcher<MaNGOS::AllCreaturesOfEntryInRangeCheck> searcher(creatures, anyCreatureCheck);
    Cell::VisitAllObjects(b, searcher, range);

    for (Creature* c : creatures)
    {
        if (!c || c->IsAlive())
            continue;
        if (c->HasFlag(UNIT_DYNAMIC_FLAGS, UNIT_DYNFLAG_LOOTABLE))
            handles.push_back(c->GetObjectGuid().GetRawValue());
    }

    if (handles.empty())
        return nullptr;

    UnitHandle* arr = new UnitHandle[handles.size()];
    std::copy(handles.begin(), handles.end(), arr);
    *out_count = static_cast<uint32_t>(handles.size());
    return arr;
}

bool BotBridge::CB_OpenLoot(BotHandle bot, UnitHandle target)
{
    // Mirrors PB2 `OpenLootAction::DoLoot` + `StoreLootAction::Execute`
    // collapsed into a single call: send CMSG_LOOT to construct the Loot,
    // then iterate and take every allowed item plus the gold, and finally
    // release. This matches what PB2 did — the two actions were split
    // only because its BT needed a gap for the network round-trip to the
    // real client, which we don't need here.
    Player* b = FindBot(bot);
    if (!b)
        return false;

    Unit* t = FindUnit(bot, target);
    if (!t)
        return false;

    Creature* c = dynamic_cast<Creature*>(t);
    if (!c)
        return false;
    if (!c->HasFlag(UNIT_DYNAMIC_FLAGS, UNIT_DYNFLAG_LOOTABLE))
        return false;
    if (b->GetDistance(c) > INTERACTION_DISTANCE)
        return false;

    ObjectGuid targetGuid = c->GetObjectGuid();

    // 1. Open the loot window (this initializes Loot for the bot).
    WorldPacket openPacket(CMSG_LOOT, 8);
    openPacket << targetGuid;
    b->GetSession()->HandleLootOpcode(openPacket);

    Loot* loot = sLootMgr.GetLoot(b, targetGuid);
    if (!loot)
        return false;

    // 2. Grab any gold.
    if (loot->GetGoldAmount() > 0)
    {
        WorldPacket moneyPacket(CMSG_LOOT_MONEY, 0);
        b->GetSession()->HandleLootMoneyOpcode(moneyPacket);
    }

    // Before AutoStore consumes the list, note the best rare-or-better item
    // for a loot reaction (a bit of life in /say). Greens/greys are too common
    // to bother announcing, so gate at ITEM_QUALITY_RARE (blue).
    std::string notableName;
    uint32 notableQuality = 0;
    {
        LootItemList items;
        loot->GetLootItemsListFor(b, items);
        for (LootItem* li : items)
        {
            if (li && li->itemProto && li->itemProto->Quality >= ITEM_QUALITY_RARE
                && li->itemProto->Quality > notableQuality)
            {
                notableQuality = li->itemProto->Quality;
                notableName = li->itemProto->Name1 ? li->itemProto->Name1 : "";
            }
        }
    }

    // 3. AutoStore handles filtering (permission, inventory space,
    //    blocked-for-roll) and removes taken items from the loot list.
    loot->AutoStore(b);

    // 4. Release the corpse window.
    WorldPacket releasePacket(CMSG_LOOT_RELEASE, 8);
    releasePacket << targetGuid;
    b->GetSession()->HandleLootReleaseOpcode(releasePacket);

    // React to a good drop. Epics get a bigger shout than blues.
    if (!notableName.empty())
    {
        const std::string msg = (notableQuality >= ITEM_QUALITY_EPIC)
            ? ("Whoa! [" + notableName + "]!")
            : ("Nice, looted [" + notableName + "].");
        b->Say(msg, LANG_UNIVERSAL);
    }

    return true;
}

bool BotBridge::CB_TakeAllLoot(BotHandle /*bot*/)
{
    // CB_OpenLoot already performs the full open → take → release flow
    // in one step, so the Rust side's follow-up call is a no-op success.
    return true;
}

// ── NPC interaction ───────────────────────────────────────────────────────

UnitHandle* BotBridge::CB_GetNearbyNpcs(BotHandle bot, float range, uint32_t npc_flags,
                                         uint32_t* out_count)
{
    *out_count = 0;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    std::vector<UnitHandle> handles;

    CreatureList creatures;
    MaNGOS::AllCreaturesOfEntryInRangeCheck check(b, 0, range);
    MaNGOS::CreatureListSearcher<MaNGOS::AllCreaturesOfEntryInRangeCheck> searcher(creatures, check);
    Cell::VisitAllObjects(b, searcher, range);

    for (Creature* c : creatures)
    {
        if (!c || !c->IsAlive())
            continue;
        if (npc_flags != 0 && !(c->GetUInt32Value(UNIT_NPC_FLAGS) & npc_flags))
            continue;
        if (!b->IsHostileTo(c))
            handles.push_back(c->GetObjectGuid().GetRawValue());
    }

    if (handles.empty())
        return nullptr;

    UnitHandle* arr = new UnitHandle[handles.size()];
    std::copy(handles.begin(), handles.end(), arr);
    *out_count = static_cast<uint32_t>(handles.size());
    return arr;
}

bool BotBridge::CB_InteractNpc(BotHandle bot, UnitHandle npc)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, npc);
    if (!b || !t)
        return false;

    Creature* creature = dynamic_cast<Creature*>(t);
    if (!creature)
        return false;

    b->PrepareGossipMenu(creature, creature->GetDefaultGossipMenuId());
    b->SendPreparedGossip(creature);
    return true;
}

bool BotBridge::CB_RepairAll(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // Durability cost from repair NPC — we pay from bot's gold
    b->DurabilityRepairAll(false, 0.0f);
    return true;
}

bool BotBridge::CB_SellGreyItems(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    bool sold = false;
    for (int i = INVENTORY_SLOT_ITEM_START; i < INVENTORY_SLOT_ITEM_END; ++i)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
        if (!item)
            continue;
        ItemPrototype const* proto = item->GetProto();
        if (!proto || proto->Quality != ITEM_QUALITY_POOR)
            continue;

        uint32_t count = item->GetCount();
        uint32_t money = proto->SellPrice * count;
        b->ModifyMoney(money);
        b->DestroyItem(INVENTORY_SLOT_BAG_0, i, true);
        sold = true;
    }

    // Also check bags
    for (int bag = INVENTORY_SLOT_BAG_START; bag < INVENTORY_SLOT_BAG_END; ++bag)
    {
        Bag* pBag = dynamic_cast<Bag*>(b->GetItemByPos(INVENTORY_SLOT_BAG_0, bag));
        if (!pBag)
            continue;
        for (uint32_t slot = 0; slot < pBag->GetBagSize(); ++slot)
        {
            Item* item = b->GetItemByPos(bag, slot);
            if (!item)
                continue;
            ItemPrototype const* proto = item->GetProto();
            if (!proto || proto->Quality != ITEM_QUALITY_POOR)
                continue;

            uint32_t count = item->GetCount();
            uint32_t money = proto->SellPrice * count;
            b->ModifyMoney(money);
            b->DestroyItem(bag, slot, true);
            sold = true;
        }
    }
    return sold;
}

bool BotBridge::CB_HasSellableItems(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    for (int i = INVENTORY_SLOT_ITEM_START; i < INVENTORY_SLOT_ITEM_END; ++i)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
        if (!item)
            continue;
        ItemPrototype const* proto = item->GetProto();
        if (proto && proto->Quality == ITEM_QUALITY_POOR && proto->SellPrice > 0)
            return true;
    }

    for (int bag = INVENTORY_SLOT_BAG_START; bag < INVENTORY_SLOT_BAG_END; ++bag)
    {
        Bag* pBag = dynamic_cast<Bag*>(b->GetItemByPos(INVENTORY_SLOT_BAG_0, bag));
        if (!pBag)
            continue;
        for (uint32_t slot = 0; slot < pBag->GetBagSize(); ++slot)
        {
            Item* item = b->GetItemByPos(bag, slot);
            if (!item)
                continue;
            ItemPrototype const* proto = item->GetProto();
            if (proto && proto->Quality == ITEM_QUALITY_POOR && proto->SellPrice > 0)
                return true;
        }
    }
    return false;
}

float BotBridge::CB_GetDurabilityPct(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 1.0f;

    uint32_t maxDura = 0;
    uint32_t curDura = 0;
    for (int i = EQUIPMENT_SLOT_START; i < EQUIPMENT_SLOT_END; ++i)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
        if (!item)
            continue;
        maxDura += item->GetUInt32Value(ITEM_FIELD_MAXDURABILITY);
        curDura += item->GetUInt32Value(ITEM_FIELD_DURABILITY);
    }
    if (maxDura == 0)
        return 1.0f;
    return static_cast<float>(curDura) / static_cast<float>(maxDura);
}

// ── Quest ─────────────────────────────────────────────────────────────────

BotQuestInfo* BotBridge::CB_GetQuestLog(BotHandle bot, uint32_t* out_count)
{
    *out_count = 0;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    std::vector<BotQuestInfo> results;
    for (uint16_t slot = 0; slot < MAX_QUEST_LOG_SIZE; ++slot)
    {
        uint32_t questId = b->GetQuestSlotQuestId(slot);
        if (questId == 0)
            continue;
        QuestStatusData const* status = &b->getQuestStatusMap()[questId];
        BotQuestInfo info{};
        info.quest_id = questId;
        info.complete = (status && status->m_status == QUEST_STATUS_COMPLETE);
        results.push_back(info);
    }

    if (results.empty())
        return nullptr;

    BotQuestInfo* arr = new BotQuestInfo[results.size()];
    std::copy(results.begin(), results.end(), arr);
    *out_count = static_cast<uint32_t>(results.size());
    return arr;
}

void BotBridge::CB_FreeQuestLog(BotQuestInfo* list)
{
    delete[] list;
}

bool BotBridge::CB_AcceptAllQuests(BotHandle bot, UnitHandle npc)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, npc);
    if (!b || !t)
        return false;

    Creature* creature = dynamic_cast<Creature*>(t);
    if (!creature)
        return false;

    // Get all quests this NPC can give
    QuestRelationsMapBounds bounds = sObjectMgr.GetCreatureQuestRelationsMapBounds(creature->GetEntry());
    bool accepted = false;
    for (auto it = bounds.first; it != bounds.second; ++it)
    {
        uint32_t questId = it->second;
        Quest const* quest = sObjectMgr.GetQuestTemplate(questId);
        if (!quest || !b->CanTakeQuest(quest, false))
            continue;

        b->AddQuest(quest, creature);
        if (b->CanCompleteQuest(questId))
            b->CompleteQuest(questId);
        accepted = true;
    }
    return accepted;
}

// Pick the best reward-choice index for a quest with a choice of rewards.
// Prefer an item the bot can actually use (equippable by class/level), ranked
// by item level; fall back to sell value so a non-usable choice still grabs
// the most valuable option. Index 0 when there is no real choice. Mirrors the
// intent of PB2's quest-reward usefulness selection.
static uint32 PickBestRewardChoice(Player* b, Quest const* quest)
{
    const uint32 count = quest->GetRewChoiceItemsCount();
    if (count <= 1)
        return 0;

    uint32 bestIdx = 0;
    uint64 bestScore = 0;
    for (uint32 i = 0; i < count; ++i)
    {
        const uint32 itemId = quest->RewChoiceItemId[i];
        if (!itemId)
            continue;
        ItemPrototype const* proto = sObjectMgr.GetItemPrototype(itemId);
        if (!proto)
            continue;

        const bool usable = (b->CanUseItem(proto) == EQUIP_ERR_OK);
        const uint64 score = (usable ? static_cast<uint64>(proto->ItemLevel) * 1000000ull : 0ull)
                           + static_cast<uint64>(proto->SellPrice);
        if (score > bestScore)
        {
            bestScore = score;
            bestIdx = i;
        }
    }
    return bestIdx;
}

bool BotBridge::CB_TurnInQuest(BotHandle bot, UnitHandle npc, uint32_t quest_id)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, npc);
    if (!b || !t)
        return false;

    Creature* creature = dynamic_cast<Creature*>(t);
    if (!creature)
        return false;

    Quest const* quest = sObjectMgr.GetQuestTemplate(quest_id);
    if (!quest)
        return false;

    if (b->GetQuestStatus(quest_id) != QUEST_STATUS_COMPLETE)
        return false;

    b->RewardQuest(quest, PickBestRewardChoice(b, quest), creature, true);
    return true;
}

// ── Unit queries (extended) ───────────────────────────────────────────────

bool BotBridge::CB_IsAttackable(BotHandle bot, UnitHandle target)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return false;

    if (!t->IsAlive())
        return false;

    return b->IsHostileTo(t) && b->CanAttack(t);
}

uint8_t BotBridge::CB_GetUnitLevel(BotHandle bot, UnitHandle target)
{
    Unit* t = FindUnit(bot, target);
    if (!t)
        return 0;
    return static_cast<uint8_t>(t->GetLevel());
}

bool BotBridge::CB_IsCastingInterruptible(BotHandle bot, UnitHandle target)
{
    Unit* t = FindUnit(bot, target);
    if (!t)
        return false;

    Spell* spell = t->GetCurrentSpell(CURRENT_GENERIC_SPELL);
    if (!spell || !spell->m_spellInfo)
        return false;

    // SPELL_INTERRUPT_FLAG_INTERRUPT is absent from this fork's enum; any
    // non-zero interrupt mask indicates the cast can be broken by damage or
    // kick, which is the behavior the Rust side relies on.
    return spell->m_spellInfo->InterruptFlags != 0;
}

uint8_t BotBridge::CB_UnitKind(BotHandle bot, UnitHandle target)
{
    if (target == 0)
        return 0;
    ObjectGuid guid = MakeGuid(target);
    // Players and pets can be decided from the guid high bits alone; no
    // world lookup required.
    if (guid.IsPlayer())
        return 1;
    if (guid.IsPet())
        return 2;

    // Critter check requires resolving the live Creature on the bot's map.
    Unit* t = FindUnit(bot, target);
    if (!t)
        return 0;
    if (t->GetTypeId() == TYPEID_UNIT && static_cast<Creature*>(t)->IsCritter())
        return 3;
    return 0;
}

// ── Pet management ────────────────────────────────────────────────────────

bool BotBridge::CB_HasPet(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    return b->GetPet() != nullptr;
}

bool BotBridge::CB_PetIsAlive(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    Pet* pet = b->GetPet();
    return pet && pet->IsAlive();
}

uint8_t BotBridge::CB_PetHappiness(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 3; // happy default
    Pet* pet = b->GetPet();
    if (!pet)
        return 3;
    return static_cast<uint8_t>(pet->GetHappinessState());
}

uint8_t BotBridge::CB_PetHealthPct(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    Pet* pet = b->GetPet();
    if (!pet || !pet->IsAlive())
        return 0;
    return static_cast<uint8_t>(
        (pet->GetHealth() * 100) / std::max(pet->GetMaxHealth(), uint32_t(1)));
}

bool BotBridge::CB_SummonPet(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // Hunter Call Pet (883) first. For warlocks, prefer the Voidwalker
    // (697) — the standard survivable solo-leveling demon — then Felhunter
    // (691), Succubus (712), and finally Imp (688). HasSpell gates each, so a
    // low-level warlock that only knows Summon Imp still gets the Imp; an
    // established warlock who has learned the Voidwalker tanks for itself
    // instead of summoning the fragile Imp the old order picked first.
    static const uint32_t callPetSpells[] = {883, 697, 691, 712, 688, 0};
    for (int i = 0; callPetSpells[i] != 0; ++i)
    {
        if (b->HasSpell(callPetSpells[i]) && b->IsSpellReady(callPetSpells[i]))
        {
            SpellEntry const* info = sSpellTemplate.LookupEntry<SpellEntry>(callPetSpells[i]);
            if (!info)
                continue;
            Spell* spell = new Spell(b, info, false);
            SpellCastTargets targets;
            targets.setUnitTarget(b);
            spell->SpellStart(&targets);
            return true;
        }
    }
    return false;
}

bool BotBridge::CB_RevivePet(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // Revive Pet (982)
    uint32_t reviveSpell = 982;
    if (!b->HasSpell(reviveSpell) || !b->IsSpellReady(reviveSpell))
        return false;

    SpellEntry const* info = sSpellTemplate.LookupEntry<SpellEntry>(reviveSpell);
    if (!info)
        return false;

    Spell* spell = new Spell(b, info, false);
    SpellCastTargets targets;
    targets.setUnitTarget(b);
    spell->SpellStart(&targets);
    return true;
}

bool BotBridge::CB_FeedPet(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    Pet* pet = b->GetPet();
    if (!pet)
        return false;

    // Feed Pet (6991) — requires a food item in inventory
    // Find a food item the pet can eat
    uint32_t petDiet = pet->GetCreatureInfo()->Family;
    for (int i = INVENTORY_SLOT_ITEM_START; i < INVENTORY_SLOT_ITEM_END; ++i)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
        if (!item)
            continue;
        ItemPrototype const* proto = item->GetProto();
        if (!proto || proto->Class != ITEM_CLASS_CONSUMABLE || proto->SubClass != ITEM_SUBCLASS_FOOD)
            continue;

        // Use Feed Pet spell (6991) with this item
        if (b->HasSpell(6991) && b->IsSpellReady(6991))
        {
            SpellEntry const* info = sSpellTemplate.LookupEntry<SpellEntry>(6991);
            if (!info)
                return false;
            Spell* spell = new Spell(b, info, false);
            SpellCastTargets targets;
            targets.setUnitTarget(pet);
            spell->SpellStart(&targets);
            return true;
        }
        break;
    }
    return false;
}

bool BotBridge::CB_PetAttack(BotHandle bot, UnitHandle target)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return false;

    Pet* pet = b->GetPet();
    if (!pet)
        return false;

    // Mirror PB2 AttackAction: send the pet at the owner's target unless it
    // is set passive. A master-less defensive pet is the common case.
    UnitAI* petAI = static_cast<Creature*>(pet)->AI();
    if (!petAI || petAI->GetReactState() == REACT_PASSIVE)
        return false;

    // Skip if the pet is already engaged on this victim — re-issuing
    // AttackStart restarts the chase spline and stutters the pet.
    if (pet->GetVictim() == t)
        return true;

    petAI->AttackStart(t);
    return true;
}

bool BotBridge::CB_CastPetSpell(BotHandle bot, uint32_t spell_id, UnitHandle target)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    Pet* pet = b->GetPet();
    if (!pet || !pet->IsAlive())
        return false;

    Unit* t = FindUnit(bot, target);
    if (!t)
        return false;

    // The pet must actually know this rank and have it off cooldown. The Rust
    // side tries each Spell Lock rank, so a "not known" here is expected, not
    // an error.
    if (!pet->HasSpell(spell_id) || pet->HasSpellCooldown(spell_id))
        return false;

    SpellEntry const* spellInfo = sSpellTemplate.LookupEntry<SpellEntry>(spell_id);
    if (!spellInfo)
        return false;

    // Face the target — pets are subject to the same in-front requirement as
    // players for most targeted spells.
    if (!pet->HasInArc(t, M_PI_F))
    {
        pet->SetFacingTo(pet->GetAngle(t));
        pet->SetInFront(t);
    }

    // Pre-validate with CheckCast (range, LoS, immunities) so we report an
    // honest success/failure to the AI rather than firing into the void.
    Spell* spell = new Spell(pet, spellInfo, false);
    SpellCastTargets targets;
    targets.setUnitTarget(t);
    spell->m_targets = targets;
    if (spell->CheckCast(true) != SPELL_CAST_OK)
    {
        delete spell;
        return false;
    }

    spell->SpellStart(&targets);
    return true;
}

// ── Dispel / party queries ────────────────────────────────────────────────

static bool IsDispelableBySpell(uint32_t debuffSpellId, Player* bot, uint8_t schoolFilter)
{
    SpellEntry const* debuffInfo = sSpellTemplate.LookupEntry<SpellEntry>(debuffSpellId);
    if (!debuffInfo)
        return false;

    uint32_t dispelMask = GetDispellMask(DispelType(debuffInfo->Dispel));

    // Caller-supplied school filter. `0` means "any school the bot can
    // dispel". Any non-zero mask restricts the search to its bits.
    if (schoolFilter != 0 && (dispelMask & schoolFilter) == 0)
        return false;

    // Dispel Magic (527/988) — removes magic
    if ((dispelMask & (1 << DISPEL_MAGIC)) && (bot->HasSpell(527) || bot->HasSpell(988)))
        return true;
    // Cleanse (4987) — removes magic, disease, poison (Paladin)
    if ((dispelMask & ((1 << DISPEL_MAGIC) | (1 << DISPEL_DISEASE) | (1 << DISPEL_POISON))) && bot->HasSpell(4987))
        return true;
    // Cure Disease (528) — removes disease (Priest)
    if ((dispelMask & (1 << DISPEL_DISEASE)) && bot->HasSpell(528))
        return true;
    // Abolish Poison (2893) — removes poison (Druid)
    if ((dispelMask & (1 << DISPEL_POISON)) && (bot->HasSpell(2893) || bot->HasSpell(8946)))
        return true;
    // Remove Curse (2782) — removes curses (Mage/Druid)
    if ((dispelMask & (1 << DISPEL_CURSE)) && (bot->HasSpell(2782) || bot->HasSpell(475)))
        return true;

    return false;
}

BotDispelTarget BotBridge::CB_FindDispellableTarget(BotHandle bot, uint8_t dispel_mask)
{
    BotDispelTarget result{};
    result.found = false;

    Player* b = FindBot(bot);
    if (!b)
        return result;

    Group* group = b->GetGroup();
    if (!group)
    {
        // Check self
        Unit::SpellAuraHolderMap const& holders = b->GetSpellAuraHolderMap();
        for (auto const& pair : holders)
        {
            if (!pair.second || IsPositiveSpell(pair.second->GetId()))
                continue;
            uint32_t spellId = pair.second->GetId();
            if (IsDispelableBySpell(spellId, b, dispel_mask))
            {
                result.unit     = b->GetGUID();
                result.spell_id = spellId;
                result.found    = true;
                return result;
            }
        }
        return result;
    }

    for (GroupReference* ref = group->GetFirstMember(); ref; ref = ref->next())
    {
        Player* member = ref->getSource();
        if (!member || !member->IsAlive() || !member->IsInWorld())
            continue;
        if (b->GetDistance(member) > 40.0f)
            continue;

        Unit::SpellAuraHolderMap const& holders = member->GetSpellAuraHolderMap();
        for (auto const& pair : holders)
        {
            if (!pair.second || IsPositiveSpell(pair.second->GetId()))
                continue;
            uint32_t spellId = pair.second->GetId();
            if (IsDispelableBySpell(spellId, b, dispel_mask))
            {
                result.unit     = member->GetGUID();
                result.spell_id = spellId;
                result.found    = true;
                return result;
            }
        }
    }
    return result;
}

// ── Consumables: potion bag query ─────────────────────────────────────────

uint32_t BotBridge::CB_FindPotionInBags(BotHandle bot, uint8_t category)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;

    // Iterate backpack + equipped bags. SpellCategory 4 is the shared
    // potion cooldown group used by every on-use potion; a spell effect
    // in {EFFECT_APPLY_AURA, EFFECT_SCHOOL_DAMAGE, EFFECT_DUMMY} combined
    // with an aura id in the stat-boost family (23–29, 189, 135) marks a
    // "buff" potion. Utility potions map to specific dummy/apply auras —
    // we match by the well-known free-action / invulnerability / swiftness
    // / living-action spell ids.
    static const uint32_t UTILITY_SPELLS[] = {
        6615,  /* Free Action Potion */
        3169,  /* Limited Invulnerability Potion */
        2379,  /* Swiftness Potion */
        6614,  /* Living Action Potion */
        7242,  /* Restorative Potion */
    };

    auto try_item = [&](Item* item) -> uint32_t {
        if (!item)
            return 0;
        ItemPrototype const* proto = item->GetProto();
        if (!proto || proto->Class != ITEM_CLASS_CONSUMABLE)
            return 0;
        // Must be usable by the bot (level, class, race).
        if (b->CanUseItem(proto) != EQUIP_ERR_OK)
            return 0;
        for (int s = 0; s < MAX_ITEM_PROTO_SPELLS; ++s)
        {
            uint32_t spellId = proto->Spells[s].SpellId;
            if (!spellId)
                continue;
            SpellEntry const* spellInfo = sSpellTemplate.LookupEntry<SpellEntry>(spellId);
            if (!spellInfo)
                continue;
            if (category == 1)
            {
                for (uint32_t u : UTILITY_SPELLS)
                    if (u == spellId)
                        return proto->ItemId;
            }
            else
            {
                // Category 0 — buff potion. Heuristic: applies a stat
                // aura and is NOT an SPELL_EFFECT_HEAL / ENERGIZE potion
                // (those are covered by factory_pick_potion_for_level).
                bool is_heal_or_mana = false;
                bool has_stat_aura   = false;
                for (int e = 0; e < MAX_EFFECT_INDEX; ++e)
                {
                    uint32_t eff = spellInfo->Effect[e];
                    if (eff == SPELL_EFFECT_HEAL || eff == SPELL_EFFECT_ENERGIZE)
                        is_heal_or_mana = true;
                    if (eff == SPELL_EFFECT_APPLY_AURA)
                    {
                        uint32_t aura = spellInfo->EffectApplyAuraName[e];
                        switch (aura)
                        {
                            case SPELL_AURA_MOD_STAT:
                            case SPELL_AURA_MOD_RESISTANCE:
                            case SPELL_AURA_MOD_DAMAGE_DONE:
                            case SPELL_AURA_MOD_ATTACK_POWER:
                            case SPELL_AURA_MOD_INCREASE_SPEED:
                            case SPELL_AURA_MOD_PERCENT_STAT:
                            case SPELL_AURA_MOD_TOTAL_STAT_PERCENTAGE:
                                has_stat_aura = true;
                                break;
                            default:
                                break;
                        }
                    }
                }
                if (has_stat_aura && !is_heal_or_mana)
                    return proto->ItemId;
            }
        }
        return 0;
    };

    // Backpack slots.
    for (int i = INVENTORY_SLOT_ITEM_START; i < INVENTORY_SLOT_ITEM_END; ++i)
    {
        if (uint32_t id = try_item(b->GetItemByPos(INVENTORY_SLOT_BAG_0, i)))
            return id;
    }
    // Equipped bag slots.
    for (int bag = INVENTORY_SLOT_BAG_START; bag < INVENTORY_SLOT_BAG_END; ++bag)
    {
        Bag* pBag = (Bag*)b->GetItemByPos(INVENTORY_SLOT_BAG_0, bag);
        if (!pBag)
            continue;
        for (uint32_t slot = 0; slot < pBag->GetBagSize(); ++slot)
        {
            if (uint32_t id = try_item(b->GetItemByPos(bag, slot)))
                return id;
        }
    }
    return 0;
}

bool BotBridge::CB_PotionCooldownReady(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    // SpellCategory 4 is the shared potion cooldown in 1.12/2.4/3.3.
    // `HasSpellCategoryCooldown` isn't exposed; instead we check one of
    // the common buff-potion spell ids (Elixir of the Mongoose) as a
    // representative of the category. If that spell is on cooldown,
    // every other category-4 potion is too.
    return !b->HasSpellCooldown(17538 /* Elixir of the Mongoose */);
}

// ── Social / group actions (11i) ──────────────────────────────────────────

bool BotBridge::CB_AcceptGroupInvite(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    // Accept a pending group invite via the internal handler.
    Group* invite = b->GetGroupInvite();
    if (!invite)
        return false;
    b->UninviteFromGroup();
    // The player was already added as an invitee — accepting finalizes membership.
    invite->AddMember(b->GetObjectGuid(), b->GetName());
    return true;
}

bool BotBridge::CB_LeaveGroup(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    Group* group = b->GetGroup();
    if (!group)
        return false;
    group->RemoveMember(b->GetObjectGuid(), 0);
    return true;
}

bool BotBridge::CB_AcceptTrade(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    // Check if the bot has an active trade window.
    if (!b->GetTrader())
        return false;
    // Accept via the session handler, same approach as PB2.
    WorldPacket p;
    b->GetSession()->HandleAcceptTradeOpcode(p);
    return true;
}

bool BotBridge::CB_AcceptDuel(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b || !b->duel)
        return false;
    // Mirror WorldSession::HandleDuelAcceptedOpcode: validate the pending
    // duel, then start the 3s countdown for both duellists.
    Player* initiator = b->duel->initiator;
    Player* opponent  = b->duel->opponent;
    if (!initiator || b == initiator || !opponent || b == opponent)
        return false;
    if (b->duel->startTimer != 0 || opponent->duel->startTimer != 0)
        return false;   // already counting down
    if (b->duel->startTime != 0 || opponent->duel->startTime != 0)
        return false;   // duel already in progress
    time_t now = time(nullptr);
    b->duel->startTimer = now;
    opponent->duel->startTimer = now;
    b->SendDuelCountdown(3000);
    opponent->SendDuelCountdown(3000);
    return true;
}

bool BotBridge::CB_DeclineDuel(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b || !b->duel)
        return false;
    b->DuelComplete(DUEL_INTERRUPTED);
    return true;
}

bool BotBridge::CB_AcceptReadyCheck(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    Group* group = b->GetGroup();
    if (!group)
        return false;

    // Leaders initiate ready checks, they don't respond to them.
    if (group->IsLeader(b->GetObjectGuid()))
        return false;

    WorldPacket data(MSG_RAID_READY_CHECK, 8);
    data << b->GetObjectGuid();
    data << uint8(1); // ready
    group->BroadcastPacket(data, false, -1, b->GetObjectGuid());
    return true;
}

// ── PvP / duel / faction (11d) ────────────────────────────────────────────

bool BotBridge::CB_IsPvpFlagged(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    return b->IsPvP();
}

uint8_t BotBridge::CB_DuelState(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b || !b->duel)
        return 0;
    // `DuelInfo::startTime` is only set once the countdown finishes and
    // the fight actually begins. Before that, the struct exists but
    // startTime is 0 — that's the "challenged / countdown" window.
    return b->duel->startTime ? 2 : 1;
}

uint8_t BotBridge::CB_ReputationRank(BotHandle bot, uint32_t faction_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return 3; // neutral fallback — safe default for missing player
    FactionEntry const* f = sFactionStore.LookupEntry(faction_id);
    if (!f)
        return 255;
    return static_cast<uint8_t>(b->GetReputationMgr().GetRank(f));
}

UnitHandle BotBridge::CB_FindDeadPartyMember(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;

    Group* group = b->GetGroup();
    if (!group)
        return 0;

    for (GroupReference* ref = group->GetFirstMember(); ref; ref = ref->next())
    {
        Player* member = ref->getSource();
        if (!member || member == b || member->IsAlive())
            continue;
        if (!member->IsInWorld())
            continue;
        // Only return members within reasonable range
        if (b->GetDistance(member) > 100.0f)
            continue;
        return member->GetGUID();
    }
    return 0;
}

// ── Battleground ──────────────────────────────────────────────────────────

bool BotBridge::CB_IsInBattleground(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    return b->InBattleGround();
}

uint8_t BotBridge::CB_BattlegroundType(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    BattleGround* bg = b->GetBattleGround();
    if (!bg)
        return 0;
    switch (bg->GetTypeId())
    {
        case BATTLEGROUND_AV:  return 1;
        case BATTLEGROUND_WS:  return 2;
        case BATTLEGROUND_AB:  return 3;
        default: return 0;
    }
}

namespace
{
    // Collect the entry ids that represent "objective" GameObjects for the bot's
    // current battleground. The returned list contains every GO entry the bot
    // may want to move toward or interact with to further its faction's goals.
    //
    // For WSG this is:
    //   - if the bot already carries a flag: its OWN team's base flag GO (capture point),
    //   - otherwise: the enemy base flag GO plus dropped-flag GOs (pickup + return).
    //
    // For AB this is every banner that is not currently controlled by the bot's team
    // (neutral/contested/enemy banners at each of the five bases).
    std::vector<uint32> CollectBgObjectiveEntries(Player* b, BattleGround* bg)
    {
        std::vector<uint32> out;
        const Team myTeam = b->GetTeam();

        switch (bg->GetTypeId())
        {
            case BATTLEGROUND_WS:
            {
                const bool carryingFlag =
                    b->HasAura(BG_WS_SPELL_SILVERWING_FLAG) ||
                    b->HasAura(BG_WS_SPELL_WARSONG_FLAG);

                if (carryingFlag)
                {
                    // Walk to own team's base flag GO to capture.
                    out.push_back(myTeam == ALLIANCE ? GO_WS_SILVERWING_FLAG
                                                    : GO_WS_WARSONG_FLAG);
                }
                else
                {
                    // Enemy base flag (pick up) + both dropped flags (return own / grab enemy).
                    out.push_back(myTeam == ALLIANCE ? GO_WS_WARSONG_FLAG
                                                    : GO_WS_SILVERWING_FLAG);
                    out.push_back(GO_WS_SILVERWING_FLAG_DROP);
                    out.push_back(GO_WS_WARSONG_FLAG_DROP);
                }
                break;
            }
            case BATTLEGROUND_AB:
            {
                // Any banner that is not already our own colour is a valid capture target.
                out.push_back(BG_AB_BANNER_CONTESTED_A);
                out.push_back(BG_AB_BANNER_CONTESTED_H);
                out.push_back(myTeam == ALLIANCE ? BG_AB_BANNER_HORDE
                                                : BG_AB_BANNER_ALLIANCE);
                // Neutral pedestals (initial state before any team claims a base).
                out.push_back(BG_AB_BANNER_STABLE);
                out.push_back(BG_AB_BANNER_BLACKSMITH);
                out.push_back(BG_AB_BANNER_FARM);
                out.push_back(BG_AB_BANNER_LUMBER_MILL);
                out.push_back(BG_AB_BANNER_MINE);
                break;
            }
            default:
                break;
        }
        return out;
    }

    // Scan the bot's map for the closest spawned GameObject matching any of the
    // given entries. Uses a wide search radius (the whole BG grid fits) so the
    // bot can navigate to an objective from anywhere on the map.
    GameObject* FindClosestBgObjective(Player* b, const std::vector<uint32>& entries,
                                       float searchRange)
    {
        if (entries.empty())
            return nullptr;

        GameObjectList gameObjects;
        MaNGOS::GameObjectInPosRangeCheck check(*b,
            b->GetPositionX(), b->GetPositionY(), b->GetPositionZ(), searchRange);
        MaNGOS::GameObjectListSearcher<MaNGOS::GameObjectInPosRangeCheck> searcher(
            gameObjects, check);
        Cell::VisitAllObjects(b, searcher, searchRange);

        GameObject* best = nullptr;
        float bestDistSq = searchRange * searchRange + 1.0f;
        for (GameObject* go : gameObjects)
        {
            if (!go || !go->IsSpawned())
                continue;
            const uint32 entry = go->GetEntry();
            bool match = false;
            for (uint32 e : entries)
            {
                if (e == entry) { match = true; break; }
            }
            if (!match)
                continue;
            float dx = go->GetPositionX() - b->GetPositionX();
            float dy = go->GetPositionY() - b->GetPositionY();
            float dz = go->GetPositionZ() - b->GetPositionZ();
            float distSq = dx * dx + dy * dy + dz * dz;
            if (distSq < bestDistSq)
            {
                bestDistSq = distSq;
                best = go;
            }
        }
        return best;
    }
} // namespace

BotSafePosition BotBridge::CB_GetBgObjective(BotHandle bot)
{
    BotSafePosition result{};
    result.found = false;

    Player* b = FindBot(bot);
    if (!b)
        return result;

    BattleGround* bg = b->GetBattleGround();
    if (!bg)
        return result;

    // Search wide enough to cover the entire battleground map (WSG ≈ 450y diagonal,
    // AB ≈ 700y). Cell::VisitAllObjects will page through grid cells as needed.
    const float kBgSearchRange = 800.0f;

    std::vector<uint32> entries = CollectBgObjectiveEntries(b, bg);
    GameObject* go = FindClosestBgObjective(b, entries, kBgSearchRange);
    if (!go)
        return result;

    result.x = go->GetPositionX();
    result.y = go->GetPositionY();
    result.z = go->GetPositionZ();
    result.found = true;
    return result;
}

bool BotBridge::CB_CaptureBgObjective(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    BattleGround* bg = b->GetBattleGround();
    if (!bg)
        return false;

    // Interact only with objective GameObjects within normal interaction range.
    // Use()/HandlePlayerClickedOnFlag drives the full capture/pickup/return flow
    // — the BG class itself handles team rules, spell application and scoring.
    std::vector<uint32> entries = CollectBgObjectiveEntries(b, bg);
    GameObject* go = FindClosestBgObjective(b, entries, INTERACTION_DISTANCE);
    if (!go)
        return false;

    go->Use(b);
    return true;
}

UnitHandle* BotBridge::CB_GetNearbyEnemies(BotHandle bot, float range, uint32_t* out_count)
{
    *out_count = 0;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    std::vector<UnitHandle> handles;

    UnitList units;
    MaNGOS::AnyUnitInObjectRangeCheck checker(b, range);
    MaNGOS::UnitListSearcher<MaNGOS::AnyUnitInObjectRangeCheck> searcher(units, checker);
    Cell::VisitAllObjects(b, searcher, range);

    for (Unit* u : units)
    {
        if (!u || u == b || !u->IsAlive())
            continue;
        // Enemy players only (for BG context)
        Player* enemy = u->ToPlayer();
        if (!enemy)
            continue;
        if (b->IsHostileTo(enemy))
            handles.push_back(enemy->GetObjectGuid().GetRawValue());
    }

    if (handles.empty())
        return nullptr;

    UnitHandle* arr = new UnitHandle[handles.size()];
    std::copy(handles.begin(), handles.end(), arr);
    *out_count = static_cast<uint32_t>(handles.size());
    return arr;
}

// ── RPG / social ──────────────────────────────────────────────────────────

BotSafePosition BotBridge::CB_GetRandomPointNearby(BotHandle bot, float range)
{
    BotSafePosition result{};
    result.found = false;

    Player* b = FindBot(bot);
    if (!b)
        return result;

    // Pick a random angle and distance
    float angle = frand(0.0f, 2.0f * static_cast<float>(M_PI));
    float dist  = frand(range * 0.3f, range);
    float x = b->GetPositionX() + std::cos(angle) * dist;
    float y = b->GetPositionY() + std::sin(angle) * dist;
    float z = b->GetPositionZ();

    b->UpdateGroundPositionZ(x, y, z);

    // Validate with navmesh pathfinding — LoS alone passes through
    // thin dungeon walls, causing bots to walk through geometry.
    PathFinder pathfinder(b);
    pathfinder.calculate(x, y, z, false);
    if (pathfinder.getPathType() & PATHFIND_NORMAL)
    {
        result.x = x;
        result.y = y;
        result.z = z;
        result.found = true;
    }
    return result;
}

bool BotBridge::CB_Emote(BotHandle bot, uint32_t emote_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    b->HandleEmoteCommand(emote_id);
    return true;
}

UnitHandle* BotBridge::CB_GetNearbyGossipNpcs(BotHandle bot, float range, uint32_t* out_count)
{
    *out_count = 0;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    std::vector<UnitHandle> handles;

    CreatureList creatures;
    MaNGOS::AllCreaturesOfEntryInRangeCheck check(b, 0, range);
    MaNGOS::CreatureListSearcher<MaNGOS::AllCreaturesOfEntryInRangeCheck> searcher(creatures, check);
    Cell::VisitAllObjects(b, searcher, range);

    for (Creature* c : creatures)
    {
        if (!c || !c->IsAlive() || b->IsHostileTo(c))
            continue;
        uint32_t npcFlags = c->GetUInt32Value(UNIT_NPC_FLAGS);
        // Has gossip flag but NOT vendor/repair/quest
        if ((npcFlags & UNIT_NPC_FLAG_GOSSIP) &&
            !(npcFlags & (UNIT_NPC_FLAG_VENDOR | UNIT_NPC_FLAG_REPAIR | UNIT_NPC_FLAG_QUESTGIVER)))
        {
            handles.push_back(c->GetObjectGuid().GetRawValue());
        }
    }

    if (handles.empty())
        return nullptr;

    UnitHandle* arr = new UnitHandle[handles.size()];
    std::copy(handles.begin(), handles.end(), arr);
    *out_count = static_cast<uint32_t>(handles.size());
    return arr;
}

// ── Gathering ─────────────────────────────────────────────────────────────

bool BotBridge::CB_HasGatheringSkill(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // Check for Mining (186), Herbalism (182), Skinning (393)
    return b->HasSkill(SKILL_MINING) || b->HasSkill(SKILL_HERBALISM) || b->HasSkill(SKILL_SKINNING);
}

uint64_t* BotBridge::CB_GetNearbyGatherables(BotHandle bot, float range, uint32_t* out_count)
{
    *out_count = 0;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    std::vector<uint64_t> handles;

    // Search for nearby game objects (ore veins, herb nodes)
    GameObjectList gameObjects;
    MaNGOS::GameObjectInPosRangeCheck check(*b,
        b->GetPositionX(), b->GetPositionY(), b->GetPositionZ(), range);
    MaNGOS::GameObjectListSearcher<MaNGOS::GameObjectInPosRangeCheck> searcher(gameObjects, check);
    Cell::VisitAllObjects(b, searcher, range);

    for (GameObject* go : gameObjects)
    {
        if (!go || !go->IsSpawned())
            continue;

        GameObjectInfo const* goInfo = go->GetGOInfo();
        if (!goInfo)
            continue;

        // Mining nodes (type 1 = GAMEOBJECT_TYPE_QUESTGIVER... type 3 = CHEST, but herb = 2, mine = 3)
        // Actually: GAMEOBJECT_TYPE_CHEST = 3, herbs use lockType LOCKTYPE_HERBALISM
        // Simpler: check if the GO requires a gathering skill to interact
        LockEntry const* lockInfo = sLockStore.LookupEntry(goInfo->GetLockId());
        if (!lockInfo)
            continue;

        bool isGatherable = false;
        for (int i = 0; i < MAX_LOCK_CASE; ++i)
        {
            if (lockInfo->Type[i] == LOCK_KEY_SKILL)
            {
                uint32_t skillId = lockInfo->Index[i];
                if (skillId == LOCKTYPE_HERBALISM && b->HasSkill(SKILL_HERBALISM))
                    isGatherable = true;
                else if (skillId == LOCKTYPE_MINING && b->HasSkill(SKILL_MINING))
                    isGatherable = true;
            }
        }

        if (isGatherable)
            handles.push_back(go->GetObjectGuid().GetRawValue());
    }

    // Also check for skinnable corpses if bot has skinning
    if (b->HasSkill(SKILL_SKINNING))
    {
        CreatureList creatures;
        MaNGOS::AllCreaturesOfEntryInRangeCheck creatureCheck(b, 0, range);
        MaNGOS::CreatureListSearcher<MaNGOS::AllCreaturesOfEntryInRangeCheck> cSearcher(creatures, creatureCheck);
        Cell::VisitAllObjects(b, cSearcher, range);

        for (Creature* c : creatures)
        {
            if (!c || c->IsAlive())
                continue;
            if (c->HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_SKINNABLE))
                handles.push_back(c->GetObjectGuid().GetRawValue());
        }
    }

    if (handles.empty())
        return nullptr;

    uint64_t* arr = new uint64_t[handles.size()];
    std::copy(handles.begin(), handles.end(), arr);
    *out_count = static_cast<uint32_t>(handles.size());
    return arr;
}

void BotBridge::CB_FreeGatherableList(uint64_t* list)
{
    delete[] list;
}

bool BotBridge::CB_GatherNode(BotHandle bot, uint64_t handle)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    GameObject* go = b->GetMap()->GetGameObject(MakeGuid(handle));
    if (go)
    {
        // Use the game object (triggers gathering)
        go->Use(b);
        return true;
    }

    // Might be a skinnable creature
    Unit* unit = b->GetMap()->GetUnit(MakeGuid(handle));
    if (unit)
    {
        Creature* creature = dynamic_cast<Creature*>(unit);
        if (creature && creature->HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_SKINNABLE))
        {
            // Skinning spell (8613 = Skinning)
            if (b->HasSpell(8613))
            {
                SpellEntry const* info = sSpellTemplate.LookupEntry<SpellEntry>(8613);
                if (info)
                {
                    Spell* spell = new Spell(b, info, false);
                    SpellCastTargets targets;
                    targets.setUnitTarget(creature);
                    spell->SpellStart(&targets);
                    return true;
                }
            }
        }
    }
    return false;
}

float BotBridge::CB_GameobjectDistance(BotHandle bot, uint64_t handle)
{
    Player* b = FindBot(bot);
    if (!b)
        return 99999.0f;

    GameObject* go = b->GetMap()->GetGameObject(MakeGuid(handle));
    if (go)
        return b->GetDistance(go);

    // Might be a creature (skinnable)
    Unit* unit = b->GetMap()->GetUnit(MakeGuid(handle));
    if (unit)
        return b->GetDistance(unit);

    return 99999.0f;
}

BotPosition BotBridge::CB_GameobjectPosition(BotHandle bot, uint64_t handle)
{
    BotPosition pos{};
    Player* b = FindBot(bot);
    if (!b)
        return pos;

    GameObject* go = b->GetMap()->GetGameObject(MakeGuid(handle));
    if (go)
    {
        pos.x = go->GetPositionX();
        pos.y = go->GetPositionY();
        pos.z = go->GetPositionZ();
        pos.o = go->GetOrientation();
        pos.map_id = go->GetMapId();
        return pos;
    }

    // Might be a creature (skinnable)
    Unit* unit = b->GetMap()->GetUnit(MakeGuid(handle));
    if (unit)
    {
        pos.x = unit->GetPositionX();
        pos.y = unit->GetPositionY();
        pos.z = unit->GetPositionZ();
        pos.o = unit->GetOrientation();
        pos.map_id = unit->GetMapId();
    }
    return pos;
}

uint64_t BotBridge::CB_NearbyGameObjectByEntry(BotHandle bot, uint32_t entry, float range)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;

    GameObjectList gameObjects;
    MaNGOS::GameObjectInPosRangeCheck check(*b,
        b->GetPositionX(), b->GetPositionY(), b->GetPositionZ(), range);
    MaNGOS::GameObjectListSearcher<MaNGOS::GameObjectInPosRangeCheck> searcher(gameObjects, check);
    Cell::VisitAllObjects(b, searcher, range);

    GameObject* best = nullptr;
    float bestDist = range + 1.0f;
    for (GameObject* go : gameObjects)
    {
        if (!go || !go->IsSpawned())
            continue;
        if (go->GetEntry() != entry)
            continue;
        float d = b->GetDistance(go);
        if (d < bestDist)
        {
            bestDist = d;
            best = go;
        }
    }
    if (!best)
        return 0;
    return best->GetObjectGuid().GetRawValue();
}

bool BotBridge::CB_UseGameObject(BotHandle bot, uint64_t handle)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    GameObject* go = b->GetMap()->GetGameObject(MakeGuid(handle));
    if (!go)
        return false;
    go->Use(b);
    return true;
}

bool BotBridge::CB_UseNearbyQuestObject(BotHandle bot, float range)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    if (range <= 0.0f)
        range = INTERACTION_DISTANCE;

    // Collect the gameobject entries the bot still needs to use for its
    // incomplete quests (GO objectives are stored as negative ReqCreatureOrGOId).
    std::unordered_set<uint32> wanted;
    for (auto const& qs : b->getQuestStatusMap())
    {
        if (qs.second.m_status != QUEST_STATUS_INCOMPLETE)
            continue;
        Quest const* quest = sObjectMgr.GetQuestTemplate(qs.first);
        if (!quest)
            continue;
        for (int j = 0; j < QUEST_OBJECTIVES_COUNT; ++j)
        {
            const int32 reqEntry = quest->ReqCreatureOrGOId[j];
            const uint32 reqCount = quest->ReqCreatureOrGOCount[j];
            if (reqEntry >= 0 || reqCount == 0)
                continue;
            if (b->GetReqKillOrCastCurrentCount(qs.first, reqEntry) >= reqCount)
                continue;
            wanted.insert(static_cast<uint32>(-reqEntry));
        }
    }
    if (wanted.empty())
        return false;

    GameObjectList gameObjects;
    MaNGOS::GameObjectInPosRangeCheck check(*b,
        b->GetPositionX(), b->GetPositionY(), b->GetPositionZ(), range);
    MaNGOS::GameObjectListSearcher<MaNGOS::GameObjectInPosRangeCheck> searcher(gameObjects, check);
    Cell::VisitAllObjects(b, searcher, range);

    for (GameObject* go : gameObjects)
    {
        if (!go || !go->IsSpawned())
            continue;
        if (!wanted.count(go->GetEntry()))
            continue;
        // Use() triggers the quest credit for "use object" objectives.
        go->Use(b);
        return true;
    }
    return false;
}

bool BotBridge::CB_IsQuestObjectiveCreature(BotHandle bot, uint32_t entry)
{
    Player* b = FindBot(bot);
    if (!b || entry == 0)
        return false;

    for (auto const& qs : b->getQuestStatusMap())
    {
        if (qs.second.m_status != QUEST_STATUS_INCOMPLETE)
            continue;
        Quest const* quest = sObjectMgr.GetQuestTemplate(qs.first);
        if (!quest)
            continue;
        for (int j = 0; j < QUEST_OBJECTIVES_COUNT; ++j)
        {
            const int32 reqEntry = quest->ReqCreatureOrGOId[j];
            const uint32 reqCount = quest->ReqCreatureOrGOCount[j];
            if (reqEntry <= 0 || reqCount == 0)
                continue;
            if (static_cast<uint32>(reqEntry) != entry)
                continue;
            // A still-needed kill objective for this creature entry.
            if (b->GetReqKillOrCastCurrentCount(qs.first, reqEntry) < reqCount)
                return true;
        }
    }
    return false;
}

namespace
{
    // Cached idle-chatter texts from ai_playerbot_texts, keyed by category.
    // Loaded once (the table is read-only after start) so the periodic
    // broadcast never hits the DB.
    std::vector<std::string> const& BotChatterTexts(std::string const& category)
    {
        static std::unordered_map<std::string, std::vector<std::string>> cache;
        auto it = cache.find(category);
        if (it != cache.end())
            return it->second;

        std::vector<std::string>& texts = cache[category];
        if (auto r = WorldDatabase.PQuery(
                "SELECT text FROM ai_playerbot_texts WHERE name = '%s'", category.c_str()))
        {
            do
            {
                texts.push_back(r->Fetch()[0].GetCppString());
            } while (r->NextRow());
        }
        return texts;
    }

    void ReplaceAll(std::string& s, std::string const& from, std::string const& to)
    {
        for (size_t p = s.find(from); p != std::string::npos; p = s.find(from, p + to.size()))
            s.replace(p, from.size(), to);
    }

    // Resolve the localized, qualified name of a chat channel for this bot,
    // matching how MgrBridge::JoinChatChannels names them: General /
    // LocalDefense are zone-qualified, Trade / GuildRecruitment are
    // city-qualified (the core's dummy "City" area 3459), global channels
    // (WorldDefense, LookingForGroup) take no qualifier. Lets broadcasts route
    // to the channel they belong in (trade chatter → Trade, not General).
    std::string ChannelNameForId(Player* b, uint32 channelId)
    {
        const int loc = static_cast<int>(b->GetSession()->GetSessionDbcLocale());
        for (uint32 i = 0; i < sChatChannelsStore.GetNumRows(); ++i)
        {
            ChatChannelsEntry const* ch = sChatChannelsStore.LookupEntry(i);
            if (!ch || ch->ChannelID != channelId)
                continue;

            std::string qualifier; // empty for global channels (pattern has no %s)
            if (channelId == 1 /*General*/ || channelId == 22 /*LocalDefense*/)
            {
                AreaTableEntry const* zone = GetAreaEntryByAreaID(b->GetZoneId());
                if (zone && zone->area_name[loc])
                    qualifier = zone->area_name[loc];
            }
            else if (channelId == 2 /*Trade*/ || channelId == 25 /*GuildRecruitment*/)
            {
                AreaTableEntry const* city = GetAreaEntryByAreaID(3459); // dummy "City"
                if (city && city->area_name[loc])
                    qualifier = city->area_name[loc];
            }

            char buf[100];
            snprintf(buf, sizeof(buf), ch->pattern[loc], qualifier.c_str());
            return buf;
        }
        return "";
    }

    // Resolve the localized name of the bot's current General channel
    // ("General - <zone>") so we can send to the channel it joined at login.
    std::string GeneralChannelName(Player* b)
    {
        AreaTableEntry const* zone = GetAreaEntryByAreaID(b->GetZoneId());
        if (!zone)
            return "";
        const int loc = static_cast<int>(b->GetSession()->GetSessionDbcLocale());
        const std::string zoneName = zone->area_name[loc] ? zone->area_name[loc] : "";
        for (uint32 i = 0; i < sChatChannelsStore.GetNumRows(); ++i)
        {
            ChatChannelsEntry const* ch = sChatChannelsStore.LookupEntry(i);
            if (!ch || ch->ChannelID != 1) // 1 = General
                continue;
            char buf[100];
            snprintf(buf, sizeof(buf), ch->pattern[loc], zoneName.c_str());
            return buf;
        }
        return "";
    }
}

bool BotBridge::CB_BotBroadcastRandom(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b || !b->GetSession())
        return false;

    // Low per-call chance so that, with the caller's ~3-minute throttle and
    // hundreds of bots, the channel sees a steady trickle of chatter rather
    // than a flood. Mirrors PB2's broadcast-chance gating.
    if (urand(1, 100) > 8)
        return false;

    static const char* categories[] = {
        "suggest_trade", "suggest_faction", "suggest_something",
        "suggest_sell", "suggest_quest"
    };
    constexpr int catCount = sizeof(categories) / sizeof(categories[0]);

    static const char* tradeGoods[] = {
        "ore", "herbs", "cloth", "leather", "gems", "potions", "enchants"
    };

    const std::string role = b->GetPlayerbotAI() ? "DPS" : "DPS"; // role string

    // Try a few random texts; broadcast the first one we can fully fill in.
    for (int attempt = 0; attempt < 8; ++attempt)
    {
        const char* category = categories[urand(0, catCount - 1)];
        std::vector<std::string> const& texts = BotChatterTexts(category);
        if (texts.empty())
            continue;

        std::string text = texts[urand(0, texts.size() - 1)];
        ReplaceAll(text, "%my_level", std::to_string(b->GetLevel()));
        ReplaceAll(text, "%my_name", b->GetName());
        ReplaceAll(text, "%my_role", role);
        ReplaceAll(text, "%category", tradeGoods[urand(0, 6)]);

        // Any remaining placeholder means we can't render it cleanly — skip.
        if (text.find('%') != std::string::npos)
            continue;

        // Route by category: buying/selling chatter belongs in Trade, the
        // rest in General. Falls back to /say if the bot hasn't joined that
        // channel (e.g. Trade when out of a city).
        const bool tradeChatter = (strcmp(category, "suggest_trade") == 0
                                || strcmp(category, "suggest_sell") == 0);
        const std::string chanName = ChannelNameForId(b, tradeChatter ? 2 /*Trade*/ : 1 /*General*/);
        if (ChannelMgr* mgr = channelMgr(b->GetTeam()))
        {
            if (Channel* ch = mgr->GetChannel(chanName, b))
            {
                ch->Say(b, text.c_str(), LANG_UNIVERSAL);
                return true;
            }
        }
        b->Say(text, LANG_UNIVERSAL);
        return true;
    }
    return false;
}

bool BotBridge::CB_BotGreetNearbyPlayer(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b || !b->IsAlive() || b->IsInCombat())
        return false;

    // Per-bot cooldown so a given real player isn't greeted repeatedly.
    static std::unordered_map<uint64_t, std::unordered_map<uint64_t, time_t>> greetedByBot;
    const time_t now = time(nullptr);
    constexpr time_t COOLDOWN = 600; // 10 minutes per target
    auto& greeted = greetedByBot[bot];
    for (auto it = greeted.begin(); it != greeted.end();)
        it = (now - it->second >= COOLDOWN) ? greeted.erase(it) : std::next(it);

    constexpr float range = 25.0f;
    std::list<Player*> players;
    MaNGOS::AnyPlayerInObjectRangeCheck check(b, range);
    MaNGOS::PlayerListSearcher<MaNGOS::AnyPlayerInObjectRangeCheck> searcher(players, check);
    Cell::VisitWorldObjects(b, searcher, range);

    Player* target = nullptr;
    for (Player* p : players)
    {
        if (!p || p == b || !p->IsAlive())
            continue;
        if (p->GetPlayerbotAI()) // skip other bots — greet only real players
            continue;
        if (b->IsInGroup(p)) // already grouped, no need to greet
            continue;
        if (greeted.count(p->GetObjectGuid().GetRawValue()))
            continue;
        target = p;
        break;
    }
    if (!target)
        return false;

    static const char* greetings[] = {
        "Hello, %s!",       "Hi there, %s!",    "Well met, %s.",
        "Greetings, %s!",   "Hey %s, how goes?", "Good to see you, %s."
    };
    std::string msg = greetings[urand(0, 5)];
    ReplaceAll(msg, "%s", target->GetName());
    b->Say(msg, LANG_UNIVERSAL);
    greeted[target->GetObjectGuid().GetRawValue()] = now;
    return true;
}

uint64_t BotBridge::CB_GetActiveEscortNpc(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;

    // Only relevant when the bot has an incomplete escort-type quest.
    bool hasEscortQuest = false;
    for (auto const& qs : b->getQuestStatusMap())
    {
        if (qs.second.m_status != QUEST_STATUS_INCOMPLETE)
            continue;
        Quest const* q = sObjectMgr.GetQuestTemplate(qs.first);
        if (q && q->GetType() == QUEST_TYPE_ESCORT)
        {
            hasEscortQuest = true;
            break;
        }
    }
    if (!hasEscortQuest)
        return 0;

    // Find a nearby creature actively running an escort. The escort AI's
    // escorted-player is private, but combined with the bot holding the escort
    // quest this is a reliable identification of the bot's escort target.
    constexpr float range = 80.0f;
    UnitList units;
    MaNGOS::AnyUnitInObjectRangeCheck check(b, range);
    MaNGOS::UnitListSearcher<MaNGOS::AnyUnitInObjectRangeCheck> searcher(units, check);
    Cell::VisitAllObjects(b, searcher, range);
    for (Unit* u : units)
    {
        Creature* c = dynamic_cast<Creature*>(u);
        if (!c || !c->IsAlive())
            continue;
        npc_escortAI* eai = dynamic_cast<npc_escortAI*>(c->AI());
        if (eai && eai->HasEscortState(STATE_ESCORT_ESCORTING))
            return c->GetObjectGuid().GetRawValue();
    }
    return 0;
}

// ── Factory: inventory mutation ───────────────────────────────────────────

namespace
{
    // Destroy every item in a range of slots within bag 0 (equipment slots,
    // backpack slots, bank slots), then optionally destroy the bag containers
    // and their contents.
    void DestroySlotRange(Player* bot, uint8 bag, uint8 firstSlot, uint8 lastSlot)
    {
        for (uint8 slot = firstSlot; slot < lastSlot; ++slot)
        {
            if (bot->GetItemByPos(bag, slot))
                bot->DestroyItem(bag, slot, true);
        }
    }

    // Destroy the contents of every bag container in [firstBag, lastBag) sitting
    // in INVENTORY_SLOT_BAG_0, then destroy the bag containers themselves.
    void DestroyBagRange(Player* bot, uint8 firstBag, uint8 lastBag)
    {
        for (uint8 bagSlot = firstBag; bagSlot < lastBag; ++bagSlot)
        {
            Item* bagItem = bot->GetItemByPos(INVENTORY_SLOT_BAG_0, bagSlot);
            if (!bagItem)
                continue;
            if (Bag* bag = bagItem->ToBag())
            {
                for (uint32 j = 0; j < bag->GetBagSize(); ++j)
                {
                    if (bot->GetItemByPos(bagSlot, j))
                        bot->DestroyItem(bagSlot, j, true);
                }
            }
        }
        // Destroy the bag containers themselves (now empty).
        for (uint8 bagSlot = firstBag; bagSlot < lastBag; ++bagSlot)
        {
            if (bot->GetItemByPos(INVENTORY_SLOT_BAG_0, bagSlot))
                bot->DestroyItem(INVENTORY_SLOT_BAG_0, bagSlot, true);
        }
    }
}

void BotBridge::CB_InventoryDestroyEquippedAndBags(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return;

    // Equipped slots
    DestroySlotRange(b, INVENTORY_SLOT_BAG_0, EQUIPMENT_SLOT_START, EQUIPMENT_SLOT_END);
    // Backpack
    DestroySlotRange(b, INVENTORY_SLOT_BAG_0, INVENTORY_SLOT_ITEM_START, INVENTORY_SLOT_ITEM_END);
    // Carried bags + contents
    DestroyBagRange(b, INVENTORY_SLOT_BAG_START, INVENTORY_SLOT_BAG_END);
}

void BotBridge::CB_InventoryDestroyAll(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return;

    // Everything that equipped+bags covers.
    DestroySlotRange(b, INVENTORY_SLOT_BAG_0, EQUIPMENT_SLOT_START, EQUIPMENT_SLOT_END);
    DestroySlotRange(b, INVENTORY_SLOT_BAG_0, INVENTORY_SLOT_ITEM_START, INVENTORY_SLOT_ITEM_END);
    DestroyBagRange(b, INVENTORY_SLOT_BAG_START, INVENTORY_SLOT_BAG_END);

    // Plus bank contents.
    DestroySlotRange(b, INVENTORY_SLOT_BAG_0, BANK_SLOT_ITEM_START, BANK_SLOT_ITEM_END);
    DestroyBagRange(b, BANK_SLOT_BAG_START, BANK_SLOT_BAG_END);
}

uint32_t BotBridge::CB_ItemCountInBags(BotHandle bot, uint32_t item_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    return b->GetItemCount(item_id, false);
}

uint32_t BotBridge::CB_InventoryAddItem(BotHandle bot, uint32_t item_id, uint32_t count)
{
    Player* b = FindBot(bot);
    if (!b || count == 0)
        return 0;
    Item* item = b->StoreNewItemInInventorySlot(item_id, count);
    if (!item)
        return 0;
    return item->GetCount();
}

uint32_t BotBridge::CB_ItemMaxStackSize(BotHandle /*bot*/, uint32_t item_id)
{
    ItemPrototype const* proto = sObjectMgr.GetItemPrototype(item_id);
    if (!proto)
        return 1;
    uint32 s = proto->GetMaxStackSize();
    return s == 0 ? 1 : s;
}

uint32_t BotBridge::CB_FactoryPickPotionForLevel(BotHandle /*bot*/, uint32_t level, uint32_t effect)
{
    return playerbot_itempool_get_random_potion(level, effect);
}

uint32_t BotBridge::CB_FactoryPickFoodForLevel(BotHandle /*bot*/, uint32_t level, uint32_t category)
{
    return playerbot_itempool_get_food(level, category);
}

uint32_t BotBridge::CB_RandomU32(BotHandle /*bot*/, uint32_t min, uint32_t max)
{
    if (max < min)
        return min;
    return urand(min, max);
}

// ── Factory: progression wipe ─────────────────────────────────────────────

void BotBridge::CB_BotClearSkill(BotHandle bot, uint32_t skill_id)
{
    Player* b = FindBot(bot);
    if (!b || skill_id == 0)
        return;
    b->SetSkill(static_cast<uint16>(skill_id), 0, 0, 0);
}

void BotBridge::CB_BotResetSpells(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    b->resetSpells();
}

void BotBridge::CB_BotResetAllQuests(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return;

    ObjectMgr::QuestMap const& questTemplates = sObjectMgr.GetQuestTemplates();
    for (ObjectMgr::QuestMap::const_iterator i = questTemplates.begin(); i != questTemplates.end(); ++i)
    {
        Quest const* quest = i->second.get();
        uint32 entry = quest->GetQuestId();

        for (uint8 slot = 0; slot < MAX_QUEST_LOG_SIZE; ++slot)
        {
            if (b->GetQuestSlotQuestId(slot) == entry)
                b->SetQuestSlot(slot, 0);
        }
        b->getQuestStatusMap().erase(entry);
    }
    CharacterDatabase.PExecute("DELETE FROM character_queststatus WHERE guid = '%u'", b->GetGUIDLow());
}

// ── Factory: misc pre/post init ───────────────────────────────────────────

void BotBridge::CB_BotRemoveAllAuras(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    b->RemoveAllAuras();
}

void BotBridge::CB_BotRemoveAuraById(BotHandle bot, uint32_t spell_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    b->RemoveAurasDueToSpell(spell_id);
}

bool BotBridge::CB_BotHasSkill(BotHandle bot, uint32_t skill_id)
{
    Player* b = FindBot(bot);
    if (!b || skill_id == 0)
        return false;
    return b->HasSkill(static_cast<uint16>(skill_id));
}

void BotBridge::CB_BotLearnSpell(BotHandle bot, uint32_t spell_id)
{
    Player* b = FindBot(bot);
    if (!b || spell_id == 0)
        return;
    b->learnSpell(spell_id, false);
}

void BotBridge::CB_BotRemoveSpell(BotHandle bot, uint32_t spell_id)
{
    Player* b = FindBot(bot);
    if (!b || spell_id == 0)
        return;
    if (b->HasSpell(spell_id))
        b->removeSpell(spell_id);
}

void BotBridge::CB_BotLearnDefaultSpells(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    b->learnDefaultSpells();
}

void BotBridge::CB_BotLearnClassLevelSpells(BotHandle bot, bool include_quest_rewards)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    b->learnClassLevelSpells(include_quest_rewards);
}

BotSpellInfo BotBridge::CB_GetSpellInfo(BotHandle /*bot*/, uint32_t spell_id)
{
    BotSpellInfo out = {};
    if (spell_id == 0)
        return out;

    SpellEntry const* s = sSpellTemplate.LookupEntry<SpellEntry>(spell_id);
    if (!s)
        return out;

    out.id                 = s->Id;
    out.is_valid           = true;
    out.is_passive         = IsPassiveSpell(s);
    out.attributes         = s->Attributes;
    out.attributes_ex      = s->AttributesEx;
    out.spell_level        = s->spellLevel;
    out.base_level         = s->baseLevel;
    out.max_level          = s->maxLevel;
    out.spell_family_name  = s->SpellFamilyName;

    for (int i = 0; i < 3; ++i)
    {
        out.effect[i]                 = s->Effect[i];
        out.effect_item_type[i]       = s->EffectItemType[i];
        out.effect_misc_value[i]      = s->EffectMiscValue[i];
        out.effect_apply_aura_name[i] = s->EffectApplyAuraName[i];
    }

    for (int i = 0; i < 2; ++i)
        out.totem[i] = s->Totem[i];

    for (int i = 0; i < 8; ++i)
    {
        out.reagent[i]       = s->Reagent[i];
        out.reagent_count[i] = s->ReagentCount[i];
    }

    out.equipped_item_class               = s->EquippedItemClass;
    out.equipped_item_subclass_mask       = s->EquippedItemSubClassMask;
    out.equipped_item_inventory_type_mask = s->EquippedItemInventoryTypeMask;

    return out;
}

uint32_t* BotBridge::CB_GetBotSpells(BotHandle bot, uint32_t* out_count)
{
    if (out_count)
        *out_count = 0;
    Player* b = FindBot(bot);
    if (!b || !out_count)
        return nullptr;

    PlayerSpellMap const& spells = b->GetSpellMap();
    uint32_t count = 0;
    for (PlayerSpellMap::const_iterator it = spells.begin(); it != spells.end(); ++it)
    {
        if (it->second.state == PLAYERSPELL_REMOVED || it->second.disabled)
            continue;
        ++count;
    }
    if (count == 0)
        return nullptr;

    uint32_t* arr = static_cast<uint32_t*>(std::malloc(count * sizeof(uint32_t)));
    if (!arr)
        return nullptr;

    uint32_t i = 0;
    for (PlayerSpellMap::const_iterator it = spells.begin(); it != spells.end(); ++it)
    {
        if (it->second.state == PLAYERSPELL_REMOVED || it->second.disabled)
            continue;
        arr[i++] = it->first;
    }
    *out_count = count;
    return arr;
}

void BotBridge::CB_FreeBotSpells(uint32_t* list)
{
    std::free(list);
}

uint32_t BotBridge::CB_ResolveSpellByName(BotHandle bot, const char* name)
{
    if (!name || !name[0])
        return 0;

    // Case-insensitive compare: lowercase the input once.
    std::string needle(name);
    for (auto& c : needle)
        c = std::tolower(static_cast<unsigned char>(c));

    // Collect all spell IDs whose name matches (multiple ranks).
    std::vector<uint32> matches;
    for (uint32 i = 1; i < sSpellTemplate.GetMaxEntry(); ++i)
    {
        SpellEntry const* s = sSpellTemplate.LookupEntry<SpellEntry>(i);
        if (!s || !s->SpellName[0])
            continue;
        std::string sname(s->SpellName[0]);
        for (auto& c : sname)
            c = std::tolower(static_cast<unsigned char>(c));
        if (sname == needle)
            matches.push_back(s->Id);
    }

    if (matches.empty())
        return 0;

    // If we have a bot, prefer the highest-rank spell the bot actually knows.
    Player* b = FindBot(bot);
    if (b)
    {
        uint32 best = 0;
        for (uint32 id : matches)
        {
            if (b->HasSpell(id) && id > best)
                best = id;
        }
        if (best)
            return best;
    }

    // Fallback: return the highest spell ID (generally the highest rank).
    uint32 best = 0;
    for (uint32 id : matches)
    {
        if (id > best)
            best = id;
    }
    return best;
}

uint32_t BotBridge::CB_ResolveItemByName(BotHandle bot, const char* name)
{
    if (!name || !name[0])
        return 0;

    // Check for WoW item link format: |cCOLOR|Hitem:ID:...|h[Name]|h|r
    // Extract the item ID directly instead of doing a name search.
    const char* hitem = strstr(name, "Hitem:");
    if (hitem)
    {
        uint32 itemId = static_cast<uint32>(strtoul(hitem + 6, nullptr, 10));
        if (itemId > 0)
            return itemId;
    }

    // Case-insensitive compare: lowercase the input once.
    std::string needle(name);
    for (auto& c : needle)
        c = std::tolower(static_cast<unsigned char>(c));

    uint32 exactMatch = 0;
    uint32 substringMatch = 0;

    for (uint32 i = 0; i < sItemStorage.GetMaxEntry(); ++i)
    {
        ItemPrototype const* proto = sItemStorage.LookupEntry<ItemPrototype>(i);
        if (!proto || !proto->Name1 || !proto->Name1[0])
            continue;

        std::string itemName(proto->Name1);
        for (auto& c : itemName)
            c = std::tolower(static_cast<unsigned char>(c));

        if (itemName == needle)
        {
            exactMatch = proto->ItemId;
            break;  // Exact match — done.
        }

        if (!substringMatch && itemName.find(needle) != std::string::npos)
            substringMatch = proto->ItemId;
    }

    return exactMatch ? exactMatch : substringMatch;
}

bool BotBridge::CB_EquipItem(BotHandle bot, uint32_t item_id)
{
    Player* b = FindBot(bot);
    if (!b || item_id == 0)
        return false;

    // Find the item in the bot's inventory.
    Item* item = nullptr;
    for (uint8 bag = INVENTORY_SLOT_BAG_0; bag < INVENTORY_SLOT_BAG_END; ++bag)
    {
        for (uint8 slot = 0; slot < MAX_BAG_SIZE; ++slot)
        {
            Item* check = b->GetItemByPos(bag, slot);
            if (check && check->GetEntry() == item_id)
            {
                item = check;
                break;
            }
        }
        if (item)
            break;
    }

    // Also check equipped slots — item might already be equipped.
    if (!item)
    {
        for (uint8 slot = EQUIPMENT_SLOT_START; slot < EQUIPMENT_SLOT_END; ++slot)
        {
            Item* check = b->GetItemByPos(INVENTORY_SLOT_BAG_0, slot);
            if (check && check->GetEntry() == item_id)
                return true;  // Already equipped.
        }
    }

    if (!item)
        return false;

    // Find the best equip slot for this item.
    uint16 dest = 0;
    ItemPrototype const* proto = item->GetProto();
    if (!proto)
        return false;

    uint8 eslot = EQUIPMENT_SLOT_END;
    for (uint8 s = EQUIPMENT_SLOT_START; s < EQUIPMENT_SLOT_END; ++s)
    {
        if (b->CanEquipItem(s, dest, item, false) == EQUIP_ERR_OK)
        {
            eslot = s;
            break;
        }
    }

    if (eslot == EQUIPMENT_SLOT_END)
        return false;

    // Perform the swap.
    b->SwapItem(item->GetPos(), (INVENTORY_SLOT_BAG_0 << 8) | eslot);
    return true;
}

bool BotBridge::CB_GiveLeader(BotHandle bot, uint64_t target_guid)
{
    Player* b = FindBot(bot);
    if (!b || target_guid == 0)
        return false;
    Group* group = b->GetGroup();
    if (!group)
        return false;
    // Only the current leader can transfer leadership.
    if (!group->IsLeader(b->GetObjectGuid()))
        return false;
    // Verify the target is a group member.
    ObjectGuid targetGuid(target_guid);
    if (!group->IsMember(targetGuid))
        return false;
    // Use HandleGroupSetLeaderOpcode just like PB2 does.
    WorldPacket p(SMSG_GROUP_SET_LEADER, 8);
    p << targetGuid;
    b->GetSession()->HandleGroupSetLeaderOpcode(p);
    return true;
}

uint64_t BotBridge::CB_ResolvePlayerByName(BotHandle bot, const char* name)
{
    if (!name || !name[0])
        return 0;
    // Try online player first.
    Player* p = sObjectAccessor.FindPlayerByName(name);
    if (p)
        return p->GetObjectGuid().GetRawValue();
    // Fallback: search character database.
    uint32 guid = sObjectMgr.GetPlayerGuidByName(std::string(name));
    if (guid)
        return ObjectGuid(HIGHGUID_PLAYER, guid).GetRawValue();
    return 0;
}

bool BotBridge::CB_UnequipItem(BotHandle bot, uint32_t item_id)
{
    Player* b = FindBot(bot);
    if (!b || item_id == 0)
        return false;
    for (uint8 slot = EQUIPMENT_SLOT_START; slot < EQUIPMENT_SLOT_END; ++slot)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, slot);
        if (item && item->GetEntry() == item_id)
        {
            ItemPosCountVec dest;
            uint8 msg = b->CanStoreItem(NULL_BAG, NULL_SLOT, dest, item, false);
            if (msg == EQUIP_ERR_OK)
            {
                b->RemoveItem(INVENTORY_SLOT_BAG_0, slot, true);
                b->StoreItem(dest, item, true);
                return true;
            }
            return false;
        }
    }
    return false;
}

bool BotBridge::CB_InviteToGroup(BotHandle bot, uint64_t target_guid)
{
    Player* b = FindBot(bot);
    if (!b || target_guid == 0)
        return false;
    ObjectGuid targetGuid(target_guid);
    Player* target = sObjectAccessor.FindPlayer(targetGuid);
    if (!target)
        return false;
    // If bot has no group, create one and invite the target.
    Group* group = b->GetGroup();
    if (!group)
    {
        group = new Group();
        if (!group->Create(b->GetObjectGuid(), b->GetName()))
        {
            delete group;
            return false;
        }
        sObjectMgr.AddGroup(group);
    }
    // Invite the target.
    if (group->IsMember(targetGuid))
        return false;
    group->AddInvite(target);
    // Build the SMSG_GROUP_INVITE packet for the target.
    WorldPacket data(SMSG_GROUP_INVITE, 10);
    data << b->GetName();
    target->GetSession()->SendPacket(data);
    return true;
}

bool BotBridge::CB_DestroyItem(BotHandle bot, uint32_t item_id)
{
    Player* b = FindBot(bot);
    if (!b || item_id == 0)
        return false;
    // Search bags for the first stack of this item.
    for (int i = INVENTORY_SLOT_BAG_START; i < INVENTORY_SLOT_BAG_END; ++i)
    {
        if (Bag* bag = dynamic_cast<Bag*>(b->GetItemByPos(INVENTORY_SLOT_BAG_0, i)))
        {
            for (uint32 j = 0; j < bag->GetBagSize(); ++j)
            {
                if (Item* item = bag->GetItemByPos(j))
                {
                    if (item->GetEntry() == item_id)
                    {
                        b->DestroyItem(i, j, true);
                        return true;
                    }
                }
            }
        }
    }
    // Also check backpack (slots 23-38).
    for (uint8 slot = INVENTORY_SLOT_ITEM_START; slot < INVENTORY_SLOT_ITEM_END; ++slot)
    {
        if (Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, slot))
        {
            if (item->GetEntry() == item_id)
            {
                b->DestroyItem(INVENTORY_SLOT_BAG_0, slot, true);
                return true;
            }
        }
    }
    return false;
}

bool BotBridge::CB_ShareQuest(BotHandle bot, uint32_t quest_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    Group* group = b->GetGroup();
    if (!group)
        return false;
    // If quest_id == 0, find first shareable quest.
    if (quest_id == 0)
    {
        for (uint16 slot = 0; slot < MAX_QUEST_LOG_SIZE; ++slot)
        {
            uint32 qid = b->GetQuestSlotQuestId(slot);
            if (qid == 0)
                continue;
            // Check if we can share it.
            Quest const* quest = sObjectMgr.GetQuestTemplate(qid);
            if (quest && !quest->HasQuestFlag(QUEST_FLAGS_PARTY_ACCEPT))
            {
                quest_id = qid;
                break;
            }
        }
    }
    if (quest_id == 0)
        return false;
    // Build CMSG_QUEST_PUSH_TO_PARTY.
    WorldPacket p(CMSG_PUSHQUESTTOPARTY, 4);
    p << quest_id;
    b->GetSession()->HandlePushQuestToParty(p);
    return true;
}

bool BotBridge::CB_DoTextEmote(BotHandle bot, uint32_t text_emote_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    // Build CMSG_TEXT_EMOTE packet.
    WorldPacket p(CMSG_TEXT_EMOTE, 12);
    p << text_emote_id;
    p << uint32(0); // emote num (unused)
    p << ObjectGuid(); // target guid (no target)
    b->GetSession()->HandleTextEmoteOpcode(p);
    return true;
}

uint32_t BotBridge::CB_BotEmptyBagSlotCount(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b) return 0;
    uint32_t empty = 0;
    for (uint8 slot = INVENTORY_SLOT_BAG_START; slot < INVENTORY_SLOT_BAG_END; ++slot)
    {
        if (!b->GetItemByPos(INVENTORY_SLOT_BAG_0, slot))
            ++empty;
    }
    return empty;
}

bool BotBridge::CB_BotStoreNewInBestSlots(BotHandle bot, uint32_t item_id, uint32_t count)
{
    Player* b = FindBot(bot);
    if (!b || item_id == 0 || count == 0) return false;
    return b->StoreNewItemInBestSlots(item_id, count);
}

bool BotBridge::CB_BotSetReputation(BotHandle bot, uint32_t faction_id, int32_t value)
{
    Player* b = FindBot(bot);
    if (!b) return false;
    FactionEntry const* f = sFactionStore.LookupEntry(faction_id);
    if (!f || !f->HasReputation()) return false;
    b->GetReputationMgr().SetReputation(f, value);
    return true;
}

// ── Factory: ammo management ──────────────────────────────────────────────

uint32_t BotBridge::CB_BotEquippedRangedSubclass(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b) return UINT32_MAX;
    Item* pItem = b->GetItemByPos(INVENTORY_SLOT_BAG_0, EQUIPMENT_SLOT_RANGED);
    if (!pItem) return UINT32_MAX;
    return pItem->GetProto()->SubClass;
}

uint32_t BotBridge::CB_BotCurrentAmmoId(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b) return 0;
    return b->GetUInt32Value(PLAYER_AMMO_ID);
}

uint32_t BotBridge::CB_FactoryPickAmmoForLevel(BotHandle /*bot*/, uint32_t level, uint32_t ammo_subclass)
{
    return playerbot_itempool_get_ammo(level, ammo_subclass);
}

void BotBridge::CB_BotSetAmmo(BotHandle bot, uint32_t item_id)
{
    Player* b = FindBot(bot);
    if (!b) return;
    b->SetAmmo(item_id);
}

// ── Factory: skills ───────────────────────────────────────────────────────

uint32_t BotBridge::CB_BotGetSkillValue(BotHandle bot, uint32_t skill_id)
{
    Player* b = FindBot(bot);
    if (!b) return 0;
    return b->GetSkillValue(static_cast<uint16>(skill_id));
}

void BotBridge::CB_BotSetSkill(BotHandle bot, uint32_t skill_id, uint32_t value, uint32_t max)
{
    Player* b = FindBot(bot);
    if (!b) return;
    b->SetSkill(static_cast<uint16>(skill_id),
                static_cast<uint16>(value),
                static_cast<uint16>(max));
}

void BotBridge::CB_BotUpdateSkillsForLevel(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b) return;
    b->UpdateSkillsForLevel(true);
}

// ── Factory: refresh (cheat mask + DB save) ───────────────────────────────

uint32_t BotBridge::CB_BotCheatMask(BotHandle bot)
{
    // `PlayerbotFactory::Refresh` only cares about the item cheat bit, which
    // is part of the global BotCheats / RndBotCheats config masks. PB2 kept
    // per-bot cheat overrides in PlayerbotAI, but the C++ stub currently has
    // no per-bot state, so we surface the union of the two config masks —
    // identical behavior to "all bots share the config cheats".
    Player* b = FindBot(bot);
    if (!b) return 0;
    return playerbot_config_bot_cheat_mask() | playerbot_config_rnd_bot_cheat_mask();
}

void BotBridge::CB_BotSaveToDbIfNotBusy(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b) return;
    // Mirrors the tail of `PlayerbotFactory::Refresh`: only save if the
    // CharacterDatabase worker queue is catching up faster than 10ms.
    if (sRandomPlayerbotMgr.GetDatabaseDelay("CharacterDatabase") < 10 * IN_MILLISECONDS)
        b->SaveToDB();
}

// ── Factory: prepare ──────────────────────────────────────────────────────

void BotBridge::CB_BotResurrectFull(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b) return;
    // Mirrors the top of `PlayerbotFactory::Prepare`: if the bot is dead,
    // resurrect it at full HP and spawn bones in one step.
    if (!b->IsAlive())
    {
        b->ResurrectPlayer(1.0f, false);
        b->SpawnCorpseBones();
    }
}

void BotBridge::CB_BotCombatStop(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b) return;
    b->CombatStop(true);
}

void BotBridge::CB_BotSetLevelAndResetXp(BotHandle bot, uint32_t level)
{
    Player* b = FindBot(bot);
    if (!b) return;
    b->SetLevel(level);
    b->SetUInt32Value(PLAYER_XP, 0);
    b->SetUInt32Value(PLAYER_NEXT_LEVEL_XP, sObjectMgr.GetXPForLevel(level));
}

void BotBridge::CB_BotSetPlayerFlag(BotHandle bot, uint32_t flag, bool set)
{
    Player* b = FindBot(bot);
    if (!b) return;
    if (set)
        b->SetFlag(PLAYER_FLAGS, flag);
    else
        b->RemoveFlag(PLAYER_FLAGS, flag);
}

bool BotBridge::CB_FactoryConfigDisableRandomLevels(BotHandle /*bot*/)
{
    return playerbot_config_disable_random_levels();
}

bool BotBridge::CB_FactoryConfigRandomBotShowHelmet(BotHandle /*bot*/)
{
    return playerbot_config_random_bot_show_helmet();
}

bool BotBridge::CB_FactoryConfigRandomBotShowCloak(BotHandle /*bot*/)
{
    return playerbot_config_random_bot_show_cloak();
}

// ── Factory: Randomize orchestration ──────────────────────────────────────
//
// These callbacks back the top-level `PlayerbotFactory::Randomize` orchestrator
// which is now implemented in Rust (`factory/randomize.rs`). Each one is a
// direct lift of one line from PB2's Randomize body so the Rust side can
// consult live engine state (random-bot ownership, master identity, guild
// membership) and mutate bot state (talents, money) without crossing the
// C++↔Rust boundary for fine-grained field access.

bool BotBridge::CB_FactoryIsRandomBot(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    return sRandomPlayerbotMgr.IsRandomBot(b);
}

bool BotBridge::CB_FactoryHasRealPlayerMaster(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    PlayerbotAI* ai = b->GetPlayerbotAI();
    if (!ai)
        return false;
    return ai->HasRealPlayerMaster();
}

bool BotBridge::CB_FactoryIsInRealGuild(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    // PB2 parity: any guilded bot counts as "in a real guild" — see
    // PlayerbotRust::IsInRealGuild (cpp_wrapper/PlayerbotRust.cpp:417).
    return b->GetGuildId() != 0;
}

uint32_t BotBridge::CB_FactoryConfigMinEnchantingBotLevel(BotHandle /*bot*/)
{
    return playerbot_config_min_enchanting_bot_level();
}

void BotBridge::CB_BotResetTalents(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    b->resetTalents(true);
}

void BotBridge::CB_BotLearnQuestRewardedSpells(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    b->learnQuestRewardedSpells();
}

uint32_t BotBridge::CB_BotGetMoney(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    return b->GetMoney();
}

void BotBridge::CB_BotSetMoney(BotHandle bot, uint32_t amount)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    b->SetMoney(amount);
}

// ── Enchant / gem helpers (ported from PlayerbotFactory::EnchantEquipment + InitGems) ──
//
// `PlayerbotFactory::EnchantEquipment` builds its per-call state from the
// `ai_playerbot_enchants` world-DB table, then iterates equipped slots and
// calls `ai->EnchantItemT()`, which resolves the enchant spell to its enchant
// id and applies it (`PlayerbotRust::EnchantItemT`, PlayerbotRust.cpp). We
// load the container, walk the slots, and invoke EnchantItemT to apply the
// per-slot enchant.
//
// InitGems is fully gated off on Classic (`#ifndef MANGOSBOT_ZERO`) in PB2,
// so `CB_FactoryInitAllGems` is a no-op on vanilla and the full TBC/WotLK
// port is compiled only for those expansions.

namespace
{
    struct EnchantTemplate
    {
        uint8  ClassId;
        uint8  SpecId;
        uint32 SpellId;
        uint8  SlotId;
    };

    // PB2 loads the enchant container once per factory instance; we cache it
    // process-wide since the `ai_playerbot_enchants` table is read-only after
    // server start and every Randomize call would otherwise re-query it.
    std::vector<EnchantTemplate>& EnchantContainer()
    {
        static std::vector<EnchantTemplate> g;
        return g;
    }

    static bool g_enchant_loaded = false;

    void DoLoadEnchantContainer()
    {
        auto& c = EnchantContainer();
        c.clear();
        g_enchant_loaded = true;

        auto result = WorldDatabase.PQuery("SELECT class, spec, spellid, slotid FROM ai_playerbot_enchants");
        if (!result)
            return;

        do
        {
            Field* fields = result->Fetch();
            EnchantTemplate t{};
            t.ClassId = fields[0].GetUInt8();
            t.SpecId  = fields[1].GetUInt8();
            t.SpellId = fields[2].GetUInt32();
            t.SlotId  = fields[3].GetUInt8();
            c.push_back(t);
        } while (result->NextRow());
    }

    // Mirrors `PlayerbotFactory::GetPlayerSpecTab` (PlayerbotFactory.cpp:51) —
    // picks the talent tab the bot has invested the most points in.
    int ComputePlayerSpecTab(Player* bot)
    {
        int maxCount = 0, bestTab = 0;
        for (int i = 0; i < 3; ++i)
        {
            int count = 0;
            uint32 classMask = bot->getClassMask();
            for (uint32 i2 = 0; i2 < sTalentStore.GetNumRows(); ++i2)
            {
                TalentEntry const* talentInfo = sTalentStore.LookupEntry(i2);
                if (!talentInfo)
                    continue;
                TalentTabEntry const* talentTabInfo = sTalentTabStore.LookupEntry(talentInfo->TalentTab);
                if (!talentTabInfo || talentTabInfo->tabpage != uint32(i) || !(talentTabInfo->ClassMask & classMask))
                    continue;
                for (int rank = MAX_TALENT_RANK - 1; rank >= 0; --rank)
                {
                    if (talentInfo->RankID[rank] && bot->HasSpell(talentInfo->RankID[rank]))
                    {
                        count += rank + 1;
                        break;
                    }
                }
            }
            if (count > maxCount)
            {
                maxCount = count;
                bestTab  = i;
            }
        }
        return bestTab;
    }

    // Mirrors `PlayerbotFactory::ApplyEnchantTemplate(uint8 spec, Item*)` —
    // iterates the container looking for entries that match the bot's class
    // and the passed spec key, then forwards each to EnchantItemT to apply.
    void ApplyEnchantTemplateForSpec(Player* bot, uint8 spec, Item* item)
    {
        PlayerbotAI* ai = bot->GetPlayerbotAI();
        if (!ai)
            return;
        for (auto const& t : EnchantContainer())
        {
            if (t.ClassId == bot->getClass() && t.SpecId == spec)
                ai->EnchantItemT(t.SpellId, t.SlotId, item);
        }
    }

    // Mirrors `PlayerbotFactory::EnchantItem` (PlayerbotFactory.cpp:830) —
    // computes a spec-key from class*10+tab and applies the template.
    void DoEnchantItem(Player* bot, Item* item)
    {
        if (!item || !bot)
            return;
        if (bot->GetLevel() < playerbot_config_min_enchanting_bot_level())
            return;
        int tab = ComputePlayerSpecTab(bot);
        uint32 tempId = uint32((uint32)bot->getClass() * (uint32)10);
        ApplyEnchantTemplateForSpec(bot, uint8(tempId + uint32(tab)), item);
    }

#ifndef MANGOSBOT_ZERO
    // Stat-bucket classifiers — lifted verbatim from
    // `PlayerbotFactory::CheckItemStats/AddItemStats/AddItemSpellStats`
    // (PlayerbotFactory.cpp:436/467/552). Kept file-local so PlayerbotFactory
    // can be deleted without leaving dangling references.

    bool GemCheckItemStats(Player* bot, uint8 sp, uint8 ap, uint8 tank)
    {
        switch (bot->getClass())
        {
        case CLASS_PRIEST:
        case CLASS_MAGE:
        case CLASS_WARLOCK:
            if (!sp || ap > sp || tank > sp)
                return false;
            break;
        case CLASS_PALADIN:
        case CLASS_WARRIOR:
            if ((!ap && !tank) || sp > ap || sp > tank)
                return false;
            break;
        case CLASS_HUNTER:
        case CLASS_ROGUE:
            if (!ap || sp > ap || sp > tank)
                return false;
            break;
#ifdef MANGOSBOT_TWO
        case CLASS_DEATH_KNIGHT:
            if ((!ap && !tank) || sp > ap || sp > tank)
                return false;
            break;
#endif
        }
        return sp || ap || tank;
    }

    void GemAddItemStats(uint32 mod, uint8& sp, uint8& ap, uint8& tank)
    {
        switch (mod)
        {
        case ITEM_MOD_HEALTH:
        case ITEM_MOD_STAMINA:
        case ITEM_MOD_MANA:
        case ITEM_MOD_INTELLECT:
        case ITEM_MOD_SPIRIT:
        case ITEM_MOD_HIT_SPELL_RATING:
        case ITEM_MOD_HASTE_RATING:
        case ITEM_MOD_HASTE_RANGED_RATING:
        case ITEM_MOD_CRIT_RANGED_RATING:
        case ITEM_MOD_HIT_RANGED_RATING:
#ifdef MANGOSBOT_TWO
        case ITEM_MOD_SPELL_HEALING_DONE:
        case ITEM_MOD_SPELL_DAMAGE_DONE:
        case ITEM_MOD_MANA_REGENERATION:
        case ITEM_MOD_ARMOR_PENETRATION_RATING:
        case ITEM_MOD_SPELL_POWER:
        case ITEM_MOD_HEALTH_REGEN:
        case ITEM_MOD_SPELL_PENETRATION:
#endif
            sp++;
            break;
        }

        switch (mod)
        {
        case ITEM_MOD_AGILITY:
        case ITEM_MOD_STRENGTH:
        case ITEM_MOD_HEALTH:
        case ITEM_MOD_STAMINA:
        case ITEM_MOD_DEFENSE_SKILL_RATING:
        case ITEM_MOD_DODGE_RATING:
        case ITEM_MOD_PARRY_RATING:
        case ITEM_MOD_BLOCK_RATING:
        case ITEM_MOD_HIT_TAKEN_MELEE_RATING:
        case ITEM_MOD_HIT_TAKEN_RANGED_RATING:
        case ITEM_MOD_HIT_TAKEN_SPELL_RATING:
        case ITEM_MOD_CRIT_TAKEN_MELEE_RATING:
        case ITEM_MOD_CRIT_TAKEN_RANGED_RATING:
        case ITEM_MOD_CRIT_TAKEN_SPELL_RATING:
        case ITEM_MOD_HIT_TAKEN_RATING:
        case ITEM_MOD_CRIT_TAKEN_RATING:
        case ITEM_MOD_RESILIENCE_RATING:
#ifdef MANGOSBOT_TWO
        case ITEM_MOD_BLOCK_VALUE:
#endif
            tank++;
            break;
        }

        switch (mod)
        {
        case ITEM_MOD_HEALTH:
        case ITEM_MOD_STAMINA:
        case ITEM_MOD_AGILITY:
        case ITEM_MOD_STRENGTH:
        case ITEM_MOD_HIT_MELEE_RATING:
        case ITEM_MOD_HIT_RANGED_RATING:
        case ITEM_MOD_CRIT_MELEE_RATING:
        case ITEM_MOD_CRIT_RANGED_RATING:
        case ITEM_MOD_HASTE_MELEE_RATING:
        case ITEM_MOD_HASTE_RANGED_RATING:
        case ITEM_MOD_HIT_RATING:
        case ITEM_MOD_CRIT_RATING:
        case ITEM_MOD_HASTE_RATING:
        case ITEM_MOD_EXPERTISE_RATING:
#ifdef MANGOSBOT_TWO
        case ITEM_MOD_ATTACK_POWER:
        case ITEM_MOD_RANGED_ATTACK_POWER:
        case ITEM_MOD_FERAL_ATTACK_POWER:
#endif
            ap++;
            break;
        }
    }

    void GemAddItemSpellStats(uint32 smod, uint8& sp, uint8& ap, uint8& tank)
    {
        switch (smod)
        {
        case SPELL_AURA_MOD_DAMAGE_DONE:
        case SPELL_AURA_MOD_HEALING_DONE:
        case SPELL_AURA_MOD_SPELL_CRIT_CHANCE:
        case SPELL_AURA_MOD_POWER_REGEN:
        case SPELL_AURA_MOD_MANA_REGEN_FROM_STAT:
        case SPELL_AURA_HASTE_SPELLS:
            sp++;
            break;
        }

        switch (smod)
        {
        case SPELL_AURA_MOD_EXPERTISE:
        case SPELL_AURA_MOD_ATTACK_POWER:
        case SPELL_AURA_MOD_CRIT_PERCENT:
        case SPELL_AURA_MOD_HIT_CHANCE:
        case SPELL_AURA_MOD_RANGED_ATTACK_POWER:
        case SPELL_AURA_EXTRA_ATTACKS:
        case SPELL_AURA_MOD_MELEE_HASTE:
        case SPELL_AURA_MOD_RANGED_HASTE:
            ap++;
            break;
        }

        switch (smod)
        {
        case SPELL_AURA_MOD_PARRY_PERCENT:
        case SPELL_AURA_MOD_DODGE_PERCENT:
        case SPELL_AURA_MOD_BLOCK_PERCENT:
        case SPELL_AURA_MOD_DAMAGE_PERCENT_TAKEN:
        case SPELL_AURA_MOD_BASE_RESISTANCE_PCT:
        case SPELL_AURA_MOD_BASE_RESISTANCE:
        case SPELL_AURA_MOD_SKILL:
        case SPELL_AURA_MOD_SHIELD_BLOCKVALUE:
        case SPELL_AURA_MOD_SHIELD_BLOCKVALUE_PCT:
            tank++;
            break;
        }
    }
#endif // !MANGOSBOT_ZERO
} // anonymous namespace

void BotBridge::CB_FactoryLoadEnchantContainer(BotHandle /*bot*/)
{
    DoLoadEnchantContainer();
}

void BotBridge::CB_FactoryEnchantAllEquipment(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    if (b->GetLevel() < playerbot_config_min_enchanting_bot_level())
        return;
    if (!g_enchant_loaded)
        DoLoadEnchantContainer();

    for (uint8 slot = 0; slot < SLOT_EMPTY; slot++)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, slot);
        if (item)
            DoEnchantItem(b, item);
    }
}

void BotBridge::CB_FactoryInitAllGems(BotHandle bot)
{
#ifdef MANGOSBOT_ZERO
    // PB2 gates the entire InitGems body on `#ifndef MANGOSBOT_ZERO` —
    // there are no gem sockets on Vanilla. Nothing to do.
    (void)bot;
#else
    Player* b = FindBot(bot);
    if (!b)
        return;

    uint32_t* gems_ptr = nullptr;
    size_t gems_len = 0;
    if (!playerbot_itempool_get_gems(&gems_ptr, &gems_len) || !gems_ptr || gems_len == 0)
        return;
    std::vector<uint32> gems(gems_ptr, gems_ptr + gems_len);
    playerbot_itempool_free_u32_list(gems_ptr, gems_len);
    for (int slot = EQUIPMENT_SLOT_START; slot < EQUIPMENT_SLOT_END; slot++)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, slot);
        if (!item)
            continue;
        ItemPrototype const* proto = item->GetProto();
        if (!proto)
            continue;

        WorldPacket data(CMSG_SOCKET_GEMS);
        data << item->GetObjectGuid();
        uint32 gem_placed[MAX_GEM_SOCKETS];
        for (int i = 0; i < MAX_GEM_SOCKETS; i++)
            gem_placed[i] = 0;

        for (uint32 enchant_slot = SOCK_ENCHANTMENT_SLOT;
             enchant_slot < SOCK_ENCHANTMENT_SLOT + MAX_GEM_SOCKETS;
             ++enchant_slot)
        {
            ObjectGuid gem_GUID;
            uint32 socketSlot  = enchant_slot - SOCK_ENCHANTMENT_SLOT;
            uint32 SocketColor = proto->Socket[socketSlot].Color;
#ifdef MANGOSBOT_TWO
            if (!SocketColor)
            {
                // prismatic socket promotion — matches PB2 InitGems.
                if (item->GetEnchantmentId(PRISMATIC_ENCHANTMENT_SLOT))
                {
                    if (socketSlot == 0 ||
                        proto->Socket[socketSlot - 1].Color ||
                        (socketSlot + 1 < MAX_GEM_SOCKETS && !proto->Socket[socketSlot + 1].Color))
                    {
                        SocketColor = SOCKET_COLOR_RED | SOCKET_COLOR_YELLOW | SOCKET_COLOR_BLUE;
                    }
                }
            }
#endif
            uint32 gem_id = 0;
            switch (SocketColor)
            {
            case SOCKET_COLOR_META:
                gem_id = 25890;
                break;
            default:
            {
                for (auto itr = gems.begin(); itr != gems.end(); ++itr)
                {
                    ItemPrototype const* gemProto = sObjectMgr.GetItemPrototype(*itr);
                    if (!gemProto)
                        continue;

                    // don't place the same gem twice on the same item
                    bool already_placed = false;
                    for (int i = 0; i < MAX_GEM_SOCKETS; i++)
                        if (gem_placed[i] == gemProto->ItemId)
                            already_placed = true;
                    if (already_placed)
                        continue;

                    GemPropertiesEntry const* gemProperty = sGemPropertiesStore.LookupEntry(gemProto->GemProperties);
                    if (!gemProperty)
                        continue;

                    SpellItemEnchantmentEntry const* pEnchant = sSpellItemEnchantmentStore.LookupEntry(gemProperty->spellitemenchantement);
                    if (!pEnchant)
                        continue;

                    uint32 GemColor = gemProperty->color;

                    if (gemProto->Flags & ITEM_FLAG_UNIQUE_EQUIPPABLE)
                    {
                        if ((b->HasItemOrGemWithIdEquipped(gemProto->ItemId, 1)) ||
                            (b->HasItemCount(gemProto->ItemId, 1)))
                            continue;
                    }

                    if (gemProto->RequiredSkillRank > b->GetSkillValue(SKILL_JEWELCRAFTING))
                        continue;

                    // no epic gems on low gear; no crap gems on epic gear
                    if (((proto->ItemLevel) < 100 && (gemProto->Quality) > 3) ||
                        ((proto->ItemLevel) > 100 && (gemProto->Quality) < 3))
                        continue;

                    uint8 sp = 0, ap = 0, tank = 0;
                    if (GemColor & SocketColor && GemColor == SocketColor)
                    {
                        for (int i = 0; i < 3; ++i)
                        {
                            if (pEnchant->type[i] != ITEM_ENCHANTMENT_TYPE_STAT &&
                                pEnchant->type[i] != ITEM_ENCHANTMENT_TYPE_EQUIP_SPELL)
                                continue;
                            switch (pEnchant->type[i])
                            {
                            case ITEM_ENCHANTMENT_TYPE_STAT:
                                GemAddItemStats(pEnchant->spellid[i], sp, ap, tank);
                                break;
                            case ITEM_ENCHANTMENT_TYPE_EQUIP_SPELL:
                            {
                                SpellEntry const* spellInfo = sSpellTemplate.LookupEntry<SpellEntry>(pEnchant->spellid[i]);
                                if (!spellInfo)
                                    continue;
                                for (int j = 0; j < MAX_EFFECT_INDEX; j++)
                                {
                                    if (spellInfo->Effect[j] != SPELL_EFFECT_APPLY_AURA)
                                        continue;
                                    GemAddItemSpellStats(spellInfo->EffectApplyAuraName[j], sp, ap, tank);
                                }
                            }
                            break;
                            }
                        }
                    }
                    if (!GemCheckItemStats(b, sp, ap, tank))
                        continue;
                    gem_id = gemProto->ItemId;
                    break;
                }
            }
            break;
            }

            if (gem_id > 0)
            {
                gem_placed[socketSlot] = gem_id;
                ItemPosCountVec dest;
                InventoryResult res = b->CanStoreNewItem(NULL_BAG, NULL_SLOT, dest, gem_id, 1);
                if (res == EQUIP_ERR_OK)
                {
                    if (Item* gem = b->StoreNewItem(dest, gem_id, true))
                    {
                        b->SendNewItem(gem, 1, false, true, false);
                        gem_GUID = gem->GetObjectGuid();
                    }
                }
            }
            data << gem_GUID;
        }
        b->GetSession()->HandleSocketOpcode(data);
    }
#endif // !MANGOSBOT_ZERO
}

// ── Factory: quests ───────────────────────────────────────────────────────

bool BotBridge::CB_QuestIsEligibleForBot(BotHandle bot, uint32_t quest_id)
{
    Player* b = FindBot(bot);
    if (!b) return false;

    Quest const* quest = sObjectMgr.GetQuestTemplate(quest_id);
    if (!quest) return false;

    if (!b->SatisfyQuestClass(quest, false))
        return false;
    if (quest->GetMinLevel() > b->GetLevel())
        return false;
    if (!b->SatisfyQuestRace(quest, false))
        return false;

    return true;
}

void BotBridge::CB_BotRewardQuestComplete(BotHandle bot, uint32_t quest_id)
{
    Player* b = FindBot(bot);
    if (!b) return;

    Quest const* quest = sObjectMgr.GetQuestTemplate(quest_id);
    if (!quest) return;

    b->SetQuestStatus(quest_id, QUEST_STATUS_COMPLETE);
    b->RewardQuest(quest, 0, b, false);
}

// ── Factory: arena team ───────────────────────────────────────────────────

uint32_t BotBridge::CB_BotGetAccountId(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b) return 0;

    WorldSession* sess = b->GetSession();
    if (!sess) return 0;

    return sess->GetAccountId();
}

// ── Factory: guild ────────────────────────────────────────────────────────

bool BotBridge::CB_FactoryQueryGuildSummary(BotHandle bot,
                                            uint32_t guild_id,
                                            uint8_t* out_leader_team,
                                            uint32_t* out_member_size,
                                            uint32_t* out_max_members_hint,
                                            char* out_name,
                                            uint32_t name_buf_len)
{
    (void)bot;
    Guild* guild = sGuildMgr.GetGuildById(guild_id);
    if (!guild)
        return false;

    if (out_leader_team)
    {
        Team leaderTeam = sObjectMgr.GetPlayerTeamByGUID(guild->GetLeaderGuid());
        // Normalize to the Rust-side 0=Alliance, 1=Horde convention.
        *out_leader_team = (leaderTeam == ALLIANCE) ? 0u : 1u;
    }
    if (out_member_size)
        *out_member_size = guild->GetMemberSize();
    if (out_max_members_hint)
        *out_max_members_hint = static_cast<uint32_t>(atoi(guild->GetGINFO().c_str()));
    if (out_name && name_buf_len > 0)
    {
        const std::string& name = guild->GetName();
        const uint32_t copy_n = std::min<uint32_t>(name_buf_len - 1,
                                                   static_cast<uint32_t>(name.size()));
        std::memcpy(out_name, name.data(), copy_n);
        out_name[copy_n] = '\0';
    }
    return true;
}

bool BotBridge::CB_FactoryGuildAddMember(BotHandle bot, uint32_t guild_id, uint32_t rank_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    Guild* guild = sGuildMgr.GetGuildById(guild_id);
    if (!guild)
        return false;

    return guild->AddMember(b->GetObjectGuid(), rank_id);
}

bool BotBridge::CB_FactoryGetGuildRankName(BotHandle bot,
                                           uint32_t guild_id,
                                           uint32_t rank_id,
                                           char* out_name,
                                           uint32_t name_buf_len)
{
    (void)bot;
    if (!out_name || name_buf_len == 0)
        return false;

    Guild* guild = sGuildMgr.GetGuildById(guild_id);
    if (!guild)
        return false;

    const std::string rankName = guild->GetRankName(rank_id);
    const uint32_t copy_n = std::min<uint32_t>(name_buf_len - 1,
                                               static_cast<uint32_t>(rankName.size()));
    std::memcpy(out_name, rankName.data(), copy_n);
    out_name[copy_n] = '\0';
    return true;
}

uint32_t BotBridge::CB_FactoryBotGuildId(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    return b->GetGuildId();
}

// ── Factory: per-bot KV store ─────────────────────────────────────────────

uint32_t BotBridge::CB_FactoryKvGetU32(BotHandle bot, const char* key)
{
    Player* b = FindBot(bot);
    if (!b || !key)
        return 0;
    return sRandomPlayerbotMgr.GetValue(b, std::string(key));
}

void BotBridge::CB_FactoryKvSetU32(BotHandle bot, const char* key, uint32_t value)
{
    Player* b = FindBot(bot);
    if (!b || !key)
        return;
    sRandomPlayerbotMgr.SetValue(b, std::string(key), value);
}

// Trainer-iteration tail of PlayerbotFactory::InitTradeSkills. Walks every
// creature template, inspects each tradeskill trainer's spell list, and
// learns any recipe the bot currently qualifies for under the
// "Apprentice + chosen profession or secondary" gate. Delegated here from
// Rust because the loop needs sCreatureStorage, sObjectMgr.GetNpcTrainer*,
// GetTrainerSpellState, SpellEntry::Effect*, and SkillLineEntry — several
// cmangos-only surfaces that are not worth plumbing through the FFI.
void BotBridge::CB_FactoryLearnTradeskillRecipes(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return;

    for (uint32 id = 0; id < sCreatureStorage.GetMaxEntry(); ++id)
    {
        CreatureInfo const* co = sCreatureStorage.LookupEntry<CreatureInfo>(id);
        if (!co)
            continue;

        if (co->TrainerType != TRAINER_TYPE_TRADESKILLS)
            continue;

        uint32 trainerId = co->TrainerTemplateId;
        if (!trainerId)
            trainerId = co->Entry;

        TrainerSpellData const* trainer_spells =
            sObjectMgr.GetNpcTrainerTemplateSpells(trainerId);
        if (!trainer_spells)
            trainer_spells = sObjectMgr.GetNpcTrainerSpells(trainerId);

        if (!trainer_spells)
            continue;

        for (TrainerSpellMap::const_iterator itr = trainer_spells->spellList.begin();
             itr != trainer_spells->spellList.end(); ++itr)
        {
            TrainerSpell const* tSpell = &itr->second;
            if (!tSpell)
                continue;

            uint32 reqLevel = 0;
            reqLevel = tSpell->isProvidedReqLevel
                           ? tSpell->reqLevel
                           : std::max(reqLevel, tSpell->reqLevel);
            TrainerSpellState state = b->GetTrainerSpellState(tSpell, reqLevel);
            if (state != TRAINER_SPELL_GREEN)
                continue;

            SpellEntry const* proto = sSpellTemplate.LookupEntry<SpellEntry>(tSpell->spell);
            if (!proto)
                continue;

            SpellEntry const* spell = sSpellTemplate.LookupEntry<SpellEntry>(tSpell->spell);
            if (spell)
            {
                std::string SpellName = spell->SpellName[0];
                if (spell->Effect[EFFECT_INDEX_1] == SPELL_EFFECT_SKILL_STEP)
                {
                    uint32 skill = spell->EffectMiscValue[EFFECT_INDEX_1];

                    if (skill && !b->HasSkill(skill))
                    {
                        SkillLineEntry const* pSkill = sSkillLineStore.LookupEntry(skill);
                        if (pSkill)
                        {
                            if ((SpellName.find("Apprentice") != std::string::npos &&
                                 pSkill->categoryId == SKILL_CATEGORY_PROFESSION) ||
                                pSkill->categoryId == SKILL_CATEGORY_SECONDARY)
                                continue;
                        }
                    }
                }
            }

            // PB2's original code had an `else` branch that cast the trainer
            // spell on the bot (`ai->CastSpell(tSpell->spell, bot)`) when
            // `learnedSpell` was not set. That path was a no-op in this fork
            // and is intentionally not replicated — bots learn via the
            // `learnedSpell`/SPELL_EFFECT_LEARN_SPELL path below.
#ifdef MANGOSBOT_ZERO
            if (tSpell->learnedSpell)
            {
                bool learned = false;
                for (int j = 0; j < 3; ++j)
                {
                    if (proto->Effect[j] == SPELL_EFFECT_LEARN_SPELL)
                    {
                        uint32 learnedSpell = proto->EffectTriggerSpell[j];
                        b->learnSpell(learnedSpell, false);
                        learned = true;
                    }
                }
                if (!learned)
                    b->learnSpell(tSpell->learnedSpell, false);
            }
#else
            if (!tSpell->learnedSpell.empty())
            {
                for (auto learnSpell : tSpell->learnedSpell)
                {
                    bool learned = false;
                    for (int j = 0; j < 3; ++j)
                    {
                        if (proto->Effect[j] == SPELL_EFFECT_LEARN_SPELL)
                        {
                            uint32 learnedSpell = proto->EffectTriggerSpell[j];
                            b->learnSpell(learnedSpell, false);
                            learned = true;
                        }
                    }
                    if (!learned)
                        b->learnSpell(learnSpell, false);
                }
            }
#endif
        }
    }
}

// ── Factory: equipment ────────────────────────────────────────────────────

uint32_t BotBridge::CB_FactoryBotGuidLow(BotHandle bot)
{
    Player* b = FindBot(bot);
    return b ? b->GetGUIDLow() : 0u;
}

uint32_t BotBridge::CB_FactoryBotEquippedItemInSlot(BotHandle bot, uint8_t slot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, slot);
    if (!item)
        return 0;
    ItemPrototype const* proto = item->GetProto();
    return proto ? proto->ItemId : 0u;
}

void BotBridge::CB_FactoryDestroyAllEquippedItems(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    // Mirrors `DestroyItemsVisitor(bot); InventoryIterateItems(..., ITERATE_ITEMS_IN_EQUIP)`
    // at the top of PlayerbotFactory::InitEquipment's non-incremental branch.
    // PB2's visitor preserves ids in `sPlayerbotAIConfig.IsInRandomQuestItemList`;
    // since bots never loot quest items through the equip pool, and the
    // `InventoryIterateItems` shim on `PlayerbotRust` is a no-op in this fork,
    // the practical semantics collapse to "destroy every equipped item".
    for (uint8_t slot = EQUIPMENT_SLOT_START; slot < EQUIPMENT_SLOT_END; ++slot)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, slot);
        if (!item)
            continue;
        b->DestroyItem(item->GetBagSlot(), item->GetSlot(), true);
    }
}

bool BotBridge::CB_FactoryEquipNewItemInSlot(BotHandle bot,
                                              uint8_t slot,
                                              uint32_t item_id,
                                              uint32_t random_enchant_id,
                                              bool /*apply_enchants*/)
{
    Player* b = FindBot(bot);
    if (!b || !item_id)
        return false;

    // Mirrors the equip-and-enchant tail of PlayerbotFactory::InitEquipment's
    // per-slot loop (PlayerbotFactory.cpp:2301-2322).  The `apply_enchants`
    // flag is accepted for parity with the FFI contract but intentionally
    // unused here: enchanting is applied in a single dedicated pass after
    // equipment is built (FactoryEnchantEquipmentViaRust →
    // CB_FactoryEnchantAllEquipment → EnchantItemT), not per equip call.
    uint16 eDest = 0;
    if (RandomPlayerbotMgr::CanEquipUnseenItem(b, slot, eDest, item_id) != EQUIP_ERR_OK)
        return false;

    Item* oldItem = b->GetItemByPos(INVENTORY_SLOT_BAG_0, slot);
    if (oldItem)
        b->DestroyItem(oldItem->GetBagSlot(), oldItem->GetSlot(), true);

    Item* pItem = b->EquipNewItem(eDest, item_id, true);
    if (!pItem)
        return false;

    if (random_enchant_id)
    {
        // overwrite random generated property + refresh the visible slot so
        // inspect/scoreboard pick up the new stats
        pItem->SetItemRandomProperties(random_enchant_id);
        b->SetVisibleItemSlot(pItem->GetSlot(), pItem);
    }
    pItem->SetOwnerGuid(b->GetObjectGuid());
    return true;
}

void BotBridge::CB_FactoryInitStatsForLevelAndUpdate(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    b->InitStatsForLevel(true);
    b->UpdateAllStats();
}

// Average equipped item level — matches the snapshot computation at
// line ~668 in CB_GetSnapshot, which mirrors
// `PlayerbotAI::GetEquipGearScore(player, false, false)` from PB2.
static uint32_t ComputeEquipGearScore(Player* p)
{
    if (!p)
        return 0;
    uint32_t sum = 0;
    uint32_t n = 0;
    for (int i = EQUIPMENT_SLOT_START; i < EQUIPMENT_SLOT_END; ++i)
    {
        Item* item = p->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
        if (!item)
            continue;
        ItemPrototype const* proto = item->GetProto();
        if (!proto)
            continue;
        sum += proto->ItemLevel;
        ++n;
    }
    return (n > 0) ? (sum / n) : 0u;
}

bool BotBridge::CB_FactoryMasterEquipGearScore(BotHandle bot, uint32_t* out_gs)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    PlayerbotAI* ai = b->GetPlayerbotAI();
    if (!ai)
        return false;
    Player* master = ai->GetMaster();
    if (!master)
        return false;
    if (out_gs)
        *out_gs = ComputeEquipGearScore(master);
    return true;
}

void BotBridge::CB_FactoryTellMaster(BotHandle bot, const char* msg)
{
    if (!msg)
        return;
    Player* b = FindBot(bot);
    if (!b)
        return;
    PlayerbotAI* ai = b->GetPlayerbotAI();
    if (!ai)
        return;
    Player* master = ai->GetMaster();
    if (!master)
        return;
    // `TellPlayerNoFacing` on PB2 whispers to the master; on this fork
    // `PlayerbotRust::TellPlayerNoFacing` is a no-op shim (PlayerbotRust.h:240),
    // so route directly through `Player::Whisper` which is the underlying
    // chat primitive PB2's helper ultimately calls.
    b->Whisper(msg, LANG_UNIVERSAL, master->GetObjectGuid());
}

// ── Factory: pet ──────────────────────────────────────────────────────────

bool BotBridge::CB_FactoryBotHasPet(BotHandle bot)
{
    Player* b = FindBot(bot);
    return b && b->GetPet() != nullptr;
}

uint32_t BotBridge::CB_FactoryPetEntry(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    Pet* pet = b->GetPet();
    return pet ? pet->GetEntry() : 0u;
}

uint32_t BotBridge::CB_FactoryPetFamily(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    Pet* pet = b->GetPet();
    if (!pet)
        return 0;
    CreatureInfo const* ci = sObjectMgr.GetCreatureTemplate(pet->GetEntry());
    return ci ? ci->Family : 0u;
}

uint32_t BotBridge::CB_FactoryPetLevel(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    Pet* pet = b->GetPet();
    return pet ? pet->GetLevel() : 0u;
}

bool BotBridge::CB_FactoryPetHasSpell(BotHandle bot, uint32_t spell_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    Pet* pet = b->GetPet();
    return pet && pet->HasSpell(spell_id);
}

uint32_t* BotBridge::CB_FactoryPetAutocastCandidateSpells(BotHandle bot, uint32_t* out_count)
{
    if (out_count)
        *out_count = 0;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;
    Pet* pet = b->GetPet();
    if (!pet)
        return nullptr;

    // Mirror PlayerbotFactory::InitPet:425-435 — skip removed / passive
    // entries so the Rust side only sees toggleable spells.
    std::vector<uint32> ids;
    ids.reserve(pet->m_spells.size());
    for (PetSpellMap::const_iterator itr = pet->m_spells.begin();
         itr != pet->m_spells.end(); ++itr)
    {
        if (itr->second.state == PETSPELL_REMOVED)
            continue;
        if (IsPassiveSpell(itr->first))
            continue;
        ids.push_back(itr->first);
    }

    if (ids.empty())
        return nullptr;

    uint32_t count = static_cast<uint32_t>(ids.size());
    uint32_t* arr  = static_cast<uint32_t*>(std::malloc(sizeof(uint32_t) * count));
    if (!arr)
        return nullptr;
    for (uint32_t i = 0; i < count; ++i)
        arr[i] = ids[i];
    *out_count = count;
    return arr;
}

uint32_t* BotBridge::CB_FactoryTameableCreaturesForBotLevel(BotHandle bot, uint32_t* out_count)
{
    if (out_count)
        *out_count = 0;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    std::vector<uint32> ids;
    for (uint32 id = 0; id < sCreatureStorage.GetMaxEntry(); ++id)
    {
        CreatureInfo const* co = sCreatureStorage.LookupEntry<CreatureInfo>(id);
        if (!co)
            continue;

#ifdef MANGOSBOT_TWO
        if (!co->isTameable(b->CanTameExoticPets()))
#else
        if (!co->isTameable())
#endif
            continue;

        if ((int)co->MinLevel > (int)b->GetLevel())
            continue;

        ids.push_back(id);
    }

    if (ids.empty())
        return nullptr;

    uint32_t count = static_cast<uint32_t>(ids.size());
    uint32_t* arr  = static_cast<uint32_t*>(std::malloc(sizeof(uint32_t) * count));
    if (!arr)
        return nullptr;
    for (uint32_t i = 0; i < count; ++i)
        arr[i] = ids[i];
    *out_count = count;
    return arr;
}

bool BotBridge::CB_FactoryCreateHunterPet(BotHandle bot, uint32_t creature_entry)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    Map* map = b->GetMap();
    if (!map)
        return false;

    CreatureInfo const* co = sCreatureStorage.LookupEntry<CreatureInfo>(creature_entry);
    if (!co)
        return false;

    uint32 guid = map->GenerateLocalLowGuid(HIGHGUID_PET);
#ifdef MANGOSBOT_TWO
    CreatureCreatePos pos(map, b->GetPositionX(), b->GetPositionY(), b->GetPositionZ(),
                          b->GetOrientation(), b->GetPhaseMask());
#else
    CreatureCreatePos pos(map, b->GetPositionX(), b->GetPositionY(), b->GetPositionZ(),
                          b->GetOrientation());
#endif
    uint32 pet_number = sObjectMgr.GeneratePetNumber();
    Pet* pet = new Pet(HUNTER_PET);
    if (!pet->Create(guid, pos, co, pet_number))
    {
        delete pet;
        return false;
    }

    pet->SetOwnerGuid(b->GetObjectGuid());
    pet->SetGuidValue(UNIT_FIELD_CREATEDBY, b->GetObjectGuid());
    pet->setFaction(b->GetFaction());
    pet->SetLevel(b->GetLevel());
    pet->InitStatsForLevel(b->GetLevel());
#ifndef MANGOSBOT_TWO
    pet->SetLoyaltyLevel(BEST_FRIEND);
#endif
    pet->SetPower(POWER_HAPPINESS, HAPPINESS_LEVEL_SIZE * 2);
    pet->GetCharmInfo()->SetPetNumber(pet->GetObjectGuid().GetEntry(), true);
    pet->GetMap()->Add((Creature*)pet);
    pet->AIM_Initialize();
    pet->AI()->SetReactState(REACT_DEFENSIVE);
    pet->InitPetCreateSpells();
    pet->LearnPetPassives();
    pet->CastPetAuras(true);
    pet->CastOwnerTalentAuras();
    pet->UpdateAllStats();
    b->SetPet(pet);
    b->SetPetGuid(pet->GetObjectGuid());
#ifdef MANGOSBOT_TWO
    pet->SetUInt32Value(UNIT_CREATED_BY_SPELL, 13481);
#endif

    pet->SavePetToDB(PET_SAVE_AS_CURRENT, b);
    b->PetSpellInitialize();
    return true;
}

void BotBridge::CB_FactoryPetRefreshStats(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    Pet* pet = b->GetPet();
    if (!pet)
        return;

    pet->InitStatsForLevel(b->GetLevel());
    pet->SetLevel(b->GetLevel());
#ifndef MANGOSBOT_TWO
    pet->SetLoyaltyLevel(BEST_FRIEND);
#endif
    pet->SetPower(POWER_HAPPINESS, HAPPINESS_LEVEL_SIZE * 2);
    pet->SetHealth(pet->GetMaxHealth());
    pet->SetFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_PLAYER_CONTROLLED);
    pet->AI()->SetReactState(REACT_DEFENSIVE);
}

void BotBridge::CB_FactoryPetLearnSpell(BotHandle bot, uint32_t spell_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    Pet* pet = b->GetPet();
    if (!pet)
        return;
    pet->learnSpell(spell_id);
}

void BotBridge::CB_FactoryPetToggleAutocast(BotHandle bot, uint32_t spell_id, bool enable)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    Pet* pet = b->GetPet();
    if (!pet)
        return;
    if (!pet->HasSpell(spell_id))
        return;
    if (IsPassiveSpell(spell_id))
        return;
    pet->ToggleAutocast(spell_id, enable);
}

void BotBridge::CB_FactoryPetForceDismiss(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    Pet* pet = b->GetPet();
    if (!pet)
        return;
    if (pet->IsAlive())
        pet->SetDeathState(JUST_DIED);
}

// ── Factory: item prototype queries ───────────────────────────────────────

uint32_t BotBridge::CB_ItemPrototypeQuality(BotHandle /*bot*/, uint32_t item_id)
{
    ItemPrototype const* proto = sObjectMgr.GetItemPrototype(item_id);
    if (!proto) return 0;
    return proto->Quality;
}

// ── Factory: random item picks ────────────────────────────────────────────

uint32_t BotBridge::CB_FactoryPickTradeForLevel(BotHandle /*bot*/, uint32_t level)
{
    return playerbot_itempool_get_random_trade(level);
}

// ── Factory: taxi nodes ───────────────────────────────────────────────────

BotTaxiNode* BotBridge::CB_GetOverworldTaxiNodes(BotHandle /*bot*/, uint8_t team, uint32_t* out_count)
{
    if (out_count)
        *out_count = 0;
    if (!out_count)
        return nullptr;

    // Mount index: C++ source has MountCreatureID[0]=horde, [1]=alliance.
    // BotWorldSnapshot.team is normalized to 0=Alliance, 1=Horde.
    const uint32 mountIdx = (team == 0) ? 1u : 0u;

    // First pass — count.
    uint32_t count = 0;
    for (uint32 i = 1; i < sTaxiNodesStore.GetNumRows(); ++i)
    {
        TaxiNodesEntry const* node = sTaxiNodesStore.LookupEntry(i);
        if (!node)
            continue;
        uint32 mapId = node->map_id;
        if (mapId != 0 && mapId != 1 && mapId != 530 && mapId != 571)
            continue;
        if (!node->MountCreatureID[mountIdx])
            continue;
        ++count;
    }
    if (count == 0)
        return nullptr;

    BotTaxiNode* arr = static_cast<BotTaxiNode*>(std::malloc(count * sizeof(BotTaxiNode)));
    if (!arr)
        return nullptr;

    uint32_t j = 0;
    for (uint32 i = 1; i < sTaxiNodesStore.GetNumRows(); ++i)
    {
        TaxiNodesEntry const* node = sTaxiNodesStore.LookupEntry(i);
        if (!node)
            continue;
        uint32 mapId = node->map_id;
        if (mapId != 0 && mapId != 1 && mapId != 530 && mapId != 571)
            continue;
        if (!node->MountCreatureID[mountIdx])
            continue;
        arr[j].index = i;
        arr[j].map_id = mapId;
        ++j;
    }
    *out_count = count;
    return arr;
}

void BotBridge::CB_FreeTaxiNodes(BotTaxiNode* list)
{
    std::free(list);
}

void BotBridge::CB_BotSetTaxiNode(BotHandle bot, uint32_t node_index)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    b->m_taxi.SetTaximaskNode(node_index);
}

namespace
{
    // BFS over the taxi node graph (edges = direct flight paths from
    // `sTaxiPathSetBySource`) to find the node sequence src..dest inclusive.
    // Empty when unreachable. ActivateTaxiPathTo's AddRoutes needs the full
    // per-hop node list — a single {src,dest} pair only works for directly
    // connected nodes, so multi-hop journeys must be expanded here.
    std::vector<uint32> ComputeTaxiRoute(uint32 src, uint32 dest)
    {
        if (src == 0 || dest == 0 || src == dest)
            return {};

        std::unordered_map<uint32, uint32> parent;
        std::queue<uint32> q;
        q.push(src);
        parent[src] = src;
        bool reached = false;
        while (!q.empty() && !reached)
        {
            const uint32 cur = q.front();
            q.pop();
            auto it = sTaxiPathSetBySource.find(cur);
            if (it == sTaxiPathSetBySource.end())
                continue;
            for (auto const& kv : it->second) // kv.first = neighbour node
            {
                const uint32 nxt = kv.first;
                if (parent.count(nxt))
                    continue;
                parent[nxt] = cur;
                if (nxt == dest)
                {
                    reached = true;
                    break;
                }
                q.push(nxt);
            }
        }

        if (!parent.count(dest))
            return {};

        std::vector<uint32> route;
        for (uint32 n = dest;; n = parent[n])
        {
            route.push_back(n);
            if (n == src)
                break;
        }
        std::reverse(route.begin(), route.end());
        return route;
    }
}

bool BotBridge::CB_NearestTaxiNodePos(BotHandle bot, BotPosition* out)
{
    Player* b = FindBot(bot);
    if (!b || !out)
        return false;
    const uint32 node = sObjectMgr.GetNearestTaxiNode(
        b->GetPositionX(), b->GetPositionY(), b->GetPositionZ(), b->GetMapId(), b->GetTeam());
    if (!node)
        return false;
    TaxiNodesEntry const* entry = sTaxiNodesStore.LookupEntry(node);
    if (!entry)
        return false;
    out->x = entry->x;
    out->y = entry->y;
    out->z = entry->z;
    out->o = 0.0f;
    out->map_id = entry->map_id;
    return true;
}

bool BotBridge::CB_TakeTaxiToward(BotHandle bot, uint32_t dest_map, float x, float y, float z)
{
    Player* b = FindBot(bot);
    if (!b || b->IsInCombat() || b->IsTaxiFlying() || b->IsMounted())
        return false;

    const Team team = b->GetTeam();
    const uint32 srcNode = sObjectMgr.GetNearestTaxiNode(
        b->GetPositionX(), b->GetPositionY(), b->GetPositionZ(), b->GetMapId(), team);
    const uint32 destNode = sObjectMgr.GetNearestTaxiNode(x, y, z, dest_map, team);
    if (!srcNode || !destNode || srcNode == destNode)
        return false;

    // The bot must be standing at the source flight master to take off.
    TaxiNodesEntry const* srcEntry = sTaxiNodesStore.LookupEntry(srcNode);
    if (!srcEntry || srcEntry->map_id != b->GetMapId())
        return false;
    const float ddx = srcEntry->x - b->GetPositionX();
    const float ddy = srcEntry->y - b->GetPositionY();
    const float ddz = srcEntry->z - b->GetPositionZ();
    const float reach = 2.0f * INTERACTION_DISTANCE;
    if (ddx * ddx + ddy * ddy + ddz * ddz > reach * reach)
        return false;

    // Find the actual flight master NPC near the bot (required by ActivateTaxi).
    Creature* flightMaster = nullptr;
    {
        UnitList units;
        MaNGOS::AnyUnitInObjectRangeCheck check(b, reach);
        MaNGOS::UnitListSearcher<MaNGOS::AnyUnitInObjectRangeCheck> searcher(units, check);
        Cell::VisitAllObjects(b, searcher, reach);
        for (Unit* u : units)
        {
            Creature* c = dynamic_cast<Creature*>(u);
            if (c && c->IsAlive() && (c->GetCreatureInfo()->NpcFlags & UNIT_NPC_FLAG_FLIGHTMASTER))
            {
                flightMaster = c;
                break;
            }
        }
    }
    if (!flightMaster)
        return false;

    std::vector<uint32> route = ComputeTaxiRoute(srcNode, destNode);
    if (route.size() < 2)
        return false; // unreachable by flight (e.g. needs a boat between continents)

    // Discover every node on the route so ActivateTaxiPathTo accepts them.
    for (uint32 n : route)
        b->m_taxi.SetTaximaskNode(n);

    return b->ActivateTaxiPathTo(route, flightMaster);
}

// Cross-continent travel via boats/zeppelins. The core carries player
// passengers across map boundaries automatically (Transport::TeleportTransport
// re-teleports them to the new map), so the bot just has to board the right
// transport at its dock and disembark on the destination continent.
//   0 = no transport route to dest_map
//   1 = disembarked (now on the destination continent)
//   2 = riding (on a transport, not there yet)
//   3 = just boarded
//   4 = walk to the dock at *out_dock and wait for the transport
uint8_t BotBridge::CB_CrossContinentTravel(BotHandle bot, uint32_t dest_map, BotPosition* out_dock)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;

    // Already aboard a transport.
    if (GenericTransport* t = b->GetTransport())
    {
        if (b->GetMapId() == dest_map)
        {
            t->RemovePassenger(b); // arrived on the destination continent
            return 1;
        }
        return 2; // keep riding
    }

    if (b->IsInCombat())
        return 0;

    Map* map = b->GetMap();
    if (!map)
        return 0;

    Transport* chosen = nullptr;
    BotPosition dock{};
    float bestDockDistSq = 0.0f;
    for (Transport* tr : map->GetTransports())
    {
        // Does this transport's route reach the destination continent?
        bool reachesDest = false;
        for (auto const& kf : tr->GetKeyFrames())
            if (kf.Node && kf.Node->mapid == dest_map)
            {
                reachesDest = true;
                break;
            }
        if (!reachesDest)
            continue;

        // Nearest boarding dock = a stop keyframe (delay > 0) on the bot's map.
        for (auto const& kf : tr->GetKeyFrames())
        {
            if (!kf.Node || kf.Node->mapid != b->GetMapId() || kf.Node->delay == 0)
                continue;
            const float dx = kf.Node->x - b->GetPositionX();
            const float dy = kf.Node->y - b->GetPositionY();
            const float dsq = dx * dx + dy * dy;
            if (!chosen || dsq < bestDockDistSq)
            {
                chosen = tr;
                bestDockDistSq = dsq;
                dock.x = kf.Node->x;
                dock.y = kf.Node->y;
                dock.z = kf.Node->z;
                dock.o = 0.0f;
                dock.map_id = kf.Node->mapid;
            }
        }
    }

    if (!chosen)
        return 0; // no boardable transport reaches the destination continent

    // Board once the transport itself is within range of the bot (i.e. the
    // bot is at the dock and the boat has pulled in); otherwise walk to the
    // dock and wait for the next arrival.
    const float bdx = chosen->GetPositionX() - b->GetPositionX();
    const float bdy = chosen->GetPositionY() - b->GetPositionY();
    const float bdz = chosen->GetPositionZ() - b->GetPositionZ();
    constexpr float BOARD_RANGE = 60.0f;
    if (bdx * bdx + bdy * bdy + bdz * bdz <= BOARD_RANGE * BOARD_RANGE)
    {
        chosen->AddPassenger(b);
        return 3;
    }

    if (out_dock)
        *out_dock = dock;
    return 4;
}

// ── Factory: talents ──────────────────────────────────────────────────────

BotTalentEntry* BotBridge::CB_GetClassTalents(BotHandle bot, uint8_t spec_no, uint32_t* out_count)
{
    if (out_count)
        *out_count = 0;
    if (!out_count)
        return nullptr;

    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    const uint32 classMask = b->getClassMask();

    // First pass — count matching rows.
    uint32_t count = 0;
    for (uint32 i = 0; i < sTalentStore.GetNumRows(); ++i)
    {
        TalentEntry const* talentInfo = sTalentStore.LookupEntry(i);
        if (!talentInfo)
            continue;
        TalentTabEntry const* tab = sTalentTabStore.LookupEntry(talentInfo->TalentTab);
        if (!tab || tab->tabpage != spec_no)
            continue;
        if ((classMask & tab->ClassMask) == 0)
            continue;
        ++count;
    }
    if (count == 0)
        return nullptr;

    BotTalentEntry* arr = static_cast<BotTalentEntry*>(std::malloc(count * sizeof(BotTalentEntry)));
    if (!arr)
        return nullptr;

    uint32_t j = 0;
    for (uint32 i = 0; i < sTalentStore.GetNumRows(); ++i)
    {
        TalentEntry const* talentInfo = sTalentStore.LookupEntry(i);
        if (!talentInfo)
            continue;
        TalentTabEntry const* tab = sTalentTabStore.LookupEntry(talentInfo->TalentTab);
        if (!tab || tab->tabpage != spec_no)
            continue;
        if ((classMask & tab->ClassMask) == 0)
            continue;

        arr[j].row = talentInfo->Row;
        for (int r = 0; r < 5; ++r)
            arr[j].rank_ids[r] = (r < MAX_TALENT_RANK) ? talentInfo->RankID[r] : 0;
        ++j;
    }
    *out_count = count;
    return arr;
}

void BotBridge::CB_FreeClassTalents(BotTalentEntry* list)
{
    std::free(list);
}

uint32_t BotBridge::CB_BotFreeTalentPoints(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    return b->GetFreeTalentPoints();
}

void BotBridge::CB_BotUpdateFreeTalentPoints(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    b->UpdateFreeTalentPoints(false);
}

uint32_t BotBridge::CB_BotPickSpecNo(BotHandle bot, bool incremental)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;

    uint32 specNo = sRandomPlayerbotMgr.GetValue(b->GetGUIDLow(), "specNo");
    if (incremental && specNo)
    {
        return specNo - 1;
    }

    uint32 point = urand(0, 100);
    uint8 cls = b->getClass();
    uint32 p1 = playerbot_config_spec_probability(cls, 0);
    uint32 p2 = p1 + playerbot_config_spec_probability(cls, 1);

    uint32 picked = (point < p1 ? 0u : (point < p2 ? 1u : 2u));
    sRandomPlayerbotMgr.SetValue(b, "specNo", picked + 1);
    return picked;
}

uint32_t BotBridge::CB_BotGetSpecTab(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;

    // Mirror AiFactory::GetPlayerSpecTab — count invested talent ranks per tab.
    int maxTalentCount = 0, bestTab = 0;
    uint32 classMask = b->getClassMask();
    for (int tab = 0; tab < 3; ++tab)
    {
        int tabCount = 0;
        for (uint32 j = 0; j < sTalentStore.GetNumRows(); ++j)
        {
            TalentEntry const* talentInfo = sTalentStore.LookupEntry(j);
            if (!talentInfo)
                continue;
            TalentTabEntry const* talentTabInfo = sTalentTabStore.LookupEntry(talentInfo->TalentTab);
            if (!talentTabInfo || talentTabInfo->tabpage != uint32(tab) || !(talentTabInfo->ClassMask & classMask))
                continue;
            for (int rank = MAX_TALENT_RANK - 1; rank >= 0; --rank)
            {
                if (talentInfo->RankID[rank] && b->HasSpell(talentInfo->RankID[rank]))
                {
                    tabCount += rank + 1;
                    break;
                }
            }
        }
        if (tabCount > maxTalentCount)
        {
            maxTalentCount = tabCount;
            bestTab = tab;
        }
    }
    return static_cast<uint32_t>(bestTab);
}

// ── Factory: config list queries ──────────────────────────────────────────

uint32_t* BotBridge::CB_GetRandomBotSpellIds(BotHandle /*bot*/, uint32_t* out_count)
{
    if (out_count)
        *out_count = 0;
    if (!out_count)
        return nullptr;

    uint32_t count = static_cast<uint32_t>(playerbot_config_random_bot_spell_ids_len());
    if (count == 0)
        return nullptr;

    uint32_t* arr = static_cast<uint32_t*>(std::malloc(count * sizeof(uint32_t)));
    if (!arr)
        return nullptr;

    for (uint32_t i = 0; i < count; ++i)
        arr[i] = playerbot_config_random_bot_spell_ids_at(i);
    *out_count = count;
    return arr;
}

// ── Chat-command helpers (Wave 2) ─────────────────────────────────────────

bool BotBridge::CB_BotJump(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    // KnockBackFrom(self, horizontal, vertical) — jumping in place.
    b->KnockBackFrom(b, 0.0f, 8.0f);
    return true;
}

bool BotBridge::CB_BotUseHearthstone(BotHandle bot)
{
    // Reuse the CB_UseItem path — hearthstone has a single OnUse spell and no
    // unit target. 6948 = "Hearthstone" item id.
    return CB_UseItem(bot, 6948, 0);
}

BotReputationEntry* BotBridge::CB_BotGetReputationList(BotHandle bot, uint32_t* out_count)
{
    if (out_count)
        *out_count = 0;
    if (!out_count)
        return nullptr;

    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    FactionStateList const& states = b->GetReputationMgr().GetStateList();
    if (states.empty())
        return nullptr;

    uint32_t count = static_cast<uint32_t>(states.size());
    BotReputationEntry* arr = static_cast<BotReputationEntry*>(
        std::malloc(count * sizeof(BotReputationEntry)));
    if (!arr)
        return nullptr;

    uint32_t j = 0;
    for (FactionStateList::const_iterator it = states.begin(); it != states.end(); ++it)
    {
        FactionState const& st = it->second;
        arr[j].faction_id = st.ID;
        arr[j].value      = st.Standing;
        arr[j].standing   = static_cast<uint8_t>(ReputationMgr::ReputationToRank(st.Standing));
        ++j;
    }
    *out_count = count;
    return arr;
}

void BotBridge::CB_BotFreeReputationList(BotReputationEntry* list)
{
    std::free(list);
}

BotSkillEntry* BotBridge::CB_BotGetLearnedSkills(BotHandle bot, uint32_t* out_count)
{
    if (out_count)
        *out_count = 0;
    if (!out_count)
        return nullptr;

    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    // Player::mSkillStatus is private — iterate the SkillLine DBC and pick
    // the ones this bot actually has.
    uint32_t count = 0;
    for (uint32 id = 1; id < sSkillLineStore.GetNumRows(); ++id)
    {
        if (!sSkillLineStore.LookupEntry(id))
            continue;
        if (!b->HasSkill(static_cast<uint16>(id)))
            continue;
        ++count;
    }
    if (count == 0)
        return nullptr;

    BotSkillEntry* arr = static_cast<BotSkillEntry*>(
        std::malloc(count * sizeof(BotSkillEntry)));
    if (!arr)
        return nullptr;

    uint32_t j = 0;
    for (uint32 id = 1; id < sSkillLineStore.GetNumRows(); ++id)
    {
        if (!sSkillLineStore.LookupEntry(id))
            continue;
        if (!b->HasSkill(static_cast<uint16>(id)))
            continue;
        arr[j].skill_id = id;
        arr[j].value    = b->GetSkillValue(static_cast<uint16>(id));
        arr[j].max      = b->GetSkillMax(static_cast<uint16>(id));
        ++j;
    }
    *out_count = count;
    return arr;
}

void BotBridge::CB_BotFreeSkillList(BotSkillEntry* list)
{
    std::free(list);
}

bool BotBridge::CB_BotQuestAcceptFrom(BotHandle bot, UnitHandle npc)
{
    // Same as CB_AcceptAllQuests — the NPC hands the bot every quest it can
    // take. The distinction between "accept all" and "accept from" lives in
    // the command dispatcher; the FFI side is identical.
    return CB_AcceptAllQuests(bot, npc);
}

bool BotBridge::CB_BotQuestAbandon(BotHandle bot, uint32_t quest_id)
{
    Player* b = FindBot(bot);
    if (!b || quest_id == 0)
        return false;

    // Remove all quest log slots matching this entry (mirrors Level3.cpp
    // HandleQuestRemoveCommand in the Karatefylla fork).
    bool removed = false;
    for (uint8 slot = 0; slot < MAX_QUEST_LOG_SIZE; ++slot)
    {
        if (b->GetQuestSlotQuestId(slot) == quest_id)
        {
            b->SetQuestSlot(slot, 0);
            b->TakeQuestSourceItem(quest_id, false);
            removed = true;
        }
    }

    b->SetQuestStatus(quest_id, QUEST_STATUS_NONE);
    b->getQuestStatusMap()[quest_id].m_rewarded = false;
    return removed;
}

// ── Chat-command helpers (Wave 3: mail + guild) ───────────────────────────

BotMailSummary BotBridge::CB_BotMailSummary(BotHandle bot)
{
    BotMailSummary s{};
    Player* b = FindBot(bot);
    if (!b)
        return s;

    for (auto it = b->GetMailBegin(); it != b->GetMailEnd(); ++it)
    {
        Mail* mail = *it;
        if (!mail)
            continue;
        ++s.total_mails;
        if (mail->money > 0)
        {
            ++s.mails_with_money;
            s.total_money += mail->money;
        }
        if (mail->HasItems())
            ++s.mails_with_items;
    }
    return s;
}

// Locate a nearby mailbox GameObject the bot can interact with. Mirrors the
// PB2 `FindMailbox` helper: scan game objects within 10 yards, return the
// first one of type MAILBOX.
static ObjectGuid FindNearbyMailbox(Player* b)
{
    constexpr float kMailboxRange = 10.0f;
    GameObjectList gos;
    MaNGOS::GameObjectInPosRangeCheck check(
        *b, b->GetPositionX(), b->GetPositionY(), b->GetPositionZ(), kMailboxRange);
    MaNGOS::GameObjectListSearcher<MaNGOS::GameObjectInPosRangeCheck> searcher(gos, check);
    Cell::VisitAllObjects(b, searcher, kMailboxRange);

    for (GameObject* go : gos)
    {
        if (go && go->IsSpawned() && go->GetGoType() == GAMEOBJECT_TYPE_MAILBOX)
            return go->GetObjectGuid();
    }
    return ObjectGuid();
}

bool BotBridge::CB_BotMailTakeAll(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    ObjectGuid mailbox = FindNearbyMailbox(b);
    if (mailbox.IsEmpty())
        return false;

    // Snapshot the message ids up front — taking money/items mutates the
    // underlying list and invalidates iterators.
    std::vector<uint32> mailIds;
    for (auto it = b->GetMailBegin(); it != b->GetMailEnd(); ++it)
    {
        if (*it)
            mailIds.push_back((*it)->messageID);
    }

    bool anyProcessed = false;
    for (uint32 id : mailIds)
    {
        Mail* mail = b->GetMail(id);
        if (!mail)
            continue;

        if (mail->money > 0)
        {
            WorldPacket moneyPacket;
            moneyPacket << mailbox;
            moneyPacket << id;
            b->GetSession()->HandleMailTakeMoney(moneyPacket);
            anyProcessed = true;
        }

        if (mail->HasItems())
        {
            // Snapshot item guids up front for the same reason.
            std::vector<uint32> itemGuids;
            for (auto const& info : mail->items)
                itemGuids.push_back(info.item_guid);

            for (uint32 itemGuid : itemGuids)
            {
                WorldPacket itemPacket;
                itemPacket << mailbox;
                itemPacket << id;
#ifndef MANGOSBOT_ZERO
                itemPacket << itemGuid;
#endif
                b->GetSession()->HandleMailTakeItem(itemPacket);
            }
            anyProcessed = true;
        }

        // Delete the (now-empty) mail.
        WorldPacket delPacket;
        delPacket << mailbox;
        delPacket << id;
#ifndef MANGOSBOT_ZERO
        delPacket << uint32(0); // mailTemplateId
#endif
        b->GetSession()->HandleMailDelete(delPacket);
    }

    return anyProcessed;
}

bool BotBridge::CB_BotGuildLeave(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    uint32 guildId = b->GetGuildId();
    if (guildId == 0)
        return false;

    Guild* g = sGuildMgr.GetGuildById(guildId);
    if (!g)
        return false;

    // DelMember refuses to remove the guild master — leader must transfer
    // or disband first. Mirror PB2 behavior and return false in that case.
    if (g->GetLeaderGuid() == b->GetObjectGuid())
        return false;

    return g->DelMember(b->GetObjectGuid());
}

bool BotBridge::CB_BotIsBehind(BotHandle bot, UnitHandle target)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    Unit* t = FindUnit(bot, target);
    if (!t || t == b)
        return false;
    // HasInArc(other, M_PI) tests the 180° front hemisphere. "bot is behind
    // target" is equivalent to the bot not being in the target's front arc.
    return !t->HasInArc(b, M_PI_F);
}

bool BotBridge::CB_BotIsAtFlank(BotHandle bot, UnitHandle target)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    Unit* t = FindUnit(bot, target);
    if (!t || t == b)
        return false;
    // The flank is the side band: outside the front 90° cone (cleave / breath)
    // AND outside the rear 90° cone (tail sweep). HasInArc(b, M_PI_F / 2) tests
    // the ±45° front cone; !HasInArc(b, 3 * M_PI_F / 2) tests the ±45° rear
    // cone. A bot in neither is at the side — safe to melee a tail-sweeping
    // dragon there.
    const bool in_front = t->HasInArc(b, M_PI_F / 2.0f);
    const bool in_rear  = !t->HasInArc(b, 3.0f * M_PI_F / 2.0f);
    return !in_front && !in_rear;
}

uint32_t BotBridge::CB_BotEquippedWeaponSubclass(BotHandle bot, uint8_t slot)
{
    Player* b = FindBot(bot);
    if (!b)
        return UINT32_MAX;
    uint8 eq_slot;
    switch (slot)
    {
        case 0: eq_slot = EQUIPMENT_SLOT_MAINHAND; break;
        case 1: eq_slot = EQUIPMENT_SLOT_OFFHAND;  break;
        case 2: eq_slot = EQUIPMENT_SLOT_RANGED;   break;
        default: return UINT32_MAX;
    }
    Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, eq_slot);
    if (!item)
        return UINT32_MAX;
    ItemPrototype const* proto = item->GetProto();
    if (!proto || proto->Class != ITEM_CLASS_WEAPON)
        return UINT32_MAX;
    return proto->SubClass;
}

uint32_t BotBridge::CB_BotItemCount(BotHandle bot, uint32_t item_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    return b->GetItemCount(item_id, false, nullptr);
}

uint8_t BotBridge::CB_BotActiveTotemMask(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return 0;
    uint8_t mask = 0;
    for (int i = 0; i < MAX_TOTEM_SLOT; ++i)
    {
        if (b->GetTotemGuid(TotemSlot(i)))
            mask |= (1u << i);
    }
    return mask;
}

bool BotBridge::CB_BotWeaponEnchanted(BotHandle bot, uint8_t slot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    uint8 eq_slot;
    switch (slot)
    {
        case 0: eq_slot = EQUIPMENT_SLOT_MAINHAND; break;
        case 1: eq_slot = EQUIPMENT_SLOT_OFFHAND;  break;
        default: return false;
    }
    Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, eq_slot);
    if (!item)
        return false;
    return item->GetEnchantmentId(TEMP_ENCHANTMENT_SLOT) != 0;
}

uint8_t BotBridge::CB_BotRunesReadyMask(BotHandle bot)
{
    // Death-knight runes are WotLK-only. Classic/TBC have no rune system
    // at all, so the safe answer is always "no runes ready".
#ifdef MANGOSBOT_TWO
    Player* b = FindBot(bot);
    if (!b || b->getClass() != CLASS_DEATH_KNIGHT)
        return 0;
    uint8_t mask = 0;
    for (uint8 i = 0; i < MAX_RUNES; ++i)
    {
        if (b->GetRuneCooldown(i) == 0)
            mask |= (1u << i);
    }
    return mask;
#else
    (void)bot;
    return 0;
#endif
}

bool BotBridge::CB_BotKnowsSpell(BotHandle bot, uint32_t spell_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    return b->HasSpell(spell_id);
}

uint32_t BotBridge::CB_GetSpellName(uint32_t spell_id, char* buf, uint32_t buf_len)
{
    if (!buf || buf_len == 0)
        return 0;
    buf[0] = '\0';

    SpellEntry const* spellInfo = sSpellTemplate.LookupEntry<SpellEntry>(spell_id);
    if (!spellInfo)
        return 0;

    const char* name = spellInfo->SpellName[0]; // English locale
    if (!name || !name[0])
        return 0;

    uint32_t len = 0;
    while (len < buf_len - 1 && name[len])
    {
        buf[len] = name[len];
        ++len;
    }
    buf[len] = '\0';
    return len;
}

// ── RTSC / file I/O helpers ───────────────────────────────────────────────

void BotBridge::CB_BotSummonMarkerCreature(BotHandle bot, uint32_t entry,
                                           float x, float y, float z, float o,
                                           uint32_t despawn_ms, float scale)
{
    Player* b = FindBot(bot);
    if (!b)
        return;
    Creature* c = b->SummonCreature(entry, x, y, z, o, TEMPSPAWN_TIMED_DESPAWN,
                                    despawn_ms);
    if (c && scale > 0.0f)
        c->SetObjectScale(scale);
}

static bool SanitizeBotFileName(const char* name, std::string& out)
{
    if (!name || !*name)
        return false;
    for (const char* p = name; *p; ++p)
    {
        char ch = *p;
        bool ok = (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') ||
                  (ch >= '0' && ch <= '9') || ch == '_' || ch == '-' || ch == '.';
        if (!ok)
            return false;
    }
    out = name;
    // Refuse traversal
    if (out.find("..") != std::string::npos)
        return false;
    return true;
}

static std::string BotDataFilePath(const std::string& safeName)
{
    std::string dir = sConfig.GetStringDefault("LogsDir", "");
    if (!dir.empty() && dir.back() != '/' && dir.back() != '\\')
        dir += '/';
    return dir + "playerbot_" + safeName + ".txt";
}

bool BotBridge::CB_BotWriteLogFile(BotHandle bot, const char* name, const char* body)
{
    (void)bot;
    std::string safe;
    if (!SanitizeBotFileName(name, safe))
        return false;
    std::ofstream f(BotDataFilePath(safe), std::ios::out | std::ios::trunc);
    if (!f.is_open())
        return false;
    if (body)
        f << body;
    return f.good();
}

bool BotBridge::CB_BotAppendLogFile(BotHandle bot, const char* name, const char* line)
{
    (void)bot;
    std::string safe;
    if (!SanitizeBotFileName(name, safe))
        return false;
    std::ofstream f(BotDataFilePath(safe), std::ios::out | std::ios::app);
    if (!f.is_open())
        return false;
    if (line)
        f << line;
    return f.good();
}

bool BotBridge::CB_BotReadLogFile(BotHandle bot, const char* name, char** out_body)
{
    (void)bot;
    if (!out_body)
        return false;
    *out_body = nullptr;
    std::string safe;
    if (!SanitizeBotFileName(name, safe))
        return false;
    std::ifstream f(BotDataFilePath(safe));
    if (!f.is_open())
        return false;
    std::ostringstream ss;
    ss << f.rdbuf();
    std::string s = ss.str();
    char* buf = static_cast<char*>(std::malloc(s.size() + 1));
    if (!buf)
        return false;
    std::memcpy(buf, s.data(), s.size());
    buf[s.size()] = '\0';
    *out_body = buf;
    return true;
}

void BotBridge::CB_BotFreeString(char* s)
{
    if (s)
        std::free(s);
}

// ── Travel destination queries ──────────────────────────────────────────

namespace
{
    // Creature entries that drop `itemId` (its loot-source), for routing
    // item-collect quest objectives. Built lazily per item from
    // `creature_loot_template`/`creature_template` and cached process-wide —
    // the loot store's in-memory map isn't publicly iterable, so a one-off
    // query per distinct quest item is the cheapest reverse lookup.
    std::vector<uint32> const& ItemDropSources(uint32 itemId)
    {
        static std::unordered_map<uint32, std::vector<uint32>> cache;
        auto it = cache.find(itemId);
        if (it != cache.end())
            return it->second;

        std::vector<uint32>& sources = cache[itemId];
        if (auto result = WorldDatabase.PQuery(
                "SELECT ct.entry FROM creature_template ct "
                "JOIN creature_loot_template clt ON ct.LootId = clt.entry "
                "WHERE clt.item = %u LIMIT 20",
                itemId))
        {
            do
            {
                sources.push_back(result->Fetch()[0].GetUInt32());
            } while (result->NextRow());
        }
        return sources;
    }
}

BotTravelDest* BotBridge::CB_BotFindTravelDests(
    BotHandle bot,
    uint32_t purpose_flags,
    float max_range,
    uint32_t max_results,
    uint32_t* out_count)
{
    *out_count = 0;

    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    if (max_range <= 0.0f)
        max_range = 1000.0f;
    if (max_results == 0)
        max_results = 10;

    // Collect candidate NPCs/GOs within range using CMaNGOS grid search.
    // We search for creatures matching NPC flags corresponding to the
    // requested purpose flags.
    struct DestCandidate {
        int32_t  entry;
        uint32_t quest_id;
        uint32_t purpose;
        uint32_t map_id;
        float    x, y, z;
        float    dist_sq;
    };
    std::vector<DestCandidate> candidates;

    // Map purpose flags to NPC flag searches.
    auto check_npc_flag = [&](uint32_t purpose_bit, uint32_t npc_flag) {
        if (!(purpose_flags & purpose_bit))
            return;

        // Grid search for creatures within range.
        UnitList nearby;
        MaNGOS::AnyUnitInObjectRangeCheck check(b, max_range);
        MaNGOS::UnitListSearcher<MaNGOS::AnyUnitInObjectRangeCheck> searcher(nearby, check);
        Cell::VisitAllObjects(b, searcher, max_range);

        for (auto* unit : nearby)
        {
            auto* creature = dynamic_cast<Creature*>(unit);
            if (!creature || !creature->IsAlive())
                continue;

            if (!(creature->GetCreatureInfo()->NpcFlags & npc_flag))
                continue;

            float cx = creature->GetPositionX();
            float cy = creature->GetPositionY();
            float cz = creature->GetPositionZ();
            float dx = b->GetPositionX() - cx;
            float dy = b->GetPositionY() - cy;
            float dsq = dx * dx + dy * dy;

            candidates.push_back({
                static_cast<int32_t>(creature->GetEntry()),
                0,
                purpose_bit,
                creature->GetMapId(),
                cx, cy, cz,
                dsq
            });
        }
    };

    // PB2 TravelPurpose bit flags -> CMaNGOS NPC flags.
    // VENDOR = 1<<9, REPAIR = 1<<8, TRAINER = 1<<7
    check_npc_flag(1 << 9, UNIT_NPC_FLAG_VENDOR);    // Vendor
    check_npc_flag(1 << 8, UNIT_NPC_FLAG_REPAIR);    // Repair
    check_npc_flag(1 << 7, UNIT_NPC_FLAG_TRAINER);   // Trainer
    check_npc_flag(1 << 0, UNIT_NPC_FLAG_QUESTGIVER); // Quest giver
    check_npc_flag(1 << 10, UNIT_NPC_FLAG_AUCTIONEER); // AH
    // Flight master
    if (purpose_flags & (1 << 7)) // Trainer also covers flight master for now
    {
        check_npc_flag(1 << 7, UNIT_NPC_FLAG_FLIGHTMASTER);
    }

    // Quest objectives (TravelPurpose QUEST_OBJECTIVE1..4 = bits 1..4). The
    // NPC-flag grid search above can't find these — a quest mob/object carries
    // no questgiver flag. For each of the bot's *incomplete* quests, resolve
    // the required creature-kill (entry>0) AND gameobject-use (entry<0)
    // objectives that still need progress to a spawn location via
    // FindCreatureData / FindGOData (map-aware, not range-limited), so the bot
    // travels to the objective area instead of grinding wherever it stands.
    // (Pure item-collect objectives whose item only drops from mobs need a
    // loot-source reverse index — a separate precomputed-relations subsystem.)
    constexpr uint32_t QUEST_ALL_OBJ = (1u << 1) | (1u << 2) | (1u << 3) | (1u << 4);
    if (purpose_flags & QUEST_ALL_OBJ)
    {
        for (auto const& qs : b->getQuestStatusMap())
        {
            if (qs.second.m_status != QUEST_STATUS_INCOMPLETE)
                continue;
            const uint32 questId = qs.first;
            Quest const* quest = sObjectMgr.GetQuestTemplate(questId);
            if (!quest)
                continue;

            for (int j = 0; j < QUEST_OBJECTIVES_COUNT; ++j)
            {
                const int32 reqEntry = quest->ReqCreatureOrGOId[j];
                const uint32 reqCount = quest->ReqCreatureOrGOCount[j];
                if (reqEntry == 0 || reqCount == 0)
                    continue;
                if (b->GetReqKillOrCastCurrentCount(questId, reqEntry) >= reqCount)
                    continue;

                uint32 destMap = 0;
                float destX = 0.0f, destY = 0.0f, destZ = 0.0f;
                bool found = false;
                if (reqEntry > 0)
                {
                    // Creature kill objective.
                    FindCreatureData worker(static_cast<uint32>(reqEntry), b);
                    sObjectMgr.DoCreatureData(worker);
                    if (CreatureDataPair const* dp = worker.GetResult())
                    {
                        destMap = dp->second.mapid;
                        destX = dp->second.posX;
                        destY = dp->second.posY;
                        destZ = dp->second.posZ;
                        found = true;
                    }
                }
                else
                {
                    // Gameobject-use objective (negative entry).
                    FindGOData worker(static_cast<uint32>(-reqEntry), b);
                    sObjectMgr.DoGOData(worker);
                    if (GameObjectDataPair const* dp = worker.GetResult())
                    {
                        destMap = dp->second.mapid;
                        destX = dp->second.posX;
                        destY = dp->second.posY;
                        destZ = dp->second.posZ;
                        found = true;
                    }
                }
                if (!found)
                    continue;

                const float dx = b->GetPositionX() - destX;
                const float dy = b->GetPositionY() - destY;
                candidates.push_back({
                    reqEntry,
                    questId,
                    static_cast<uint32_t>(1u << (1 + j)), // QUEST_OBJECTIVE(j+1)
                    destMap,
                    destX, destY, destZ,
                    dx * dx + dy * dy
                });
            }

            // Item-collect objectives: route to a creature that drops the
            // required item (its loot source), so the bot farms + loots it.
            // The existing kill/loot behaviour finishes the objective once the
            // bot is positioned at the spawn.
            for (int j = 0; j < QUEST_ITEM_OBJECTIVES_COUNT; ++j)
            {
                const uint32 itemId = quest->ReqItemId[j];
                const uint32 itemCount = quest->ReqItemCount[j];
                if (!itemId || !itemCount)
                    continue;
                if (b->GetItemCount(itemId, true) >= itemCount)
                    continue; // already have enough

                for (uint32 creatureEntry : ItemDropSources(itemId))
                {
                    FindCreatureData worker(creatureEntry, b);
                    sObjectMgr.DoCreatureData(worker);
                    CreatureDataPair const* dp = worker.GetResult();
                    if (!dp)
                        continue;
                    CreatureData const* data = &dp->second;
                    const float dx = b->GetPositionX() - data->posX;
                    const float dy = b->GetPositionY() - data->posY;
                    candidates.push_back({
                        static_cast<int32_t>(creatureEntry),
                        questId,
                        static_cast<uint32_t>(1u << (1 + j)), // QUEST_OBJECTIVE(j+1)
                        data->mapid,
                        data->posX, data->posY, data->posZ,
                        dx * dx + dy * dy
                    });
                    break; // one routable loot source is enough for this item
                }
            }
        }
    }

    // Sort by distance.
    std::sort(candidates.begin(), candidates.end(),
        [](const DestCandidate& a, const DestCandidate& b) {
            return a.dist_sq < b.dist_sq;
        });

    // Deduplicate by entry — keep only the nearest instance of each entry.
    std::unordered_set<int32_t> seen_entries;
    std::vector<DestCandidate> unique;
    for (auto& c : candidates)
    {
        if (seen_entries.count(c.entry))
            continue;
        seen_entries.insert(c.entry);
        unique.push_back(c);
        if (unique.size() >= max_results)
            break;
    }

    if (unique.empty())
        return nullptr;

    auto* result = static_cast<BotTravelDest*>(
        std::malloc(sizeof(BotTravelDest) * unique.size()));
    if (!result)
        return nullptr;

    for (size_t i = 0; i < unique.size(); ++i)
    {
        result[i].entry = unique[i].entry;
        result[i].quest_id = unique[i].quest_id;
        result[i].purpose = unique[i].purpose;
        result[i].map_id = unique[i].map_id;
        result[i].x = unique[i].x;
        result[i].y = unique[i].y;
        result[i].z = unique[i].z;
    }

    *out_count = static_cast<uint32_t>(unique.size());
    return result;
}

void BotBridge::CB_BotFreeTravelDests(BotTravelDest* list)
{
    if (list)
        std::free(list);
}

// ── World buffs ──────────────────────────────────────────────────────────────

bool BotBridge::CB_AddAura(BotHandle bot, uint32_t spell_id)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    SpellEntry const* spellInfo = sSpellTemplate.LookupEntry<SpellEntry>(spell_id);
    if (!spellInfo)
        return false;

    if (!IsSpellAppliesAura(spellInfo, (1 << EFFECT_INDEX_0) | (1 << EFFECT_INDEX_1) | (1 << EFFECT_INDEX_2)) &&
        !IsSpellHaveEffect(spellInfo, SPELL_EFFECT_PERSISTENT_AREA_AURA))
    {
        return false;
    }

    SpellAuraHolder* holder = CreateSpellAuraHolder(spellInfo, b, b);

    for (uint32 i = 0; i < MAX_EFFECT_INDEX; ++i)
    {
        uint8 eff = spellInfo->Effect[i];
        if (eff >= MAX_SPELL_EFFECTS)
            continue;
        if (IsAreaAuraEffect(eff) ||
            eff == SPELL_EFFECT_APPLY_AURA ||
            eff == SPELL_EFFECT_PERSISTENT_AREA_AURA)
        {
            int32 basePoints = spellInfo->CalculateSimpleValue(SpellEffectIndex(i));
            int32 damage = basePoints;
            Aura* aur = CreateAura(spellInfo, SpellEffectIndex(i), &damage, &basePoints, holder, b);
            holder->AddAura(aur, SpellEffectIndex(i));
        }
    }
    if (!b->AddSpellAuraHolder(holder))
        delete holder;

    return true;
}

uint32_t BotBridge::CB_GetNeededWorldBuffs(BotHandle bot, uint32_t* out_spells, uint32_t max_out)
{
    Player* b = FindBot(bot);
    if (!b || !out_spells || max_out == 0)
        return 0;

    size_t worldBuffCount = playerbot_config_world_buffs_len();
    if (worldBuffCount == 0)
        return 0;

    // 1 = Alliance, 2 = Horde (matches PB2's worldBuff.factionId convention)
    uint32 factionId = (b->GetTeam() == ALLIANCE) ? 1 : 2;

    uint32_t count = 0;
    for (size_t wbIdx = 0; wbIdx < worldBuffCount; ++wbIdx)
    {
        if (count >= max_out)
            break;

        struct { uint32 spellId, factionId, classId, specId, minLevel, maxLevel, eventId; } wb{};
        if (!playerbot_config_world_buff_at(wbIdx, &wb.spellId, &wb.factionId,
                                            &wb.classId, &wb.specId, &wb.minLevel,
                                            &wb.maxLevel, &wb.eventId))
            continue;

        if (wb.factionId != 0 && wb.factionId != factionId)
            continue;

        if (wb.classId != 0 && wb.classId != b->getClass())
            continue;

        if (wb.specId != 0)
        {
            // Compute spec tab like PB2's AiFactory::GetPlayerSpecTab
            int maxTalentCount = 0, bestTab = 0;
            for (int tab = 0; tab < 3; ++tab)
            {
                int tabCount = 0;
                uint32 classMask = b->getClassMask();
                for (uint32 j = 0; j < sTalentStore.GetNumRows(); ++j)
                {
                    TalentEntry const* talentInfo = sTalentStore.LookupEntry(j);
                    if (!talentInfo) continue;
                    TalentTabEntry const* talentTabInfo = sTalentTabStore.LookupEntry(talentInfo->TalentTab);
                    if (!talentTabInfo || talentTabInfo->tabpage != uint32(tab) || !(talentTabInfo->ClassMask & classMask)) continue;
                    for (int rank = MAX_TALENT_RANK - 1; rank >= 0; --rank)
                    {
                        if (talentInfo->RankID[rank] && b->HasSpell(talentInfo->RankID[rank]))
                        {
                            tabCount += rank + 1;
                            break;
                        }
                    }
                }
                if (tabCount > maxTalentCount) { maxTalentCount = tabCount; bestTab = tab; }
            }
            if (wb.specId != uint32(bestTab + 1))
                continue;
        }

        if (wb.minLevel != 0 && wb.minLevel > b->GetLevel())
            continue;

        if (wb.maxLevel != 0 && wb.maxLevel < b->GetLevel())
            continue;

        if (b->HasAura(wb.spellId))
            continue;

        out_spells[count++] = wb.spellId;
    }

    return count;
}

// ── Heal interrupt ───────────────────────────────────────────────────────────

bool BotBridge::CB_InterruptOwnCast(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    bool interrupted = false;
    for (int type = CURRENT_MELEE_SPELL; type < CURRENT_CHANNELED_SPELL; type++)
    {
        // Skip melee and auto-repeat — only interrupt generic/channeled
        if (type == CURRENT_MELEE_SPELL || type == CURRENT_AUTOREPEAT_SPELL)
            continue;

        Spell* currentSpell = b->GetCurrentSpell((CurrentSpellTypes)type);
        if (currentSpell && currentSpell->CanBeInterrupted())
        {
            b->InterruptSpell((CurrentSpellTypes)type);
            interrupted = true;
        }
    }

    return interrupted;
}

// ── NPC interaction (gossip) ─────────────────────────────────────────────────

bool BotBridge::CB_GossipHello(BotHandle bot, uint32_t npc_entry)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // Find a nearby creature with this entry
    Creature* target = nullptr;
    CreatureList creatures;
    MaNGOS::AllCreaturesOfEntryInRangeCheck check(b, npc_entry, INTERACTION_DISTANCE * 2);
    MaNGOS::CreatureListSearcher<MaNGOS::AllCreaturesOfEntryInRangeCheck> searcher(creatures, check);
    Cell::VisitAllObjects(b, searcher, INTERACTION_DISTANCE * 2);

    for (Creature* c : creatures)
    {
        if (c->IsAlive() && c->IsWithinDistInMap(b, INTERACTION_DISTANCE))
        {
            target = c;
            break;
        }
    }

    if (!target)
        return false;

    b->PrepareGossipMenu(target, target->GetDefaultGossipMenuId());
    b->SendPreparedGossip(target);
    return true;
}

bool BotBridge::CB_BuyFromVendor(BotHandle bot, uint32_t item_id, uint32_t qty)
{
    Player* b = FindBot(bot);
    if (!b || qty == 0)
        return false;

    // Find a nearby vendor
    Creature* vendor = nullptr;
    CreatureList creatures;
    MaNGOS::AllCreaturesOfEntryInRangeCheck check(b, 0, INTERACTION_DISTANCE * 2);
    MaNGOS::CreatureListSearcher<MaNGOS::AllCreaturesOfEntryInRangeCheck> searcher(creatures, check);
    Cell::VisitAllObjects(b, searcher, INTERACTION_DISTANCE * 2);

    for (Creature* c : creatures)
    {
        if (c->IsAlive() && c->isVendor() && c->IsWithinDistInMap(b, INTERACTION_DISTANCE))
        {
            vendor = c;
            break;
        }
    }

    if (!vendor)
        return false;

    // Look up the item in the vendor's item list
    VendorItemData const* vItems = vendor->GetVendorItems();
    if (!vItems)
        return false;

    for (uint32 slot = 0; slot < vItems->GetItemCount(); ++slot)
    {
        VendorItem const* vItem = vItems->GetItem(slot);
        if (!vItem || vItem->item != item_id)
            continue;

        ItemPrototype const* proto = ObjectMgr::GetItemPrototype(item_id);
        if (!proto)
            return false;

        uint32 price = proto->BuyPrice * qty;
        if (b->GetMoney() < price)
            return false;

        // Add item and deduct gold
        if (b->StoreNewItemInBestSlots(item_id, qty))
        {
            b->ModifyMoney(-int32(price));
            return true;
        }
        return false;
    }

    return false;
}

// ── Mail ─────────────────────────────────────────────────────────────────────

bool BotBridge::CB_MailItemToMaster(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b || !b->GetGroup())
        return false;

    // Find the group leader (master)
    ObjectGuid leaderGuid = b->GetGroup()->GetLeaderGuid();
    if (leaderGuid == b->GetObjectGuid())
        return false;

    // For simplicity, just mail the first non-grey item in bags
    // PB2 uses more complex item selection; this covers the basic case
    for (int i = INVENTORY_SLOT_ITEM_START; i < INVENTORY_SLOT_ITEM_END; ++i)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
        if (!item)
            continue;

        ItemPrototype const* proto = item->GetProto();
        if (!proto || proto->Quality <= ITEM_QUALITY_POOR)
            continue;

        MailDraft draft("Bot item", "Sent by bot");
        draft.AddItem(item);
        draft.SendMailTo(MailReceiver(leaderGuid), MailSender(b), MAIL_CHECK_MASK_NONE);
        b->DestroyItem(INVENTORY_SLOT_BAG_0, i, true);
        return true;
    }

    return false;
}

// ── Bank ─────────────────────────────────────────────────────────────────────

bool BotBridge::CB_BankDeposit(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // Check if near a banker
    bool nearBanker = false;
    CreatureList creatures;
    MaNGOS::AllCreaturesOfEntryInRangeCheck check(b, 0, INTERACTION_DISTANCE * 2);
    MaNGOS::CreatureListSearcher<MaNGOS::AllCreaturesOfEntryInRangeCheck> searcher(creatures, check);
    Cell::VisitAllObjects(b, searcher, INTERACTION_DISTANCE * 2);

    for (Creature* c : creatures)
    {
        if (c->IsAlive() && c->isBanker() && c->IsWithinDistInMap(b, INTERACTION_DISTANCE))
        {
            nearBanker = true;
            break;
        }
    }

    if (!nearBanker)
        return false;

    // Move items from bags to bank — deposit grey/excess items
    bool deposited = false;
    for (int i = INVENTORY_SLOT_ITEM_START; i < INVENTORY_SLOT_ITEM_END; ++i)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
        if (!item)
            continue;

        ItemPrototype const* proto = item->GetProto();
        if (!proto)
            continue;

        // Deposit grey items and trade goods
        if (proto->Quality <= ITEM_QUALITY_POOR || proto->Class == ITEM_CLASS_TRADE_GOODS)
        {
            ItemPosCountVec dest;
            if (b->CanBankItem(NULL_BAG, NULL_SLOT, dest, item, false) == EQUIP_ERR_OK)
            {
                b->RemoveItem(INVENTORY_SLOT_BAG_0, i, true);
                b->BankItem(dest, item, true);
                deposited = true;
            }
        }
    }

    return deposited;
}

bool BotBridge::CB_BankWithdraw(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // Check if near a banker
    bool nearBanker = false;
    CreatureList creatures;
    MaNGOS::AllCreaturesOfEntryInRangeCheck check(b, 0, INTERACTION_DISTANCE * 2);
    MaNGOS::CreatureListSearcher<MaNGOS::AllCreaturesOfEntryInRangeCheck> searcher(creatures, check);
    Cell::VisitAllObjects(b, searcher, INTERACTION_DISTANCE * 2);

    for (Creature* c : creatures)
    {
        if (c->IsAlive() && c->isBanker() && c->IsWithinDistInMap(b, INTERACTION_DISTANCE))
        {
            nearBanker = true;
            break;
        }
    }

    if (!nearBanker)
        return false;

    // Withdraw useful items from bank to bags
    bool withdrawn = false;
    for (int i = BANK_SLOT_ITEM_START; i < BANK_SLOT_ITEM_END; ++i)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
        if (!item)
            continue;

        ItemPosCountVec dest;
        if (b->CanStoreItem(NULL_BAG, NULL_SLOT, dest, item, false) == EQUIP_ERR_OK)
        {
            b->RemoveItem(INVENTORY_SLOT_BAG_0, i, true);
            b->StoreItem(dest, item, true);
            withdrawn = true;
        }
    }

    return withdrawn;
}

// ── Auction House ────────────────────────────────────────────────────────────

bool BotBridge::CB_AhPost(BotHandle bot)
{
    // Simplified AH posting — finds auctioneer, posts first AH-worthy item
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // Find auctioneer
    Creature* auctioneer = nullptr;
    CreatureList creatures;
    MaNGOS::AllCreaturesOfEntryInRangeCheck check(b, 0, INTERACTION_DISTANCE * 2);
    MaNGOS::CreatureListSearcher<MaNGOS::AllCreaturesOfEntryInRangeCheck> searcher(creatures, check);
    Cell::VisitAllObjects(b, searcher, INTERACTION_DISTANCE * 2);

    for (Creature* c : creatures)
    {
        if (c->IsAlive() && c->isAuctioner() && c->IsWithinDistInMap(b, INTERACTION_DISTANCE))
        {
            auctioneer = c;
            break;
        }
    }

    if (!auctioneer)
        return false;

    AuctionHouseEntry const* ahEntry = AuctionHouseMgr::GetAuctionHouseEntry(auctioneer);
    if (!ahEntry)
        return false;

    for (int i = INVENTORY_SLOT_ITEM_START; i < INVENTORY_SLOT_ITEM_END; ++i)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
        if (!item)
            continue;

        ItemPrototype const* proto = item->GetProto();
        if (!proto || proto->Quality < ITEM_QUALITY_UNCOMMON)
            continue;

        // Calculate price based on vendor sell price
        uint32 price = proto->SellPrice * 3;
        if (price == 0)
            price = proto->BuyPrice / 2;
        if (price == 0)
            continue;

        uint32 deposit = AuctionHouseMgr::GetAuctionDeposit(ahEntry, 24 * HOUR, item);
        if (b->GetMoney() < deposit)
            continue;

        b->ModifyMoney(-int32(deposit));

        // Remove item from player inventory
        b->MoveItemFromInventory(item->GetBagSlot(), item->GetSlot(), true);
        item->DeleteFromInventoryDB();
        item->SaveToDB();

        // Add to AH using the fork's AuctionHouseObject::AddAuction API
        sAuctionMgr.AddAItem(item);
        AuctionHouseObject* ahObj = sAuctionMgr.GetAuctionsMap(ahEntry);
        ahObj->AddAuction(ahEntry, item, 24 * HOUR, price, price * 2, deposit, b);

        return true;
    }

    return false;
}

bool BotBridge::CB_AhBid(BotHandle bot)
{
    // Simplified AH bidding — bid on the cheapest useful item
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // Find auctioneer
    Creature* auctioneer = nullptr;
    CreatureList creatures;
    MaNGOS::AllCreaturesOfEntryInRangeCheck check(b, 0, INTERACTION_DISTANCE * 2);
    MaNGOS::CreatureListSearcher<MaNGOS::AllCreaturesOfEntryInRangeCheck> searcher(creatures, check);
    Cell::VisitAllObjects(b, searcher, INTERACTION_DISTANCE * 2);

    for (Creature* c : creatures)
    {
        if (c->IsAlive() && c->isAuctioner() && c->IsWithinDistInMap(b, INTERACTION_DISTANCE))
        {
            auctioneer = c;
            break;
        }
    }

    if (!auctioneer)
        return false;

    AuctionHouseEntry const* ahEntry = AuctionHouseMgr::GetAuctionHouseEntry(auctioneer);
    if (!ahEntry)
        return false;

    AuctionHouseObject* auctionHouse = sAuctionMgr.GetAuctionsMap(ahEntry);
    if (!auctionHouse)
        return false;

    // Find an affordable item to bid on
    for (auto& pair : auctionHouse->GetAuctions())
    {
        AuctionEntry* entry = pair.second;
        if (!entry || entry->owner == b->GetGUIDLow())
            continue;

        uint32 bidPrice = (entry->bidder == 0) ? entry->startbid : entry->bid + entry->GetAuctionOutBid();
        if (bidPrice > b->GetMoney())
            continue;

        // Place bid
        b->ModifyMoney(-int32(bidPrice));
        entry->UpdateBid(bidPrice, b);
        return true;
    }

    return false;
}


// ── Fishing ──────────────────────────────────────────────────────────────────

bool BotBridge::CB_StartFishing(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // Check if the bot has a fishing pole equipped or in bags
    // Fishing spell ID = 7731
    SpellEntry const* spellInfo = sSpellTemplate.LookupEntry<SpellEntry>(7731);
    if (!spellInfo)
        return false;

    // Stop movement first
    b->GetMotionMaster()->Clear();
    b->GetMotionMaster()->MoveIdle();

    // Cast fishing spell
    Spell* spell = new Spell(b, spellInfo, false);
    SpellCastTargets targets;
    targets.setDestination(b->GetPositionX() + 3.0f * cos(b->GetOrientation()),
                           b->GetPositionY() + 3.0f * sin(b->GetOrientation()),
                           b->GetPositionZ());
    spell->SpellStart(&targets);
    return true;
}

// ── Battleground ─────────────────────────────────────────────────────────────

bool BotBridge::CB_QueueBg(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    if (b->InBattleGround() || b->GetLevel() < 10)
        return false;

    if (!b->HasFreeBattleGroundQueueId())
        return false;

    if (!b->CanJoinToBattleground())
        return false;

    // Pick a random BG type appropriate for level
    BattleGroundTypeId bgTypeId = BATTLEGROUND_WS;
    uint32 level = b->GetLevel();
    if (level >= 51)
        bgTypeId = (urand(0, 1) == 0) ? BATTLEGROUND_AV : BATTLEGROUND_WS;
    else if (level >= 20)
        bgTypeId = (urand(0, 1) == 0) ? BATTLEGROUND_WS : BATTLEGROUND_AB;

    BattleGround* bg = sBattleGroundMgr.GetBattleGroundTemplate(bgTypeId);
    if (!bg)
        return false;

    if (!b->GetBGAccessByLevel(bgTypeId))
        return false;

    BattleGroundBracketId bracketId = sBattleGroundMgr.GetBattleGroundBracketIdFromLevel(bgTypeId, b->GetLevel());

    uint32 mapId = GetBattleGrounMapIdByTypeId(bgTypeId);
    uint32 instanceId = 0;
    uint8 joinAsGroup = b->GetGroup() && b->GetGroup()->IsLeader(b->GetObjectGuid());

    // Send the CMSG_BATTLEMASTER_JOIN packet (Classic format)
    WorldPacket packet(CMSG_BATTLEMASTER_JOIN, 20);
    packet << b->GetObjectGuid() << mapId << instanceId << joinAsGroup;
    b->GetSession()->HandleBattlemasterJoinOpcode(packet);

    return true;
}

bool BotBridge::CB_AcceptBgInvite(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    if (!b->InBattleGroundQueue())
        return false;

    for (uint32 i = 0; i < PLAYER_MAX_BATTLEGROUND_QUEUES; ++i)
    {
        BattleGroundQueueTypeId bgQueueTypeId = b->GetBattleGroundQueueTypeId(i);
        if (bgQueueTypeId == BATTLEGROUND_QUEUE_NONE)
            continue;

        BattleGroundTypeId bgTypeId = BattleGroundMgr::BgTemplateId(bgQueueTypeId);

        // Send CMSG_BATTLEFIELD_PORT to accept the invite
        WorldPacket packet(CMSG_BATTLEFIELD_PORT, 20);
        packet << bgTypeId << uint8(1); // 1 = accept
        b->GetSession()->HandleBattlefieldPortOpcode(packet);
        return true;
    }

    return false;
}

BotPosition BotBridge::CB_GetBgObjectivePos(BotHandle bot, uint8_t objective_type)
{
    BotPosition pos{};
    Player* b = FindBot(bot);
    if (!b || !b->InBattleGround())
        return pos;

    BattleGround* bg = b->GetBattleGround();
    if (!bg)
        return pos;

    // Return a position based on the objective type and BG type
    // These are approximate positions for common objectives
    switch (bg->GetTypeId())
    {
        case BATTLEGROUND_WS:
            if (objective_type == 2 || objective_type == 3) // flag
            {
                if (b->GetTeam() == ALLIANCE)
                { pos.x = 1539.0f; pos.y = 1481.0f; pos.z = 352.0f; } // Horde flag room
                else
                { pos.x = 1519.0f; pos.y = 1467.0f; pos.z = 373.0f; } // Alliance flag room
            }
            break;
        case BATTLEGROUND_AB:
            // Stables position (first node)
            pos.x = 1166.0f; pos.y = 1200.0f; pos.z = -56.0f;
            break;
        case BATTLEGROUND_AV:
            // Middle of the map
            pos.x = -375.0f; pos.y = -118.0f; pos.z = 69.0f;
            break;
        default:
            break;
    }

    return pos;
}

// ── LFG ──────────────────────────────────────────────────────────────────────

bool BotBridge::CB_LfgJoin(BotHandle bot)
{
    // LFG is WotLK only — Classic/TBC return false
#ifdef MANGOSBOT_TWO
    Player* b = FindBot(bot);
    if (!b)
        return false;
    // WotLK LFG queue logic would go here
    return false;
#else
    (void)bot;
    return false;
#endif
}

bool BotBridge::CB_LfgAccept(BotHandle bot)
{
#ifdef MANGOSBOT_TWO
    Player* b = FindBot(bot);
    if (!b)
        return false;
    return false;
#else
    (void)bot;
    return false;
#endif
}

// ── Dungeon awareness ────────────────────────────────────────────────────────

BotPosition BotBridge::CB_GetTankPosition(BotHandle bot)
{
    BotPosition pos{};
    Player* b = FindBot(bot);
    if (!b || !b->GetGroup())
        return pos;

    // Find the main tank in the group
    Group* group = b->GetGroup();
    for (auto itr = group->GetFirstMember(); itr != nullptr; itr = itr->next())
    {
        Player* member = itr->getSource();
        if (!member || member == b)
            continue;

        // Check if this member is the main tank
        if (group->GetMainTankGuid() == member->GetObjectGuid())
        {
            pos.x = member->GetPositionX();
            pos.y = member->GetPositionY();
            pos.z = member->GetPositionZ();
            return pos;
        }
    }

    // Fallback: use group leader position
    Player* leader = sObjectMgr.GetPlayer(group->GetLeaderGuid());
    if (leader && leader != b)
    {
        pos.x = leader->GetPositionX();
        pos.y = leader->GetPositionY();
        pos.z = leader->GetPositionZ();
    }

    return pos;
}

bool BotBridge::CB_IsUnitCc(BotHandle bot, UnitHandle target)
{
    Unit* t = FindUnit(bot, target);
    if (!t)
        return false;

    // Check for common CC auras
    // Polymorph variants
    if (t->HasAuraType(SPELL_AURA_MOD_CONFUSE))
        return true;
    // Sap, Gouge, Fear, etc.
    if (t->HasAuraType(SPELL_AURA_MOD_STUN) && !t->HasAuraType(SPELL_AURA_MECHANIC_IMMUNITY))
        return true;
    if (t->HasAuraType(SPELL_AURA_MOD_FEAR))
        return true;
    // Hibernate, Entangling Roots, Freezing Trap
    if (t->HasAuraType(SPELL_AURA_MOD_ROOT))
        return true;
    // Banish
    if (t->HasAuraType(SPELL_AURA_SCHOOL_IMMUNITY))
        return true;

    return false;
}

// ── Debug ────────────────────────────────────────────────────────────────────

bool BotBridge::CB_DebugDumpState(BotHandle bot, uint8_t kind)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // Debug dump is handled on the Rust side — this just confirms
    // the bot is valid.
    return true;
}

// ── EngBags: inventory enumeration ───────────────────────────────────────────

static void FillItemEntry(BotInventoryItem& out, Item const* item, uint32_t mailIdx = 0)
{
    ItemPrototype const* proto = item->GetProto();
    out.item_id         = proto->ItemId;
    out.count           = item->GetCount();
    out.enchant_id      = item->GetEnchantmentId(PERM_ENCHANTMENT_SLOT);
    out.random_property = item->GetItemRandomPropertyId();
    out.suffix_factor   = item->GetItemSuffixFactor();
    out.quality          = proto->Quality;
    out.soulbound        = item->IsSoulBound() ? 1 : 0;
    out.mail_index       = mailIdx;
    // Copy the name, ensuring null termination.
    strncpy(out.name, proto->Name1, sizeof(out.name) - 1);
    out.name[sizeof(out.name) - 1] = '\0';
}

BotInventoryItem* BotBridge::CB_BotGetInventory(BotHandle bot, uint32_t* out_count)
{
    *out_count = 0;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    std::vector<BotInventoryItem> items;

    // Main backpack (INVENTORY_SLOT_ITEM_START .. INVENTORY_SLOT_ITEM_END)
    for (int i = INVENTORY_SLOT_ITEM_START; i < INVENTORY_SLOT_ITEM_END; ++i)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
        if (!item || !item->GetProto())
            continue;
        BotInventoryItem e{};
        FillItemEntry(e, item);
        items.push_back(e);
    }

    // Extra bags (INVENTORY_SLOT_BAG_START .. INVENTORY_SLOT_BAG_END)
    for (int bag = INVENTORY_SLOT_BAG_START; bag < INVENTORY_SLOT_BAG_END; ++bag)
    {
        Bag* pBag = dynamic_cast<Bag*>(b->GetItemByPos(INVENTORY_SLOT_BAG_0, bag));
        if (!pBag)
            continue;
        for (uint32 slot = 0; slot < pBag->GetBagSize(); ++slot)
        {
            Item* item = b->GetItemByPos(bag, slot);
            if (!item || !item->GetProto())
                continue;
            BotInventoryItem e{};
            FillItemEntry(e, item);
            items.push_back(e);
        }
    }

    if (items.empty())
        return nullptr;

    *out_count = static_cast<uint32_t>(items.size());
    auto* arr = new BotInventoryItem[items.size()];
    memcpy(arr, items.data(), sizeof(BotInventoryItem) * items.size());
    return arr;
}

BotInventoryItem* BotBridge::CB_BotGetEquipped(BotHandle bot, uint32_t* out_count)
{
    *out_count = 0;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    std::vector<BotInventoryItem> items;

    for (uint8 slot = EQUIPMENT_SLOT_START; slot < EQUIPMENT_SLOT_END; ++slot)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, slot);
        if (!item || !item->GetProto())
            continue;
        BotInventoryItem e{};
        FillItemEntry(e, item);
        items.push_back(e);
    }

    if (items.empty())
        return nullptr;

    *out_count = static_cast<uint32_t>(items.size());
    auto* arr = new BotInventoryItem[items.size()];
    memcpy(arr, items.data(), sizeof(BotInventoryItem) * items.size());
    return arr;
}

BotInventoryItem* BotBridge::CB_BotGetBankItems(BotHandle bot, uint32_t* out_count)
{
    *out_count = 0;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    std::vector<BotInventoryItem> items;

    for (int i = BANK_SLOT_ITEM_START; i < BANK_SLOT_ITEM_END; ++i)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
        if (!item || !item->GetProto())
            continue;
        BotInventoryItem e{};
        FillItemEntry(e, item);
        items.push_back(e);
    }

    if (items.empty())
        return nullptr;

    *out_count = static_cast<uint32_t>(items.size());
    auto* arr = new BotInventoryItem[items.size()];
    memcpy(arr, items.data(), sizeof(BotInventoryItem) * items.size());
    return arr;
}

BotInventoryItem* BotBridge::CB_BotGetMailItems(BotHandle bot, uint32_t* out_count)
{
    *out_count = 0;
    Player* b = FindBot(bot);
    if (!b)
        return nullptr;

    std::vector<BotInventoryItem> items;
    uint32_t mailIdx = 1;

    for (auto it = b->GetMailBegin(); it != b->GetMailEnd(); ++it, ++mailIdx)
    {
        Mail* mail = *it;
        if (!mail || !mail->HasItems())
            continue;

        for (auto const& info : mail->items)
        {
            Item* item = b->GetMItem(info.item_guid);
            if (!item || !item->GetProto())
                continue;
            BotInventoryItem e{};
            FillItemEntry(e, item, mailIdx);
            items.push_back(e);
        }
    }

    if (items.empty())
        return nullptr;

    *out_count = static_cast<uint32_t>(items.size());
    auto* arr = new BotInventoryItem[items.size()];
    memcpy(arr, items.data(), sizeof(BotInventoryItem) * items.size());
    return arr;
}

void BotBridge::CB_BotFreeInventoryList(BotInventoryItem* list)
{
    delete[] list;
}

// ── EngBags: item actions ────────────────────────────────────────────────────

bool BotBridge::CB_SellItem(BotHandle bot, uint32_t item_id)
{
    Player* b = FindBot(bot);
    if (!b || item_id == 0)
        return false;

    // Search bags for the item.
    auto findAndSell = [&](uint8 bag, uint8 slot) -> bool {
        Item* item = b->GetItemByPos(bag, slot);
        if (!item || item->GetEntry() != item_id)
            return false;
        ItemPrototype const* proto = item->GetProto();
        if (!proto || proto->SellPrice == 0)
            return false;
        uint32 money = proto->SellPrice * item->GetCount();
        b->ModifyMoney(money);
        b->DestroyItem(bag, slot, true);
        return true;
    };

    // Main backpack
    for (int i = INVENTORY_SLOT_ITEM_START; i < INVENTORY_SLOT_ITEM_END; ++i)
        if (findAndSell(INVENTORY_SLOT_BAG_0, i))
            return true;

    // Extra bags
    for (int bag = INVENTORY_SLOT_BAG_START; bag < INVENTORY_SLOT_BAG_END; ++bag)
    {
        Bag* pBag = dynamic_cast<Bag*>(b->GetItemByPos(INVENTORY_SLOT_BAG_0, bag));
        if (!pBag)
            continue;
        for (uint32 slot = 0; slot < pBag->GetBagSize(); ++slot)
            if (findAndSell(bag, slot))
                return true;
    }

    return false;
}

static bool FindNearbyBanker(Player* b)
{
    CreatureList creatures;
    MaNGOS::AllCreaturesOfEntryInRangeCheck check(b, 0, INTERACTION_DISTANCE * 2);
    MaNGOS::CreatureListSearcher<MaNGOS::AllCreaturesOfEntryInRangeCheck> searcher(creatures, check);
    Cell::VisitAllObjects(b, searcher, INTERACTION_DISTANCE * 2);
    for (Creature* c : creatures)
    {
        if (c->IsAlive() && c->isBanker() && c->IsWithinDistInMap(b, INTERACTION_DISTANCE))
            return true;
    }
    return false;
}

bool BotBridge::CB_BankDepositItem(BotHandle bot, uint32_t item_id)
{
    Player* b = FindBot(bot);
    if (!b || item_id == 0 || !FindNearbyBanker(b))
        return false;

    // Find the item in bags.
    for (int i = INVENTORY_SLOT_ITEM_START; i < INVENTORY_SLOT_ITEM_END; ++i)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
        if (!item || item->GetEntry() != item_id)
            continue;
        ItemPosCountVec dest;
        if (b->CanBankItem(NULL_BAG, NULL_SLOT, dest, item, false) == EQUIP_ERR_OK)
        {
            b->RemoveItem(INVENTORY_SLOT_BAG_0, i, true);
            b->BankItem(dest, item, true);
            return true;
        }
    }
    // Extra bags
    for (int bag = INVENTORY_SLOT_BAG_START; bag < INVENTORY_SLOT_BAG_END; ++bag)
    {
        Bag* pBag = dynamic_cast<Bag*>(b->GetItemByPos(INVENTORY_SLOT_BAG_0, bag));
        if (!pBag) continue;
        for (uint32 slot = 0; slot < pBag->GetBagSize(); ++slot)
        {
            Item* item = b->GetItemByPos(bag, slot);
            if (!item || item->GetEntry() != item_id)
                continue;
            ItemPosCountVec dest;
            if (b->CanBankItem(NULL_BAG, NULL_SLOT, dest, item, false) == EQUIP_ERR_OK)
            {
                b->RemoveItem(bag, slot, true);
                b->BankItem(dest, item, true);
                return true;
            }
        }
    }
    return false;
}

bool BotBridge::CB_BankWithdrawItem(BotHandle bot, uint32_t item_id)
{
    Player* b = FindBot(bot);
    if (!b || item_id == 0 || !FindNearbyBanker(b))
        return false;

    for (int i = BANK_SLOT_ITEM_START; i < BANK_SLOT_ITEM_END; ++i)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
        if (!item || item->GetEntry() != item_id)
            continue;
        ItemPosCountVec dest;
        if (b->CanStoreItem(NULL_BAG, NULL_SLOT, dest, item, false) == EQUIP_ERR_OK)
        {
            b->RemoveItem(INVENTORY_SLOT_BAG_0, i, true);
            b->StoreItem(dest, item, true);
            return true;
        }
    }
    return false;
}

bool BotBridge::CB_BotMailTakeIndex(BotHandle bot, uint32_t mail_index)
{
    Player* b = FindBot(bot);
    if (!b || mail_index == 0)
        return false;

    ObjectGuid mailbox = FindNearbyMailbox(b);
    if (mailbox.IsEmpty())
        return false;

    // Find the mail at the given 1-based index.
    uint32_t idx = 1;
    for (auto it = b->GetMailBegin(); it != b->GetMailEnd(); ++it, ++idx)
    {
        if (idx != mail_index)
            continue;

        Mail* mail = *it;
        if (!mail)
            return false;

        uint32 id = mail->messageID;

        if (mail->money > 0)
        {
            WorldPacket pkt;
            pkt << mailbox;
            pkt << id;
            b->GetSession()->HandleMailTakeMoney(pkt);
        }

        if (mail->HasItems())
        {
            std::vector<uint32> itemGuids;
            for (auto const& info : mail->items)
                itemGuids.push_back(info.item_guid);
            for (uint32 itemGuid : itemGuids)
            {
                WorldPacket pkt;
                pkt << mailbox;
                pkt << id;
#ifndef MANGOSBOT_ZERO
                pkt << itemGuid;
#endif
                b->GetSession()->HandleMailTakeItem(pkt);
            }
        }

        WorldPacket delPkt;
        delPkt << mailbox;
        delPkt << id;
#ifndef MANGOSBOT_ZERO
        delPkt << uint32(0);
#endif
        b->GetSession()->HandleMailDelete(delPkt);
        return true;
    }

    return false;
}

bool BotBridge::CB_SendMailItem(BotHandle bot, uint32_t item_id)
{
    Player* b = FindBot(bot);
    if (!b || item_id == 0 || !b->GetGroup())
        return false;

    ObjectGuid leaderGuid = b->GetGroup()->GetLeaderGuid();
    if (leaderGuid == b->GetObjectGuid())
        return false;

    // Search bags for the item.
    for (int i = INVENTORY_SLOT_ITEM_START; i < INVENTORY_SLOT_ITEM_END; ++i)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
        if (!item || item->GetEntry() != item_id)
            continue;

        MailDraft draft("Bot item", "Sent by bot");
        draft.AddItem(item);
        draft.SendMailTo(MailReceiver(leaderGuid), MailSender(b), MAIL_CHECK_MASK_NONE);
        b->DestroyItem(INVENTORY_SLOT_BAG_0, i, true);
        return true;
    }

    // Extra bags
    for (int bag = INVENTORY_SLOT_BAG_START; bag < INVENTORY_SLOT_BAG_END; ++bag)
    {
        Bag* pBag = dynamic_cast<Bag*>(b->GetItemByPos(INVENTORY_SLOT_BAG_0, bag));
        if (!pBag) continue;
        for (uint32 slot = 0; slot < pBag->GetBagSize(); ++slot)
        {
            Item* item = b->GetItemByPos(bag, slot);
            if (!item || item->GetEntry() != item_id)
                continue;

            MailDraft draft("Bot item", "Sent by bot");
            draft.AddItem(item);
            draft.SendMailTo(MailReceiver(leaderGuid), MailSender(b), MAIL_CHECK_MASK_NONE);
            b->DestroyItem(bag, slot, true);
            return true;
        }
    }

    return false;
}

bool BotBridge::CB_TradeAddItem(BotHandle bot, uint32_t item_id, uint32_t count)
{
    Player* b = FindBot(bot);
    if (!b || item_id == 0)
        return false;

    if (!b->GetTrader())
        return false;

    TradeData* pTrade = b->GetTradeData();
    if (!pTrade)
        return false;

    uint32_t added = 0;
    uint32_t wanted = (count == 0) ? 1 : count;

    // Search backpack
    for (int i = INVENTORY_SLOT_ITEM_START; i < INVENTORY_SLOT_ITEM_END && added < wanted; ++i)
    {
        Item* item = b->GetItemByPos(INVENTORY_SLOT_BAG_0, i);
        if (!item || item->GetEntry() != item_id || item->IsInTrade())
            continue;

        // Find a free trade slot
        for (uint8 slot = 0; slot < TRADE_SLOT_TRADED_COUNT; ++slot)
        {
            if (pTrade->GetItem(TradeSlots(slot)) == nullptr)
            {
                pTrade->SetItem(TradeSlots(slot), item);
                ++added;
                break;
            }
        }
    }

    // Extra bags
    for (int bag = INVENTORY_SLOT_BAG_START; bag < INVENTORY_SLOT_BAG_END && added < wanted; ++bag)
    {
        Bag* pBag = dynamic_cast<Bag*>(b->GetItemByPos(INVENTORY_SLOT_BAG_0, bag));
        if (!pBag) continue;
        for (uint32 slot = 0; slot < pBag->GetBagSize() && added < wanted; ++slot)
        {
            Item* item = b->GetItemByPos(bag, slot);
            if (!item || item->GetEntry() != item_id || item->IsInTrade())
                continue;

            for (uint8 ts = 0; ts < TRADE_SLOT_TRADED_COUNT; ++ts)
            {
                if (pTrade->GetItem(TradeSlots(ts)) == nullptr)
                {
                    pTrade->SetItem(TradeSlots(ts), item);
                    ++added;
                    break;
                }
            }
        }
    }

    return added > 0;
}

uint32_t BotBridge::CB_GetSpellCraftItem(uint32_t spell_id)
{
    SpellEntry const* spellInfo = sSpellTemplate.LookupEntry<SpellEntry>(spell_id);
    if (!spellInfo)
        return 0;

    for (int i = 0; i < MAX_EFFECT_INDEX; ++i)
    {
        if (spellInfo->Effect[i] == SPELL_EFFECT_CREATE_ITEM && spellInfo->EffectItemType[i] != 0)
            return spellInfo->EffectItemType[i];
    }

    return 0;
}

bool BotBridge::CB_GetItemInfo(uint32_t item_id, char* name_buf, uint32_t name_buf_len, uint8_t* out_quality)
{
    ItemPrototype const* proto = sItemStorage.LookupEntry<ItemPrototype>(item_id);
    if (!proto)
        return false;

    if (name_buf && name_buf_len > 0)
    {
        strncpy(name_buf, proto->Name1, name_buf_len - 1);
        name_buf[name_buf_len - 1] = '\0';
    }
    if (out_quality)
        *out_quality = proto->Quality;

    return true;
}
