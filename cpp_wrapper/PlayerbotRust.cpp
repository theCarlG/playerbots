/**
 * PlayerbotRust.cpp — C++ shim that drives the Rust AI module.
 */

#include "botpch.h"
#include "PlayerbotRust.h"

#include "Entities/Player.h"
#include "Globals/ObjectAccessor.h"
#include "Groups/Group.h"
#include "Log/Log.h"
#include "Server/WorldSession.h"
#include "Server/Opcodes.h"
#include "World/World.h"
#include "Server/WorldPacket.h"
#include "BotConfig.h"
#include "MotionGenerators/MotionMaster.h"
#include "MotionGenerators/MovementGenerator.h"
#include "BotBridge.h"
#include "Spells/Spell.h"

// Spell id used by the RTSC "Aedm" marker to encode ground-targeted positions.
static const uint32_t RTSC_MOVE_SPELL = 30758;

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
    // Phase D: the Rust side parses the `.conf` file itself via
    // `playerbot_config_load()` — called from `PlayerbotAIConfig::Initialize()`
    // in `cpp_wrapper/BotConfig.cpp`. No per-field forwarding required here.
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
    , m_masterGuid()
    , m_callbacks(BotBridge::MakeCallbacks())
    // m_rustState default-constructs to nullptr via its unique_ptr deleter.
{
    MANGOS_ASSERT(m_bot != nullptr);
    BotHandle handle = m_bot->GetGUID();
    m_rustState.reset(playerbot_create(handle, &m_callbacks));
}

// Dtor is defaulted: `m_rustState` is a unique_ptr whose deleter calls
// `playerbot_destroy`, so teardown is automatic and exception-safe.
PlayerbotRust::~PlayerbotRust() = default;

// ── AI tick ───────────────────────────────────────────────────────────────

void PlayerbotRust::UpdateAIInternal(uint32 elapsed, bool minimal)
{
    if (!m_rustState || !m_bot || !m_bot->IsInWorld())
        return;

    // Per-tick housekeeping that the Rust side can't do because it lives
    // behind the FFI. These three blocks mirror PB2's DoNextAction prelude
    // and AcceptInvitationAction:
    //
    //   1. Auto-accept pending group invites (PB2 parity).
    //   2. Drop a stale master (master logged out or left the group).
    //   3. Claim a new master when a real player joined the bot's group.
    //
    // Doing them in C++ keeps the FFI surface small: the Rust tick just
    // reads the up-to-date master_guid that we pushed via set_master.
    AutoAcceptGroupInvite();
    RefreshMaster();

    playerbot_update(m_rustState.get(), static_cast<uint32_t>(elapsed), minimal);
}

// ── Group invite auto-accept (PB2 AcceptInvitationAction parity) ─────────
//
// Any time the bot has a pending group invite (`Player::GetGroupInvite`), we
// accept it automatically if the inviter is still online and was allowed to
// invite — i.e. the sender's security tier would have been >= INVITE at the
// time they pushed the invite. We don't get a sender pointer here directly,
// so we approximate: accept invites from same-faction real players, which is
// exactly what PB2 does (it gates on `ai->IsOpposing(inviter)` first).
//
// After accepting we promote the inviter to master — matching PB2.
void PlayerbotRust::AutoAcceptGroupInvite()
{
    Group* invite = m_bot->GetGroupInvite();
    if (!invite)
        return;

    ObjectGuid inviterGuid = invite->GetLeaderGuid();
    Player* inviter = sObjectAccessor.FindPlayer(inviterGuid);
    if (!inviter)
    {
        // Leader already gone — clear the stale invite so the bot stops
        // trying every tick.
        m_bot->SetGroupInvite(nullptr);
        return;
    }

    // Opposing faction: refuse unless server allows cross-faction groups.
    if (inviter->GetTeam() != m_bot->GetTeam()
        && !sWorld.getConfig(CONFIG_BOOL_ALLOW_TWO_SIDE_INTERACTION_GROUP))
    {
        m_bot->SetGroupInvite(nullptr);
        return;
    }

    // Build the accept-opcode payload the session handler expects. WotLK
    // added a role mask; classic/TBC ignore the extra bytes but sending a
    // zero is harmless.
    WorldPacket accept(CMSG_GROUP_ACCEPT, 4);
    accept << uint32(0);
    m_bot->GetSession()->HandleGroupAcceptOpcode(accept);

    // Promote inviter to master. SetMaster pushes this into Rust and
    // resets strategies so the bot immediately starts following.
    SetMaster(inviter);
}

