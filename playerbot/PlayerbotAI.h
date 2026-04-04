#pragma once
// PlayerbotAI.h — transition shim.
//
// The old strategy-engine-based PlayerbotAI has been replaced by the Rust AI
// module (PlayerbotRust in cpp_wrapper/PlayerbotRust.h).
//
// This header provides:
//   1. A minimal PlayerbotAI class definition so existing code that includes
//      this header continues to compile.
//   2. A transition path: callers that relied on strategy-engine APIs
//      (GetAiObjectContext, GetEngine, ChangeStrategy, etc.) must be updated
//      to use PlayerbotRust or removed — those methods no longer exist.
//
// Integration validation is required before those callers are fully updated.

#include "PlayerbotAIBase.h"
#include "PlayerbotRust.h"

class Player;
class PlayerbotMgr;
class Item;
struct AreaTrigger;

bool IsAlliance(uint8 race);

/// Compatibility shim: existing code that creates PlayerbotAI will get a
/// PlayerbotRust instead. Defined as a trivial subclass (not a `using`
/// alias) so it matches CMaNGOS core's `class PlayerbotAI;` forward
/// declaration in Player.h — a type alias would conflict with the
/// previously-seen class declaration.
///
/// This shim is intentionally narrow: it does NOT provide the old
/// strategy-engine methods (GetValue, GetTrigger, etc.). Callers must be
/// updated to use PlayerbotRust APIs or removed.
class PlayerbotAI : public PlayerbotRust
{
public:
    using PlayerbotRust::PlayerbotRust;

    // ── Core callback stubs ──────────────────────────────────────────────
    // The fork's Player.cpp calls back into PlayerbotAI from a handful of
    // places (durability loss, area-trigger gating). The old strategy engine
    // implemented these; the Rust AI does not yet, so they're no-ops here.
    // Keeping them inline avoids pulling new .cpp files into the build until
    // there's actual behavior worth porting to Rust.
    void DurabilityLoss(Item* /*item*/, double /*percent*/) {}
    bool CanEnterArea(AreaTrigger const* /*at*/) { return false; }

    // Outgoing packet hook: core WorldSession::SendPacket forwards every
    // packet destined for this bot through here. The Rust AI does not
    // consume raw packets yet, so this is a no-op.
    void HandleBotOutgoingPacket(const WorldPacket& /*packet*/) {}

    // Spell.cpp queries for bot-specific immunity overrides and for whether
    // the bot is carrying the items a spell requires. No Rust backing yet.
    bool IsImmuneToSpell(uint32 /*spellId*/) { return false; }
    bool HasSpellItems(uint32 /*spellId*/, Item const* /*castItem*/) const { return true; }
};
