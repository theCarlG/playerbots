/**
 * LoginBridge.cpp — CMaNGOS-side implementation of `LoginCallbacks`.
 *
 * See the header for the division of labour: Rust owns the login queue
 * state machine; this file is only the narrow surface where the queue
 * has to reach CMaNGOS for SQL, async holders, KV state, or the actual
 * login/logout dispatch.
 *
 * Threading: the Rust login worker thread calls every `CB_*` callback
 * except `CB_PerformLogin` / `CB_PerformLogout`, which run on the main
 * world thread via `playerbot_login_update`. The internal
 * `LoginHolderTracker` serialises access to its per-guid holder map
 * with a mutex so the worker thread and the DB-callback thread can both
 * mutate it safely.
 */

#include "botpch.h"
#include "LoginBridge.h"

#include <cstdlib>
#include <cstring>
#include <mutex>
#include <string>
#include <unordered_map>

#include "Database/DatabaseEnv.h"
#include "Database/DatabaseImpl.h"
#include "Entities/ObjectGuid.h"
#include "Globals/ObjectMgr.h"
#include "Groups/Group.h"
#include "Log/Log.h"
#include "World/World.h"

#include "BotConfig.h"
#include "MgrBridge.h"
#include "RandomPlayerbotMgr.h"

// ── Mirror of the core LoginQueryHolder classes ───────────────────────────
//
// The core defines `LoginQueryHolder` in `CharacterHandler.cpp` and the
// `PlayerbotLoginQueryHolder` subclass under `#ifdef ENABLE_PLAYERBOTS`.
// Because both are file-scope classes we re-declare them here with the
// same layout so the linker resolves `LoginQueryHolder::Initialize()` and
// the `PlayerbotLoginQueryHolder` ctor to the core's definitions. This
// mirrors exactly what the old `playerbot/PlayerbotLoginMgr.cpp` did.

class LoginQueryHolder : public SqlQueryHolder
{
private:
    uint32 m_accountId;
    ObjectGuid m_guid;
public:
    LoginQueryHolder(uint32 accountId, ObjectGuid guid)
        : m_accountId(accountId), m_guid(guid) {}
    ObjectGuid GetGuid() const { return m_guid; }
    uint32 GetAccountId() const { return m_accountId; }
    bool Initialize();
};

class PlayerbotLoginQueryHolder : public LoginQueryHolder
{
private:
    uint32 masterAccountId;
    PlayerbotHolder* playerbotHolder;

public:
    PlayerbotLoginQueryHolder(PlayerbotHolder* holder, uint32 masterAccount,
                              uint32 accountId, uint32 guid)
        : LoginQueryHolder(accountId, ObjectGuid(HIGHGUID_PLAYER, guid))
        , masterAccountId(masterAccount)
        , playerbotHolder(holder) {}

    uint32 GetMasterAccountId() const { return masterAccountId; }
    PlayerbotHolder* GetPlayerbotHolder() { return playerbotHolder; }
};

// ── Holder tracker singleton ─────────────────────────────────────────────
//
// Owns the per-guid `HolderEntry` map and exposes the `OnHolderReady`
// method that `CharacterDatabase.DelayQueryHolder` invokes once the
// async query completes. Stored as a function-local static so the
// initialisation order is deterministic even if `playerbot_login_init`
// is called very early during startup.

namespace {

struct HolderEntry
{
    SqlQueryHolder* holder = nullptr;
    uint8_t         state  = PLAYERBOT_HOLDER_EMPTY;
};

class LoginHolderTracker
{
public:
    static LoginHolderTracker& Instance()
    {
        static LoginHolderTracker inst;
        return inst;
    }

    /// Create a new holder for `(account, guid)` and queue the async
    /// DelayQueryHolder. Returns true on success. If a holder was
    /// already tracked for this guid it is discarded first — the Rust
    /// side should not normally call `send_holder` twice, but the
    /// legacy code was tolerant of this pattern and we preserve that.
    bool Send(uint32 account, uint32 guid)
    {
        // Create and initialise the holder outside the lock — the ctor
        // + SetPQuery chain can take a non-trivial amount of time.
        auto* holder = new PlayerbotLoginQueryHolder(
            &sRandomPlayerbotMgr, 0u, account, guid);
        if (!holder->Initialize())
        {
            delete holder;
            return false;
        }

        {
            std::lock_guard<std::mutex> lk(m_mutex);
            auto& entry = m_entries[guid];
            // Leak any previous holder pointer — ownership was handed
            // off to `DelayQueryHolder` which deletes it on completion.
            entry.holder = holder;
            entry.state  = PLAYERBOT_HOLDER_SENT;
        }

        // `DelayQueryHolder` takes ownership: once the query completes
        // it calls `OnHolderReady` and eventually `delete`s the holder
        // (by way of `HandlePlayerLogin`). Until then we keep the
        // pointer in `m_entries` so `CB_PerformLogin` can still reach
        // the holder to feed it to `HandlePlayerBotLoginCallback`.
        CharacterDatabase.DelayQueryHolder(
            this, &LoginHolderTracker::OnHolderReady, holder);
        return true;
    }

