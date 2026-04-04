/**
 * PlayerbotRust.cpp — C++ shim that drives the Rust AI module.
 */

#include "botpch.h"
#include "PlayerbotRust.h"

#include "Entities/Player.h"

// ── Constructor / Destructor ──────────────────────────────────────────────

PlayerbotRust::PlayerbotRust(Player* bot)
    : PlayerbotAIBase()
    , m_bot(bot)
    , m_callbacks(BotBridge::MakeCallbacks())
    , m_rustState(nullptr)
{
    MANGOS_ASSERT(m_bot != nullptr);
    BotHandle handle = m_bot->GetGUID();
    m_rustState = playerbot_create(handle, &m_callbacks);
}

PlayerbotRust::~PlayerbotRust()
{
    if (m_rustState)
    {
        playerbot_destroy(m_rustState);
        m_rustState = nullptr;
    }
}

// ── AI tick ───────────────────────────────────────────────────────────────

void PlayerbotRust::UpdateAIInternal(uint32 elapsed, bool minimal)
{
    if (!m_rustState || !m_bot || !m_bot->IsInWorld())
        return;

    playerbot_update(m_rustState, static_cast<uint32_t>(elapsed), minimal);
}

// ── Push event forwarding ─────────────────────────────────────────────────

void PlayerbotRust::OnUnitSpellCast(uint64_t caster, uint32_t spell_id,
                                     uint64_t target, bool success)
{
    if (m_rustState)
        playerbot_unit_spell_cast(m_rustState, caster, spell_id, target, success);
}

void PlayerbotRust::OnAuraChanged(uint64_t unit, uint32_t spell_id,
                                   bool applied, uint8_t stacks)
{
    if (m_rustState)
        playerbot_aura_changed(m_rustState, unit, spell_id, applied, stacks);
}

void PlayerbotRust::OnUnitDied(uint64_t victim, uint64_t killer)
{
    if (m_rustState)
        playerbot_unit_died(m_rustState, victim, killer);
}

void PlayerbotRust::OnDamageTaken(uint32_t damage, uint32_t spell_id, uint64_t dealer)
{
    if (m_rustState)
        playerbot_damage_taken(m_rustState, damage, spell_id, dealer);
}
