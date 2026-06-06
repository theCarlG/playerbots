#include "Config/Config.h"

#include <atomic>
#include "playerbot.h"
#include "BotBridge.h"
#include "BotConfig.h"
#include "Accounts/AccountMgr.h"
#include "Globals/ObjectMgr.h"
#include "Database/DatabaseEnv.h"
#include "PlayerbotAI.h"
#include "PlayerbotRust.h"
#include "Entities/Player.h"

#include "Grids/GridNotifiers.h"
#include "Grids/GridNotifiersImpl.h"
#include "Grids/CellImpl.h"

#include "BattleGround/BattleGround.h"
#include "BattleGround/BattleGroundMgr.h"
#include "Chat/ChannelMgr.h"
#include "Guilds/GuildMgr.h"
#include "World/WorldState.h"
#include "LoginBridge.h"
#include "botffi.h"
#include "Entities/Transports.h"

#ifndef MANGOSBOT_ZERO
#ifdef CMANGOS
#include "Arena/ArenaTeam.h"
#endif
#ifdef MANGOS
#include "ArenaTeam.h"
#endif
#endif
#include <iomanip>
#include <float.h>

#if PLATFORM == PLATFORM_WINDOWS
#include "windows.h"
#include "psapi.h"
#endif

using namespace MaNGOS;

INSTANTIATE_SINGLETON_1(RandomPlayerbotMgr);

#ifdef CMANGOS
#include <boost/thread/thread.hpp>
#endif



class botPIDImpl
{
public:
    botPIDImpl(double dt, double max, double min, double Kp, double Ki, double Kd);
    ~botPIDImpl();
    double calculate(double setpoint, double pv);
    void adjust(double Kp, double Ki, double Kd) { _Kp = Kp; _Ki = Ki; _Kd = Kd; }
    void reset() { _integral = 0; }

private:
    double _dt;
    double _max;
    double _min;
    double _Kp;
    double _Ki;
    double _Kd;
    double _pre_error;
    double _integral;
};


botPID::botPID(double dt, double max, double min, double Kp, double Ki, double Kd)
{
    pimpl = new botPIDImpl(dt, max, min, Kp, Ki, Kd);
}
void botPID::adjust(double Kp, double Ki, double Kd)
{
    pimpl->adjust(Kp, Ki, Kd);
}
void botPID::reset()
{
    pimpl->reset();
}
double botPID::calculate(double setpoint, double pv)
{
    return pimpl->calculate(setpoint, pv);
}
botPID::~botPID()
{
    delete pimpl;
}


/**
 * Implementation
 */
botPIDImpl::botPIDImpl(double dt, double max, double min, double Kp, double Ki, double Kd) :
    _dt(dt),
    _max(max),
    _min(min),
    _Kp(Kp),
    _Ki(Ki),
    _Kd(Kd),
    _pre_error(0),
    _integral(0)
{
}

double botPIDImpl::calculate(double setpoint, double pv)
{

    // Calculate error
    double error = setpoint - pv;

    // Proportional term
    double Pout = _Kp * error;

    // Integral term
    _integral += error * _dt;

    double Iout = _Ki * _integral;

    // Derivative term
    double derivative = (error - _pre_error) / _dt;
    double Dout = _Kd * derivative;

    // Calculate total output
    double output = Pout + Iout + Dout;

    // Restrict to max/min
    if (output > _max)
    {
        output = _max;
        _integral -= error * _dt; //Stop integral buildup at max
    }
    else if (output < _min)
    {
        output = _min;
        _integral -= error * _dt; //Stop integral buildup at min
    }

    // Save error to previous error
    _pre_error = error;

    return output;
}

botPIDImpl::~botPIDImpl()
{
}

RandomPlayerbotMgr::RandomPlayerbotMgr()
: PlayerbotHolder()
, processTicks(0)
, loginProgressBar(NULL)
{
    // Initialize the Rust AI module once at server startup.
    PlayerbotRust::InitRustModule();

    // Repopulate the random-bot account allow-list. The core's bot-login
    // gate (CharacterHandler::HandlePlayerBotLogin) calls
    // sPlayerbotAIConfig.IsInRandomAccountList() to decide whether an
    // autonomous (master-less) random-bot login is permitted. PB2 filled
    // this list in RandomPlayerbotMgr::Init from the account table; the
    // Rust migration moved the authoritative copy into the Rust factory
    // singleton but left the C++ mirror un-loaded, so every random bot was
    // rejected at login with "Attempt to add not allowed bot ...". Reload
    // it here, by the configured account prefix, before any bot login runs.
    if (playerbot_config_enabled())
    {
        sPlayerbotAIConfig.randomBotAccounts.clear();
        std::string prefix = sPlayerbotAIConfig.randomBotAccountPrefix;
        if (!prefix.empty())
        {
            if (auto results = LoginDatabase.PQuery(
                    "SELECT id FROM account WHERE username LIKE '%s%%'", prefix.c_str()))
            {
                do
                {
                    Field* fields = results->Fetch();
                    sPlayerbotAIConfig.randomBotAccounts.push_back(fields[0].GetUInt32());
                } while (results->NextRow());
            }
        }
        sLog.outString("Playerbots: loaded %u random-bot account(s) (prefix '%s')",
                       (uint32)sPlayerbotAIConfig.randomBotAccounts.size(), prefix.c_str());
    }

    if (playerbot_config_enabled() && playerbot_config_random_bot_autologin())
    {
        PrepareTeleportCache();

        //1) Proportional: Amount activity is adjusted based on diff being above or below wanted diff. (100 wanted diff & 0.1 p = 150 diff = -5% activity)
        //2) Integral: Same as proportional but builds up each tick. (100 wanted diff & 0.01 i = 150 diff = -0.5% activity each tick)
        //3) Derative: Based on speed of diff. (+5 diff last tick & 0.05 d = -0.25% activity)
        pid.adjust(0.05,0.001,0.05);
        BgCheckTimer = 0;
        LfgCheckTimer = 0;
        PlayersCheckTimer = 0;
        EventTimeSyncTimer = 0;
        OfflineGroupBotsTimer = 0;
        guildsDeleted = false;
        arenaTeamsDeleted = false;

        std::list<uint32> availableBots = GetBots();

        for (auto& bot : availableBots)
        {
            if(GetEventValue(bot,"login"))
                SetEventValue(bot, "login", 0, 0);
        }

#ifndef MANGOSBOT_ZERO
        // load random bot team members
        auto results = CharacterDatabase.PQuery("SELECT guid FROM arena_team_member");
        if (results)
        {
            sLog.outString("Loading arena team bot members...");
            do
            {
                Field* fields = results->Fetch();
                uint32 lowguid = fields[0].GetUInt32();
                arenaTeamMembers.push_back(lowguid);
            } while (results->NextRow());
        }
#endif
        // sync event timers
        SyncEventTimers();

        showLoginWarning = true;
    }
}

