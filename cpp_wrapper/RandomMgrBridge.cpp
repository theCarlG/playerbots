/**
 * RandomMgrBridge.cpp — CMaNGOS-side implementation of `RandomMgrCallbacks`.
 *
 * See the header for the division of labour: the Rust `random_mgr`
 * module owns the random-bot tick loop, event cache, scheduler, teleport
 * cache, bucket matrices, and stats computation; this file is the narrow
 * surface where those pieces still need to reach CMaNGOS for SQL, live
 * object state, or per-bot action dispatch.
 *
 * Threading: the Rust random-mgr worker thread drives every `CB_*`
 * callback except the `dispatch_*` family — those are routed from the
 * worker back to the main world thread via the Rust command channel,
 * so by the time they fire here we are under the CMaNGOS world lock.
 */

#include "botpch.h"
#include "RandomMgrBridge.h"

#include <cstdlib>
#include <cstring>
#include <ctime>
#include <string>
#include <vector>

#include "BotConfig.h"
#include "AuctionHouse/AuctionHouseMgr.h"
#include "BattleGround/BattleGround.h"
#include "BattleGround/BattleGroundMgr.h"
#include "Database/DatabaseEnv.h"
#include "Entities/Player.h"
#include "Globals/ObjectAccessor.h"
#include "Globals/ObjectMgr.h"
#include "Groups/Group.h"
#include "Log/Log.h"
#include "World/World.h"

#include "playerbot/PlayerbotAI.h"
#include "RandomPlayerbotMgr.h"

// ── Small helpers ────────────────────────────────────────────────────────

