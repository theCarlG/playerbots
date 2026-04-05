/**
 * PlayerbotRust.cpp — C++ shim that drives the Rust AI module.
 */

#include "botpch.h"
#include "PlayerbotRust.h"

#include "Entities/Player.h"
#include "Log/Log.h"

// ── Log sink bridge ──────────────────────────────────────────────────────
//
// Rust calls this via a function pointer installed by playerbot_set_log_sink.
// Level values mirror PLAYERBOT_LOG_{ERROR,WARN,INFO,DEBUG} in botffi.h.
static void PlayerbotRustLogSink(uint8_t level, const char* msg)
{
    if (!msg) return;
    switch (level)
    {
        case PLAYERBOT_LOG_ERROR: sLog.outError("%s", msg);  break;
        case PLAYERBOT_LOG_WARN:  sLog.outBasic("%s", msg);  break;
        case PLAYERBOT_LOG_INFO:  sLog.outDetail("%s", msg); break;
        case PLAYERBOT_LOG_DEBUG: sLog.outDebug("%s", msg);  break;
        default:                  sLog.outBasic("%s", msg);  break;
    }
}

// ── Global init / shutdown ───────────────────────────────────────────────

void PlayerbotRust::InitRustModule()
{
    playerbot_set_log_sink(&PlayerbotRustLogSink);
    playerbot_init();
    // TODO: read config values from PlayerbotAIConfig and pass them
    // playerbot_set_config(react_delay, max_wait, eat_hp, drink_mana, debug);
}

void PlayerbotRust::ShutdownRustModule()
{
    playerbot_shutdown();
}

void PlayerbotRust::WorldUpdate(uint32_t elapsed_ms)
{
    playerbot_world_update(elapsed_ms);
}

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

// ── Command handling ─────────────────────────────────────────────────────

void PlayerbotRust::HandleCommand(uint32 /*type*/, const std::string& text,
                                   Player& sender, uint32 /*lang*/)
{
    if (!m_rustState || text.empty())
        return;

    uint64_t senderGuid = sender.GetObjectGuid().GetRawValue();

    // Privileged = owner, party leader, or GM. Mirrors the old
    // PlayerbotSecurity check without the SQL access-level layer.
    bool privileged = false;
    if (m_bot)
    {
        ObjectGuid masterGuid = m_bot->GetSession() && m_bot->GetSession()->GetPlayer() ?
            m_bot->GetSession()->GetPlayer()->GetObjectGuid() : ObjectGuid();
        if (sender.GetObjectGuid() == masterGuid) privileged = true;
        if (!privileged && m_bot->GetGroup() && m_bot->GetGroup()->IsLeader(sender.GetObjectGuid()))
            privileged = true;
        if (!privileged && sender.GetSession() && sender.GetSession()->GetSecurity() > SEC_PLAYER)
            privileged = true;
    }

    playerbot_chat_command(m_rustState, senderGuid,
                           privileged ? 1 : 0, text.c_str());
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

void PlayerbotRust::ClearInventoryViaRust(uint8_t mode)
{
    if (m_rustState)
        playerbot_factory_clear_inventory(m_rustState, mode);
}

void PlayerbotRust::InitConsumablesViaRust(uint8_t kind)
{
    if (m_rustState)
        playerbot_factory_init_consumables(m_rustState, kind);
}

void PlayerbotRust::ResetProgressionViaRust(uint8_t kind)
{
    if (m_rustState)
        playerbot_factory_reset_progression(m_rustState, kind);
}

void PlayerbotRust::FactoryMiscViaRust(uint8_t kind)
{
    if (m_rustState)
        playerbot_factory_misc(m_rustState, kind);
}

void PlayerbotRust::FactoryInitTalentsViaRust(uint32_t spec_no)
{
    if (m_rustState)
        playerbot_factory_init_talents(m_rustState, spec_no);
}
