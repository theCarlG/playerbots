# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a C++ AI bot module for CMaNGOS (World of Warcraft private server emulator), supporting Classic, TBC, and WotLK expansions. It compiles as a static library (`libplayerbots.a`) that is linked into the CMaNGOS core.

## Build System

This module does not build standalone — it must be checked out inside a CMaNGOS core repo (e.g., `mangos-classic`, `mangos-tbc`, or `mangos-wotlk`) under `src/modules/Bots/`.

```bash
# Configure (from CMaNGOS core root)
cmake -DBUILD_PLAYERBOTS=ON -B bin/builddir -S .

# Build
cmake --build bin/builddir --config Release -- -j8

# Install
cmake --install bin/builddir
```

The CI uses `clang`/`clang++` on Ubuntu 22.04 with Boost 1.83.0. The expansion is selected at compile time via a CMake definition that maps to a preprocessor macro (`MANGOSBOT_ZERO` = Classic, `MANGOSBOT_ONE` = TBC, `MANGOSBOT_TWO` = WotLK). All three expansions are built and tested in CI on every push.

There are no unit tests — `BotTests.h/cpp` is a log-analysis system for gameplay behavior, not a test runner.

## Architecture

### Strategy Pattern Core

Every bot's behavior is driven by a priority-based strategy engine:

- **`Engine`** (`playerbot/strategy/Engine.h`) — Iterates available actions each tick, picks the highest-priority one whose trigger fires, and executes it.
- **`Action`** (`playerbot/strategy/Action.h`) — A discrete bot behavior (cast spell, move, loot, etc.). Returns `true` if it executed successfully.
- **`Trigger`** (`playerbot/strategy/Trigger.h`) — A boolean condition checked by the engine to determine if an action is relevant.
- **`Value`** (`playerbot/strategy/Value.h`) — Lazy-evaluated, cached game-state computation (target health%, threat level, nearby enemies, etc.).
- **`Multiplier`** (`playerbot/strategy/Multiplier.h`) — Adjusts action priority based on situational context.
- **`Strategy`** (`playerbot/strategy/Strategy.h`) — Groups related Action+Trigger pairs and registers them with the engine.
- **`AiObjectContext`** (`playerbot/strategy/AiObjectContext.h`) — Per-bot registry of all named actions, values, triggers, and multipliers; acts as a service locator.

### Directory Layout

```
playerbot/           # Core AI controller and helpers
  PlayerbotAI.h/cpp  # Main AI entry point; manages strategy engines and bot lifecycle
  AiFactory.h/cpp    # Creates engines and AiObjectContext per bot/class/spec
  strategy/
    *.h              # Base class definitions (Action, Trigger, Value, Engine, etc.)
    actions/         # 100+ action implementations (combat, questing, looting, trading...)
    triggers/        # Trigger definitions
    values/          # Computed value definitions
    generic/         # Cross-class strategies (combat roles, BGs, chat commands, dungeons...)
    druid/ hunter/ mage/ paladin/ priest/ rogue/ shaman/ warlock/ warrior/ deathknight/
                     # Class-specific AiObjectContext, strategies, actions, triggers, values
ahbot/               # Auction House bot (separate subsystem)
sql/
  characters/        # Character DB schema patches
  world/
    classic/ tbc/ wotlk/  # Expansion-specific world DB data (apply only your expansion's folder)
```

### Adding New Behavior

1. **New Action**: Subclass `Action`, implement `Execute()`, register it by name in the relevant class's `AiObjectContext`.
2. **New Trigger**: Subclass `Trigger` (or `Trigger<T>`), implement `IsActive()`, register in `AiObjectContext`.
3. **New Value**: Subclass `Value<T>`, implement `Calculate()`, register in `AiObjectContext`.
4. **Wire up**: In the relevant `Strategy` subclass, add `ACTION_NODE("action name", "trigger name")` entries.

### Expansion Conditional Compilation

Use `#ifdef MANGOSBOT_ZERO` / `MANGOSBOT_ONE` / `MANGOSBOT_TWO` guards when behavior differs between expansions. Avoid a single `#else` covering two expansions — be explicit.

### Key Helper Classes

- `PlayerbotSecurity` — Checks who is allowed to command a bot (owner, party, GM).
- `ChatHelper` — Parses natural-language chat commands directed at bots.
- `PerformanceMonitor` / `MemoryMonitor` — Built-in profiling; used to guard expensive operations.
- `LootObjectStack` — Tracks lootable objects near the bot.
- `FleeManager` — Computes flee destinations in combat.

## Configuration

Config template: `playerbot/aiplayerbot<expansion>.conf.dist.in`. After building, copy the appropriate `.conf.dist` to the server directory and rename it (remove `.dist`). This file controls random bot population, BG/arena participation, gear generation, and performance tuning.

AHBot config template: `ahbot/ahbot.conf.dist.in`.

## Database

SQL patches live in `sql/`. Apply `characters/` patches to the characters DB and `world/<expansion>/` patches to the world DB. Only apply the folder matching your expansion. The `InstallFullDB.sh` script in the CMaNGOS core automates this when `PLAYERBOTS_DB="YES"` is set in `InstallFullDB.config`.