namespace {

/// Copy `src` into a malloc'd array the Rust side will free via the
/// matching `free_*` callback. Returns the element count (which is
/// `src.size()` on success, 0 on malloc failure / empty input).
template <typename T>
uint32_t HandOffVector(const std::vector<T>& src, T** out_rows)
{
    if (!out_rows)
        return 0;
    *out_rows = nullptr;
    if (src.empty())
        return 0;

    const size_t bytes = src.size() * sizeof(T);
    auto* buf = static_cast<T*>(std::malloc(bytes));
    if (!buf)
        return 0;
    std::memcpy(buf, src.data(), bytes);
    *out_rows = buf;
    return static_cast<uint32_t>(src.size());
}

/// Copy a std::string into a fixed-size char buffer (null-padded).
/// Used to marshal DB strings into PoD structs the FFI exports.
void CopyFixedString(char* dst, size_t dst_len, const std::string& src)
{
    if (!dst || dst_len == 0) return;
    const size_t n = std::min(src.size(), dst_len - 1);
    std::memcpy(dst, src.data(), n);
    dst[n] = '\0';
}

// ── Free functions — used as the pattern `free_*_list(ptr, count)`. ──
// Every query callback returns a pointer the Rust side must release via
// one of these. They all resolve to std::free.
template <typename T>
void FreeHandOff(T* rows, uint32_t /*count*/)
{
    std::free(rows);
}

// ── SQL helpers for the event KV store ────────────────────────────────
//
// The legacy class held a full eventCache map; during Phase H transition
// we let both C++ and Rust hold independent caches and hit the DB
// directly from the bridge. Task #9 removes the C++ cache entirely.

void UpsertEventRow(uint32_t bot, const char* event, uint32_t value,
                    uint32_t last_change_time, uint32_t valid_in,
                    const char* data)
{
    if (!event) return;

    CharacterDatabase.PExecute(
        "DELETE FROM ai_playerbot_random_bots WHERE owner = 0 AND bot = '%u' "
        "AND event = '%s'", bot, event);

    if (value == 0)
        return;

    if (data && data[0])
    {
        CharacterDatabase.PExecute(
            "INSERT INTO ai_playerbot_random_bots "
            "(owner, bot, `time`, validIn, event, `value`, `data`) "
            "VALUES ('%u', '%u', '%u', '%u', '%s', '%u', '%s')",
            0u, bot, last_change_time, valid_in, event, value, data);
    }
    else
    {
        CharacterDatabase.PExecute(
            "INSERT INTO ai_playerbot_random_bots "
            "(owner, bot, `time`, validIn, event, `value`) "
            "VALUES ('%u', '%u', '%u', '%u', '%s', '%u')",
            0u, bot, last_change_time, valid_in, event, value);
    }
}

void DeleteEventRow(uint32_t bot, const char* event)
{
    if (!event) return;
    CharacterDatabase.PExecute(
        "DELETE FROM ai_playerbot_random_bots WHERE owner = 0 AND bot = '%u' "
        "AND event = '%s'", bot, event);
}

void DeleteAllEventsForBot(uint32_t bot)
{
    CharacterDatabase.PExecute(
        "DELETE FROM ai_playerbot_random_bots WHERE owner = 0 AND bot = '%u'",
        bot);
}

void DeleteAllEvents()
{
    CharacterDatabase.Execute(
        "DELETE FROM ai_playerbot_random_bots");
}

void BumpEventTimes(uint32_t delta_seconds)
{
    CharacterDatabase.PExecute(
        "UPDATE ai_playerbot_random_bots SET time = time + %u "
        "WHERE owner = 0 AND bot <> 0", delta_seconds);
}

// ── Query helpers — each builds a std::vector<...Row> the bridge
// callback then hands off to the Rust side via HandOffVector. ─────────

std::vector<BotEventRow> QueryAllEventsImpl()
{
    std::vector<BotEventRow> out;
    auto result = CharacterDatabase.Query(
        "SELECT bot, event, value, `time`, validIn, data "
        "FROM ai_playerbot_random_bots WHERE owner = 0");
    if (!result)
        return out;

    do
    {
        Field* fields = result->Fetch();
        BotEventRow row{};
        row.bot              = fields[0].GetUInt32();
        CopyFixedString(row.event, sizeof(row.event), fields[1].GetCppString());
        row.value            = fields[2].GetUInt32();
        row.last_change_time = fields[3].GetUInt32();
        row.valid_in         = fields[4].GetUInt32();
        CopyFixedString(row.data, sizeof(row.data), fields[5].GetCppString());
        out.push_back(row);
    } while (result->NextRow());

    return out;
}

std::vector<BotEventRow> QueryEventsForBotImpl(uint32_t bot)
{
    std::vector<BotEventRow> out;
    auto result = CharacterDatabase.PQuery(
        "SELECT `event`, `value`, `time`, validIn, `data` "
        "FROM ai_playerbot_random_bots WHERE owner = 0 AND bot = '%u'", bot);
    if (!result)
        return out;

    do
    {
        Field* fields = result->Fetch();
        BotEventRow row{};
        row.bot              = bot;
        CopyFixedString(row.event, sizeof(row.event), fields[0].GetCppString());
        row.value            = fields[1].GetUInt32();
        row.last_change_time = fields[2].GetUInt32();
        row.valid_in         = fields[3].GetUInt32();
        CopyFixedString(row.data, sizeof(row.data), fields[4].GetCppString());
        out.push_back(row);
    } while (result->NextRow());

    return out;
}

std::vector<BotTeleportRow> QueryTeleportCacheImpl()
{
    std::vector<BotTeleportRow> out;
    auto result = CharacterDatabase.Query(
        "SELECT `map_id`, `x`, `y`, `z`, `level` "
        "FROM `ai_playerbot_tele_cache`");
    if (!result)
        return out;

    do
    {
        Field* fields = result->Fetch();
        BotTeleportRow row{};
        row.map_id = fields[0].GetUInt32();
        row.x      = fields[1].GetFloat();
        row.y      = fields[2].GetFloat();
        row.z      = fields[3].GetFloat();
        row.level  = fields[4].GetUInt32();
        out.push_back(row);
    } while (result->NextRow());

    return out;
}

/// Direct port of the "creature walker" half of
/// `RandomPlayerbotMgr::PrepareTeleportCache` — scans the world's
/// creature/creature_template tables level by level and writes one
/// `ai_playerbot_tele_cache` row per qualifying spawn. Called from
/// `CB_RebuildTeleportCache` when the Rust side notices the cache
/// table is empty.
void RebuildTeleportCacheImpl()
{
    uint32 maxLevel = playerbot_config_random_bot_max_level();
    const uint32 worldMax = sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL);
    if (maxLevel > worldMax)
        maxLevel = worldMax;

