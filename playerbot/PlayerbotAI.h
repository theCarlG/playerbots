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

bool IsAlliance(uint8 race);

/// Compatibility alias: existing code that creates PlayerbotAI will get a
/// PlayerbotRust instead.  This alias is intentionally narrow — it does NOT
/// provide the old strategy-engine methods (GetValue, GetTrigger, etc.).
/// Callers must be updated to use PlayerbotRust APIs or removed.
using PlayerbotAI = PlayerbotRust;
