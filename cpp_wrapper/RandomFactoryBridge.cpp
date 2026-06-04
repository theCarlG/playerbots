/**
 * RandomFactoryBridge.cpp — CMaNGOS-side implementation of
 * `RandomFactoryCallbacks`.
 *
 * See the header for the division of labour: the Rust `random` module
 * owns every decision that used to live in
 * `playerbot/RandomPlayerbotFactory.cpp` (race/class availability,
 * probability-weighted race/class selection, bijective-base-26 name
 * postfix expansion, and orchestration of the three public entry
 * points). This file is the narrow surface that still has to reach
 * CMaNGOS — DB scans, account/character lifecycle, guild/arena team
 * creation, the character appearance picker, and a urand/log sink.
 *
 * Memory ownership: every `query_*_list`-shaped callback returns a
 * `std::malloc`'d array copied from an internal `std::vector`; Rust
 * releases it via the matching `free_*_list` callback, which calls
 * `std::free`.
 */

#include "botpch.h"
#include "RandomFactoryBridge.h"

#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

#include "Accounts/AccountMgr.h"
#include "Database/DatabaseEnv.h"
#include "Entities/ObjectGuid.h"
#include "Entities/Player.h"
#include "Globals/ObjectAccessor.h"
#include "Globals/ObjectMgr.h"
#include "Guilds/Guild.h"
#include "Guilds/GuildMgr.h"
#include "Log/Log.h"
#include "Server/DBCStores.h"
#include "Server/DBCStructure.h"
#include "Server/WorldSession.h"
#include "Social/SocialMgr.h"
#include "Util/Util.h"

#include "BotConfig.h"
#include "RandomPlayerbotMgr.h"

#ifndef MANGOSBOT_ZERO
#ifdef CMANGOS
#include "Arena/ArenaTeam.h"
#endif
#ifdef MANGOS
#include "ArenaTeam.h"
#endif
#endif

namespace
{
// ── Small helpers ────────────────────────────────────────────────────────

/// Copy a null-terminated C string into a fixed-size buffer, always
/// leaving a terminator. `dst_size` is the total size of the target
/// buffer including the terminator byte.
void CopyCString(char* dst, size_t dst_size, const char* src)
{
    if (!dst || dst_size == 0)
        return;
    if (!src)
    {
        dst[0] = '\0';
        return;
    }
    const size_t n = std::strlen(src);
    const size_t copy = n < dst_size - 1 ? n : dst_size - 1;
    std::memcpy(dst, src, copy);
    dst[copy] = '\0';
}

/// Hand a `std::vector<T>` off to Rust as a `std::malloc`'d raw array.
/// The caller frees via the matching `free_*` callback which uses
/// `std::free`.
template <typename T>
uint32_t HandOff(const std::vector<T>& rows, T** out_rows)
{
    if (!out_rows)
        return 0;
    *out_rows = nullptr;
    if (rows.empty())
        return 0;

    const size_t bytes = rows.size() * sizeof(T);
    auto* buf = static_cast<T*>(std::malloc(bytes));
    if (!buf)
        return 0;
    std::memcpy(buf, rows.data(), bytes);
    *out_rows = buf;
    return static_cast<uint32_t>(rows.size());
}

/// Resolve a logged-in player from the low half of an ObjectGuid.
Player* FindPlayerByLow(uint32 guid_low)
{
    if (guid_low == 0)
        return nullptr;
    ObjectGuid og(HIGHGUID_PLAYER, guid_low);
    return sObjectMgr.GetPlayer(og);
}

} // namespace

// ── RNG ─────────────────────────────────────────────────────────────────

uint32_t RandomFactoryBridge::CB_UrandRange(uint32_t min_v, uint32_t max_v)
{
    if (max_v < min_v)
        return min_v;
    return urand(min_v, max_v);
}

// ── Bot delete event ────────────────────────────────────────────────────

void RandomFactoryBridge::CB_QueryBotDeleteEvent(BotDeleteEventState* out)
{
    if (!out)
        return;
    out->scheduled = false;
    out->delete_friends = false;

    auto result = CharacterDatabase.Query(
        "select value from ai_playerbot_random_bots where event = 'bot_delete'");
    if (!result)
        return;

    Field* fields = result->Fetch();
    uint32 deleteType = fields[0].GetUInt32();
    out->scheduled = true;
    out->delete_friends = deleteType > 1;
}