    sLog.outString("Rebuilding random teleport cache for %u levels...",
                   maxLevel);

    // Clear any stale rows first so the rebuild is idempotent.
    CharacterDatabase.Execute("DELETE FROM ai_playerbot_tele_cache");

    char* mapsRaw = playerbot_config_random_bot_maps_as_string();
    std::string randomBotMaps = mapsRaw ? mapsRaw : "";
    playerbot_config_free_cstr(mapsRaw);

    for (uint32 level = 1; level <= maxLevel; ++level)
    {
        auto results = WorldDatabase.PQuery(
            "SELECT `map`, `position_x`, `position_y`, `position_z` "
            "FROM (SELECT `map`, `position_x`, `position_y`, `position_z`, "
            "t.maxlevel, t.minlevel, "
            "%u - (t.maxlevel + t.minlevel) / 2 delta "
            "FROM creature c INNER JOIN creature_template t ON c.id = t.entry "
            "WHERE t.CreatureType != 8 AND t.NpcFlags = 0 AND t.Rank = 0 AND "
            "NOT (t.extraFlags & 1024 OR t.extraFlags & 65536 OR "
            "t.extraflags & 64 OR t.unitFlags & 256 OR t.unitFlags & 512) "
            "AND t.lootid != 0) q "
            "WHERE delta >= 0 AND delta <= %u AND map in (%s)",
            level,
            playerbot_config_random_bot_tele_level(),
            randomBotMaps.c_str());
        if (!results)
            continue;

        CharacterDatabase.BeginTransaction();
        do
        {
            Field* fields = results->Fetch();
            uint16 mapId = fields[0].GetUInt16();
            float x = fields[1].GetFloat();
            float y = fields[2].GetFloat();
            float z = fields[3].GetFloat();
            CharacterDatabase.PExecute(
                "INSERT INTO `ai_playerbot_tele_cache` "
                "(`level`, `map_id`, `x`, `y`, `z`) "
                "VALUES (%u, %u, %f, %f, %f)",
                level, mapId, x, y, z);
        } while (results->NextRow());
        CharacterDatabase.CommitTransaction();
    }
}

std::vector<BotNamedLocationRow> QueryNamedLocationsImpl()
{
    std::vector<BotNamedLocationRow> out;
    auto result = WorldDatabase.Query(
        "SELECT `name`, `map_id`, `position_x`, `position_y`, `position_z`, "
        "`orientation` FROM `ai_playerbot_named_location` "
        "WHERE `name` NOT LIKE 'FISH_LOCATION%'");
    if (!result)
        return out;

    do
    {
        Field* fields = result->Fetch();
        BotNamedLocationRow row{};
        CopyFixedString(row.name, sizeof(row.name), fields[0].GetCppString());
        row.map_id     = fields[1].GetUInt32();
        row.x          = fields[2].GetFloat();
        row.y          = fields[3].GetFloat();
        row.z          = fields[4].GetFloat();
        row.orientation = fields[5].GetFloat();
        out.push_back(row);
    } while (result->NextRow());

    return out;
}