// ── Per-tick master refresh (PB2 DoNextAction parity) ────────────────────
//
// Two jobs:
//   1. Clear `m_masterGuid` when the recorded master is no longer a valid
//      commander — logged out, or no longer shares a group with the bot.
//   2. If the bot currently has no master but is grouped with at least one
//      real (non-bot) player, claim that player as master. This is the
//      single most important piece of PB2 parity for random bots, which
//      are never assigned a master at login and therefore have no "master"
//      to follow until something like this runs each tick.
void PlayerbotRust::RefreshMaster()
{
    Group* group = m_bot->GetGroup();

    // ── 1) Validate the current master. ───────────────────────────────
    if (!m_masterGuid.IsEmpty())
    {
        Player* master = sObjectAccessor.FindPlayer(m_masterGuid);
        bool stillValid = master != nullptr && master->IsInWorld();
        // If the bot is in a group, the master must also be in it.
        if (stillValid && group && !group->IsMember(m_masterGuid))
            stillValid = false;
        if (!stillValid)
            SetMaster(nullptr);
    }

    // ── 2) Claim a new master from the group if we don't have one. ───
    if (m_masterGuid.IsEmpty() && group)
    {
        for (GroupReference* it = group->GetFirstMember(); it; it = it->next())
        {
            Player* member = it->getSource();
            if (!member || member == m_bot)
                continue;
            // Real players only — skip other bots.
            if (member->GetPlayerbotAI())
                continue;
            if (!member->IsInWorld())
                continue;
            SetMaster(member);
            break;
        }
    }
}

// ── Master tracking ──────────────────────────────────────────────────────

void PlayerbotRust::SetMaster(Player* master)
{
    ObjectGuid newGuid = master ? master->GetObjectGuid() : ObjectGuid();
    if (newGuid == m_masterGuid)
        return;

    m_masterGuid = newGuid;

    if (m_rustState)
    {
        // Clear cached strategy state (pending chat commands, encounter FSM,
        // throttles) before telling Rust about the new master so that the
        // next tick rebuilds behaviour from scratch — PB2 parity.
        playerbot_reset_strategies(m_rustState.get());
        playerbot_set_master(m_rustState.get(), m_masterGuid.GetRawValue());
    }
}

Player* PlayerbotRust::GetMaster() const
{
    if (m_masterGuid.IsEmpty())
        return nullptr;
    return sObjectAccessor.FindPlayer(m_masterGuid);
}

bool PlayerbotRust::HasRealPlayerMaster() const
{
    if (m_masterGuid.IsEmpty())
        return false;
    Player* master = sObjectAccessor.FindPlayer(m_masterGuid);
    if (!master || !master->IsInWorld())
        return false;
    // A "real" master is a flesh-and-blood player — not another bot.
    return master->GetPlayerbotAI() == nullptr;
}

PlayerbotRust::GrouperType PlayerbotRust::GetGrouperType() const
{
    Group* group = m_bot ? m_bot->GetGroup() : nullptr;
    if (!group)
        return GrouperType::SOLO;
    if (group->GetLeaderGuid() != m_bot->GetObjectGuid())
        return GrouperType::MEMBER;
    // Bot leads — bucket by size (2..5). Raids collapse into LEADER_5.
    uint32 sz = group->GetMembersCount();
    if (sz <= 2) return GrouperType::LEADER_2;
    if (sz == 3) return GrouperType::LEADER_3;
    if (sz == 4) return GrouperType::LEADER_4;
    return GrouperType::LEADER_5;
}

void PlayerbotRust::ResetStrategies(bool /*incremental*/)
{
    if (m_rustState)
        playerbot_reset_strategies(m_rustState.get());
}

// ── Command handling ─────────────────────────────────────────────────────