// ── Account scan / create / delete ──────────────────────────────────────

uint32_t RandomFactoryBridge::CB_QueryAccountIdByName(const char* name)
{
    if (!name || !*name)
        return 0;

    auto result = LoginDatabase.PQuery(
        "SELECT id FROM account where username = '%s'", name);
    if (!result)
        return 0;

    Field* fields = result->Fetch();
    return fields[0].GetUInt32();
}

uint32_t RandomFactoryBridge::CB_QueryAccountIdsLikePrefix(const char* prefix,
                                                           uint32_t** out_ids)
{
    if (out_ids)
        *out_ids = nullptr;
    if (!prefix)
        return 0;

    auto results = LoginDatabase.PQuery(
        "SELECT id FROM account where username like '%s%%'", prefix);
    if (!results)
        return 0;

    std::vector<uint32_t> ids;
    ids.reserve(static_cast<size_t>(results->GetRowCount()));
    do
    {
        Field* fields = results->Fetch();
        ids.push_back(fields[0].GetUInt32());
    } while (results->NextRow());

    return HandOff(ids, out_ids);
}

void RandomFactoryBridge::CB_FreeU32List(uint32_t* ptr, uint32_t /*count*/)
{
    std::free(ptr);
}

void RandomFactoryBridge::CB_CreateAccount(const char* name,
                                           const char* password,
                                           uint8_t max_expansion)
{
    if (!name)
        return;
    std::string account = name;
    std::string pass = password ? password : "";

#ifndef MANGOSBOT_ZERO
    sAccountMgr.CreateAccount(account, pass, max_expansion);
#else
    (void)max_expansion;
    sAccountMgr.CreateAccount(account, pass);
#endif
}

void RandomFactoryBridge::CB_DeleteAccount(uint32_t account_id)
{
    sAccountMgr.DeleteAccount(account_id);
}

// ── Character count / query / friend list ──────────────────────────────

uint32_t RandomFactoryBridge::CB_GetCharacterCount(uint32_t account_id)
{
    return sAccountMgr.GetCharactersCount(account_id);
}

uint32_t RandomFactoryBridge::CB_QueryCharactersByAccount(uint32_t account_id,
                                                          uint32_t** out_guids)
{
    if (out_guids)
        *out_guids = nullptr;

    auto result = CharacterDatabase.PQuery(
        "SELECT guid FROM characters WHERE account='%u'", account_id);
    if (!result)
        return 0;

    std::vector<uint32_t> guids;
    guids.reserve(static_cast<size_t>(result->GetRowCount()));
    do
    {
        Field* fields = result->Fetch();
        guids.push_back(fields[0].GetUInt32());
    } while (result->NextRow());

    return HandOff(guids, out_guids);
}

uint32_t RandomFactoryBridge::CB_QueryFriendGuids(uint32_t** out_guids)
{
    if (out_guids)
        *out_guids = nullptr;

    auto result = CharacterDatabase.PQuery(
        "SELECT friend FROM character_social WHERE flags='%u'",
        SOCIAL_FLAG_FRIEND);
    if (!result)
        return 0;

    std::vector<uint32_t> guids;
    guids.reserve(static_cast<size_t>(result->GetRowCount()));
    do
    {
        Field* fields = result->Fetch();
        guids.push_back(fields[0].GetUInt32());
    } while (result->NextRow());

    return HandOff(guids, out_guids);
}

void RandomFactoryBridge::CB_OnPlayerLoginError(uint32_t guid_low)
{
    sRandomPlayerbotMgr.OnPlayerLoginError(guid_low);
}

void RandomFactoryBridge::CB_DeleteCharacterFromDb(uint32_t guid_low,
                                                    uint32_t account_id)
{
    ObjectGuid guid(HIGHGUID_PLAYER, guid_low);
    // Matches legacy RandomPlayerbotFactory::CreateRandomBots:
    //     Player::DeleteFromDB(guid, accId, false, true);
    Player::DeleteFromDB(guid, account_id, false, true);
}

void RandomFactoryBridge::CB_PruneRandomBotsTable(void)
{
    CharacterDatabase.Execute(
        "DELETE FROM ai_playerbot_random_bots "
        "WHERE bot NOT IN (SELECT guid FROM characters)");
}

// ── Name pool ──────────────────────────────────────────────────────────