std::vector<BotBgQueueEntry> QueryBgQueueImpl()
{
    // The legacy `CheckBgQueue` walked `sRandomPlayerbotMgr.players` to
    // rebuild the BgPlayers/BgBots/ArenaBots matrices directly on the
    // class. The Rust side now owns those matrices, so we surface the
    // *raw* queue state here (one entry per queued player/bot) and let
    // Rust fold it into its own buckets via `apply_bg_row`. Returning an
    // empty snapshot is the safe default when no bots are queued.
    std::vector<BotBgQueueEntry> out;
    for (auto& kv : sRandomPlayerbotMgr.GetPlayers())
    {
        Player* player = kv.second;
        if (!player || !player->IsInWorld())
            continue;
        if (!player->InBattleGroundQueue())
            continue;
        if (player->InBattleGround() && player->GetBattleGround() &&
            player->GetBattleGround()->GetStatus() == STATUS_WAIT_LEAVE)
            continue;

        for (int i = 0; i < PLAYER_MAX_BATTLEGROUND_QUEUES; ++i)
        {
            BattleGroundQueueTypeId queueTypeId =
                player->GetBattleGroundQueueTypeId(i);
            if (queueTypeId == BATTLEGROUND_QUEUE_NONE)
                continue;

            BattleGroundTypeId bgTypeId =
                sBattleGroundMgr.BgTemplateId(queueTypeId);
#ifdef MANGOSBOT_TWO
            BattleGround* bg =
                sBattleGroundMgr.GetBattleGroundTemplate(bgTypeId);
            uint32 mapId = bg ? bg->GetMapId() : 0;
            PvPDifficultyEntry const* pvpDiff =
                GetBattlegroundBracketByLevel(mapId, player->GetLevel());
            if (!pvpDiff)
                continue;
            BattleGroundBracketId bracketId = pvpDiff->GetBracketId();
#else
            BattleGroundBracketId bracketId =
                sBattleGroundMgr.GetBattleGroundBracketIdFromLevel(
                    bgTypeId, player->GetLevel());
            uint32 mapId = 0;
#endif

            BotBgQueueEntry row{};
            row.queue_type   = static_cast<uint32_t>(queueTypeId);
            row.bracket_id   = static_cast<uint32_t>(bracketId);
            row.team_id      = player->GetTeam() == ALLIANCE ? 0u : 1u;
            row.is_bot       = player->GetPlayerbotAI() != nullptr;
            row.level        = player->GetLevel();
            row.map_id       = mapId;
            row.arena_rating = 0;
            row.arena_type   = 0;
            out.push_back(row);
        }
    }
    return out;
}

std::vector<BotLfgQueueEntry> QueryLfgQueueImpl()
{
    // Classic / TBC have no LFG queue — return empty. WotLK-specific
    // walker lives here behind MANGOSBOT_TWO once wired up.
    return {};
}

std::vector<BotAhMirrorRow> QueryAhRowsImpl()
{
    std::vector<BotAhMirrorRow> out;
    static const AuctionHouseType kHouses[] = {
        AUCTION_HOUSE_ALLIANCE, AUCTION_HOUSE_HORDE, AUCTION_HOUSE_NEUTRAL,
    };

    for (AuctionHouseType house : kHouses)
    {
        AuctionHouseObject* auctionHouse = sAuctionMgr.GetAuctionsMap(house);
        if (!auctionHouse) continue;

        for (auto const& auction : auctionHouse->GetAuctions())
        {
            AuctionEntry* entry = auction.second;
            if (!entry) continue;
            if (!entry->buyout) continue;
            if (!entry->itemCount) continue;

            BotAhMirrorRow row{};
            row.item_id = entry->itemTemplate;
            row.buyout  = entry->buyout;
            row.count   = entry->itemCount;
            out.push_back(row);
        }
    }
    return out;
}

std::vector<BotRealPlayerLevel> QueryRealPlayerLevelsImpl()
{
    std::vector<BotRealPlayerLevel> out;
    // Walk every real (non-bot) player currently in world.
    HashMapHolder<Player>::MapType const& map = sObjectAccessor.GetPlayers();
    for (auto const& kv : map)
    {
        Player* p = kv.second;
        if (!p || !p->IsInWorld()) continue;
        if (p->GetPlayerbotAI()) continue; // skip bots

        BotRealPlayerLevel row{};
        row.level       = p->GetLevel();
        row.total_time  = p->GetTotalPlayedTime();
        out.push_back(row);
    }
    return out;
}