RandomPlayerbotMgr::~RandomPlayerbotMgr()
{
    // Shutdown the Rust AI module when the server stops.
    PlayerbotRust::ShutdownRustModule();
}

int RandomPlayerbotMgr::GetMaxAllowedBotCount()
{
    return GetEventValue(0, "bot_count");
}

// Static item-equip simulation helpers — pure CMaNGOS dispatch used by
// BotBridge to test whether a bot can equip an item it does not yet own.
Item* RandomPlayerbotMgr::CreateTempItem(uint32 item, uint32 count, Player const* player, uint32 randomPropertyId)
{
    if (count < 1)
        return nullptr;                                        // don't create item at zero count

    if (ItemPrototype const* pProto = ObjectMgr::GetItemPrototype(item))
    {
        if (count > pProto->GetMaxStackSize())
            count = pProto->GetMaxStackSize();

        MANGOS_ASSERT(count != 0 && "pProto->Stackable == 0 but checked at loading already");

        Item* pItem = NewItemOrBag(pProto);
        if (pItem->Create(0, item, player))
        {
            pItem->SetCount(count);
            if (int32 randId = randomPropertyId ? randomPropertyId : Item::GenerateItemRandomPropertyId(item))
                pItem->SetItemRandomProperties(randId);

            return pItem;
        }
        delete pItem;
    }
    return nullptr;
}

InventoryResult RandomPlayerbotMgr::CanEquipUnseenItem(Player* player, uint8 slot, uint16& dest, uint32 item)
{
    dest = 0;
    Item* pItem = RandomPlayerbotMgr::CreateTempItem(item, 1, player);

    if (pItem)
    {
        InventoryResult result = player->CanEquipItem(slot, dest, pItem, true, false);

        pItem->RemoveFromUpdateQueueOf(player);

        if (!player->GetItemUpdateQueue().empty() && !player->GetItemUpdateQueue().back()) //Prevent queue overflow.
            player->GetItemUpdateQueue().pop_back();

        delete pItem;
        return result;
    }

    return EQUIP_ERR_ITEM_NOT_FOUND;
}

void RandomPlayerbotMgr::UpdateAIInternal(uint32 elapsed, bool minimal)
{
    // Phase H: the Rust `random_mgr` module now owns the entire random
    // bot tick loop (event cache, PID scaler, BG/LFG buckets, stats,
    // scheduling, process loop). `PlayerbotRust::WorldUpdate` forwards
    // this tick to the worker through `playerbot_random_mgr_update`;
    // the legacy body that used to live here has been retired and the
    // worker dispatches per-bot actions back to C++ through the bridge.
    PlayerbotRust::WorldUpdate(static_cast<uint32_t>(elapsed));

    // Reel in biting fishing bobbers every tick — the bite window is only a few
    // seconds, far shorter than a fishing bot's slow LOD AI tick, so the catch
    // is serviced here on the world thread instead of from the bot's own tick.
    BotBridge::PollFishing();

    // Activity telemetry: every ~60s, count what the bots are actually doing.
    // Lets a headless run be judged on real engagement (combat / movement)
    // rather than coarse DB position diffs (which undercount bots grinding in
    // place). Greppable prefix "[BotActivity]".
    static uint32 s_activityAccum = 0;
    s_activityAccum += elapsed;
    if (s_activityAccum >= 60000)
    {
        s_activityAccum = 0;
        uint32 bots = 0, inCombat = 0, moving = 0, dead = 0, mounted = 0;
        {
            HashMapHolder<Player>::ReadGuard g(HashMapHolder<Player>::GetLock());
            for (auto const& it : sObjectAccessor.GetPlayers())
            {
                Player* p = it.second;
                if (!p || !p->GetPlayerbotAI())
                    continue;
                ++bots;
                if (!p->IsAlive())
                    ++dead;
                else
                {
                    if (p->IsInCombat())
                        ++inCombat;
                    if (p->IsMoving())
                        ++moving;
                    if (p->IsMounted())
                        ++mounted;
                }
            }
        }
        extern std::atomic<uint64_t> g_botFishCasts;
        extern std::atomic<uint64_t> g_botFishCatches;
        extern std::atomic<uint64_t> g_botFishBobbers;
        sLog.outString(
            "[BotActivity] online=%u inCombat=%u moving=%u mounted=%u dead=%u fishCasts=%llu fishBobbers=%llu fishCatches=%llu",
            bots, inCombat, moving, mounted, dead,
            (unsigned long long)g_botFishCasts.load(std::memory_order_relaxed),
            (unsigned long long)g_botFishBobbers.load(std::memory_order_relaxed),
            (unsigned long long)g_botFishCatches.load(std::memory_order_relaxed));
    }

    SetAIInternalUpdateDelay(playerbot_config_random_bot_update_interval());
}




void RandomPlayerbotMgr::DatabasePing(QueryResult* result, uint32 pingStart, std::string db)
{
    sRandomPlayerbotMgr.SetDatabaseDelay(db, sWorld.GetCurrentMSTime() - pingStart);
    delete result;
}












void RandomPlayerbotMgr::SyncEventTimers()
{
    uint32 oldTime = GetValue(uint32(0), "current_time");
    if (oldTime)
    {
        uint32 curTime = time(nullptr);
        uint32 timeDiff = curTime - oldTime;
        CharacterDatabase.PExecute("UPDATE ai_playerbot_random_bots SET time = time + %u WHERE owner = 0 AND bot <> 0", timeDiff);
    }
}



void RandomPlayerbotMgr::ScheduleTeleport(uint32 bot, uint32 time)
{
    if (!time)
        time = 60 + urand(playerbot_config_random_bot_teleport_min_interval(), playerbot_config_random_bot_teleport_max_interval());
    SetEventValue(bot, "teleport", 1, time);
}


bool RandomPlayerbotMgr::AddRandomBot(uint32 bot)
{
    Player* player = GetPlayerBot(bot);
    if (player)
        return true;

    uint32 accountId = sObjectMgr.GetPlayerAccountIdByGUID(ObjectGuid(HIGHGUID_PLAYER, bot));

    if (!sPlayerbotAIConfig.IsInRandomAccountList(accountId))
    {
        sLog.outError("Bot #%d login fail: Not random bot!", bot);
        return false;
    }

    if (!GetEventValue(bot, "login"))
    {
        AddPlayerBot(bot, 0);
        SetEventValue(bot, "add", 1, urand(playerbot_config_min_random_bot_in_world_time(), playerbot_config_max_random_bot_in_world_time()));
        SetEventValue(bot, "logout", 0, 0);
        SetEventValue(bot, "login", 1, -1);
        uint32 randomTime = urand(playerbot_config_min_random_bot_revive_time(), playerbot_config_max_random_bot_revive_time());
        SetEventValue(bot, "update", 1, randomTime);
        currentBots.push_back(bot);
        sLog.outDetail("Random bot added #%d", bot);
    }

    return true;
}