uint32_t RandomFactoryBridge::CB_QueryNamePool(BotNamePoolRow** out_rows)
{
    if (out_rows)
        *out_rows = nullptr;

    auto result = CharacterDatabase.PQuery(
        "SELECT n.gender, n.name, e.guid FROM ai_playerbot_names n "
        "LEFT OUTER JOIN characters e ON e.name = n.name");
    if (!result)
        return 0;

    std::vector<BotNamePoolRow> rows;
    rows.reserve(static_cast<size_t>(result->GetRowCount()));
    do
    {
        Field* fields = result->Fetch();
        BotNamePoolRow row{};
        row.gender_index = fields[0].GetUInt8();
        std::string name = fields[1].GetString();
        CopyCString(row.name, sizeof(row.name), name.c_str());
        row.is_taken = fields[2].GetUInt32() != 0;
        rows.push_back(row);
    } while (result->NextRow());

    return HandOff(rows, out_rows);
}

void RandomFactoryBridge::CB_FreeNamePoolList(BotNamePoolRow* rows,
                                              uint32_t /*count*/)
{
    std::free(rows);
}

uint32_t RandomFactoryBridge::CB_QueryExistingCharacterNames(BotNamePoolRow** out_rows)
{
    if (out_rows)
        *out_rows = nullptr;

    auto result = CharacterDatabase.PQuery("SELECT e.name FROM characters e");
    if (!result)
        return 0;

    std::vector<BotNamePoolRow> rows;
    rows.reserve(static_cast<size_t>(result->GetRowCount()));
    do
    {
        Field* fields = result->Fetch();
        BotNamePoolRow row{};
        row.gender_index = 0;
        std::string name = fields[0].GetString();
        CopyCString(row.name, sizeof(row.name), name.c_str());
        row.is_taken = true;
        rows.push_back(row);
    } while (result->NextRow());

    return HandOff(rows, out_rows);
}

bool RandomFactoryBridge::CB_QueryRandomBotName(uint8_t race_and_gender,
                                                char* out_buf,
                                                uint32_t buf_len)
{
    if (!out_buf || buf_len == 0)
        return false;
    out_buf[0] = '\0';

    auto result = CharacterDatabase.PQuery(
        "SELECT n.name FROM ai_playerbot_names n "
        "LEFT OUTER JOIN characters e ON e.name = n.name "
        "WHERE e.guid IS NULL and n.gender = '%u' "
        "ORDER BY rand() LIMIT 1",
        static_cast<uint32>(race_and_gender));
    if (!result)
        return false;

    Field* fields = result->Fetch();
    std::string name = fields[0].GetString();
    if (name.empty())
        return false;

    CopyCString(out_buf, buf_len, name.c_str());
    return true;
}

// ── Character creation ─────────────────────────────────────────────────

bool RandomFactoryBridge::CB_QueryCharAppearance(uint8_t race, uint8_t gender,
                                                 BotCharAppearance* out)
{
    if (!out)
        return false;
    std::memset(out, 0, sizeof(*out));

    std::vector<uint8> skinColors, facialHairTypes;
    std::vector<std::pair<uint8, uint8>> faces, hairs;

    for (CharSectionsMap::const_iterator itr = sCharSectionMap.begin();
         itr != sCharSectionMap.end(); ++itr)
    {
        CharSectionsEntry const* entry = itr->second;
        if (entry->Race != race || entry->Gender != gender)
            continue;

#ifndef MANGOSBOT_TWO
        switch (entry->BaseSection)
        {
            case SECTION_TYPE_SKIN:
                skinColors.push_back(entry->ColorIndex);
                break;
            case SECTION_TYPE_FACE:
                faces.emplace_back(entry->VariationIndex, entry->ColorIndex);
                break;
            case SECTION_TYPE_FACIAL_HAIR:
                facialHairTypes.push_back(entry->ColorIndex);
                break;
            case SECTION_TYPE_HAIR:
                hairs.emplace_back(entry->VariationIndex, entry->ColorIndex);
                break;
            default:
                break;
        }
#else
        switch (entry->BaseSection)
        {
            case SECTION_TYPE_SKIN:
                skinColors.push_back(entry->Color);
                break;
            case SECTION_TYPE_FACE:
                faces.emplace_back(entry->VariationIndex, entry->Color);
                break;
            case SECTION_TYPE_FACIAL_HAIR:
                facialHairTypes.push_back(entry->Color);
                break;
            case SECTION_TYPE_HAIR:
                hairs.emplace_back(entry->VariationIndex, entry->Color);
                break;
            default:
                break;
        }
#endif
    }

    if (skinColors.empty() || faces.empty() || hairs.empty())
        return false;

    std::pair<uint8, uint8> face = faces[urand(0, faces.size() - 1)];
    std::pair<uint8, uint8> hair = hairs[urand(0, hairs.size() - 1)];

    // Matches the legacy exclusion: tauren never get facial hair, and
    // non-nightelf / non-undead females never do either. On WotLK the
    // slot is always zero because the DBC layout no longer exposes a
    // per-facial-hair color index for random bots (see legacy TODO).
    const bool excludeCheck =
        (race == RACE_TAUREN) ||
        (gender == GENDER_FEMALE && race != RACE_NIGHTELF && race != RACE_UNDEAD);

#ifndef MANGOSBOT_TWO
    uint8 facialHair = 0;
    if (!excludeCheck && !facialHairTypes.empty())
        facialHair = facialHairTypes[urand(0, facialHairTypes.size() - 1)];
#else
    (void)excludeCheck;
    uint8 facialHair = 0;
#endif

    // Legacy Player::Create passes `face.second` as the skin color slot
    // and `face.first` as the face id. Expose both slots from the pair
    // so the Rust side forwards them unchanged.
    out->skin_color  = face.second;
    out->face_id     = face.first;
    out->face_color  = face.second;
    out->hair_style  = hair.first;
    out->hair_color  = hair.second;
    out->facial_hair = facialHair;
    out->success     = true;
    return true;
}

