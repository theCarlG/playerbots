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

#include "playerbot/PlayerbotAIBase.h"
#include "BotConfig.h"
#include "Entities/ObjectGuid.h"
#include "botffi.h"
#include "BotBridge.h"

class Player;
class Unit;
class Item;

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

    /// Clear the bot's inventory via the Rust factory module.
    ///   mode = 0: equipped + carried bags (bank left intact)
    ///   mode = 1: everything including bank
    /// Porting plan: more factory operations (potions/food/reagents restock,
    /// gear re-roll) will route through similar FFI entry points as they move
    /// from C++ PlayerbotFactory into playerbot-rs/src/factory/.
    void ClearInventoryViaRust(uint8_t mode);

    /// Initialize consumables via the Rust factory module.
    ///   kind = 0: potions, 1: food, 2: reagents
    void InitConsumablesViaRust(uint8_t kind);

    /// Wipe a slice of the bot's progression via the Rust factory module.
    ///   kind = 0: trade skills, 1: spells, 2: quests
    void ResetProgressionViaRust(uint8_t kind);

    /// Run a miscellaneous factory step via the Rust factory module.
    ///   kind = 0: cancel auras, 1: init skill tool kit
    void FactoryMiscViaRust(uint8_t kind);

    /// Learn talents for one spec tab (0..2) via the Rust factory module.
    void FactoryInitTalentsViaRust(uint32_t spec_no);

    /// Pick a spec and spend all talent points across it (plus any leftover
    /// into the complementary tab) via the Rust factory module.
    void FactoryInitTalentsTreeViaRust(bool incremental);

    /// Top up consumables on a bot via the Rust factory module (ports
    /// `PlayerbotFactory::Refresh`). Gated on the bot's item cheat bit.
    void FactoryRefreshViaRust();

    /// Reset the bot's chassis (resurrect, combat-stop, level + XP, helmet
    /// / cloak flags) via the Rust factory module. Ports
    /// `PlayerbotFactory::Prepare`. `level` is the target level handed to
    /// the C++ factory ctor.
    void FactoryPrepareViaRust(uint32_t level);

    /// Reward every eligible quest from a caller-owned list via the Rust
    /// factory module. Ports `PlayerbotFactory::InitQuests`. `ids` / `len`
    /// describe the PB2 `std::list<uint32>` flattened to a contiguous array.
    void FactoryInitQuestsViaRust(const uint32_t* ids, size_t len);

    /// Kick off a random arena-team creation pass when the bot qualifies, via
    /// the Rust factory module. Ports `PlayerbotFactory::InitArenaTeam`. The
    /// Rust side consults the random-factory singleton for both gates
    /// (account in random list, current arena-team count below target) and
    /// forwards to `playerbot_random_factory_create_arena_teams` when they
    /// pass. On Classic this is a silent no-op.
    void FactoryInitArenaTeamViaRust();

    /// Stock a guild tabard if the bot is already guilded, otherwise pick a
    /// same-team random guild from the random-factory singleton's
    /// `random_bot_guilds` list, join it at a random rank, and stock a tabard
    /// on join, via the Rust factory module. Ports
    /// `PlayerbotFactory::InitGuild`.
    void FactoryInitGuildViaRust();

    /// Run `InitSkills` (weapons/armor/riding) followed by `InitTradeSkills`
    /// (professions + first aid/fishing/cooking + TBC+ armorsmith chain +
    /// opaque trainer-iteration loop) via the Rust factory module. Ports
    /// `PlayerbotFactory::InitAllSkills`.
    void FactoryInitAllSkillsViaRust();

    /// Run only the tradeskill half of `InitAllSkills` — profession
    /// assignment, universal secondaries, plate-class armorsmith chain,
    /// and the opaque trainer-iteration callback. Ports
    /// `PlayerbotFactory::InitTradeSkills`.
    void FactoryInitTradeSkillsViaRust();

    /// Re-roll the bot's equipped gear from the itempool cache via the
    /// Rust factory module. Ports `PlayerbotFactory::InitEquipment`.
    /// `flags` packs the four legacy boolean parameters:
    ///   bit 0 = incremental, bit 1 = syncWithMaster,
    ///   bit 2 = progressive, bit 3 = partialUpgrade.
    /// `itemQuality` pins the quality tier when non-zero (`0` = follow
    /// the progressive/incremental ladder).
    void FactoryInitEquipmentViaRust(uint32_t flags, uint32_t itemQuality);

    /// Hunter-only pet creation + stat refresh + mass-autocast + force-
    /// dismiss pass via the Rust factory module. Ports
    /// `PlayerbotFactory::InitPet`. No-op for every non-hunter class.
    void FactoryInitPetViaRust();

    /// Per-class / per-expansion pet spell learning via the Rust factory
    /// module. Ports `PlayerbotFactory::InitPetSpells`. Hunter + warlock
    /// only; every other class is a no-op on the Rust side.
    void FactoryInitPetSpellsViaRust();

    /// Top-level "re-roll this bot from scratch" pass via the Rust factory
    /// module. Ports `PlayerbotFactory::Randomize(bool incremental,
    /// bool syncWithMaster)`. `level` is the target level from the factory
    /// ctor; `itemQuality` is the `.py equip <quality>` override
    /// (0 = follow the progressive ladder).
    void FactoryRandomizeViaRust(uint32_t level,
                                  bool     incremental,
                                  bool     syncWithMaster,
                                  uint32_t itemQuality);

    /// Top up ranged-weapon ammo for warrior / rogue / hunter bots via the
    /// Rust factory module. Ports `PlayerbotFactory::InitAmmo`. No-op for
    /// every other class.
    void FactoryInitAmmoViaRust();

    /// Enchant every equipped slot via the Rust factory module. Ports
    /// `PlayerbotFactory::EnchantEquipment`. The full body (enchant-
    /// container load, spec-tab dispatch, per-slot iteration) lives on the
    /// C++ bridge side; this is a thin wrapper that forwards.
    void FactoryEnchantEquipmentViaRust();

    /// Socket a random gem into every empty socket via the Rust factory
    /// module. Ports `PlayerbotFactory::InitGems`. Classic is a
    /// compile-time no-op on the bridge side.
    void FactoryInitGemsViaRust();

    void TellPlayerNoFacing(Player* /*target*/, const std::string& /*msg*/) {}
    void CastSpell(uint32_t /*spellId*/, Unit* /*target*/) {}
    void EnchantItemT(uint32_t /*spellId*/, uint8_t /*slot*/, Item* /*item*/) {}

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
