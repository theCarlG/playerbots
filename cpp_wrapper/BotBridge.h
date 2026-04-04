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
    void        CB_FreeUnitList(UnitHandle* list);

    // ── Pathfinding / positioning ──────────────────────────────────────────
    BotPosition     CB_GetBehindPosition(BotHandle bot, UnitHandle target, float distance);
    BotSafePosition CB_GetSafePosition(BotHandle bot, float search_radius);
    BotPosition     CB_GetSpreadPosition(BotHandle bot, UnitHandle center, float radius,
                                          uint8_t idx, uint8_t total);
    bool            CB_CanReach(BotHandle bot, float x, float y, float z);

    // ── Commands ───────────────────────────────────────────────────────────
    bool CB_CastSpell(BotHandle bot, uint32_t spell_id, UnitHandle target);
    bool CB_CastSpellPos(BotHandle bot, uint32_t spell_id, float x, float y, float z);
    bool CB_MoveTo(BotHandle bot, float x, float y, float z);
    bool CB_Follow(BotHandle bot, UnitHandle target, float dist, float angle);
    bool CB_StopMoving(BotHandle bot);
    bool CB_Attack(BotHandle bot, UnitHandle target);
    bool CB_AutoAttack(BotHandle bot, bool enable);
    bool CB_Say(BotHandle bot, const char* msg, uint32_t lang);
    bool CB_UseItem(BotHandle bot, uint32_t item_id, UnitHandle target);
    bool CB_Taunt(BotHandle bot, UnitHandle target);

    // ── Group / raid ────────────────────────────────────────────────────────
    UnitHandle CB_GroupGetTank(BotHandle bot);
    UnitHandle CB_GroupGetHealer(BotHandle bot);
    uint8_t    CB_GroupGetRole(BotHandle bot, UnitHandle member);

    // ── Internal helpers ────────────────────────────────────────────────────
    Player* FindBot(BotHandle bot);
    Unit*   FindUnit(BotHandle bot, UnitHandle handle);
    BotUnitSnapshot FillUnitSnapshot(Unit* unit);
} // namespace BotBridge