bool RandomFactoryBridge::CB_CreateRandomBotCharacter(const BotCreateParams* params)
{
    if (!params)
        return false;

    WorldSession* session = new WorldSession(params->account_id, nullptr, SEC_PLAYER,
#ifdef MANGOSBOT_TWO
        2, 0, LOCALE_enUS, "", 0, 0, false);
#endif
#ifdef MANGOSBOT_ONE
        2, 0, LOCALE_enUS, "", 0, 0, false);
#endif
#ifdef MANGOSBOT_ZERO
        0, LOCALE_enUS, "", 0);
#endif

    session->SetNoAnticheat();

    Player* player = new Player(session);
    if (!player || !session)
    {
        sLog.outError(
            "BOTS: Unable to create session or player for random acc %u - name: \"%s\"; race: %u; class: %u",
            params->account_id, params->name, params->race, params->cls);
        delete player;
        delete session;
        return false;
    }

    if (!player->Create(sObjectMgr.GeneratePlayerLowGuid(), params->name,
                        params->race, params->cls, params->gender,
                        params->skin_color, // face.second
                        params->face_id,    // face.first
                        params->hair_style, // hair.first
                        params->hair_color, // hair.second
                        params->facial_hair, 0))
    {
        player->DeleteFromDB(player->GetObjectGuid(), params->account_id, true, true);
        delete player;
        delete session;
        sLog.outError(
            "Unable to create random bot for account %u - name: \"%s\"; race: %u; class: %u",
            params->account_id, params->name, params->race, params->cls);
        return false;
    }

    player->setCinematic(2);
    player->SetAtLoginFlag(AT_LOGIN_NONE);
    sObjectAccessor.AddObject(player);

    sLog.outDebug(
        "Random bot created for account %u - name: \"%s\"; race: %u; class: %u",
        params->account_id, params->name, params->race, params->cls);
    return true;
}

void RandomFactoryBridge::CB_SaveAndLogoutAllOnline(void)
{
    // First pass: synchronously SaveToDB every in-world player. The
    // legacy code fanned this out with std::async; we run it straight-
    // line and let the CharacterDatabase worker thread absorb the
    // burst, which matches the current bridge threading model.
    std::vector<Player*> players;
    for (const auto& pl : sObjectAccessor.GetPlayers())
    {
        if (Player* p = pl.second)
        {
            p->SaveToDB();
            players.push_back(p);
        }
    }

    // Second pass: log each player out and tear down the session we
    // allocated in CB_CreateRandomBotCharacter.
    for (Player* player : players)
    {
        WorldSession* session = player->GetSession();
        if (session)
            session->LogoutPlayer();
        sObjectAccessor.RemoveObject(player);
        delete player;
        delete session;
    }
}

// ── Guild enumeration / creation ───────────────────────────────────────