void RandomPlayerbotMgr::MovePlayerBot(uint32 guid, PlayerbotHolder* newHolder)
{
    if (!playerbot_config_enabled())
        return;

    players[guid] = this->GetPlayerBot(guid);
    PlayerbotHolder::MovePlayerBot(guid, newHolder);
}



void RandomPlayerbotMgr::Revive(Player* player)
{
    uint32 bot = player->GetGUIDLow();

    //sLog.outString("Bot %d revived", bot);
    SetEventValue(bot, "dead", 0, 0);
    SetEventValue(bot, "revive", 0, 0);

    if (player->GetDeathState() == CORPSE)
    {
        RandomTeleport(player);
    }
    else
    {
        RandomTeleportForLevel(player, false);
    }
}

void RandomPlayerbotMgr::RandomTeleport(Player* bot, std::vector<WorldLocation> &locs, bool hearth, bool activeOnly)
{
    if (bot->IsBeingTeleported())
        return;

    if (bot->InBattleGround())
        return;

    if (bot->InBattleGroundQueue())
        return;

	if (bot->GetLevel() < 5)
		return;

    if (bot->GetGroup() && !bot->GetGroup()->IsLeader(bot->GetObjectGuid()))
        return;

    if (bot->IsTaxiFlying())
        return;

    if (locs.empty())
    {
        sLog.outError("Cannot teleport bot %s - no locations available", bot->GetName());
        return;
    }

    std::vector<WorldLocation> tlocs(locs);

    //Do not teleport to maps disabled in config
    tlocs.erase(std::remove_if(tlocs.begin(), tlocs.end(), [](const WorldLocation& l) { return !playerbot_config_is_random_bot_map(l.mapid); }), tlocs.end());

    //Random shuffle
    if (tlocs.size() > 1)
    {
        for (size_t i = tlocs.size() - 1; i > 0; --i)
            std::swap(tlocs[i], tlocs[urand(0, i)]);
    }

    // teleport to active areas only
    if (playerbot_config_random_bot_teleport_near_player() && activeOnly)
    {
        tlocs.erase(std::remove_if(tlocs.begin(), tlocs.end(), [this](const WorldLocation& l)
        {
            uint32 mapId = l.mapid;
            Map* tMap = sMapMgr.FindMap(mapId, 0);
            if (tMap && tMap->IsContinent() && tMap->HasActiveZones())
            {
                uint32 zoneId = sTerrainMgr.GetZoneId(mapId, l.coord_x, l.coord_y, l.coord_z);
                if (tMap->HasActiveZone(zoneId))
                {
                    if (playerbot_config_random_bot_teleport_near_player_max_amount() > 0 && playerbot_config_random_bot_teleport_near_player_max_amount_radius() > 0.0f)
                    {
                        uint32 botsNearTeleportPoint = 0;
                        float maxRadiusSq = playerbot_config_random_bot_teleport_near_player_max_amount_radius() * playerbot_config_random_bot_teleport_near_player_max_amount_radius();
                        ForEachPlayerbot([&](Player* otherBot)
                        {
                            if (otherBot && !otherBot->IsBeingTeleported() && zoneId == otherBot->GetZoneId())
                            {
                                float dx = l.coord_x - otherBot->GetPositionX();
                                float dy = l.coord_y - otherBot->GetPositionY();
                                if ((dx * dx + dy * dy) <= maxRadiusSq)
                                {
                                    botsNearTeleportPoint++;
                                }
                            }
                        });

                        return botsNearTeleportPoint >= playerbot_config_random_bot_teleport_near_player_max_amount();
                    }
                    else
                    {
                        return false;
                    }
                }
            }

            return true;
        }),
        tlocs.end());
    }

    // filter starter zones
    tlocs.erase(std::remove_if(tlocs.begin(), tlocs.end(), [bot](const WorldLocation& l)
    {
        uint32 mapId = l.mapid;
        uint32 zoneId, areaId;
        sTerrainMgr.GetZoneAndAreaId(zoneId, areaId, mapId, l.coord_x, l.coord_y, l.coord_z);
        AreaTableEntry const* area = GetAreaEntryByAreaID(areaId);
        if (zoneId && zoneId != areaId)
        {
            AreaTableEntry const* zone = GetAreaEntryByAreaID(zoneId);
            if (!zone)
                return true;

            bool isEnemyZone = false;
            switch (zone->team)
            {
            case AREATEAM_ALLY:
                isEnemyZone = bot->GetTeam() != ALLIANCE;
                break;
            case AREATEAM_HORDE:
                isEnemyZone = bot->GetTeam() != HORDE;
                break;
            default:
                isEnemyZone = false;
                break;
            }
            if (isEnemyZone && (bot->GetLevel() < 21 || (zone->flags & AREA_FLAG_CAPITAL)))
                return true;

            // filter other races zones
            if (bot->GetLevel() < 30)
            {
                if ((zoneId == 12 || zoneId == 40) && bot->getRace() != RACE_HUMAN)
                    return true;
                if ((zoneId == 1 || zoneId == 38) && bot->getRace() != RACE_DWARF)
                    return true;
                if ((zoneId == 85 || zoneId == 130) && bot->getRace() != RACE_UNDEAD)
                    return true;
                if ((zoneId == 141 || zoneId == 148) && bot->getRace() != RACE_NIGHTELF)
                    return true;
                if ((zoneId == 14 || zoneId == 17) && !(bot->getRace() == RACE_ORC || bot->getRace() == RACE_TROLL))
                    return true;
                if ((zoneId == 215) && bot->getRace() != RACE_TAUREN)
                    return true;
                // redridge / duskwood
                if ((zoneId == 44 || zoneId == 10) && bot->GetTeam() != ALLIANCE)
                    return true;
#ifndef MANGOSBOT_ZERO
                if ((zoneId == 3524 || zoneId == 3525) && bot->getRace() != RACE_DRAENEI)
                    return true;
                if ((zoneId == 3430 || zoneId == 3433) && bot->getRace() != RACE_BLOODELF)
                    return true;
#endif
            }
        }

        if (!area)
            return true;

        bool isEnemyZone = false;
        switch (area->team)
        {
        case AREATEAM_ALLY:
            isEnemyZone = bot->GetTeam() != ALLIANCE;
            break;
        case AREATEAM_HORDE:
            isEnemyZone = bot->GetTeam() != HORDE;
            break;
        default:
            isEnemyZone = false;
            break;
        }
        return isEnemyZone && bot->GetLevel() < 21;

    }), tlocs.end());

    if (tlocs.empty())
    {
        if (activeOnly)
        {
            if (hearth)
                return RandomTeleportForRpg(bot, false);
            else
                return RandomTeleportForLevel(bot, false);
        }

        sLog.outError("Cannot teleport bot %s - no locations available", bot->GetName());

        return;
    }

    int index = 0;

    for (int i = 0; i < tlocs.size(); i++)
    {
        for (int attemtps = 0; attemtps < 3; ++attemtps)
        {
            WorldLocation loc = tlocs[i];

#ifdef MANGOSBOT_ONE
            // Teleport to Dark Portal area if event is in progress
            if (sWorldState.GetExpansion() == EXPANSION_NONE && bot->GetLevel() > 54 && urand(0, 100) > 20)
            {
                if (urand(0, 1))
                    loc = WorldLocation(uint32(0), -11772.43f, -3272.84f, -17.9f, 3.32447f);
                else
                    loc = WorldLocation(uint32(0), -11741.70f, -3130.3f, -11.7936f, 3.32447f);
            }
#endif

            float x = loc.coord_x + (attemtps > 0 ? urand(0, playerbot_config_grind_distance()) - playerbot_config_grind_distance() / 2 : 0);
            float y = loc.coord_y + (attemtps > 0 ? urand(0, playerbot_config_grind_distance()) - playerbot_config_grind_distance() / 2 : 0);
            float z = loc.coord_z;

            Map* map = sMapMgr.FindMap(loc.mapid, 0);
            if (!map)
                continue;

            uint32 areaId = sTerrainMgr.GetAreaId(loc.mapid, x, y, z);
            AreaTableEntry const* area = GetAreaEntryByAreaID(areaId);
            if (!area)
                continue;

#ifndef MANGOSBOT_ZERO
            // Do not teleport to outland before portal opening (allow new races zones)
            if (sWorldState.GetExpansion() == EXPANSION_NONE && (loc.mapid == 571 || (loc.mapid == 530 && area->team != 2 && area->team != 4)))
                continue;
#endif

#ifdef MANGOSBOT_TWO
            float ground = map->GetHeight(bot->GetPhaseMask(), x, y, z + 0.5f);
#else
            float ground = map->GetHeight(x, y, z + 0.5f);
#endif
            if (ground <= INVALID_HEIGHT)
                continue;

            z = 0.05f + ground;
            sLog.outDetail("Random teleporting bot %s to %s %f,%f,%f (%u/%zu locations)",
                bot->GetName(), area->area_name[0], x, y, z, attemtps, tlocs.size());

            if (bot->IsTaxiFlying())
                bot->GetMotionMaster()->MovementExpired();

            if (hearth)
                bot->SetHomebindToLocation(loc, area->ID);

            bot->GetMotionMaster()->Clear();
            bot->TeleportTo(loc.mapid, x, y, z, 0);
            bot->SendHeartBeat();

            if (bot->GetGroup())
            {
                for (GroupReference* gref = bot->GetGroup()->GetFirstMember(); gref; gref = gref->next())
                {
                    Player* member = gref->getSource();
                    if (bot != member)
                    {
                        if (member->IsTaxiFlying())
                            member->GetMotionMaster()->MovementExpired();
                        if (hearth)
                            member->SetHomebindToLocation(loc, area->ID);

                        member->GetMotionMaster()->Clear();
                        member->TeleportTo(loc.mapid, x, y, z, 0);
                        member->SendHeartBeat();
                    }

                }
            }
            return;
        }
    }

    sLog.outError("Cannot teleport bot %s - no locations available", bot->GetName());
}