BotSecurityLevel PlayerbotRust::ComputeSenderSecurity(Player& sender) const
{
    // Full port of PB2 PlayerbotSecurity::LevelFor, with the single
    // simplification that PB2's GUILD tier is collapsed into TALK (we only
    // expose four tiers on the Rust side: DENY/TALK/INVITE/ALLOW_ALL).
    //
    // The critical rule for RaidControl parity is step 6: when the bot has
    // no group, any same-faction real player gets INVITE — that's how a
    // stranger can whisper "invite" / "follow" to an unclaimed random bot
    // and have it react.
    if (!m_bot)
        return BOT_SECURITY_DENY_ALL;

    // 1) GM account — unconditional AllowAll.
    if (sender.GetSession() && sender.GetSession()->GetSecurity() >= SEC_GAMEMASTER)
        return BOT_SECURITY_ALLOW_ALL;

    // 2) Same account (owner alting their own bot).
    if (sender.GetSession() && m_bot->GetSession()
        && sender.GetSession()->GetAccountId() == m_bot->GetSession()->GetAccountId())
        return BOT_SECURITY_ALLOW_ALL;

    // 3) Recorded master is always AllowAll.
    if (!m_masterGuid.IsEmpty() && sender.GetObjectGuid() == m_masterGuid)
        return BOT_SECURITY_ALLOW_ALL;

    // 4) Bot is a member of the *sender's* group — AllowAll (PB2 early-out).
    if (Group* senderGroup = sender.GetGroup())
    {
        for (GroupReference* it = senderGroup->GetFirstMember(); it; it = it->next())
        {
            if (it->getSource() == m_bot)
                return BOT_SECURITY_ALLOW_ALL;
        }
    }

    // 5) Opposing faction — refuse unless two-side interaction is enabled.
    if (m_bot->GetTeam() != sender.GetTeam())
    {
        if (sWorld.getConfig(CONFIG_BOOL_ALLOW_TWO_SIDE_INTERACTION_GROUP))
            return BOT_SECURITY_ALLOW_ALL;
        return BOT_SECURITY_DENY_ALL;
    }

    // 6) Level gate — if the bot outlevels the sender by more than the
    //    configured threshold, drop to TALK (info queries only). Same-guild
    //    senders bypass this.
    int32 levelDiff = int32(m_bot->GetLevel()) - int32(sender.GetLevel());
    bool sameGuild  = m_bot->GetGuildId() && m_bot->GetGuildId() == sender.GetGuildId();
    if (levelDiff > sPlayerbotAIConfig.levelCheck && !sameGuild)
        return BOT_SECURITY_TALK;

    // 7) Battleground queue gate.
    if (m_bot->InBattleGroundQueue() && !sameGuild)
        return BOT_SECURITY_TALK;

    // 8) If the bot has no group, any same-faction real player gets INVITE —
    //    this is how RaidControl whispers "follow" / "invite" to an
    //    unclaimed random bot get through. It matches PB2 exactly.
    Group* botGroup = m_bot->GetGroup();
    if (!botGroup)
        return BOT_SECURITY_INVITE;

    // 9) Sender is in the bot's group — AllowAll.
    for (GroupReference* it = botGroup->GetFirstMember(); it; it = it->next())
    {
        if (it->getSource() == &sender)
            return BOT_SECURITY_ALLOW_ALL;
    }

    // 10) Bot is in a group the sender is not in. If the bot leads, it can
    //     still invite — INVITE tier. Otherwise defer to TALK (collapsed
    //     from PB2's GUILD).
    if (botGroup->GetLeaderGuid() == m_bot->GetObjectGuid())
        return BOT_SECURITY_INVITE;

    return BOT_SECURITY_TALK;
}

void PlayerbotRust::HandleCommand(uint32 type, const std::string& text,
                                   Player& sender, uint32 lang)
{
    if (!m_rustState || text.empty())
        return;

    uint64_t senderGuid = sender.GetObjectGuid().GetRawValue();
    BotSecurityLevel sec = ComputeSenderSecurity(sender);

    playerbot_chat_command(m_rustState.get(), senderGuid,
                           static_cast<uint8_t>(sec),
                           static_cast<uint32_t>(type),
                           static_cast<uint32_t>(lang),
                           text.c_str());
}