uint32_t RandomFactoryBridge::CB_QueryCharacterAccounts(BotCharacterAccount** out_rows)
{
    if (out_rows)
        *out_rows = nullptr;

    auto result = CharacterDatabase.Query(
        "select `account`, `guid` from `characters`");
    if (!result)
        return 0;

    std::vector<BotCharacterAccount> rows;
    rows.reserve(static_cast<size_t>(result->GetRowCount()));
    do
    {
        Field* fields = result->Fetch();
        BotCharacterAccount row{};
        row.account_id = fields[0].GetUInt32();
        row.guid_low   = fields[1].GetUInt32();
        rows.push_back(row);
    } while (result->NextRow());

    return HandOff(rows, out_rows);
}

void RandomFactoryBridge::CB_FreeCharacterAccountList(BotCharacterAccount* rows,
                                                     uint32_t /*count*/)
{
    std::free(rows);
}

uint32_t RandomFactoryBridge::CB_GetGuildIdByLeader(uint32_t leader_guid_low)
{
    ObjectGuid leader(HIGHGUID_PLAYER, leader_guid_low);
    Guild* guild = sGuildMgr.GetGuildByLeader(leader);
    return guild ? guild->GetId() : 0;
}

uint32_t RandomFactoryBridge::CB_GetGuildLeaderAccountId(uint32_t guild_id)
{
    Guild* guild = sGuildMgr.GetGuildById(guild_id);
    if (!guild)
        return 0;
    return sObjectMgr.GetPlayerAccountIdByGUID(guild->GetLeaderGuid());
}

void RandomFactoryBridge::CB_DisbandGuild(uint32_t guild_id)
{
    Guild* guild = sGuildMgr.GetGuildById(guild_id);
    if (guild)
        guild->Disband();
}

bool RandomFactoryBridge::CB_GetPlayerSnapshot(uint32_t guid_low,
                                               BotPlayerSnapshot* out)
{
    if (!out)
        return false;
    std::memset(out, 0, sizeof(*out));

    Player* player = FindPlayerByLow(guid_low);
    if (!player)
    {
        out->in_world = false;
        return false;
    }

    out->in_world = true;
    out->level    = player->GetLevel();
    out->guild_id = player->GetGuildId();
    out->team     = (player->GetTeam() == HORDE) ? 1u : 0u;
    CopyCString(out->name, sizeof(out->name), player->GetName());
    return true;
}

bool RandomFactoryBridge::CB_QueryRandomGuildName(char* out_buf, uint32_t buf_len)
{
    if (!out_buf || buf_len == 0)
        return false;
    out_buf[0] = '\0';

    auto maxResult = CharacterDatabase.Query(
        "SELECT MAX(name_id) FROM ai_playerbot_guild_names");
    if (!maxResult)
        return false;

    Field* maxFields = maxResult->Fetch();
    uint32 maxId = maxFields[0].GetUInt32();
    uint32 id = urand(0, maxId);

    auto result = CharacterDatabase.PQuery(
        "SELECT n.name FROM ai_playerbot_guild_names n "
        "LEFT OUTER JOIN guild e ON e.name = n.name "
        "WHERE e.guildid IS NULL AND n.name_id >= '%u' LIMIT 1",
        id);
    if (!result)
        return false;

    Field* fields = result->Fetch();
    std::string name = fields[0].GetString();
    if (name.empty())
        return false;

    CopyCString(out_buf, buf_len, name.c_str());
    return true;
}

uint32_t RandomFactoryBridge::CB_CreateRandomGuild(uint32_t leader_guid_low,
                                                    const char* name)
{
    if (!name || !*name)
        return 0;

    ObjectGuid leader(HIGHGUID_PLAYER, leader_guid_low);
    Player* player = sObjectMgr.GetPlayer(leader);
    if (!player || player->GetGuildId())
        return 0;

    Guild* guild = new Guild();
    std::string guildName = name;
    if (!guild->Create(player, guildName))
    {
        delete guild;
        sLog.outError("Error creating random guild %s", name);
        return 0;
    }

    sGuildMgr.AddGuild(guild);
    return guild->GetId();
}

void RandomFactoryBridge::CB_SetGuildEmblem(uint32_t guild_id,
                                             uint32_t style, uint32_t color,
                                             uint32_t border_style,
                                             uint32_t border_color,
                                             uint32_t background_color)
{
    Guild* guild = sGuildMgr.GetGuildById(guild_id);
    if (!guild)
        return;
    guild->SetEmblem(style, color, border_style, border_color, background_color);
}