static std::string GetAreaNameForLocation(const WorldLocation& loc)
{
    uint32 areaId = sTerrainMgr.GetAreaId(loc.mapid, loc.coord_x, loc.coord_y, loc.coord_z);
    AreaTableEntry const* area = GetAreaEntryByAreaID(areaId);
    if (area)
        return area->area_name[0];
    return "";
}

std::vector<std::pair<uint32, uint32>> RandomPlayerbotMgr::RpgLocationsNear(const WorldLocation pos, const std::map<uint32, std::map<uint32, std::vector<std::string>>>& areaNames, uint32 radius)
{
    std::vector<std::pair<uint32, uint32>> results;
    float minDist = FLT_MAX;
    std::string hasZone = "-", wantZone = GetAreaNameForLocation(pos);

    for (uint32 level = 1; level < playerbot_config_random_bot_max_level() + 1; level++)
    {
        for (uint32 r = 1; r < MAX_RACES; r++)
        {
            uint32 i = 0;
            for (auto p : rpgLocsCacheLevel[r][level])
            {
                std::string currentZone = areaNames.at(level).at(r)[i];
                i++;

                if (currentZone != wantZone && hasZone == wantZone)
                    continue;

                if (currentZone == wantZone && hasZone != wantZone)
                    minDist = FLT_MAX;

                float dx = pos.coord_x - p.coord_x;
                float dy = pos.coord_y - p.coord_y;
                float dz = pos.coord_z - p.coord_z;
                float dist = sqrt(dx * dx + dy * dy + dz * dz);

                if (dist > radius || dist > minDist)
                    continue;

                if (dist < minDist)
                    results.clear();

                results.push_back(std::make_pair(r, level));

                hasZone = currentZone;

                minDist = dist;
            }
        }
    }

    return results;
}

