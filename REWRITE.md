# Playerbots Rust Rewrite — Implementation Phases

This document tracks the incremental migration of the CMaNGOS playerbots module from
C++ to Rust. The old C++ AI runs in parallel during phases 1–5 (selected per-bot at
init). No flag-day switch; no CMaNGOS modification required at any phase.

See the architecture plan at `.claude/plans/enumerated-noodling-summit.md` for full
design rationale, FFI contract, BT design, and non-negotiables.

---

## Phase 1 — FFI Layer & BT Engine Foundation ✅ COMPLETE

**Goal:** Rust module compiles, links, bots receive snapshots, do nothing harmful.

**Deliverables completed:**
- `cpp_wrapper/botffi.h` — complete `extern "C"` contract (BotCallbacks vtable, all
  push-event exports, snapshot structs)
- `playerbot-rs/` crate with `staticlib` output and `bindgen` on `botffi.h`
- `src/ffi/interface.rs` — `BotInterface` trait + `RealInterface` (wraps C vtable)
- `src/engine/bt_nodes.rs` — `Sequence`, `Selector`, `UtilitySelector`,
  `CooldownGate`, `GcdGate`, `ThrottleGate`, `Condition`, `ActionLeaf`, all
  convenience constructors, `cast_on_target`, `cast_on_current_target`
- `src/engine/blackboard.rs` — O(1) enum-keyed blackboard
- `src/engine/timers.rs` — GCD + spell cooldown tracking, `advance()` cleans expired
- `src/engine/snapshot.rs` — `WorldSnapshotExt` / `UnitSnapshotExt` traits
- `src/engine/context.rs` — `TickContext<'a>`, `TestCtxOwned`, `NullInterface`
- `src/engine/group_state.rs` — `GroupState`, `EncounterAssignments`
- `src/bot/state.rs` — `BotState` (owns interface, snap, timers, events, BT root)
- `src/bot/tick.rs` — tick loop: refresh snap, drain events, build ctx, tick BT
- `src/bot/init.rs` — `create_bot()`, Phase 1 stub tree (no-op)
- `src/lib.rs` — all `extern "C"` exports
- `src/encounters/` — `EncounterFsm` trait, sub-modules stubbed

**Tests:** 17 unit tests passing via `cargo test` (zero CMaNGOS infrastructure).

**C++ cleanup:** none yet (Phase 1 is Rust-only plumbing).

---

## Phase 2 — C++ Wrapper + Arms Warrior ✅ IN PROGRESS

**Goal:** Bot is wired end-to-end. One class (Arms Warrior) fully functional.
CMaNGOS calls the Rust module instead of the old engine for Arms Warriors.

**Deliverables:**
- `cpp_wrapper/BotBridge.h/.cpp` — implements all `BotCallbacks` function pointers
  using CMaNGOS APIs (`ObjectAccessor`, `Player::CastSpell`, `ThreatManager`, etc.)
- `cpp_wrapper/PlayerbotRust.h/.cpp` — C++ shim inheriting `PlayerbotAIBase`;
  delegates all calls to Rust via `playerbot_create/update/destroy`
- `CMakeLists.txt` updated — cpp_wrapper sources added, Cargo build target added,
  `libplayerbot_rs.a` linked
- `src/data/spells/vanilla.rs` — Warrior (Arms) spell ID constants for Classic
- `src/classes/warrior/arms.rs` — Arms Warrior BT rotation
- `src/bot/init.rs` updated — dispatches Arms Warrior to the real tree

**C++ cleanup:** None in Phase 2. The old engine stays as the fallback for all
non-Arms-Warrior bots. Arms Warriors will use the new Rust tree; all others still
use the C++ strategy engine.

---

## Phase 3 — All 9 Classes × 3 Specs

**Goal:** Every class/spec has a working BT rotation. All combat behavior is driven
by the Rust module.