std::vector<BotStatsRow> QueryBotStatsImpl()
{
    std::vector<BotStatsRow> out;
    for (auto& kv : sRandomPlayerbotMgr.GetPlayers())
    {
        Player* p = kv.second;
        if (!p) continue;
        BotStatsRow row{};
        row.guid     = p->GetObjectGuid().GetCounter();
        row.race     = p->getRace();
        row.class_id = p->getClass();
        row.team     = p->GetTeam() == ALLIANCE ? 0 : 1;
        row.level    = p->GetLevel();

        uint32_t flags = 0;
        // The "ACTIVE" flag maps to the legacy `selfbot` / activity check
        // — a bot counts as active if its AI is actually ticking.
        if (p->IsInWorld())                       flags |= PLAYERBOT_STATS_FLAG_ACTIVE;
        if (p->IsDead())                          flags |= PLAYERBOT_STATS_FLAG_DEAD;
        if (p->IsInCombat())                      flags |= PLAYERBOT_STATS_FLAG_COMBAT;
        if (p->IsTaxiFlying())                    flags |= PLAYERBOT_STATS_FLAG_TAXI;
        if (p->IsMoving())                        flags |= PLAYERBOT_STATS_FLAG_MOVING;
        if (p->IsMounted())                       flags |= PLAYERBOT_STATS_FLAG_MOUNTED;
        if (p->isAFK())                           flags |= PLAYERBOT_STATS_FLAG_AFK;
        row.flags = flags;

        out.push_back(row);
    }
    return out;
}

std::vector<uint32_t> OwnedBotGuidsImpl()
{
    std::vector<uint32_t> out;
    for (auto& kv : sRandomPlayerbotMgr.GetPlayers())
    {
        if (kv.second)
            out.push_back(kv.first);
    }
    return out;
}

} // namespace

// ── Callback implementations ─────────────────────────────────────────────
//
// Everything under this line is stored in the `RandomMgrCallbacks` vtable
// built by `MakeCallbacks()`. Keep them static-friendly free functions so
// the vtable is a trivial `sizeof(RandomMgrCallbacks)` struct of pointers.

namespace {

// ── RNG ───────────────────────────────────────────────────────────────
uint32_t CB_UrandRange(uint32_t min_v, uint32_t max_v)
{
    return urand(min_v, max_v);
}

// ── World globals ─────────────────────────────────────────────────────
uint32_t CB_CurrentMsTime()
{
    return sWorld.GetCurrentMSTime();
}

bool CB_IsShuttingDown()
{
    return sWorld.IsShutdowning();
}

uint32_t CB_WorldMaxLevel()
{
    return sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL);
}

void CB_WorldDiffSample(BotWorldDiffSample* out)
{
    if (!out) return;
    // Matches what `RandomPlayerbotMgr::ScaleBotActivity` fed into the
    // PID controller: the three diff counters `sWorld` exposes on every
    // supported Mangos build.
    out->current_diff_ms = static_cast<float>(World::GetCurrentDiff());
    out->average_diff_ms = static_cast<float>(World::GetAverageDiff());
    out->max_diff_ms     = static_cast<float>(World::GetMaxDiff());
}

void CB_DatabasePing()
{
    CharacterDatabase.AsyncPQuery(
        &RandomPlayerbotMgr::DatabasePing,
        sWorld.GetCurrentMSTime(),
        std::string("CharacterDatabase"),
        "select 1");
}

uint32_t CB_DatabaseDelayMs(const char* db)
{
    if (!db) return 0;
    return sRandomPlayerbotMgr.GetDatabaseDelay(std::string(db));
}