void RandomPlayerbotMgr::PrepareTeleportCache()
{
    uint32 maxLevel = playerbot_config_random_bot_max_level();
    if (maxLevel > sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL))
        maxLevel = sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL);

    auto results = CharacterDatabase.PQuery("SELECT `map_id`, `x`, `y`, `z`, `level` FROM `ai_playerbot_tele_cache`");
    if (results)
    {
        sLog.outString("Loading random teleport caches for %d levels...", maxLevel);
        do
        {
            Field* fields = results->Fetch();
            uint16 mapId = fields[0].GetUInt16();
            float x = fields[1].GetFloat();
            float y = fields[2].GetFloat();
            float z = fields[3].GetFloat();
            uint16 level = fields[4].GetUInt16();
            WorldLocation loc(mapId, x, y, z, 0);
            locsPerLevelCache[level].push_back(loc);
        } while (results->NextRow());
    }
    else
    {
        sLog.outString("Preparing random teleport caches for %d levels...", maxLevel);
        char* mapsRaw = playerbot_config_random_bot_maps_as_string();
        std::string randomBotMaps = mapsRaw ? mapsRaw : "";
        playerbot_config_free_cstr(mapsRaw);
        BarGoLink bar(maxLevel);
        for (uint8 level = 1; level <= maxLevel; level++)
        {
            auto results = WorldDatabase.PQuery("SELECT `map`, `position_x`, `position_y`, `position_z` "
                "FROM (SELECT `map`, `position_x`, `position_y`, `position_z`, t.maxlevel, t.minlevel, "
                "%u - (t.maxlevel + t.minlevel) / 2 delta "
                "FROM creature c INNER JOIN creature_template t ON c.id = t.entry WHERE t.CreatureType != 8 AND t.NpcFlags = 0 AND t.Rank = 0 AND NOT (t.extraFlags & 1024 OR t.extraFlags & 65536 OR t.extraflags & 64 OR t.unitFlags & 256 OR t.unitFlags & 512) AND t.lootid != 0) q "
                "WHERE delta >= 0 AND delta <= %u AND map in (%s)",
                level,
                playerbot_config_random_bot_tele_level(),
                randomBotMaps.c_str()
            );
            if (results)
            {
                CharacterDatabase.BeginTransaction();
                do
                {
                    Field* fields = results->Fetch();
                    uint16 mapId = fields[0].GetUInt16();
                    float x = fields[1].GetFloat();
                    float y = fields[2].GetFloat();
                    float z = fields[3].GetFloat();
                    WorldLocation loc(mapId, x, y, z, 0);
                    locsPerLevelCache[level].push_back(loc);

                    CharacterDatabase.PExecute("INSERT INTO `ai_playerbot_tele_cache` (`level`, `map_id`, `x`, `y`, `z`) VALUES (%u, %u, %f, %f, %f)",
                        level, mapId, x, y, z);
                } while (results->NextRow());
                CharacterDatabase.CommitTransaction();
            }
            bar.step();
        }
    }

    sLog.outString("Preparing RPG teleport caches for %d factions...", sFactionTemplateStore.GetNumRows());

    results = WorldDatabase.PQuery("SELECT map, position_x, position_y, position_z, "
        "r.race, r.minl, r.maxl "
        "FROM creature c INNER JOIN ai_playerbot_rpg_races r ON c.id = r.entry "
        "WHERE r.race < 15");

    if (results)
    {
        do
        {
            for (uint32 level = 1; level < playerbot_config_random_bot_max_level() + 1; level++)
            {
                Field* fields = results->Fetch();
                uint16 mapId = fields[0].GetUInt16();
                float x = fields[1].GetFloat();
                float y = fields[2].GetFloat();
                float z = fields[3].GetFloat();
                //uint32 faction = fields[4].GetUInt32();
                //string name = fields[5].GetCppString();
                uint32 race = fields[4].GetUInt32();
                uint32 minl = fields[5].GetUInt32();
                uint32 maxl = fields[6].GetUInt32();

                if (level > maxl || level < minl) continue;

                WorldLocation loc(mapId, x, y, z, 0);
                for (uint32 r = 1; r < MAX_RACES; r++)
                {
                    if (race == r || race == 0) rpgLocsCacheLevel[r][level].push_back(loc);
                }
            }
            //bar.step();
        } while (results->NextRow());
    }

    sLog.outString("Enhancing RPG teleport cache");

    std::map<uint32, std::map<uint32, std::vector<std::string>>> areaNames;

    for (uint32 level = 1; level < playerbot_config_random_bot_max_level() + 1; level++)
    {
        for (uint32 r = 1; r < MAX_RACES; r++)
        {
            for (auto p : rpgLocsCacheLevel[r][level])
            {
                areaNames[level][r].push_back(GetAreaNameForLocation(p));
            }
        }
    }

    std::vector<std::pair<std::pair<uint32, uint32>, WorldLocation>> newPoints;
    std::vector<std::pair<std::pair<uint32, uint32>, WorldLocation>> innPoints;

    //Static portals.
    for (uint32 entry = 1; entry < sGOStorage.GetMaxEntry(); ++entry)
    {
        GameObjectInfo const* data = sGOStorage.LookupEntry<GameObjectInfo>(entry);
        if (!data)
            continue;

        if (data->type != GAMEOBJECT_TYPE_SPELLCASTER)
            continue;

        const SpellEntry* pSpellInfo = sSpellTemplate.LookupEntry<SpellEntry>(data->spellcaster.spellId);
        if (!pSpellInfo)
            continue;

        if (pSpellInfo->EffectTriggerSpell[0])
            pSpellInfo = sSpellTemplate.LookupEntry<SpellEntry>(pSpellInfo->EffectTriggerSpell[0]);

        if (!pSpellInfo)
            continue;

        if (pSpellInfo->Effect[0] != SPELL_EFFECT_TELEPORT_UNITS && pSpellInfo->Effect[1] != SPELL_EFFECT_TELEPORT_UNITS && pSpellInfo->Effect[2] != SPELL_EFFECT_TELEPORT_UNITS)
            continue;

        SpellTargetPosition const* pos = sSpellMgr.GetSpellTargetPosition(pSpellInfo->Id);

        if (!pos)
            continue;

        WorldLocation portalLoc(pos->target_mapId, pos->target_X, pos->target_Y, pos->target_Z, 0);
        std::vector<std::pair<uint32, uint32>> ranges = RpgLocationsNear(portalLoc, areaNames);

        for (auto& range : ranges)
            newPoints.push_back(std::make_pair(std::make_pair(range.first, range.second), portalLoc));
    }

    //Creatures.
    auto creatureWorker = [&](CreatureDataPair const& dataPair) -> bool
    {
        CreatureInfo const* cInfo = ObjectMgr::GetCreatureTemplate(dataPair.second.id);

        if (!cInfo)
            return false;

        if (cInfo->ExtraFlags & CREATURE_EXTRA_FLAG_INVISIBLE)
            return false;

        static const uint32 allowedNpcFlags[] = {
            UNIT_NPC_FLAG_BATTLEMASTER,
            UNIT_NPC_FLAG_BANKER,
            UNIT_NPC_FLAG_AUCTIONEER,
            UNIT_NPC_FLAG_TRAINER,
            UNIT_NPC_FLAG_VENDOR,
            UNIT_NPC_FLAG_REPAIR,
            UNIT_NPC_FLAG_INNKEEPER
        };

        for (auto flag : allowedNpcFlags)
        {
            if ((cInfo->NpcFlags & flag) != 0)
            {
                WorldLocation creatureLoc(dataPair.second.mapid, dataPair.second.posX, dataPair.second.posY, dataPair.second.posZ, 0);
                std::vector<std::pair<uint32, uint32>> ranges = RpgLocationsNear(creatureLoc, areaNames);

                if (cInfo->NpcFlags & UNIT_NPC_FLAG_INNKEEPER)
                {
                    for (auto& range : ranges)
                        innPoints.push_back(std::make_pair(std::make_pair(range.first, range.second), creatureLoc));
                }
                else
                {
                    for (auto& range : ranges)
                        newPoints.push_back(std::make_pair(std::make_pair(range.first, range.second), creatureLoc));
                }
                break;
            }
        }
        return false;
    };
    sObjectMgr.DoCreatureData(creatureWorker);

    for (auto newPoint : newPoints)
        rpgLocsCacheLevel[newPoint.first.first][newPoint.first.second].push_back(newPoint.second);

    for (auto innPoint : innPoints)
        innCacheLevel[innPoint.first.first][innPoint.first.second].push_back(std::make_pair(ObjectGuid(), innPoint.second));
}