    /// `DelayQueryHolder` callback — runs on the DB worker thread once
    /// the SELECTs for the login query holder have all returned. Mark
    /// the corresponding entry as `RECEIVED`; the main thread drains
    /// the entry later when `CB_PerformLogin` fires.
    void OnHolderReady(QueryResult* /*dummy*/, SqlQueryHolder* holder)
    {
        if (!holder)
            return;
        auto* lqh = static_cast<LoginQueryHolder*>(holder);
        uint32 guid = lqh->GetGuid().GetCounter();

        std::lock_guard<std::mutex> lk(m_mutex);
        auto it = m_entries.find(guid);
        if (it != m_entries.end())
            it->second.state = PLAYERBOT_HOLDER_RECEIVED;
    }

    uint8_t State(uint32 guid)
    {
        std::lock_guard<std::mutex> lk(m_mutex);
        auto it = m_entries.find(guid);
        if (it == m_entries.end())
            return PLAYERBOT_HOLDER_EMPTY;
        return it->second.state;
    }

    /// Discard the tracked entry without touching the holder pointer —
    /// used from `CB_ClearHolder`. Note the holder itself is owned by
    /// `CharacterDatabase`; dropping our reference just means we no
    /// longer expose it via `CB_PerformLogin`.
    void Clear(uint32 guid)
    {
        std::lock_guard<std::mutex> lk(m_mutex);
        m_entries.erase(guid);
    }

    /// Take ownership of a RECEIVED holder for `guid`. Returns nullptr
    /// if the entry doesn't exist, isn't RECEIVED, or has already been
    /// consumed. On success the entry is removed from the map so a
    /// future `send_holder(guid)` can create a fresh one.
    SqlQueryHolder* ConsumeReceived(uint32 guid)
    {
        std::lock_guard<std::mutex> lk(m_mutex);
        auto it = m_entries.find(guid);
        if (it == m_entries.end() || it->second.state != PLAYERBOT_HOLDER_RECEIVED)
            return nullptr;
        SqlQueryHolder* holder = it->second.holder;
        m_entries.erase(it);
        return holder;
    }

private:
    std::mutex m_mutex;
    std::unordered_map<uint32, HolderEntry> m_entries;
};

} // namespace

// ── Candidate enumeration ────────────────────────────────────────────────

uint32_t LoginBridge::CB_QueryCandidates(const char* account_prefix,
                                         BotCandidateInfo** out_rows)
{
    if (!out_rows)
        return 0;
    *out_rows = nullptr;

    std::string prefix = account_prefix ? account_prefix : "";
    prefix += "%";

    // 1) Collect the random-bot account IDs. Matches
    //    `PlayerBotLoginMgr::LoadBotsFromDb`.
    std::unordered_map<uint32, bool> accountSet;
    {
        auto result = LoginDatabase.PQuery(
            "SELECT id FROM account where UPPER(username) like UPPER('%s')",
            prefix.c_str());
        if (!result)
            return 0;
        do
        {
            Field* fields = result->Fetch();
            accountSet.emplace(fields[0].GetUInt32(), true);
        } while (result->NextRow());
    }

    if (accountSet.empty())
        return 0;

    // 2) Walk every character row and keep the ones whose account is
    //    in the set. We hit `characters` in one pass rather than per
    //    account so the DB work matches the original SQL exactly.
    auto result = CharacterDatabase.PQuery(
        "SELECT account, guid, race, class, level, online, totaltime, "
        "map, position_x, position_y, position_z, orientation, "
        "(SELECT guildid FROM guild_member m WHERE m.guid = c.guid) guildId "
        "FROM characters c");
    if (!result)
        return 0;

    // Worst-case upper bound: one row per iteration. We grow the buffer
    // dynamically to avoid two passes.
    std::vector<BotCandidateInfo> rows;
    rows.reserve(256);

    do
    {
        Field* fields  = result->Fetch();
        uint32 account = fields[0].GetUInt32();
        if (!accountSet.count(account))
            continue;

        BotCandidateInfo info{};
        info.account_id       = account;
        info.guid             = fields[1].GetUInt32();
        info.race             = fields[2].GetUInt8();
        info.cls              = fields[3].GetUInt8();
        info.level            = fields[4].GetUInt32();
        info.is_online        = fields[5].GetBool();
        info.total_played_time = fields[6].GetUInt32();
        info.map_id           = fields[7].GetUInt32();
        info.x                = fields[8].GetFloat();
        info.y                = fields[9].GetFloat();
        info.z                = fields[10].GetFloat();
        info.o                = fields[11].GetFloat();
        info.guild_id         = fields[12].GetUInt32();
        info.group_id         = 0;  // not snapshotted from DB (matches legacy)
        rows.push_back(info);
    } while (result->NextRow());

    if (rows.empty())
        return 0;

    // Hand the buffer to Rust — it will call back into
    // `CB_FreeCandidateList` to release it.
    const size_t bytes = rows.size() * sizeof(BotCandidateInfo);
    auto* buf = static_cast<BotCandidateInfo*>(std::malloc(bytes));
    if (!buf)
        return 0;
    std::memcpy(buf, rows.data(), bytes);
    *out_rows = buf;
    return static_cast<uint32_t>(rows.size());
}