**Deliverables:**
- `src/classes/{warrior,paladin,priest,druid,hunter,mage,rogue,shaman,warlock}/`
  — arms/fury/protection, holy/prot/ret, holy/disc/shadow, balance/feral/resto,
    bm/mm/survival, arcane/fire/frost, assassination/combat/subtlety,
    elem/enhance/resto, affliction/demo/destro
- `src/data/spells/` — complete spell ID tables for all classes (vanilla, tbc, wotlk)
- `src/classes/deathknight/` — blood/frost/unholy (wotlk feature-gated)
- `src/bot/init.rs` updated — full dispatch table for all class/spec combinations

**C++ cleanup:**
- Delete `playerbot/strategy/warrior/` directory
- Delete `playerbot/strategy/paladin/`, `priest/`, `druid/`, `hunter/`, `mage/`,
  `rogue/`, `shaman/`, `warlock/`, `deathknight/`
- Remove class-specific sections from `playerbot/strategy/AiObjectContext.cpp`
- Remove class-specific glob entries from `CMakeLists.txt`

---

## Phase 4 — Non-Combat Behavior

**Goal:** Bots follow leaders, buff group members, eat/drink, loot, handle basic
out-of-combat logic.

**Deliverables:**
- `src/noncombat/follow.rs` — follow master/group leader with configurable distance
- `src/noncombat/buffing.rs` — group buff logic (class-specific buffs, rebuff timers)
- `src/noncombat/consumables.rs` — eat/drink when HP/mana below threshold
- `src/noncombat/looting.rs` — loot nearby corpses after combat
- `src/bot/init.rs` updated — non-combat subtrees plugged into root selector

**C++ cleanup:**
- Delete `playerbot/strategy/generic/` directory (non-combat strategies)
- Delete `playerbot/strategy/actions/` (movement, looting, buff actions)
- Delete `playerbot/strategy/triggers/` (non-combat triggers)
- Delete `playerbot/strategy/values/` (computed values layer)

---

## Phase 5 — Encounter FSMs (Boss AI)

**Goal:** Phase-aware boss AI for all major raid encounters. Group coordination
(tank assignments, healer rotation, special role assignments) via `GroupState`.

**Encounters by raid:**

### Molten Core (10 bosses)
- Lucifron, Magmadar, Gehennas, Garr, Baron Geddon, Shazzrah, Sulfuron Harbinger,
  Golemagg, Majordomo Executus, Ragnaros

### Blackwing Lair (8 bosses)
- Razorgore, Vaelastrasz, Broodlord Lashlayer, Firemaw, Ebonroc, Flamegor,
  Chromaggus, Nefarian

### Onyxia's Lair (1 boss)
- Onyxia (3-phase: ground → air → ground+whelps)

### Ruins of Ahn'Qiraj / Temple of AQ (AQ20/AQ40)
- AQ20: Kurinnaxx, General Rajaxx, Moam, Buru the Gorger, Ayamiss, Ossirian
- AQ40: The Prophet Skeram, Battleguard Sartura, Fankriss, Viscidus, Princess Huhuran,
  Twin Emperors, Ouro, C'Thun

### Naxxramas (15 bosses) — most mechanically complex
- Anub'Rekhan, Grand Widow Faerlina, Maexxna (Spider Wing)
- Noth the Plaguebringer, Heigan the Unclean, Loatheb (Plague Wing)
- Instructor Razuvious, Gothik the Harvester, Four Horsemen (Death Knight Wing)
- Patchwerk, Grobbulus, Gluth, Thaddius (Construct Wing)
- Sapphiron, Kel'Thuzad

### TBC Raids (feature-gated on `tbc` feature)
- Karazhan (12 encounters)
- Gruul's Lair, Magtheridon's Lair
- Serpentshrine Cavern (6 bosses)
- Tempest Keep (4 bosses)
- Hyjal Summit (5 bosses)
- Black Temple (9 bosses)
- Zul'Aman, Sunwell Plateau (6 bosses)

**Deliverables:**
- `src/encounters/{molten_core,blackwing_lair,onyxias_lair,aq20,aq40,naxxramas}/`
  — one file per boss: typed phase enum + FSM impl + BT subtree