void RandomPlayerbotMgr::RandomTeleportForLevel(Player* bot, bool activeOnly)
{
    if (bot->InBattleGround())
        return;

    sLog.outDetail("Preparing location to random teleporting bot %s for level %u", bot->GetName(), bot->GetLevel());
    RandomTeleport(bot, locsPerLevelCache[bot->GetLevel()], false, activeOnly);
    Refresh(bot);

    float botX = bot->GetPositionX(), botY = bot->GetPositionY(), botZ = bot->GetPositionZ();

    ObjectGuid closestInn;
    float minDistance = -1.0f;
    for (auto& [innGuid, innPosition] : innCacheLevel[bot->getRace()][bot->GetLevel()])
    {
        float dx = botX - innPosition.coord_x;
        float dy = botY - innPosition.coord_y;
        float dz = botZ - innPosition.coord_z;
        float distance = dx * dx + dy * dy + dz * dz;
        if (minDistance > 0 || distance >= minDistance)
            continue;

        minDistance = distance;
        closestInn = innGuid;
    }

    if (closestInn)
    {
        WorldPacket data(SMSG_TRAINER_BUY_SUCCEEDED, (8 + 4));
        data << closestInn;
        data << uint32(3286);                                   // Bind
        bot->GetSession()->SendPacket(data);
    }
}

void RandomPlayerbotMgr::RandomTeleport(Player* bot)
{
    if (bot->InBattleGround())
        return;

    // Simply teleport to a level-appropriate location
    RandomTeleportForLevel(bot, true);
    Refresh(bot);
}

void RandomPlayerbotMgr::InstaRandomize(Player* bot)
{
    sRandomPlayerbotMgr.Randomize(bot);

    if(bot->GetLevel() > sWorld.getConfig(CONFIG_UINT32_START_PLAYER_LEVEL))
        sRandomPlayerbotMgr.RandomTeleportForLevel(bot, false);
}

void RandomPlayerbotMgr::Randomize(Player* bot)
{
    if (!bot || !bot->IsInWorld() || bot->IsBeingTeleported() || bot->GetSession()->isLogingOut())
        return;

    bool initialRandom = false;
    if (bot->GetLevel() <= playerbot_config_randombot_starting_level())
        initialRandom = true;
#ifdef MANGOSBOT_TWO
    else if (bot->GetLevel() < 60 && bot->getClass() == CLASS_DEATH_KNIGHT)
        initialRandom = true;
#endif

    // give bot random level if is above or below level sync
    if (!initialRandom && players.size() && playerbot_config_sync_level_with_players())
    {
        uint32 maxLevel = std::max(playerbot_config_random_bot_min_level(), std::min(playersLevel + playerbot_config_sync_level_max_above(), sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL)));
        if (bot->GetLevel() > maxLevel || (bot->GetLevel() + playerbot_config_sync_level_max_above()) < playersLevel)
            initialRandom = true;
    }

    if (initialRandom)
    {
        RandomizeFirst(bot);
        sLog.outDetail("Bot #%d %s:%d <%s>: gear/level randomised", bot->GetGUIDLow(), bot->GetTeam() == ALLIANCE ? "A" : "H", bot->GetLevel(), bot->GetName());
    }
    else if (playerbot_config_random_gear_upgrade_enabled())
    {
        UpdateGearSpells(bot);
        sLog.outDetail("Bot #%d %s:%d <%s>: gear upgraded", bot->GetGUIDLow(), bot->GetTeam() == ALLIANCE ? "A" : "H", bot->GetLevel(), bot->GetName());
    }
    else
    {
        // schedule randomise
        uint32 randomTime = urand(playerbot_config_min_random_bot_randomize_time(), playerbot_config_max_random_bot_randomize_time());
        SetEventValue(bot->GetGUIDLow(), "randomize", 1, randomTime);
    }

    //SetValue(bot, "version", MANGOSBOT_VERSION);
}

void RandomPlayerbotMgr::UpdateGearSpells(Player* bot)
{
    uint32 maxLevel = playerbot_config_random_bot_max_level();
    if (maxLevel > sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL))
        maxLevel = sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL);

    uint32 lastLevel = GetValue(bot, "level");
    uint32 level = bot->GetLevel();
    if (PlayerbotRust* ai = bot->GetPlayerbotAI())
        ai->FactoryRandomizeViaRust(level, /*incremental*/ true, /*sync*/ false, 0);

    if (lastLevel != level)
        SetValue(bot, "level", level);

    // schedule randomise
    uint32 randomTime = urand(playerbot_config_min_random_bot_randomize_time(), playerbot_config_max_random_bot_randomize_time());
    SetEventValue(bot->GetGUIDLow(), "randomize", 1, randomTime);
}

void RandomPlayerbotMgr::RandomizeFirst(Player* bot)
{
    uint32 maxLevel = playerbot_config_random_bot_max_level();
    if (maxLevel > sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL))
        maxLevel = sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL);

    // if lvl sync is enabled, max level is limited by online players lvl
    if (playerbot_config_sync_level_with_players())
        maxLevel = std::max(playerbot_config_random_bot_min_level(), std::min(playersLevel+ playerbot_config_sync_level_max_above(), sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL)));

    uint32 level = urand(std::max(uint32(sWorld.getConfig(CONFIG_UINT32_START_PLAYER_LEVEL)), playerbot_config_random_bot_min_level()), maxLevel);

#ifdef MANGOSBOT_TWO
    if (bot->getClass() == CLASS_DEATH_KNIGHT)
        level = urand(std::max(bot->GetLevel(), sWorld.getConfig(CONFIG_UINT32_START_HEROIC_PLAYER_LEVEL)), std::max(sWorld.getConfig(CONFIG_UINT32_START_HEROIC_PLAYER_LEVEL), maxLevel));
