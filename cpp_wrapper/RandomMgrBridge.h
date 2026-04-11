/**
 * RandomMgrBridge.h — C++ side of the Phase H random-mgr rewrite.
 *
 * Implements the `RandomMgrCallbacks` vtable declared in `botffi.h`. The
 * Rust `playerbot::random_mgr` module owns the entire random-bot lifecycle
 * (event cache, PID-driven activity scaling, BG/LFG bucket matrices,
 * teleport cache, stats snapshot, command dispatch); this file is the
 * narrow surface left on the C++ side that still needs to talk to CMaNGOS:
 *
 *   1. RNG + world globals — `urand`, `sWorld.Get*`, `CharacterDatabase`
 *      ping/delay.
 *   2. Event KV store — SQL against `ai_playerbot_random_bots` and
 *      targeted bumps via `TIME` arithmetic.
 *   3. Per-bot action dispatch — `sRandomPlayerbotMgr.Randomize` and
 *      friends. These are kept as delegation calls during Phase H so the
 *      legacy C++ bot-action path keeps running unchanged. Phase H.9
 *      (task #9) eventually moves them into free functions here.
 *   4. Teleport-cache / named-location table loads — SQL against
 *      `ai_playerbot_tele_cache` and `ai_playerbot_named_location`.
 *   5. BG / LFG / arena queue snapshot, AH mirror row walker, real
 *      player level scan, per-bot stats snapshot.
 *   6. Log sinks.
 *
 * Threading: the Rust random-mgr worker thread calls every `CB_*`
 * callback from its own thread. The `dispatch_*` family is the one
 * exception — the worker forwards those to the main thread via its own
 * command channel, so by the time the C++ bridge runs them we are on
 * the main world thread under the CMaNGOS lock.
 *
 * The bridge is stateless at the API level — every function is a
 * free-standing static that looks things up from the per-call arguments.
 */

#pragma once

#include "botffi.h"

namespace RandomMgrBridge
{
    /// Build the populated `RandomMgrCallbacks` vtable the Rust random
    /// manager consumes. Every field points at a `RandomMgrBridge::CB_*`
    /// free function defined in `RandomMgrBridge.cpp`. Safe to call from
    /// any thread; the returned struct is a PoD value and is passed by
    /// value into `playerbot_random_mgr_init`.
    RandomMgrCallbacks MakeCallbacks();
} // namespace RandomMgrBridge