- `src/encounters/coordinator.rs` — `GroupCoordinator` (role assignment at pull,
  re-assignment on death, inter-bot communication via `GroupState`)
- `src/encounters/timeline.rs` — `BossTimeline`, `TimelineEntry` for predictive pre-hot
  and proactive cooldown usage
- `src/bot/tick.rs` updated — encounter FSM update integrated into tick loop
- `src/combat/interrupts.rs` — interrupt assignment and coordination
- `src/combat/positioning.rs` — melee stack-behind, ranged spread

**C++ cleanup:**
- Delete `playerbot/strategy/generic/*Dungeon*` files (old encounter strategies)
- Delete `playerbot/strategy/generic/*Raid*` files
- Delete `playerbot/strategy/generic/RpgTravelAction.cpp` (encounter travel)

---

## Phase 6 — Final C++ Cleanup & Integration Validation

**Goal:** All old C++ strategy engine code removed. Rust module is the sole AI driver.

**C++ cleanup:**
- Delete `playerbot/strategy/Engine.h/.cpp` (old priority-based engine)
- Delete `playerbot/strategy/Action.h/.cpp`, `Trigger.h/.cpp`, `Value.h/.cpp`,
  `Multiplier.h/.cpp`, `Strategy.h/.cpp`
- Delete `playerbot/strategy/AiObjectContext.h/.cpp`
- Delete `playerbot/strategy/ReactionEngine.h/.cpp`
- Delete `playerbot/PlayerbotAI.h/.cpp` (replaced by `PlayerbotRust`)
- Delete `playerbot/AiFactory.h/.cpp` (replaced by `playerbot_create` in Rust)
- Update `CMakeLists.txt` — remove all old strategy GLOB entries, keep only
  cpp_wrapper sources and Rust build target
- Update any remaining `#include "PlayerbotAI.h"` to `#include "PlayerbotRust.h"`
- Update `PlayerbotMgr` to use `PlayerbotRust` exclusively

**Integration validation (requires CMaNGOS server):**

| Milestone | Requirement |
|-----------|-------------|
| FFI live | Bot logs in, receives non-zero snapshot, does not crash server |
| Basic combat | Arms Warrior attacks, uses priority abilities in correct order |
| Full classes | All 9 classes × 3 specs run for 1 hour without stuck bots |
| Non-combat | Bots follow, buff, drink/eat, loot correctly |
| First encounter | 10-bot group completes Onyxia |
| Full MC | 10-bot group completes all 10 Molten Core bosses |
| Naxxramas | 40-bot raid completes Naxxramas |

---

## Expansion Support Matrix

| Content | Classic (`vanilla`) | TBC (`tbc`) | WotLK (`wotlk`) |
|---------|:---:|:---:|:---:|
| All 9 classes (no DK) | ✓ | ✓ | ✓ |
| Death Knight | — | — | ✓ |
| Molten Core, BWL, Onyxia | ✓ | ✓ | ✓ |
| AQ20, AQ40, Naxxramas | ✓ | ✓ | ✓ |
| Karazhan, Gruul, Mags | — | ✓ | ✓ |
| SSC, TK, Hyjal, BT | — | ✓ | ✓ |
| Ulduar, ToC, ICC | — | — | ✓ |

---

## Design Non-Negotiables

1. **No CMaNGOS modifications.** All hooks use existing entry points.
2. **No regressions.** Every mechanic that worked in C++ must work in Rust, and be
   better. Each spec is tested against known-correct spell priority before the C++
   version is retired.
3. **`cargo test` always passes** — zero CMaNGOS infrastructure required.
4. **No string-keyed registries.** All dispatch is through Rust's type system.
5. **Zero per-tick heap allocations.** BT nodes are built once at init.
6. **Adding a new boss = one new file.** No changes to shared infrastructure.
7. **The FFI boundary (`botffi.h`) is the only C/Rust contract.** Both sides can
   evolve independently as long as the contract is honored.