// ── Event KV store ────────────────────────────────────────────────────
uint32_t CB_QueryAllEvents(uint32_t /*unused*/, BotEventRow** out_rows)
{
    return HandOffVector(QueryAllEventsImpl(), out_rows);
}

uint32_t CB_QueryEventsForBot(uint32_t bot, BotEventRow** out_rows)
{
    return HandOffVector(QueryEventsForBotImpl(bot), out_rows);
}

void CB_FreeEventRowList(BotEventRow* rows, uint32_t count)
{
    FreeHandOff(rows, count);
}

void CB_UpsertEvent(const BotEventRow* row)
{
    if (!row) return;
    UpsertEventRow(row->bot, row->event, row->value,
                   row->last_change_time, row->valid_in, row->data);
}

void CB_DeleteEvent(uint32_t bot, const char* event)
{
    DeleteEventRow(bot, event);
}

void CB_DeleteAllEventsForBot(uint32_t bot)
{
    DeleteAllEventsForBot(bot);
}

void CB_DeleteAllEvents()
{
    DeleteAllEvents();
}

void CB_BumpEventTimes(uint32_t delta_seconds)
{
    BumpEventTimes(delta_seconds);
}

// ── Bot roster / per-bot actions ──────────────────────────────────────
uint32_t CB_OwnedBotGuids(uint32_t** out_ids)
{
    return HandOffVector(OwnedBotGuidsImpl(), out_ids);
}

void CB_FreeU32List(uint32_t* ids, uint32_t count)
{
    FreeHandOff(ids, count);
}

bool CB_BotLevel(uint32_t guid, uint32_t* out_level)
{
    if (!out_level) return false;
    *out_level = 0;
    Player* p = sRandomPlayerbotMgr.GetPlayer(guid);
    if (!p) return false;
    *out_level = p->GetLevel();
    return true;
}

bool CB_HasPlayerBot(uint32_t guid)
{
    return sRandomPlayerbotMgr.GetPlayer(guid) != nullptr;
}

void CB_DispatchRandomize(uint32_t guid)
{
    if (Player* p = sRandomPlayerbotMgr.GetPlayer(guid))
        sRandomPlayerbotMgr.Randomize(p);
}

void CB_DispatchInstaRandomize(uint32_t guid)
{
    if (Player* p = sRandomPlayerbotMgr.GetPlayer(guid))
        sRandomPlayerbotMgr.InstaRandomize(p);
}

void CB_DispatchRandomizeFirst(uint32_t guid)
{
    if (Player* p = sRandomPlayerbotMgr.GetPlayer(guid))
        sRandomPlayerbotMgr.RandomizeFirst(p);
}

void CB_DispatchUpdateGearSpells(uint32_t guid)
{
    if (Player* p = sRandomPlayerbotMgr.GetPlayer(guid))
        sRandomPlayerbotMgr.UpdateGearSpells(p);
}

void CB_DispatchRefresh(uint32_t guid)
{
    if (Player* p = sRandomPlayerbotMgr.GetPlayer(guid))
        sRandomPlayerbotMgr.Refresh(p);
}

bool CB_DispatchRandomTeleportForLevel(uint32_t guid, bool active_only)
{
    Player* p = sRandomPlayerbotMgr.GetPlayer(guid);
    if (!p) return false;
    sRandomPlayerbotMgr.RandomTeleportForLevel(p, active_only);
    return true;
}

bool CB_DispatchRandomTeleportForRpg(uint32_t guid, bool active_only)
{
    Player* p = sRandomPlayerbotMgr.GetPlayer(guid);
    if (!p) return false;
    sRandomPlayerbotMgr.RandomTeleportForRpg(p, active_only);
    return true;
}

void CB_DispatchRandomTeleport(uint32_t guid)
{
    if (Player* p = sRandomPlayerbotMgr.GetPlayer(guid))
        sRandomPlayerbotMgr.RandomTeleportForLevel(p, false);
}

