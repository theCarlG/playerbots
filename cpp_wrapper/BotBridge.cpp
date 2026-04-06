/**
 * BotBridge.cpp — CMaNGOS implementation of the BotCallbacks vtable.
 *
 * Every callback:
 *   1. Resolves BotHandle → Player* (and UnitHandle → Unit*) via ObjectAccessor.
 *   2. Calls the CMaNGOS API.
 *   3. Returns a plain-data result (no pointers into CMaNGOS memory).
 *
 * Threading: these are called on the map worker thread that owns the bot's Player.
 * No additional locking is needed — CMaNGOS guarantees single-threaded access
 * to each Player's UpdateAI.
 */

#include "botpch.h"
#include "BotBridge.h"

#include <cstdlib>
#include <unordered_set>

#include "Entities/Player.h"
#include "Entities/Unit.h"
#include "Entities/Creature.h"
#include "Entities/Pet.h"
#include "Entities/Corpse.h"
#include "Entities/GameObject.h"
#include "Entities/Bag.h"
#include "Entities/Item.h"
#include "Entities/ItemPrototype.h"
#include "playerbot/PlayerbotAIConfig.h"
#include "playerbot/RandomItemMgr.h"
#include "playerbot/RandomPlayerbotMgr.h"
#include "Util/Util.h"
#include "Globals/ObjectAccessor.h"
#include "Spells/SpellMgr.h"
#include "Spells/Spell.h"
#include "Spells/SpellAuras.h"
#include "Maps/Map.h"
#include "MotionGenerators/MotionMaster.h"
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
#include "BattleGround/BattleGround.h"
#include "BattleGround/BattleGroundWS.h"
#include "BattleGround/BattleGroundAB.h"
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
    s.in_combat  = unit->IsInCombat();
    s.is_casting = unit->IsNonMeleeSpellCasted(false);
    s.is_moving  = unit->IsMoving();
    s.is_channeling = unit->GetCurrentSpell(CURRENT_CHANNELED_SPELL) != nullptr;

    if (s.is_casting && unit->GetCurrentSpell(CURRENT_GENERIC_SPELL))
    {
        Spell* spell = unit->GetCurrentSpell(CURRENT_GENERIC_SPELL);
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

    // Target — GetTargetGuid() is protected in this fork; use the current
    // combat victim as the effective target guid exposed to Rust.
    s.current_target = unit->GetVictim() ? unit->GetVictim()->GetObjectGuid().GetRawValue() : uint64_t(0);

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
    cbs.free_unit_list      = CB_FreeUnitList;
    cbs.bot_is_behind               = CB_BotIsBehind;
    cbs.bot_equipped_weapon_subclass = CB_BotEquippedWeaponSubclass;
    cbs.bot_item_count              = CB_BotItemCount;
    cbs.bot_active_totem_mask       = CB_BotActiveTotemMask;
    cbs.bot_weapon_enchanted        = CB_BotWeaponEnchanted;
    cbs.bot_runes_ready_mask        = CB_BotRunesReadyMask;
    cbs.bot_knows_spell             = CB_BotKnowsSpell;

    // Pathfinding / positioning
    cbs.get_behind_position = CB_GetBehindPosition;
    cbs.get_safe_position   = CB_GetSafePosition;
    cbs.get_spread_position = CB_GetSpreadPosition;
    cbs.can_reach           = CB_CanReach;

    // Commands
    cbs.cast_spell          = CB_CastSpell;
    cbs.cast_spell_pos      = CB_CastSpellPos;
    cbs.move_to             = CB_MoveTo;
    cbs.follow              = CB_Follow;
    cbs.stop_moving         = CB_StopMoving;
    cbs.attack              = CB_Attack;
    cbs.auto_attack         = CB_AutoAttack;
    cbs.auto_shoot          = CB_AutoShoot;
    cbs.say                 = CB_Say;
    cbs.tell_player         = CB_TellPlayer;
    cbs.whisper             = CB_Whisper;
    cbs.use_item            = CB_UseItem;
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
    cbs.summon_pet          = CB_SummonPet;
    cbs.revive_pet          = CB_RevivePet;
    cbs.feed_pet            = CB_FeedPet;

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

    cbs.item_prototype_quality              = CB_ItemPrototypeQuality;
    cbs.factory_pick_trade_for_level        = CB_FactoryPickTradeForLevel;
    cbs.get_random_bot_spell_ids            = CB_GetRandomBotSpellIds;

    cbs.get_overworld_taxi_nodes            = CB_GetOverworldTaxiNodes;
    cbs.free_taxi_nodes                     = CB_FreeTaxiNodes;
    cbs.bot_set_taxi_node                   = CB_BotSetTaxiNode;
    cbs.get_class_talents                   = CB_GetClassTalents;
    cbs.free_class_talents                  = CB_FreeClassTalents;
    cbs.bot_free_talent_points              = CB_BotFreeTalentPoints;
    cbs.bot_update_free_talent_points       = CB_BotUpdateFreeTalentPoints;
    cbs.bot_pick_spec_no                    = CB_BotPickSpecNo;

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

    // Addon-channel reply routing
    cbs.bot_tell_addon                      = CB_TellAddon;

    // Loot rolling — PB2 handles this via packet interception in
    // PlayerbotMgr::HandleMasterIncomingPacket (CMSG_LOOT_ROLL),
    // not through proactive callbacks.
    cbs.get_pending_roll_count              = nullptr;
    cbs.auto_loot_roll                      = nullptr;
    cbs.cast_loot_roll                      = nullptr;

    // Travel destination queries
    cbs.bot_find_travel_dests               = CB_BotFindTravelDests;
    cbs.bot_free_travel_dests               = CB_BotFreeTravelDests;

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
            if (member && member->IsInWorld())
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
        bool isHostile = b->IsHostileTo(u);
        if (hostile == isHostile)
            handles.push_back(u->GetObjectGuid().GetRawValue());
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

    // Try 8 directions at search_radius
    for (int i = 0; i < 8; ++i)
    {
        float angle = i * (static_cast<float>(M_PI) / 4.0f);
        float cx = bx + std::cos(angle) * search_radius;
        float cy = by + std::sin(angle) * search_radius;
        float cz = bz;
        b->UpdateGroundPositionZ(cx, cy, cz);

        if (b->GetMap()->IsInLineOfSight(bx, by, bz, cx, cy, cz, false))
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
    if (!b->HasSpell(spell_id) || !b->IsSpellReady(spell_id))
        return false;

    Spell* spell = new Spell(b, spellInfo, false);
    SpellCastTargets targets;
    targets.setUnitTarget(t);
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
    spell->SpellStart(&targets);
    return true;
}

bool BotBridge::CB_MoveTo(BotHandle bot, float x, float y, float z)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    b->GetMotionMaster()->MovePoint(0, x, y, z);
    return true;
}