// ── Legacy management shims ──────────────────────────────────────────────
//
// These replace the previously-empty inline stubs in PlayerbotRust.h.
// PlayerbotMgr / RandomPlayerbotMgr still call them as if they were the
// old PlayerbotAI — we route each one through real CMaNGOS APIs so the
// behavior matches PB2 exactly.

bool PlayerbotRust::IsInRealGuild() const
{
    if (!m_bot)
        return false;
    // Treat any guilded bot as "in a real guild" — matches PB2's
    // factory/random-bot check, which uses this flag to decide whether a
    // bot is claimed by a player (guild) or fair game for re-rolling.
    return m_bot->GetGuildId() != 0;
}

void PlayerbotRust::StopMoving()
{
    if (!m_bot)
        return;
    MotionMaster* mm = m_bot->GetMotionMaster();
    if (!mm)
        return;
    mm->Clear();
    mm->MoveIdle();
    // Also stop the movement spline so the bot doesn't continue gliding
    // in its pre-stop direction. Without this, mm->Clear() removes the
    // generator but the active spline keeps running.
    m_bot->StopMoving();
}

void PlayerbotRust::TellPlayer(Player* target, const std::string& msg)
{
    if (!m_bot || !target || msg.empty())
        return;
    m_bot->Whisper(msg, LANG_UNIVERSAL, target->GetObjectGuid());
}

void PlayerbotRust::HandleTeleportAck()
{
    // Mirrors PB2's PlayerbotAI::HandleTeleportAck: near-teleport needs an
    // ACK packet round-trip; far-teleport just forwards the worldport ACK.
    // Either way the pre-teleport motion generator must be interrupted or
    // the bot resumes its old movement on arrival.
    if (!m_bot)
        return;

    StopMoving();

    // Clear the movement dedup cache so the next follow/chase after
    // teleport isn't falsely deduped against the pre-teleport state.
    // Without this, the bot arrives at its new position with no active
    // movement generator but the dedup thinks it's already following,
    // leaving the bot stranded (or gliding on a stale spline).
    BotBridge::ClearMoveState(m_bot->GetObjectGuid().GetRawValue());

    if (m_bot->IsBeingTeleportedNear())
    {
        if (MotionMaster* mm = m_bot->GetMotionMaster())
        {
            if (!mm->empty())
                if (MovementGenerator* movgen = mm->top())
                    movgen->Interrupt(*m_bot);
        }

        WorldPacket p(MSG_MOVE_TELEPORT_ACK, 8 + 4 + 4);
#ifdef MANGOSBOT_TWO
        p << m_bot->GetObjectGuid().WriteAsPacked();
#else
        p << m_bot->GetObjectGuid();
#endif
        p << static_cast<uint32>(0);                    // flags (unused)
        p << static_cast<uint32>(time(nullptr));        // time (unused)
        m_bot->GetSession()->HandleMoveTeleportAckOpcode(p);

        SetAIInternalUpdateDelay(urand(1000, 2000));
    }
    else if (m_bot->IsBeingTeleportedFar())
    {
        m_bot->GetSession()->HandleMoveWorldportAckOpcode();
        SetAIInternalUpdateDelay(urand(2000, 5000));
    }
}

// ── Push event forwarding ─────────────────────────────────────────────────

void PlayerbotRust::OnUnitSpellCast(uint64_t caster, uint32_t spell_id,
                                     uint64_t target, bool success)
{
    if (m_rustState)
        playerbot_unit_spell_cast(m_rustState.get(), caster, spell_id, target, success);
}

void PlayerbotRust::OnAuraChanged(uint64_t unit, uint32_t spell_id,
                                   bool applied, uint8_t stacks)
{
    if (m_rustState)
        playerbot_aura_changed(m_rustState.get(), unit, spell_id, applied, stacks);
}

void PlayerbotRust::OnUnitDied(uint64_t victim, uint64_t killer)
{
    if (m_rustState)
        playerbot_unit_died(m_rustState.get(), victim, killer);
}

