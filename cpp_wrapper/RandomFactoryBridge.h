/**
 * RandomFactoryBridge.h — C++ side of the Phase G random-bot factory
 * rewrite.
 *
 * Implements the `RandomFactoryCallbacks` vtable declared in `botffi.h`.
 * The Rust `playerbot` crate now owns every decision that used to live
 * in `playerbot/RandomPlayerbotFactory.cpp`: race/class availability,
 * probability-weighted race/class selection, name-pool expansion via
 * the bijective-base-26 postfix helper, and orchestration of the three
 * public entry points (`create_random_bots`, `create_random_guilds`,
 * `create_random_arena_teams`). This file is the narrow surface left on
 * the C++ side that still needs to talk to CMaNGOS:
 *
 *   1. DB scans over `account`, `characters`, `character_social`,
 *      `ai_playerbot_names`, `ai_playerbot_guild_names`, and
 *      `ai_playerbot_arena_team_names`.
 *   2. Account create / delete via `sAccountMgr`.
 *   3. `Player::Create` / `Player::DeleteFromDB` +
 *      `sObjectAccessor` bookkeeping for the per-character pass.
 *   4. Guild create / disband / emblem via `sGuildMgr` + `Guild::*`.
 *   5. `ArenaTeam::*` lifecycle (TBC/WotLK only — the Classic build
 *      leaves the arena callbacks as NULL pointers).
 *   6. A `urand` wrapper and a debug-log sink.
 *
 * Memory ownership: every `query_*_list`-shaped callback allocates a
 * freshly-malloc'd array that the C++ bridge owns; Rust releases it via
 * the matching `free_*_list` callback before returning control.
 *
 * Threading: all callbacks run from the thread that drives the Rust
 * orchestration entry points, which matches the legacy C++ behaviour.
 * No background worker is spawned — the legacy `std::async` fan-out for
 * `CreateAccount` and `SaveToDB` has been folded into straight-line
 * synchronous calls; CMaNGOS' own `CharacterDatabase` queue absorbs the
 * burst. The bridge is therefore safe to implement with ordinary
 * blocking DB calls.
 */

#pragma once

#include "botffi.h"

namespace RandomFactoryBridge
{
    /// Build the populated `RandomFactoryCallbacks` vtable the Rust
    /// random factory consumes. Every field points at a
    /// `RandomFactoryBridge::CB_*` free function defined in
    /// `RandomFactoryBridge.cpp`. Safe to call from any thread; the
    /// returned struct is a PoD value and is passed by value into
    /// `playerbot_random_factory_init`.
    RandomFactoryCallbacks MakeCallbacks();

    // ── RNG ─────────────────────────────────────────────────────────
    uint32_t CB_UrandRange(uint32_t min, uint32_t max);

    // ── Bot delete event ────────────────────────────────────────────
    void CB_QueryBotDeleteEvent(BotDeleteEventState* out);

    // ── Account scan / create / delete ──────────────────────────────
    uint32_t CB_QueryAccountIdByName(const char* name);
    uint32_t CB_QueryAccountIdsLikePrefix(const char* prefix, uint32_t** out_ids);
    void     CB_FreeU32List(uint32_t* ptr, uint32_t count);
    void     CB_CreateAccount(const char* name, const char* password, uint8_t max_expansion);
    void     CB_DeleteAccount(uint32_t account_id);

    // ── Character count / query / friend list ──────────────────────
    uint32_t CB_GetCharacterCount(uint32_t account_id);
    uint32_t CB_QueryCharactersByAccount(uint32_t account_id, uint32_t** out_guids);
    uint32_t CB_QueryFriendGuids(uint32_t** out_guids);
    void     CB_OnPlayerLoginError(uint32_t guid_low);
    void     CB_DeleteCharacterFromDb(uint32_t guid_low, uint32_t account_id);
    void     CB_PruneRandomBotsTable(void);

    // ── Name pool ──────────────────────────────────────────────────
    uint32_t CB_QueryNamePool(BotNamePoolRow** out_rows);
    void     CB_FreeNamePoolList(BotNamePoolRow* rows, uint32_t count);
    uint32_t CB_QueryExistingCharacterNames(BotNamePoolRow** out_rows);
    bool     CB_QueryRandomBotName(uint8_t race_and_gender, char* out_buf, uint32_t buf_len);

    // ── Character creation ─────────────────────────────────────────
    bool CB_QueryCharAppearance(uint8_t race, uint8_t gender, BotCharAppearance* out);
    bool CB_CreateRandomBotCharacter(const BotCreateParams* params);
    void CB_SaveAndLogoutAllOnline(void);

    // ── Guild enumeration / creation ───────────────────────────────
    uint32_t CB_QueryCharacterAccounts(BotCharacterAccount** out_rows);
    void     CB_FreeCharacterAccountList(BotCharacterAccount* rows, uint32_t count);
    uint32_t CB_GetGuildIdByLeader(uint32_t leader_guid_low);
    uint32_t CB_GetGuildLeaderAccountId(uint32_t guild_id);
    void     CB_DisbandGuild(uint32_t guild_id);
    bool     CB_GetPlayerSnapshot(uint32_t guid_low, BotPlayerSnapshot* out);
    bool     CB_QueryRandomGuildName(char* out_buf, uint32_t buf_len);
    uint32_t CB_CreateRandomGuild(uint32_t leader_guid_low, const char* name);
    void     CB_SetGuildEmblem(uint32_t guild_id, uint32_t style, uint32_t color,
                                uint32_t border_style, uint32_t border_color,
                                uint32_t background_color);
    void     CB_SetGuildInfo(uint32_t guild_id, const char* info);

    // ── Arena team (TBC/WotLK only — NULL on Classic) ──────────────
    uint32_t CB_QueryRandomBotAddEvent(uint32_t** out_guids);
    bool     CB_GetArenaTeamByCaptain(uint32_t captain_guid_low,
                                       uint32_t* out_team_id, uint8_t* out_type);
    void     CB_DisbandArenaTeam(uint32_t team_id);
    uint32_t CB_GetPlayerArenaTeamIdByType(uint32_t guid_low, uint8_t team_type);
    bool     CB_QueryRandomArenaTeamName(char* out_name, uint32_t buf_len, uint8_t* out_type);
    uint32_t CB_CreateArenaTeam(uint32_t captain_guid_low, uint8_t team_type, const char* name);
    bool     CB_AddArenaTeamMember(uint32_t team_id, uint32_t member_guid_low);
    uint32_t CB_GetArenaTeamMembersSize(uint32_t team_id);
    void     CB_SetArenaTeamEmblem(uint32_t team_id,
                                    uint32_t background_color, uint32_t style,
                                    uint32_t color, uint32_t border_style,
                                    uint32_t border_color);
    void     CB_SetArenaTeamRating(uint32_t team_id, uint32_t rating);
    void     CB_SaveArenaTeam(uint32_t team_id);

    // ── Debug logging ──────────────────────────────────────────────
    void CB_LogDebug(const char* msg);
} // namespace RandomFactoryBridge