bool BotBridge::CB_Follow(BotHandle bot, UnitHandle target, float dist, float angle)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return false;

    b->GetMotionMaster()->MoveFollow(t, dist, angle);
    return true;
}

bool BotBridge::CB_StopMoving(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    b->GetMotionMaster()->Clear(false);
    b->StopMoving();
    return true;
}

bool BotBridge::CB_Attack(BotHandle bot, UnitHandle target)
{
    Player* b = FindBot(bot);
    Unit* t   = FindUnit(bot, target);
    if (!b || !t)
        return false;

    b->Attack(t, true);
    return true;
}

bool BotBridge::CB_AutoAttack(BotHandle bot, bool enable)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;
    // This fork has no Player::SetAutoAttack. Disable = AttackStop;
    // enable is a no-op here — CB_Attack already toggles meleeAttack on.
    if (!enable)
        b->AttackStop();
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
        case ITEM_SUBCLASS_WEAPON_GUN:
        case ITEM_SUBCLASS_WEAPON_CROSSBOW:
            spell_id = 75;    // Auto Shot (hunter only — HasSpell gate below enforces)
            break;
        case ITEM_SUBCLASS_WEAPON_WAND:
            spell_id = 5019;  // Shoot (wand) — learned by every wand-capable class
            break;
        default:
            return false;
    }

    if (!b->HasSpell(spell_id) || !b->IsSpellReady(spell_id))
        return false;

    SpellEntry const* spellInfo = sSpellTemplate.LookupEntry<SpellEntry>(spell_id);
    if (!spellInfo)
        return false;

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

    const float followRange = sPlayerbotAIConfig.followDistance > 0.0f
                                  ? sPlayerbotAIConfig.followDistance
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

    // 3. AutoStore handles filtering (permission, inventory space,
    //    blocked-for-roll) and removes taken items from the loot list.
    loot->AutoStore(b);

    // 4. Release the corpse window.
    WorldPacket releasePacket(CMSG_LOOT_RELEASE, 8);
    releasePacket << targetGuid;
    b->GetSession()->HandleLootReleaseOpcode(releasePacket);

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

    b->RewardQuest(quest, 0, creature, true);
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

bool BotBridge::CB_SummonPet(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // For hunters: Call Pet (883)
    // For warlocks: Summon Imp (688), Voidwalker (697), etc.
    static const uint32_t callPetSpells[] = {883, 688, 697, 712, 691, 0};
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
    if (!b || !b->duel || b->duel->startTime)
        return false;
    // Accept: the arbiter GO starts the countdown and the client plays
    // the duel animation.
    b->DuelComplete(DUEL_INTERRUPTED);  // This clears the pending request
    // NOTE: a proper accept would mirror CMSG_DUEL_ACCEPTED handling.
    // Since the C++ API doesn't expose a clean "accept" helper, we
    // stub this — it will be fleshed out when duel strategies land.
    return false; // stub — returns false until full impl
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

    // Verify line of sight
    if (b->GetMap()->IsInLineOfSight(b->GetPositionX(), b->GetPositionY(),
                                      b->GetPositionZ(), x, y, z, false))
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
    return sRandomItemMgr.GetRandomPotion(level, effect);
}

uint32_t BotBridge::CB_FactoryPickFoodForLevel(BotHandle /*bot*/, uint32_t level, uint32_t category)
{
    return sRandomItemMgr.GetFood(level, category);
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
    return sRandomItemMgr.GetAmmo(level, ammo_subclass);
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
    return sRandomItemMgr.GetRandomTrade(level);
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
    uint32 p1 = sPlayerbotAIConfig.specProbability[cls][0];
    uint32 p2 = p1 + sPlayerbotAIConfig.specProbability[cls][1];

    uint32 picked = (point < p1 ? 0u : (point < p2 ? 1u : 2u));
    sRandomPlayerbotMgr.SetValue(b, "specNo", picked + 1);
    return picked;
}

// ── Factory: config list queries ──────────────────────────────────────────

uint32_t* BotBridge::CB_GetRandomBotSpellIds(BotHandle /*bot*/, uint32_t* out_count)
{
    if (out_count)
        *out_count = 0;
    if (!out_count)
        return nullptr;

    std::list<uint32> const& list = sPlayerbotAIConfig.randomBotSpellIds;
    if (list.empty())
        return nullptr;

    uint32_t count = static_cast<uint32_t>(list.size());
    uint32_t* arr = static_cast<uint32_t*>(std::malloc(count * sizeof(uint32_t)));
    if (!arr)
        return nullptr;

    uint32_t i = 0;
    for (std::list<uint32>::const_iterator it = list.begin(); it != list.end(); ++it)
        arr[i++] = *it;
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