void PlayerbotRust::OnDamageTaken(uint32_t damage, uint32_t spell_id, uint64_t dealer)
{
    if (m_rustState)
        playerbot_damage_taken(m_rustState.get(), damage, spell_id, dealer);
}

// ── Master packet forwarding ─────────────────────────────────────────────

void PlayerbotRust::HandleMasterIncomingPacket(const WorldPacket& packet)
{
    if (!m_rustState)
        return;
    // `ByteBuffer::contents()` dereferences `&_storage[0]`, which asserts
    // under libstdc++ debug mode when the packet body is empty. Many opcodes
    // are legitimately zero-byte (e.g. SMSG_GROUP_DESTROYED), so pass a null
    // data pointer for those — the Rust side already handles size == 0.
    const uint32_t size = static_cast<uint32_t>(packet.size());
    const uint8_t* data = size > 0 ? packet.contents() : nullptr;
    playerbot_packet_in(m_rustState.get(),
                        static_cast<uint16_t>(packet.GetOpcode()),
                        data, size);

    // RTSC: master cast "Aedm" (30758) on the ground. Decode the destination
    // position from CMSG_CAST_SPELL and hand it to the Rust side as a
    // high-level command. This mirrors PB2's `SeeSpellAction`. Packet layout
    // differs per expansion; see PB2 SeeSpellAction.cpp for reference.
    if (packet.GetOpcode() == CMSG_CAST_SPELL && size > 0)
    {
        Player* master = GetMaster();
        if (!master)
            return;
        try
        {
            WorldPacket p(packet);
            p.rpos(0);
            uint32 spellId = 0;
#ifndef MANGOSBOT_TWO
            p >> spellId;
#endif
#ifdef MANGOSBOT_ONE
            uint8 castCount;
            p >> castCount;
#endif
#ifdef MANGOSBOT_TWO
            uint8 castCount, castFlags;
            p >> castCount;
            p >> spellId;
            p >> castFlags;
#endif
            if (spellId != RTSC_MOVE_SPELL)
                return;
            SpellCastTargets targets;
            p >> targets.ReadForCaster(master);
            float x = targets.m_destPos.x;
            float y = targets.m_destPos.y;
            float z = targets.m_destPos.z;
            playerbot_rtsc_spell(m_rustState.get(), x, y, z);
        }
        catch (...)
        {
            // Malformed packet — ignore silently, the server core will
            // already have logged the underlying parse error.
        }
    }
}

void PlayerbotRust::HandleMasterOutgoingPacket(const WorldPacket& packet)
{
    if (!m_rustState)
        return;
    const uint32_t size = static_cast<uint32_t>(packet.size());
    const uint8_t* data = size > 0 ? packet.contents() : nullptr;
    playerbot_packet_out(m_rustState.get(),
                         static_cast<uint16_t>(packet.GetOpcode()),
                         data, size);
}

void PlayerbotRust::ClearInventoryViaRust(uint8_t mode)
{
    if (m_rustState)
        playerbot_factory_clear_inventory(m_rustState.get(), mode);
}

void PlayerbotRust::InitConsumablesViaRust(uint8_t kind)
{
    if (m_rustState)
        playerbot_factory_init_consumables(m_rustState.get(), kind);
}

void PlayerbotRust::ResetProgressionViaRust(uint8_t kind)
{
    if (m_rustState)
        playerbot_factory_reset_progression(m_rustState.get(), kind);
}

void PlayerbotRust::FactoryMiscViaRust(uint8_t kind)
{
    if (m_rustState)
        playerbot_factory_misc(m_rustState.get(), kind);
}

void PlayerbotRust::FactoryInitTalentsViaRust(uint32_t spec_no)
{
    if (m_rustState)
        playerbot_factory_init_talents(m_rustState.get(), spec_no);
}

void PlayerbotRust::FactoryInitTalentsTreeViaRust(bool incremental)
{
    if (m_rustState)
        playerbot_factory_init_talents_tree(m_rustState.get(), incremental);
}

bool PlayerbotRust::ToggleMonitor()
{
    if (m_rustState)
        return playerbot_toggle_monitor(m_rustState.get());
    return false;
}
