/**
 * LoginBridge.h — C++ side of the Phase E login queue rewrite.
 *
 * Implements the `LoginCallbacks` vtable declared in `botffi.h`. The Rust
 * `playerbot` crate owns the entire login/logout queue (selection,
 * criterion evaluation, state machine); this file is the narrow surface
 * left on the C++ side that still needs to talk to CMaNGOS:
 *
 *   1. Candidate enumeration — the SQL scan of `characters` joined
 *      against the random-bot accounts, formerly
 *      `PlayerBotLoginMgr::LoadBotsFromDb`.
 *   2. Async holder lifecycle — `CharacterDatabase.DelayQueryHolder` and
 *      the `PlayerbotLoginQueryHolder` bookkeeping.
 *   3. `RandomPlayerbotMgr` KV/state accessors — `GetValue`, `SetValue`,
 *      `GetPlayersLevel`, `GetDatabaseDelay`, etc.
 *   4. Actual `perform_login` / `perform_logout` dispatch —
 *      `sRandomPlayerbotMgr.HandlePlayerBotLoginCallback` and
 *      `sRandomPlayerbotMgr.LogoutPlayerBot`.
 *
 * Everything else that used to live in `playerbot/PlayerbotLoginMgr.cpp`
 * (the criterion evaluator, `LoginSpace` bookkeeping, the attempt loop,
 * the worker thread, the debug print plumbing) now lives in the
 * `crates/playerbot/src/login` module.
 *
 * The bridge is stateless at the API level — every function is a
 * free-standing static that looks things up from the per-call arguments.
 * A tiny internal holder map (`g_holders`) is kept alive for the duration
 * of an async `DelayQueryHolder` round trip.
 */

#pragma once

#include "botffi.h"

namespace LoginBridge
{
    /// Build the populated `LoginCallbacks` vtable the Rust login worker
    /// consumes. Every field points at a `LoginBridge::CB_*` free
    /// function defined in `LoginBridge.cpp`. Safe to call from any
    /// thread; the returned struct is a PoD value and is passed by value
    /// into `playerbot_login_init`.
    LoginCallbacks MakeCallbacks();

    // ── Candidate enumeration ────────────────────────────────────────
    uint32_t CB_QueryCandidates(const char* account_prefix, BotCandidateInfo** out_rows);
    void     CB_FreeCandidateList(BotCandidateInfo* rows, uint32_t count);

    // ── Async holder lifecycle ───────────────────────────────────────
    bool    CB_SendHolder(uint32_t account, uint32_t guid);
    uint8_t CB_GetHolderState(uint32_t guid);
    void    CB_ClearHolder(uint32_t guid);

    // ── RandomPlayerbotMgr KV / state accessors ──────────────────────
    uint32_t CB_RandomMgrGetValue(uint32_t guid, const char* key);
    void     CB_RandomMgrSetValue(uint32_t guid, const char* key, uint32_t value,
                                  int32_t valid_in_s);
    uint32_t CB_RandomMgrGetPlayersLevel(void);
    uint32_t CB_RandomMgrGetMaxOnlineBotCount(void);
    uint32_t CB_RandomMgrGetWorldMaxLevel(void);
    uint32_t CB_RandomMgrDatabaseDelayMs(void);
    void     CB_RandomMgrDatabasePing(void);

    // ── Login / logout dispatch (main thread only) ───────────────────
    bool CB_PerformLogin(uint32_t guid);
    bool CB_PerformLogout(uint32_t guid);

    // ── Debug logging ────────────────────────────────────────────────
    void CB_LogDebug(const char* msg);

    /// Build the current `BotRealPlayerInfo` snapshot from
    /// `sRandomPlayerbotMgr.GetPlayers()` into `out`. Used by the
    /// RPBM update tick before calling `playerbot_login_update`.
    /// Returns the number of entries written.
    uint32_t BuildRealPlayerSnapshot(BotRealPlayerInfo* out, uint32_t max_out);
} // namespace LoginBridge