void LoginBridge::CB_FreeCandidateList(BotCandidateInfo* rows, uint32_t /*count*/)
{
    std::free(rows);
}

// ── Async holder lifecycle ───────────────────────────────────────────────

bool LoginBridge::CB_SendHolder(uint32_t account, uint32_t guid)
{
    return LoginHolderTracker::Instance().Send(account, guid);
}

uint8_t LoginBridge::CB_GetHolderState(uint32_t guid)
{
    return LoginHolderTracker::Instance().State(guid);
}

void LoginBridge::CB_ClearHolder(uint32_t guid)
{
    LoginHolderTracker::Instance().Clear(guid);
}

// ── RandomPlayerbotMgr KV / state accessors ──────────────────────────────

uint32_t LoginBridge::CB_RandomMgrGetValue(uint32_t guid, const char* key)
{
    if (!key)
        return 0;
    return sRandomPlayerbotMgr.GetValue(guid, std::string(key));
}

void LoginBridge::CB_RandomMgrSetValue(uint32_t guid, const char* key,
                                       uint32_t value, int32_t valid_in_s)
{
    if (!key)
        return;
    sRandomPlayerbotMgr.SetValue(guid, std::string(key), value, "", valid_in_s);
}

uint32_t LoginBridge::CB_RandomMgrGetPlayersLevel(void)
{
    return sRandomPlayerbotMgr.GetPlayersLevel();
}

uint32_t LoginBridge::CB_RandomMgrGetMaxOnlineBotCount(void)
{
    // Matches `PlayerBotLoginMgr::GetMaxOnlineBotCount`.
    return sRandomPlayerbotMgr.GetValue(uint32(0), "bot_count");
}

uint32_t LoginBridge::CB_RandomMgrGetWorldMaxLevel(void)
{
    return sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL);
}

uint32_t LoginBridge::CB_RandomMgrDatabaseDelayMs(void)
{
    return sRandomPlayerbotMgr.GetDatabaseDelay(std::string("CharacterDatabase"));
}

void LoginBridge::CB_RandomMgrDatabasePing(void)
{
    CharacterDatabase.AsyncPQuery(
        &RandomPlayerbotMgr::DatabasePing,
        sWorld.GetCurrentMSTime(),
        std::string("CharacterDatabase"),
        "select 1");
}

// ── Login / logout dispatch (main thread only) ───────────────────────────