void RandomFactoryBridge::CB_SetGuildInfo(uint32_t guild_id, const char* info)
{
    Guild* guild = sGuildMgr.GetGuildById(guild_id);
    if (!guild || !info)
        return;
    guild->SetGINFO(std::string(info));
}

// ── Arena team (TBC/WotLK only) ────────────────────────────────────────

#ifndef MANGOSBOT_ZERO

uint32_t RandomFactoryBridge::CB_QueryRandomBotAddEvent(uint32_t** out_guids)
{
    if (out_guids)
        *out_guids = nullptr;

    auto result = CharacterDatabase.Query(
        "select `bot` from ai_playerbot_random_bots where event = 'add'");
    if (!result)
        return 0;

    std::vector<uint32_t> guids;
    guids.reserve(static_cast<size_t>(result->GetRowCount()));
    do
    {
        Field* fields = result->Fetch();
        guids.push_back(fields[0].GetUInt32());
    } while (result->NextRow());

    return HandOff(guids, out_guids);
}

bool RandomFactoryBridge::CB_GetArenaTeamByCaptain(uint32_t captain_guid_low,
                                                   uint32_t* out_team_id,
                                                   uint8_t* out_type)
{
    if (out_team_id) *out_team_id = 0;
    if (out_type)    *out_type    = 0;

    ObjectGuid captain(HIGHGUID_PLAYER, captain_guid_low);
    ArenaTeam* team = sObjectMgr.GetArenaTeamByCaptain(captain);
    if (!team)
        return false;

    if (out_team_id) *out_team_id = team->GetId();
    if (out_type)    *out_type    = static_cast<uint8_t>(team->GetType());
    return true;
}

void RandomFactoryBridge::CB_DisbandArenaTeam(uint32_t team_id)
{
    ArenaTeam* team = sObjectMgr.GetArenaTeamById(team_id);
    if (team)
        team->Disband(nullptr);
}

uint32_t RandomFactoryBridge::CB_GetPlayerArenaTeamIdByType(uint32_t guid_low,
                                                             uint8_t team_type)
{
    Player* player = FindPlayerByLow(guid_low);
    if (!player)
        return 0;
    uint8 slot = ArenaTeam::GetSlotByType(static_cast<ArenaType>(team_type));
    return player->GetArenaTeamId(slot);
}

bool RandomFactoryBridge::CB_QueryRandomArenaTeamName(char* out_name,
                                                      uint32_t buf_len,
                                                      uint8_t* out_type)
{
    if (!out_name || buf_len == 0 || !out_type)
        return false;
    out_name[0] = '\0';
    *out_type = 0;

    auto maxResult = CharacterDatabase.Query(
        "SELECT MAX(name_id) FROM ai_playerbot_arena_team_names");
    if (!maxResult)
        return false;

    Field* maxFields = maxResult->Fetch();
    uint32 maxId = maxFields[0].GetUInt32();
    uint32 id = urand(0, maxId);

    // Combine the legacy two-step query (name + type) into a single
    // row so we avoid an extra round-trip for each candidate.
    auto result = CharacterDatabase.PQuery(
        "SELECT n.name, n.type FROM ai_playerbot_arena_team_names n "
        "LEFT OUTER JOIN arena_team e ON e.name = n.name "
        "WHERE e.arenateamid IS NULL AND n.name_id >= '%u' LIMIT 1",
        id);
    if (!result)
        return false;

    Field* fields = result->Fetch();
    std::string name = fields[0].GetString();
    if (name.empty())
        return false;

    CopyCString(out_name, buf_len, name.c_str());
    *out_type = fields[1].GetUInt8();
    return true;
}

uint32_t RandomFactoryBridge::CB_CreateArenaTeam(uint32_t captain_guid_low,
                                                  uint8_t team_type,
                                                  const char* name)
{
    if (!name || !*name)
        return 0;

    ObjectGuid captain(HIGHGUID_PLAYER, captain_guid_low);
    Player* player = sObjectMgr.GetPlayer(captain);
    if (!player)
        return 0;

    ArenaTeam* team = new ArenaTeam();
    std::string teamName = name;
    if (!team->Create(player->GetObjectGuid(),
                      static_cast<ArenaType>(team_type), teamName))
    {
        delete team;
        sLog.outError("Error creating arena team %s", name);
        return 0;
    }

    team->SetCaptain(player->GetObjectGuid());
    sObjectMgr.AddArenaTeam(team);
    return team->GetId();
}