#endif

    if (urand(0, 100) < 100 * playerbot_config_random_bot_max_level_chance() && level < maxLevel)
        level = maxLevel;

#ifndef MANGOSBOT_ZERO
    if (sWorldState.GetExpansion() == EXPANSION_NONE && level > 60)
        level = 60;
#endif

#ifdef MANGOSBOT_TWO
    // do not allow level down death knights
    if (bot->getClass() == CLASS_DEATH_KNIGHT && level < sWorld.getConfig(CONFIG_UINT32_START_HEROIC_PLAYER_LEVEL))
        return;

    // only randomise death knights to min lvl 60
    if (bot->getClass() == CLASS_DEATH_KNIGHT && level < 60)
        level = 60;
#endif

    if (level == sWorld.getConfig(CONFIG_UINT32_START_PLAYER_LEVEL))
        return;

    SetValue(bot, "level", level);
    if (PlayerbotRust* ai = bot->GetPlayerbotAI())
        ai->FactoryRandomizeViaRust(level, /*incremental*/ false, /*sync*/ false, 0);

    // schedule randomise
    uint32 randomTime = urand(playerbot_config_min_random_bot_randomize_time(), playerbot_config_max_random_bot_randomize_time());
    SetEventValue(bot->GetGUIDLow(), "randomize", 1, randomTime);

    if (bot->GetGroup())
        bot->RemoveFromGroup();
}


void RandomPlayerbotMgr::Refresh(Player* bot)
{
    if (bot->IsBeingTeleportedFar() || !bot->IsInWorld())
        return;

    if (!bot->IsAlive())
    {
        bot->ResurrectPlayer(1.0f);
        bot->SpawnCorpseBones();
    }

    if (playerbot_config_disable_random_levels())
        return;

    if (bot->InBattleGround())
        return;

    sLog.outDetail("Refreshing bot #%d <%s>", bot->GetGUIDLow(), bot->GetName());

    bot->DurabilityRepairAll(false, 1.0f
#ifndef MANGOSBOT_ZERO
        , false
#endif
    );
	bot->SetHealthPercent(100);
	bot->SetPvP(true);

    if (PlayerbotRust* ai = bot->GetPlayerbotAI())
        ai->FactoryRefreshViaRust();

    if (bot->GetMaxPower(POWER_MANA) > 0)
        bot->SetPower(POWER_MANA, bot->GetMaxPower(POWER_MANA));

    if (bot->GetMaxPower(POWER_ENERGY) > 0)
        bot->SetPower(POWER_ENERGY, bot->GetMaxPower(POWER_ENERGY));

    uint32 money = bot->GetMoney();
    bot->SetMoney(money + 500 * sqrt(urand(1, bot->GetLevel() * 5)));
}

bool RandomPlayerbotMgr::IsRandomBot(Player* bot)
{
    if (bot)
    {
        // Free alt bots (player-owned) are not random bots
        if (sPlayerbotAIConfig.IsFreeAltBot(bot))
            return false;

        if (sPlayerbotAIConfig.IsInRandomAccountList(bot->GetSession()->GetAccountId()))
            return true;

        return IsRandomBot(bot->GetGUIDLow());
    }

    return false;
}

bool RandomPlayerbotMgr::IsRandomBot(uint32 bot)
{
    ObjectGuid guid = ObjectGuid(HIGHGUID_PLAYER, bot);
    if (sPlayerbotAIConfig.IsInRandomAccountList(sObjectMgr.GetPlayerAccountIdByGUID(guid)))
        return true;

    return GetEventValue(bot, "add");
}

std::list<uint32> RandomPlayerbotMgr::GetBots()
{
    if (!currentBots.empty()) return currentBots;

    auto results = CharacterDatabase.Query(
            "SELECT bot FROM ai_playerbot_random_bots WHERE owner = 0 AND event = 'add'");

    if (results)
    {
        do
        {
            Field* fields = results->Fetch();
            uint32 bot = fields[0].GetUInt32();
            currentBots.push_back(bot);
        } while (results->NextRow());
    }

    return currentBots;
}


// The event KV store (DB-backed cache) was ported to Rust in Phase H
// (`crates/playerbot/src/random_mgr/events.rs`). These C++ methods are
// now thin FFI forwarders into that single source of truth — there is
// no C++-side `eventCache` or DB query duplication.
uint32 RandomPlayerbotMgr::GetEventValue(uint32 bot, std::string event)
{
    return playerbot_random_mgr_get_value(bot, event.c_str());
}

uint32 RandomPlayerbotMgr::SetEventValue(uint32 bot, std::string event, uint32 value, uint32 validIn, std::string data)
{
    playerbot_random_mgr_set_event_value(bot, event.c_str(), value, validIn, data.c_str());
    return value;
}

uint32 RandomPlayerbotMgr::GetValue(uint32 bot, std::string type)
{
    return GetEventValue(bot, type);
}

uint32 RandomPlayerbotMgr::GetValue(Player* bot, std::string type)
{
    return GetValue(bot->GetObjectGuid().GetCounter(), type);
}

void RandomPlayerbotMgr::SetValue(uint32 bot, std::string type, uint32 value, std::string data, int32 validIn)
{
    SetEventValue(bot, type, value, validIn == -1 ? 15*24*3600 : validIn, data);
}

void RandomPlayerbotMgr::SetValue(Player* bot, std::string type, uint32 value, std::string data, int32 validIn)
{
    SetValue(bot->GetObjectGuid().GetCounter(), type, value, data, validIn);
}

