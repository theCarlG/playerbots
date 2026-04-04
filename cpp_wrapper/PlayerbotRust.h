/**
 * PlayerbotRust.h — Drop-in replacement for PlayerbotAI.
 *
 * Inherits PlayerbotAIBase so CMaNGOS calls it identically.
 * All AI logic is delegated to the Rust module via playerbot_create/update/destroy.
 *
 * Usage:
 *   // In PlayerbotMgr::AddPlayerBot or equivalent:
 *   bot->SetPlayerbotAI(new PlayerbotRust(bot));
 */

#pragma once

#include "playerbot/PlayerbotAIBase.h"
#include "botffi.h"
#include "BotBridge.h"

class Player;

class PlayerbotRust : public PlayerbotAIBase
{
public:
    /**
     * Create the Rust AI state for `bot`. The BotCallbacks vtable is built
     * once here and passed to playerbot_create — it lives for the object's
     * lifetime since it points only to static functions.
     */
    explicit PlayerbotRust(Player* bot);

    /**
     * Destroy the Rust AI state. Must be called before the Player object
     * is destroyed (e.g. in ~PlayerbotMgr or ~Player::RemovePlayerbotAI).
     */
    virtual ~PlayerbotRust();

    // ── PlayerbotAIBase interface ────────────────────────────────────────

    /**
     * Main AI tick — delegates to playerbot_update.
     * Called from Player::UpdateAI on the map worker thread.
     */
    void UpdateAIInternal(uint32 elapsed, bool minimal = false) override;

    // ── Push event forwarding ────────────────────────────────────────────
    // Called by BotBridge hooks registered in the CMaNGOS event system.
    // These forward immediately to Rust — no buffering in C++.

    void OnUnitSpellCast(uint64_t caster, uint32_t spell_id, uint64_t target, bool success);
    void OnAuraChanged(uint64_t unit, uint32_t spell_id, bool applied, uint8_t stacks);
    void OnUnitDied(uint64_t victim, uint64_t killer);
    void OnDamageTaken(uint32_t damage, uint32_t spell_id, uint64_t dealer);

    // ── Accessors ─────────────────────────────────────────────────────────
    Player* GetBot() const { return m_bot; }

private:
    Player*      m_bot;           // the CMaNGOS Player this AI drives
    BotCallbacks m_callbacks;     // the vtable passed to playerbot_create
    void*        m_rustState;     // opaque BotState* from playerbot_create
};