void CB_DispatchRevive(uint32_t guid)
{
    if (Player* p = sRandomPlayerbotMgr.GetPlayer(guid))
        sRandomPlayerbotMgr.Revive(p);
}

void CB_DispatchChangeStrategy(uint32_t guid)
{
    if (Player* p = sRandomPlayerbotMgr.GetPlayer(guid))
        sRandomPlayerbotMgr.ChangeStrategy(p);
}

void CB_DispatchRemove(uint32_t guid)
{
    if (Player* p = sRandomPlayerbotMgr.GetPlayer(guid))
        sRandomPlayerbotMgr.Remove(p);
}

void CB_DispatchLogout(uint32_t guid)
{
    sRandomPlayerbotMgr.LogoutPlayerBot(ObjectGuid(HIGHGUID_PLAYER, guid));
}

// ── Teleport cache ────────────────────────────────────────────────────
uint32_t CB_QueryTeleportCache(BotTeleportRow** out_rows)
{
    return HandOffVector(QueryTeleportCacheImpl(), out_rows);
}

void CB_FreeTeleportRowList(BotTeleportRow* rows, uint32_t count)
{
    FreeHandOff(rows, count);
}

void CB_RebuildTeleportCache()
{
    // Walks `creature`/`creature_template` and rewrites the
    // `ai_playerbot_tele_cache` rows. Rust re-queries the cache after
    // this runs.
    RebuildTeleportCacheImpl();
}

// ── Named locations ───────────────────────────────────────────────────
uint32_t CB_QueryNamedLocations(BotNamedLocationRow** out_rows)
{
    return HandOffVector(QueryNamedLocationsImpl(), out_rows);
}

void CB_FreeNamedLocationList(BotNamedLocationRow* rows, uint32_t count)
{
    FreeHandOff(rows, count);
}

// ── BG / LFG / AH / real-player ──────────────────────────────────────
uint32_t CB_QueryBgQueue(BotBgQueueEntry** out_rows)
{
    return HandOffVector(QueryBgQueueImpl(), out_rows);
}

void CB_FreeBgQueueList(BotBgQueueEntry* rows, uint32_t count)
{
    FreeHandOff(rows, count);
}

uint32_t CB_QueryLfgQueue(BotLfgQueueEntry** out_rows)
{
    return HandOffVector(QueryLfgQueueImpl(), out_rows);
}

void CB_FreeLfgQueueList(BotLfgQueueEntry* rows, uint32_t count)
{
    FreeHandOff(rows, count);
}

uint32_t CB_QueryAhRows(BotAhMirrorRow** out_rows)
{
    return HandOffVector(QueryAhRowsImpl(), out_rows);
}

void CB_FreeAhRowList(BotAhMirrorRow* rows, uint32_t count)
{
    FreeHandOff(rows, count);
}

uint32_t CB_QueryRealPlayerLevels(BotRealPlayerLevel** out_rows)
{
    return HandOffVector(QueryRealPlayerLevelsImpl(), out_rows);
}

void CB_FreeRealPlayerLevelList(BotRealPlayerLevel* rows, uint32_t count)
{
    FreeHandOff(rows, count);
}

// ── Per-bot stats snapshot ─────────────────────────────────────────────
uint32_t CB_QueryBotStats(BotStatsRow** out_rows)
{
    return HandOffVector(QueryBotStatsImpl(), out_rows);
}

void CB_FreeBotStatsList(BotStatsRow* rows, uint32_t count)
{
    FreeHandOff(rows, count);
}

// ── Logging sinks ─────────────────────────────────────────────────────
void CB_LogInfo(const char* msg)
{
    if (msg) sLog.outString("%s", msg);
}

void CB_LogError(const char* msg)
{
    if (msg) sLog.outError("%s", msg);
}

void CB_LogFile(const char* /*file*/, const char* line)
{
    // Legacy `sLog.outFile` routed a line to a named log file; during
    // the transition we collapse this onto the normal info sink.
    if (line) sLog.outString("%s", line);
}

} // namespace