bool RandomFactoryBridge::CB_AddArenaTeamMember(uint32_t team_id,
                                                uint32_t member_guid_low)
{
    ArenaTeam* team = sObjectMgr.GetArenaTeamById(team_id);
    if (!team)
        return false;
    ObjectGuid member(HIGHGUID_PLAYER, member_guid_low);
    return team->AddMember(member);
}

uint32_t RandomFactoryBridge::CB_GetArenaTeamMembersSize(uint32_t team_id)
{
    ArenaTeam* team = sObjectMgr.GetArenaTeamById(team_id);
    if (!team)
        return 0;
    return team->GetMembersSize();
}

void RandomFactoryBridge::CB_SetArenaTeamEmblem(uint32_t team_id,
                                                 uint32_t background_color,
                                                 uint32_t style, uint32_t color,
                                                 uint32_t border_style,
                                                 uint32_t border_color)
{
    ArenaTeam* team = sObjectMgr.GetArenaTeamById(team_id);
    if (!team)
        return;
    team->SetEmblem(background_color, style, color, border_style, border_color);
}

void RandomFactoryBridge::CB_SetArenaTeamRating(uint32_t team_id, uint32_t rating)
{
    ArenaTeam* team = sObjectMgr.GetArenaTeamById(team_id);
    if (!team)
        return;
    team->SetRatingForAll(rating);
}

void RandomFactoryBridge::CB_SaveArenaTeam(uint32_t team_id)
{
    ArenaTeam* team = sObjectMgr.GetArenaTeamById(team_id);
    if (!team)
        return;
    team->SaveToDB();
}

#else

// Classic stubs — the arena callbacks are NULL in MakeCallbacks() and
// the Rust side must not reach these, but keep the symbols so
// linking succeeds if they ever do.

uint32_t RandomFactoryBridge::CB_QueryRandomBotAddEvent(uint32_t** out_guids)
{
    if (out_guids) *out_guids = nullptr;
    return 0;
}

bool RandomFactoryBridge::CB_GetArenaTeamByCaptain(uint32_t, uint32_t* out_team_id, uint8_t* out_type)
{
    if (out_team_id) *out_team_id = 0;
    if (out_type)    *out_type    = 0;
    return false;
}

void     RandomFactoryBridge::CB_DisbandArenaTeam(uint32_t) {}
uint32_t RandomFactoryBridge::CB_GetPlayerArenaTeamIdByType(uint32_t, uint8_t) { return 0; }

bool RandomFactoryBridge::CB_QueryRandomArenaTeamName(char* out_name, uint32_t buf_len,
                                                      uint8_t* out_type)
{
    if (out_name && buf_len) out_name[0] = '\0';
    if (out_type)            *out_type   = 0;
    return false;
}

uint32_t RandomFactoryBridge::CB_CreateArenaTeam(uint32_t, uint8_t, const char*) { return 0; }
bool     RandomFactoryBridge::CB_AddArenaTeamMember(uint32_t, uint32_t) { return false; }
uint32_t RandomFactoryBridge::CB_GetArenaTeamMembersSize(uint32_t) { return 0; }
void     RandomFactoryBridge::CB_SetArenaTeamEmblem(uint32_t, uint32_t, uint32_t,
                                                     uint32_t, uint32_t, uint32_t) {}
void     RandomFactoryBridge::CB_SetArenaTeamRating(uint32_t, uint32_t) {}
void     RandomFactoryBridge::CB_SaveArenaTeam(uint32_t) {}

#endif

// ── Debug logging ──────────────────────────────────────────────────────

void RandomFactoryBridge::CB_LogDebug(const char* msg)
{
    if (msg)
        sLog.outBasic("%s", msg);
}

// ── Vtable builder ─────────────────────────────────────────────────────