bool LoginBridge::CB_PerformLogin(uint32_t guid)
{
    ObjectGuid og(HIGHGUID_PLAYER, guid);

    // If the bot is somehow already in world, treat this as a no-op
    // but drop the tracked holder so subsequent ticks don't get stuck
    // waiting for a response that will never come.
    if (sObjectMgr.GetPlayer(og, false))
    {
        LoginHolderTracker::Instance().Clear(guid);
        return false;
    }

    SqlQueryHolder* holder =
        LoginHolderTracker::Instance().ConsumeReceived(guid);
    if (!holder)
        return false;

    // Hand the holder off to the core's login pipeline. This call
    // eventually runs `botSession->HandlePlayerLogin(lqh)` which deletes
    // the holder, so we must NOT touch it again after this point.
    sRandomPlayerbotMgr.HandlePlayerBotLoginCallback(nullptr, holder);

    // Apply the legacy "add" TTL so the bot stays in world for a
    // randomised interval before the timed-logout path picks it up.
    if (sPlayerbotAIConfig.randomBotTimedLogout)
    {
        sRandomPlayerbotMgr.SetValue(
            guid, "add", 1, "",
            urand(sPlayerbotAIConfig.minRandomBotInWorldTime,
                  sPlayerbotAIConfig.maxRandomBotInWorldTime));
    }

    // Confirm the bot is actually in world after HandlePlayerLogin.
    return sObjectMgr.GetPlayer(og, false) != nullptr;
}

bool LoginBridge::CB_PerformLogout(uint32_t guid)
{
    ObjectGuid og(HIGHGUID_PLAYER, guid);
    Player* player = sObjectMgr.GetPlayer(og, false);
    if (!player)
        return false;

    sRandomPlayerbotMgr.SetValue(guid, "add", 0, "", 0);
    sRandomPlayerbotMgr.LogoutPlayerBot(guid);

    // If the logout dispatch rejected the request, leave state alone
    // and let the next tick try again.
    if (sObjectMgr.GetPlayer(og, false))
        return false;

    if (sPlayerbotAIConfig.randomBotTimedOffline)
    {
        sRandomPlayerbotMgr.SetValue(
            guid, "logout", 1, "",
            urand(sPlayerbotAIConfig.minRandomBotInWorldTime,
                  sPlayerbotAIConfig.maxRandomBotInWorldTime));
    }
    return true;
}

// ── Debug logging ────────────────────────────────────────────────────────

void LoginBridge::CB_LogDebug(const char* msg)
{
    if (msg)
        sLog.outError("%s", msg);
}

// ── Real player snapshot builder ─────────────────────────────────────────

uint32_t LoginBridge::BuildRealPlayerSnapshot(BotRealPlayerInfo* out,
                                              uint32_t max_out)
{
    if (!out || max_out == 0)
        return 0;

    uint32_t count = 0;
    for (auto& [guid, player] : sRandomPlayerbotMgr.GetPlayers())
    {
        if (!player)
            continue;
        if (count >= max_out)
            break;
        BotRealPlayerInfo& dst = out[count++];
        dst.guid    = guid;
        dst.map_id  = player->GetMapId();
        dst.x       = player->GetPositionX();
        dst.y       = player->GetPositionY();
        dst.z       = player->GetPositionZ();
        dst.group_id = player->GetGroup() ? player->GetGroup()->GetId() : 0;
        dst.guild_id = player->GetGuildId();
    }
    return count;
}

// ── Vtable builder ───────────────────────────────────────────────────────

LoginCallbacks LoginBridge::MakeCallbacks()
{
    LoginCallbacks cbs{};
    cbs.query_candidates                  = &LoginBridge::CB_QueryCandidates;
    cbs.free_candidate_list               = &LoginBridge::CB_FreeCandidateList;
    cbs.send_holder                       = &LoginBridge::CB_SendHolder;
    cbs.get_holder_state                  = &LoginBridge::CB_GetHolderState;
    cbs.clear_holder                      = &LoginBridge::CB_ClearHolder;
    cbs.random_mgr_get_value              = &LoginBridge::CB_RandomMgrGetValue;
    cbs.random_mgr_set_value              = &LoginBridge::CB_RandomMgrSetValue;
    cbs.random_mgr_get_players_level      = &LoginBridge::CB_RandomMgrGetPlayersLevel;
    cbs.random_mgr_get_max_online_bot_count = &LoginBridge::CB_RandomMgrGetMaxOnlineBotCount;
    cbs.random_mgr_get_world_max_level    = &LoginBridge::CB_RandomMgrGetWorldMaxLevel;
    cbs.random_mgr_database_delay_ms      = &LoginBridge::CB_RandomMgrDatabaseDelayMs;
    cbs.random_mgr_database_ping          = &LoginBridge::CB_RandomMgrDatabasePing;
    cbs.perform_login                     = &LoginBridge::CB_PerformLogin;
    cbs.perform_logout                    = &LoginBridge::CB_PerformLogout;
    cbs.log_debug                         = &LoginBridge::CB_LogDebug;
    return cbs;
}
