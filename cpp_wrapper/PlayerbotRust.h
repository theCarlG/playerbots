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

#include <memory>

#include "PlayerbotAIBase.h"
#include "BotConfig.h"
#include "Entities/ObjectGuid.h"
#include "botffi.h"
#include "BotBridge.h"

class Player;
class Unit;
class Item;
struct AreaTrigger;

// Chat-command security tiers. Mirrors PB2's PlayerbotSecurityLevel but
// collapses GUILD into TALK. The byte value is what the Rust FFI receives
// via playerbot_chat_command's `security` parameter.
enum BotSecurityLevel : uint8_t {
    BOT_SECURITY_DENY_ALL  = 0,
    BOT_SECURITY_TALK      = 1,
    BOT_SECURITY_INVITE    = 2,
    BOT_SECURITY_ALLOW_ALL = 3,
};

// ── Legacy helpers kept only so the stripped management code compiles ──
// These replace types that lived on the deleted PlayerbotAI / strategy
// engine. None of them do anything — all real inventory / cast / gearscore
// logic now lives in the Rust module.

enum class IterateItemsMask : uint8_t {
    ITERATE_ITEMS_IN_BAGS  = 1 << 0,
    ITERATE_ITEMS_IN_EQUIP = 1 << 1,
    ITERATE_ITEMS_IN_BANK  = 1 << 2,
    ITERATE_ALL_ITEMS      = 0xFF,
};