RandomFactoryCallbacks RandomFactoryBridge::MakeCallbacks()
{
    RandomFactoryCallbacks cbs{};

    cbs.urand_range                  = &RandomFactoryBridge::CB_UrandRange;
    cbs.query_bot_delete_event       = &RandomFactoryBridge::CB_QueryBotDeleteEvent;

    cbs.query_account_id_by_name     = &RandomFactoryBridge::CB_QueryAccountIdByName;
    cbs.query_account_ids_like_prefix = &RandomFactoryBridge::CB_QueryAccountIdsLikePrefix;
    cbs.free_u32_list                = &RandomFactoryBridge::CB_FreeU32List;
    cbs.create_account               = &RandomFactoryBridge::CB_CreateAccount;
    cbs.delete_account               = &RandomFactoryBridge::CB_DeleteAccount;

    cbs.get_character_count          = &RandomFactoryBridge::CB_GetCharacterCount;
    cbs.query_characters_by_account  = &RandomFactoryBridge::CB_QueryCharactersByAccount;
    cbs.query_friend_guids           = &RandomFactoryBridge::CB_QueryFriendGuids;
    cbs.on_player_login_error        = &RandomFactoryBridge::CB_OnPlayerLoginError;
    cbs.delete_character_from_db     = &RandomFactoryBridge::CB_DeleteCharacterFromDb;
    cbs.prune_random_bots_table      = &RandomFactoryBridge::CB_PruneRandomBotsTable;

    cbs.query_name_pool              = &RandomFactoryBridge::CB_QueryNamePool;
    cbs.free_name_pool_list          = &RandomFactoryBridge::CB_FreeNamePoolList;
    cbs.query_existing_character_names = &RandomFactoryBridge::CB_QueryExistingCharacterNames;
    cbs.query_random_bot_name        = &RandomFactoryBridge::CB_QueryRandomBotName;

    cbs.query_char_appearance        = &RandomFactoryBridge::CB_QueryCharAppearance;
    cbs.create_random_bot_character  = &RandomFactoryBridge::CB_CreateRandomBotCharacter;
    cbs.save_and_logout_all_online   = &RandomFactoryBridge::CB_SaveAndLogoutAllOnline;

    cbs.query_character_accounts     = &RandomFactoryBridge::CB_QueryCharacterAccounts;
    cbs.free_character_account_list  = &RandomFactoryBridge::CB_FreeCharacterAccountList;
    cbs.get_guild_id_by_leader       = &RandomFactoryBridge::CB_GetGuildIdByLeader;
    cbs.get_guild_leader_account_id  = &RandomFactoryBridge::CB_GetGuildLeaderAccountId;
    cbs.disband_guild                = &RandomFactoryBridge::CB_DisbandGuild;
    cbs.get_player_snapshot          = &RandomFactoryBridge::CB_GetPlayerSnapshot;
    cbs.query_random_guild_name      = &RandomFactoryBridge::CB_QueryRandomGuildName;
    cbs.create_random_guild          = &RandomFactoryBridge::CB_CreateRandomGuild;
    cbs.set_guild_emblem             = &RandomFactoryBridge::CB_SetGuildEmblem;
    cbs.set_guild_info               = &RandomFactoryBridge::CB_SetGuildInfo;

#ifndef MANGOSBOT_ZERO
    cbs.query_random_bot_add_event        = &RandomFactoryBridge::CB_QueryRandomBotAddEvent;
    cbs.get_arena_team_by_captain         = &RandomFactoryBridge::CB_GetArenaTeamByCaptain;
    cbs.disband_arena_team                = &RandomFactoryBridge::CB_DisbandArenaTeam;
    cbs.get_player_arena_team_id_by_type  = &RandomFactoryBridge::CB_GetPlayerArenaTeamIdByType;
    cbs.query_random_arena_team_name      = &RandomFactoryBridge::CB_QueryRandomArenaTeamName;
    cbs.create_arena_team                 = &RandomFactoryBridge::CB_CreateArenaTeam;
    cbs.add_arena_team_member             = &RandomFactoryBridge::CB_AddArenaTeamMember;
    cbs.get_arena_team_members_size       = &RandomFactoryBridge::CB_GetArenaTeamMembersSize;
    cbs.set_arena_team_emblem             = &RandomFactoryBridge::CB_SetArenaTeamEmblem;
    cbs.set_arena_team_rating             = &RandomFactoryBridge::CB_SetArenaTeamRating;
    cbs.save_arena_team                   = &RandomFactoryBridge::CB_SaveArenaTeam;
#else
    // Classic: the arena lifecycle does not exist on this expansion.
    // Leave every arena slot NULL so the Rust side short-circuits to
    // its default trait impls when (incorrectly) called.
#endif

    cbs.log_debug = &RandomFactoryBridge::CB_LogDebug;
    return cbs;
}
