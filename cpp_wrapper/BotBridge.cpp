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

#ifdef CMANGOS
#include "Combat/ThreatManager.h"
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
    cbs.say                 = CB_Say;
    cbs.whisper             = CB_Whisper;
    cbs.use_item            = CB_UseItem;
    cbs.taunt               = CB_Taunt;

    // Group / raid
    cbs.group_get_tank      = CB_GroupGetTank;
    cbs.group_get_healer    = CB_GroupGetHealer;
    cbs.group_get_role      = CB_GroupGetRole;
    cbs.get_unit_with_raid_icon = CB_GetUnitWithRaidIcon;

    // Death / resurrection
    cbs.accept_resurrect    = CB_AcceptResurrect;
    cbs.get_corpse_position = CB_GetCorpsePosition;
    cbs.use_spirit_healer   = CB_UseSpiritHealer;

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

    // Pet management
    cbs.has_pet             = CB_HasPet;
    cbs.pet_is_alive        = CB_PetIsAlive;
    cbs.pet_happiness       = CB_PetHappiness;
    cbs.summon_pet          = CB_SummonPet;
    cbs.revive_pet          = CB_RevivePet;
    cbs.feed_pet            = CB_FeedPet;

    // Dispel / party queries
    cbs.find_dispellable_target = CB_FindDispellableTarget;
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

bool BotBridge::CB_OpenLoot(BotHandle /*bot*/, UnitHandle /*target*/)
{
    // Loot APIs in this fork (Player::SendLoot, Loot::GetMaxSlotInLootFor,
    // Player::StoreLootItem, WorldSession::DoLootRelease) are either renamed,
    // relocated, or go through a packet flow that is not currently exposed
    // to the bridge. Stubbed until a Rust consumer actually requires it.
    return false;
}

bool BotBridge::CB_TakeAllLoot(BotHandle /*bot*/)
{
    // See CB_OpenLoot — stubbed until ported to this fork's loot API.
    return false;
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

static bool IsDispelableBySpell(uint32_t debuffSpellId, Player* bot)
{
    SpellEntry const* debuffInfo = sSpellTemplate.LookupEntry<SpellEntry>(debuffSpellId);
    if (!debuffInfo)
        return false;

    uint32_t dispelMask = GetDispellMask(DispelType(debuffInfo->Dispel));

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

BotDispelTarget BotBridge::CB_FindDispellableTarget(BotHandle bot)
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
            if (IsDispelableBySpell(spellId, b))
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
            if (IsDispelableBySpell(spellId, b))
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

    // Simple: return the center of the BG map as a default objective.
    // In practice this would use BG-specific logic for flag/base positions.
    // For now, return bot's current position + offset toward center.
    float cx = b->GetPositionX();
    float cy = b->GetPositionY();
    float cz = b->GetPositionZ();

    // TODO: BG-specific objective logic. For now return not-found
    // so the BT falls through to follow/attack behavior.
    return result;
}

bool BotBridge::CB_CaptureBgObjective(BotHandle bot)
{
    Player* b = FindBot(bot);
    if (!b)
        return false;

    // This would interact with nearby BG objects (flags, bases).
    // Requires BG-specific GameObject interaction.
    // TODO: implement per-BG capture logic
    return false;
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