class IterateItemsVisitor {
public:
    IterateItemsVisitor() = default;
    virtual ~IterateItemsVisitor() = default;
    virtual bool Visit(Item* /*item*/) { return true; }
};

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

    // ── Packet forwarding ────────────────────────────────────────────────
    //
    // Called from the core's packet-dispatch hooks whenever the bot's master
    // sends or receives a WoW packet that the bot should react to
    // (movement updates, target changes, loot rolls, etc.). We hand the raw
    // buffer to the Rust side via `playerbot_packet_{in,out}`; the Rust
    // tick queues it as a `BotEvent::PacketIn/Out` and reacts from there.
    void HandleMasterIncomingPacket(const WorldPacket& packet);
    void HandleMasterOutgoingPacket(const WorldPacket& packet);

    // ── Command handling ─────────────────────────────────────────────────
    void HandleCommand(uint32 type, const std::string& text, Player& sender, uint32 lang = 0);

    /// Handle a near/far teleport ACK. Mirrors PB2's
    /// `PlayerbotAI::HandleTeleportAck` — interrupts the current movement
    /// generator, replies to the near-teleport opcode, and forwards the
    /// world-port ACK for long-distance teleports. Called from
    /// `PlayerbotMgr::UpdateSessions` whenever `IsBeingTeleported()` is
    /// true, so skipping it leaves a bot frozen in the teleport pending
    /// state forever.
    void HandleTeleportAck();

    // ── Accessors ─────────────────────────────────────────────────────────
    Player* GetBot() const { return m_bot; }
    Player* GetMaster() const;
    ObjectGuid GetMasterGuid() const { return m_masterGuid; }
    void* GetRustState() const { return m_rustState.get(); }

    // ── Management hooks (called by PlayerbotMgr/RandomPlayerbotMgr) ──
    //
    // The rest of the "legacy stub" block below is still empty because no
    // current caller consumes the return value meaningfully — they either
    // ignore it or are slated for removal as the C++ strategy code is
    // torn out. The three hooks we *do* implement are the ones PB2 uses
    // to decide whether a bot is a random/free bot versus one claimed by
    // a real player; getting them wrong makes RandomPlayerbotMgr treat
    // every bot as owned and skip wandering / re-gearing work.
    bool HasRealPlayerMaster() const;
    /// True iff the bot is a member of a guild that contains at least one
    /// real (non-bot) player. Mirrors PB2's check used by
    /// `RandomPlayerbotMgr` / `PlayerbotFactory` to decide whether a
    /// random bot should be re-rolled or left alone. For now approximated
    /// by "bot is in any guild" — guild rosters in this fork are
    /// player/bot-mixed, and the stricter check would need to walk the
    /// guild member list. The looser check matches PB2's behavior of
    /// treating guilded bots as "claimed".
    bool IsInRealGuild() const;
    bool IsRealPlayer() const { return false; }

    /// Toggle per-bot debug monitor (logs commands, BT path, settings to file).
    bool ToggleMonitor();
    bool GetShouldLogOut() const { return false; }
    /// Stop the bot's current movement. Mirrors PB2's
    /// `PlayerbotAI::StopMoving`. Called from `PlayerbotMgr` on bot
    /// removal and from `HandleTeleportAck` to clear the pre-teleport
    /// movement state.
    void StopMoving();
    /// Whisper `msg` to `target`. Real impl equivalent to PB2's
    /// `TellPlayerNoFacing` with `isPrivate=true`. Called from
    /// `PlayerbotMgr` for logout/goodbye/hello messages. `target` may be
    /// null (e.g. masterless bot) — in that case the call is a no-op.
    void TellPlayer(Player* target, const std::string& msg);
    void SetPlayerFriend(bool /*val*/) {}
    AreaTableEntry const* GetCurrentZone() const { return nullptr; }
    std::string GetLocalizedAreaName(AreaTableEntry const* /*area*/) const { return ""; }

    enum class GrouperType { SOLO, MEMBER, LEADER_2, LEADER_3, LEADER_4, LEADER_5 };
    GrouperType GetGrouperType() const;
    std::string HandleRemoteCommand(const std::string& /*cmd*/) { return ""; }
    bool HasCheat(BotCheatMask /*mask*/) const { return false; }
    /// Drop cached strategy/encounter state so the next tick rebuilds
    /// behaviour from scratch. Forwarded into Rust.
    void ResetStrategies(bool /*incremental*/ = false);
    void AllowActivity(uint32_t /*activity*/, bool /*allow*/) {}
    void SetMaster(Player* master);
    float GetLevelFloat() const { return 0.0f; }
    Unit* GetUnit(ObjectGuid /*guid*/) const { return nullptr; }

    // Stubs for legacy PlayerbotFactory. Inventory iteration, gearscore
    // calculation, direct spell casting and item enchanting are all owned
    // by the Rust module now. These keep the factory compiling until it
    // is ported / removed.
    void InventoryIterateItems(IterateItemsVisitor* /*v*/, IterateItemsMask /*mask*/) {}
    uint32_t GetEquipGearScore(Player* /*p*/, bool /*withBags*/, bool /*withBank*/) { return 0; }

    // ── Factory FFI wrappers ──────────────────────────────────────────────
    // Each method forwards a single call to the Rust factory module.
    // Inlined here to keep PlayerbotRust.cpp focused on non-trivial logic.
    void ClearInventoryViaRust(uint8_t mode)              { if (m_rustState) playerbot_factory_clear_inventory(m_rustState.get(), mode); }
    void InitConsumablesViaRust(uint8_t kind)              { if (m_rustState) playerbot_factory_init_consumables(m_rustState.get(), kind); }
    void ResetProgressionViaRust(uint8_t kind)             { if (m_rustState) playerbot_factory_reset_progression(m_rustState.get(), kind); }
    void FactoryMiscViaRust(uint8_t kind)                  { if (m_rustState) playerbot_factory_misc(m_rustState.get(), kind); }
    void FactoryInitTalentsViaRust(uint32_t spec_no)       { if (m_rustState) playerbot_factory_init_talents(m_rustState.get(), spec_no); }
    void FactoryInitTalentsTreeViaRust(bool inc)           { if (m_rustState) playerbot_factory_init_talents_tree(m_rustState.get(), inc); }
    void FactoryRefreshViaRust()                           { if (m_rustState) playerbot_factory_refresh(m_rustState.get()); }
    void FactoryPrepareViaRust(uint32_t level)             { if (m_rustState) playerbot_factory_prepare(m_rustState.get(), level); }
    void FactoryInitQuestsViaRust(const uint32_t* ids, size_t len) { if (m_rustState) playerbot_factory_init_quests(m_rustState.get(), ids, len); }
    void FactoryInitArenaTeamViaRust()                     { if (m_rustState) playerbot_factory_init_arena_team(m_rustState.get()); }
    void FactoryInitGuildViaRust()                         { if (m_rustState) playerbot_factory_init_guild(m_rustState.get()); }
    void FactoryInitAllSkillsViaRust()                     { if (m_rustState) playerbot_factory_init_all_skills(m_rustState.get()); }
    void FactoryInitTradeSkillsViaRust()                   { if (m_rustState) playerbot_factory_init_trade_skills(m_rustState.get()); }
    void FactoryInitEquipmentViaRust(uint32_t f, uint32_t q) { if (m_rustState) playerbot_factory_init_equipment(m_rustState.get(), f, q); }
    void FactoryInitPetViaRust()                           { if (m_rustState) playerbot_factory_init_pet(m_rustState.get()); }
    void FactoryInitPetSpellsViaRust()                     { if (m_rustState) playerbot_factory_init_pet_spells(m_rustState.get()); }
    void FactoryRandomizeViaRust(uint32_t level, bool inc, bool sync, uint32_t q) { if (m_rustState) playerbot_factory_randomize(m_rustState.get(), level, inc, sync, q); }
    void FactoryInitAmmoViaRust()                          { if (m_rustState) playerbot_factory_init_ammo(m_rustState.get()); }
    void FactoryEnchantEquipmentViaRust()                  { if (m_rustState) playerbot_factory_enchant_equipment(m_rustState.get()); }
    void FactoryInitGemsViaRust()                          { if (m_rustState) playerbot_factory_init_gems(m_rustState.get()); }

    void TellPlayerNoFacing(Player* /*target*/, const std::string& /*msg*/) {}
    void CastSpell(uint32_t /*spellId*/, Unit* /*target*/) {}
    void EnchantItemT(uint32_t /*spellId*/, uint8_t /*slot*/, Item* /*item*/) {}

    // ── Core callback stubs (ex-PlayerbotAI shim) ───────────────────────
    // The fork's Player.cpp calls back into PlayerbotAI from a handful of
    // places (durability loss, area-trigger gating). The old strategy engine
    // implemented these; the Rust AI does not yet, so they're no-ops here.
    void DurabilityLoss(Item* /*item*/, double /*percent*/) {}
    bool CanEnterArea(AreaTrigger const* /*at*/) { return false; }

    // Outgoing packet hook: core WorldSession::SendPacket forwards every
    // packet destined for this bot through here.
    void HandleBotOutgoingPacket(const WorldPacket& packet);

    // Spell.cpp queries for bot-specific immunity overrides and for whether
    // the bot is carrying the items a spell requires. No Rust backing yet.
    bool IsImmuneToSpell(uint32 /*spellId*/) { return false; }
    bool HasSpellItems(uint32 /*spellId*/, Item const* /*castItem*/) const { return true; }

    // ── Global init/shutdown (call from RandomPlayerbotMgr) ─────────────
    static void InitRustModule();
    static void ShutdownRustModule();
    static void WorldUpdate(uint32_t elapsed_ms);