// ── Vtable builder ───────────────────────────────────────────────────────

RandomMgrCallbacks RandomMgrBridge::MakeCallbacks()
{
    RandomMgrCallbacks cbs{};

    cbs.urand_range                     = &CB_UrandRange;

    cbs.current_ms_time                 = &CB_CurrentMsTime;
    cbs.is_shutting_down                = &CB_IsShuttingDown;
    cbs.world_max_level                 = &CB_WorldMaxLevel;
    cbs.world_diff_sample               = &CB_WorldDiffSample;
    cbs.database_ping                   = &CB_DatabasePing;
    cbs.database_delay_ms               = &CB_DatabaseDelayMs;

    cbs.query_all_events                = &CB_QueryAllEvents;
    cbs.query_events_for_bot            = &CB_QueryEventsForBot;
    cbs.free_event_row_list             = &CB_FreeEventRowList;
    cbs.upsert_event                    = &CB_UpsertEvent;
    cbs.delete_event                    = &CB_DeleteEvent;
    cbs.delete_all_events_for_bot       = &CB_DeleteAllEventsForBot;
    cbs.delete_all_events               = &CB_DeleteAllEvents;
    cbs.bump_event_times                = &CB_BumpEventTimes;

    cbs.owned_bot_guids                 = &CB_OwnedBotGuids;
    cbs.free_u32_list                   = &CB_FreeU32List;
    cbs.bot_level                       = &CB_BotLevel;
    cbs.has_player_bot                  = &CB_HasPlayerBot;

    cbs.dispatch_randomize              = &CB_DispatchRandomize;
    cbs.dispatch_insta_randomize        = &CB_DispatchInstaRandomize;
    cbs.dispatch_randomize_first        = &CB_DispatchRandomizeFirst;
    cbs.dispatch_update_gear_spells     = &CB_DispatchUpdateGearSpells;
    cbs.dispatch_refresh                = &CB_DispatchRefresh;
    cbs.dispatch_random_teleport_for_level = &CB_DispatchRandomTeleportForLevel;
    cbs.dispatch_random_teleport_for_rpg   = &CB_DispatchRandomTeleportForRpg;
    cbs.dispatch_random_teleport        = &CB_DispatchRandomTeleport;
    cbs.dispatch_revive                 = &CB_DispatchRevive;
    cbs.dispatch_change_strategy        = &CB_DispatchChangeStrategy;
    cbs.dispatch_remove                 = &CB_DispatchRemove;
    cbs.dispatch_logout                 = &CB_DispatchLogout;

    cbs.query_teleport_cache            = &CB_QueryTeleportCache;
    cbs.free_teleport_row_list          = &CB_FreeTeleportRowList;
    cbs.rebuild_teleport_cache          = &CB_RebuildTeleportCache;

    cbs.query_named_locations           = &CB_QueryNamedLocations;
    cbs.free_named_location_list        = &CB_FreeNamedLocationList;

    cbs.query_bg_queue                  = &CB_QueryBgQueue;
    cbs.free_bg_queue_list              = &CB_FreeBgQueueList;
    cbs.query_lfg_queue                 = &CB_QueryLfgQueue;
    cbs.free_lfg_queue_list             = &CB_FreeLfgQueueList;
    cbs.query_ah_rows                   = &CB_QueryAhRows;
    cbs.free_ah_row_list                = &CB_FreeAhRowList;
    cbs.query_real_player_levels        = &CB_QueryRealPlayerLevels;
    cbs.free_real_player_level_list     = &CB_FreeRealPlayerLevelList;

    cbs.query_bot_stats                 = &CB_QueryBotStats;
    cbs.free_bot_stats_list             = &CB_FreeBotStatsList;

    cbs.log_info                        = &CB_LogInfo;
    cbs.log_error                       = &CB_LogError;
    cbs.log_file                        = &CB_LogFile;

    return cbs;
}