bool RandomPlayerbotMgr::HandlePlayerbotConsoleCommand(ChatHandler* handler, char const* args)
{
    if (!playerbot_config_enabled())
    {
        sLog.outError("Playerbot system is currently disabled!");
        return false;
    }

    bool isRA = false;

    if (handler->GetSession()) //Client command
        isRA = true;
    else if (static_cast<CliHandler*>(handler) && static_cast<CliHandler*>(handler)->GetAccountId()) //RA call with account.
        isRA = true;

    // Parsing + dispatch live in Rust (random_mgr::commands). Forward the
    // raw command; Rust returns the output lines and a bitset of the
    // side effects only C++ can perform.
    const char* text = (args && *args) ? args : "help";
    uint32 flags = 0;
    char* out = playerbot_random_mgr_console_command(text, &flags);

    if (out)
    {
        std::stringstream ss(out);
        playerbot_free_string(out);
        std::string line;
        while (std::getline(ss, line))
        {
            sLog.outString("%s", line.c_str());
            if (isRA)
                handler->SendSysMessage(line.c_str());
        }

        if (flags & PLAYERBOT_CONSOLE_FLAG_UPDATE_TICK)
            sRandomPlayerbotMgr.UpdateAIInternal(0);
        if (flags & PLAYERBOT_CONSOLE_FLAG_LOGIN_DEBUG)
            playerbot_login_toggle_debug();
        if (flags & PLAYERBOT_CONSOLE_FLAG_CLEAN_MAP)
        {
            for (uint32 i = 0; i < sMapStore.GetNumRows(); ++i)
            {
                if (!sMapStore.LookupEntry(i))
                    continue;
                uint32 mapId = sMapStore.LookupEntry(i)->MapID;
                boost::thread t([mapId]() { WorldPosition::unloadMapAndVMaps(mapId); });
                t.detach();
            }
        }
        return true;
    }

    // Not a random-mgr command — fall through to the holder commands.
    std::list<std::string> messages = sRandomPlayerbotMgr.HandlePlayerbotCommand(args, NULL, static_cast<CliHandler*>(handler) ? static_cast<CliHandler*>(handler)->GetAccessLevel() : SEC_PLAYER);
    for (std::list<std::string>::iterator i = messages.begin(); i != messages.end(); ++i)
    {
        sLog.outString("%s", i->c_str());
        if (isRA)
            handler->SendSysMessage(i->c_str());
    }

    if (!messages.empty())
        return true;

    if (isRA)
        handler->SendSysMessage("usage: help/list/reload/more.. or add/init/remove/more.. PLAYERNAME");

    return true;
}

void RandomPlayerbotMgr::HandleCommand(uint32 type, const std::string& text, Player& fromPlayer, std::string channelName, Team team, uint32 lang)
{
    ForEachPlayerbot([&](Player* bot)
    {
        if (type == CHAT_MSG_SAY)
        {
            if (bot->GetMapId() != fromPlayer.GetMapId() || bot->GetDistance2d(&fromPlayer) > 25)
            {
                return;
            }
        }

        if (type == CHAT_MSG_YELL)
        {
            if (bot->GetMapId() != fromPlayer.GetMapId() || bot->GetDistance2d(&fromPlayer) > 300)
            {
                return;
            }
        }

        if (team != TEAM_BOTH_ALLOWED && bot->GetTeam() != team)
        {
            return;
        }

        if (type == CHAT_MSG_GUILD && bot->GetGuildId() != fromPlayer.GetGuildId())
        {
            return;
        }

        if (!channelName.empty())
        {
            if (ChannelMgr* cMgr = channelMgr(bot->GetTeam()))
            {
                Channel* chn = cMgr->GetChannel(channelName, bot);
                if (!chn)
                {
                    return;
                }
            }
        }

        bot->GetPlayerbotAI()->HandleCommand(type, text, fromPlayer, lang);
    });
}

void RandomPlayerbotMgr::OnPlayerLogout(Player* player)
{
    bool hadPlayerBot = GetPlayerBot(player->GetGUIDLow());

    DisablePlayerBot(player->GetGUIDLow());

    if (!hadPlayerBot && player->GetPlayerbotAI() && player->GetGroup() && sPlayerbotAIConfig.IsFreeAltBot(player))
        player->GetSession()->SetOffline(); //Prevent groupkick

    players.erase(player->GetGUIDLow());
}

void RandomPlayerbotMgr::OnBotLoginInternal(Player * const bot)
{
    sLog.outDetail("%u/%d Bot %s logged in", GetPlayerbotsAmount(), sRandomPlayerbotMgr.GetMaxAllowedBotCount(), bot->GetName());
	//if (loginProgressBar && playerBots.size() < sRandomPlayerbotMgr.GetMaxAllowedBotCount()) { loginProgressBar->step(); }
	//if (loginProgressBar && playerBots.size() >= sRandomPlayerbotMgr.GetMaxAllowedBotCount() - 1) {
    //if (loginProgressBar && playerBots.size() + 1 >= sRandomPlayerbotMgr.GetMaxAllowedBotCount()) {
	//	sLog.outString("All bots logged in");
    //    delete loginProgressBar;
	//}
}

void RandomPlayerbotMgr::OnPlayerLogin(Player* player)
{
    if (!playerbot_config_enabled())
        return;

    // Master/strategy management is no longer available on PlayerbotRust

    if (IsFreeBot(player))
    {
        uint32 guid = player->GetGUIDLow();
        if (!sPlayerbotAIConfig.IsFreeAltBot(player))
           SetEventValue(guid, "login", 0, 0);
    }
    else
    {
        players[player->GetGUIDLow()] = player;
        sLog.outDebug("Including non-random bot player %s into random bot update", player->GetName());
    }
}

void RandomPlayerbotMgr::OnPlayerLoginError(uint32 bot)
{
    SetEventValue(bot, "add", 0, 0);
    SetEventValue(bot, "login", 0, 0);
    currentBots.remove(bot);
}


Player* RandomPlayerbotMgr::GetPlayer(uint32 playerGuid)
{
    PlayerBotMap::const_iterator it = players.find(playerGuid);
    return (it == players.end()) ? nullptr : it->second ? it->second : nullptr;
}








void RandomPlayerbotMgr::ChangeStrategy(Player* player)
{
    uint32 bot = player->GetGUIDLow();

    if (urand(0, 100) > 100 * playerbot_config_random_bot_rpg_chance()) // select grind / pvp
    {
        sLog.outDetail("Bot #%d %s:%d <%s>: sent to grind spot", bot, player->GetTeam() == ALLIANCE ? "A" : "H", player->GetLevel(), player->GetName());
        // teleport in different places only if players are online
        RandomTeleportForLevel(player, players.size());
        ScheduleTeleport(bot);
    }
    else
    {
        sLog.outDetail("Bot #%d %s:%d <%s>: sent to inn", bot, player->GetTeam() == ALLIANCE ? "A" : "H", player->GetLevel(), player->GetName());
        RandomTeleportForRpg(player, players.size());
        ScheduleTeleport(bot);
    }
}

void RandomPlayerbotMgr::RandomTeleportForRpg(Player* bot, bool activeOnly)
{
    uint32 race = bot->getRace();
    uint32 level = bot->GetLevel();
    sLog.outDetail("Random teleporting bot %s for RPG (%zu locations available)", bot->GetName(), rpgLocsCacheLevel[race][level].size());
    RandomTeleport(bot, rpgLocsCacheLevel[race][level], true, activeOnly);
    Refresh(bot);

    // Travel system removed — cooldown handled in Rust.
}

void RandomPlayerbotMgr::Remove(Player* bot)
{
    uint32 owner = bot->GetGUIDLow();
    CharacterDatabase.PExecute("DELETE FROM ai_playerbot_random_bots WHERE owner = 0 AND bot = '%d'", owner);
    playerbot_random_mgr_drop_bot_events(owner);

    LogoutPlayerBot(owner);
}