private:
    // RAII deleter for the Rust-owned `BotState*`. Ensures that any
    // `m_rustState` escape path — normal dtor, stack unwinding through the
    // ctor, `reset()` — runs the Rust-side destructor exactly once.
    struct RustStateDeleter {
        void operator()(void* p) const noexcept
        {
            if (p)
                playerbot_destroy(p);
        }
    };

    Player*      m_bot;           // the CMaNGOS Player this AI drives
    ObjectGuid   m_masterGuid;    // the player that commands this bot (if any)
    BotCallbacks m_callbacks;     // the vtable passed to playerbot_create
    std::unique_ptr<void, RustStateDeleter> m_rustState;  // opaque BotState* from playerbot_create

    /// Compute the chat-command security tier for `sender`. Mirrors PB2's
    /// PlayerbotSecurity::LevelFor with GUILD collapsed into TALK.
    BotSecurityLevel ComputeSenderSecurity(Player& sender) const;

    /// Accept any pending group invite the bot has received. Mirrors PB2's
    /// `AcceptInvitationAction::Execute`. Called once per tick from
    /// `UpdateAIInternal` before the Rust update.
    void AutoAcceptGroupInvite();

    /// Validate the current master and, if the bot is masterless in a
    /// group, claim the first real player in that group. Mirrors PB2's
    /// `PlayerbotAI::DoNextAction` master-assignment loop. Called once per
    /// tick from `UpdateAIInternal`.
    void RefreshMaster();
};
