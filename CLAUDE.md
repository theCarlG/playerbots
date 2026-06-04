# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is an AI bot module for CMaNGOS (a World of Warcraft private server emulator), supporting Classic, TBC, and WotLK expansions. It links into the CMaNGOS core as `src/modules/Bots/`.

**The module is mid-migration from C++ to Rust.** The bot AI — behavior trees, class rotations, encounters, GOAP/BDI, factory, commands, login/random-spawn/item-pool management — now lives entirely in Rust under `playerbot-rs/` (~80k LOC). The C++ under `cpp_wrapper/` is the FFI shim and remaining management plumbing that gets called by the CMaNGOS core and dispatches into Rust. `RUST_MIGRATION.md` is the authoritative plan and phase log; read it before doing migration work. The old C++ strategy engine (`playerbot/strategy/`) has been deleted — do not look for it.

`playerbot/` now holds only a handful of leftover C++ headers/config templates; the real C++ is in `cpp_wrapper/`.

## Build & Test

### Rust (the AI — testable in isolation, no CMaNGOS needed)

This is the inner loop for almost all AI work. It builds and tests on any box with Rust + clang, no server or DB. **`clang` is required** (bindgen runs against `cpp_wrapper/botffi.h`).

```bash
cd playerbot-rs
cargo test  --workspace --features wotlk          # or: vanilla | tbc
cargo clippy --workspace --features wotlk -- -D warnings
cargo build -p playerbot --features wotlk         # produces target/debug/libplayerbot_rs.a
cargo test  -p playerbot --features wotlk <name>  # run a single test by name substring
```

Exactly one expansion feature must be selected (`vanilla`, `tbc`, `wotlk`). CI (`.github/workflows/rust-tests.yml`) runs `cargo test` + `cargo clippy -- -D warnings` across all three on every push. **Clippy warnings are build failures** — keep it clean. The workspace lint config is in `playerbot-rs/Cargo.toml`.

### Full module (linked into CMaNGOS)

The module cannot build standalone; check it out inside a CMaNGOS core at `src/modules/Bots/`. This repo targets the **Karatefylla `mangos-classic`** fork, whose API differs from upstream cmangos (e.g. `sSpellTemplate`, `SpellStart`, `Cell::VisitAllObjects`, `UnitList`/`CreatureList`). From the CMaNGOS core root:

```bash
cmake -DBUILD_PLAYERBOTS=ON -B bin/builddir -S .
cmake --build bin/builddir --config Release -- -j8
cmake --install bin/builddir
```

CMake (`CMakeLists.txt`) maps the CMaNGOS project name → Rust feature: `Classic`→`vanilla`, `TBC`→`tbc`, else `wotlk`, and drives `cargo build -p playerbot` via a custom target, then links the resulting `libplayerbot_rs.a` alongside the `cpp_wrapper/` objects. The build type maps to the cargo profile (`Release`/`RelWithDebInfo` → `--release`).

## Architecture

### Three-crate Rust workspace (`playerbot-rs/crates/`)

The split exists to force clean `unsafe` boundaries and keep the AI testable without CMaNGOS.

- **`cmangos-sys`** — raw FFI. `bindgen` output for `cpp_wrapper/botffi.h` only (no CMaNGOS headers, so it builds anywhere). POD structs + the `BotCallbacks` vtable type. Zero logic.
- **`cmangos`** — the safe boundary. Defines the **`World` trait** (`world.rs`) — the AI's *only* point of contact with the game. Two impls: `VtableWorld` (`real.rs`, wraps the raw vtable — the only place that touches `unsafe`) and `MockWorld` (`mock.rs`, a fully-implemented in-memory fake used by every test, gated behind the `mock` feature). RAII owned-list guards (`owned.rs`: `OwnedList<T,F>` + aliases like `AuraList`, `UnitList`, `QuestLog`) mean Rust never calls a `free_*` callback by hand. `#[repr(transparent)]` ID newtypes in `ids.rs` (`SpellId`, `ItemId`, …). Per-domain world facets in `item_world.rs`, `login_world.rs`, `random_*_world.rs`.
- **`playerbot`** — all AI logic. `#![deny(unsafe_code)]` everywhere except `exports.rs` (the `extern "C"` boundary CMaNGOS calls into). Depends on `cmangos`, never on `cmangos-sys`. Artifact is `libplayerbot_rs.a`. Tests drive `MockWorld` directly — no `mem::zeroed()` fakery.

Hard rules carried by the migration (see `RUST_MIGRATION.md`): the `playerbot` crate must contain **zero `free_*_list` calls** and **zero `unimplemented!`/`todo!`/`panic!("not yet")`** — `MockWorld` is held to the same bar (every method has real behavior, not a stub).

### Behavior-tree AI engine (`crates/playerbot/src/`)

Behavior is an **enum-based behavior tree**, not the old strategy/priority engine. Key modules:

- `engine/bt.rs` — the `Bt` enum: a declarative tree of `Seq`/`Sel`/`Not`/`Throttle` composites with condition and action leaves. Built once at init, ticked every update. No `Box<dyn>` closures, no per-tick allocation. `engine/context.rs` carries the `TickContext` (holds the `&dyn World`).
- `classes/<class>/` — per-class rotations (all nine + `deathknight`).
- `combat/`, `noncombat/`, `strategies/`, `world/` — reactive targeting, buffing/consumables, situational strategies (kite, CC, loot…), and out-of-combat world behavior (quest, gather, vendor, travel…).
- `encounters/<raid>/` — scripted boss logic per instance (Molten Core, BWL, Naxx, Kara, AQ, ZG, Onyxia…).
- `bdi/` + `goap/` — belief/desire/intention layer and GOAP planner driving higher-level goals.
- `factory/` — bot character generation (gear, talents, spells, skills, consumables). Multi-step mutations go through a `FactoryTransaction` guard (`commit()` is an explicit marker; uncommitted drop logs a warning).
- `commands/` — chat-command parsing/dispatch (RTSC protocol). `rtsc.rs` is the real-time strategy/command channel.
- `login/`, `random/`, `random_mgr/`, `itempool/`, `manager/`, `config/` — ported management subsystems (login queue, random-bot spawn scheduling, random item pool, per-master/session manager, config runtime). DB access goes through typed FFI callbacks on `BotCallbacks`; the *driving* loops are Rust (some on worker threads).

### Snapshot / FFI model

The C++ side pushes a ~1.2 KB `BotWorldSnapshot` POD into Rust each tick; variable-length data (auras, threat, nearby units, quest log, inventory…) is pulled on demand via paired `get_*`/`free_*` callbacks and surfaced to the AI as RAII `OwnedList`s. Any new game query is a new callback in `botffi.h` (raw side), a `World` trait method + `OwnedList` alias (safe side), a `VtableWorld` impl, **and** a `MockWorld` impl with a test. The FFI contract (`cpp_wrapper/botffi.h`) is the single source of truth; changing it forces a `cmangos-sys` rebuild.

### C++ wrapper (`cpp_wrapper/`)

`BotBridge.cpp` implements the ~227-entry `BotCallbacks` vtable (CMaNGOS API dispatch). `PlayerbotRust.cpp` is the thin `PlayerbotAIBase` subclass driving the Rust tick. `MgrBridge`, `RandomPlayerbotMgr`, `LoginBridge`, `BotConfig`, `ItemBridge`, `RandomFactoryBridge`, `RandomMgrBridge` are the remaining management/DB plumbing. Phases L–N of the migration will gut the fat ones; per the plan, when a phase ports a C++ file it is **deleted** from the repo and `CMakeLists.txt`, not left co-existing.

## Conventions

- **Expansion conditionals:** Rust uses `#[cfg(feature = "vanilla" | "tbc" | "wotlk")]`; the legacy C++ used `MANGOSBOT_ZERO`/`ONE`/`TWO`. Be explicit per expansion — avoid an `#else` that silently covers two.
- **No stubs / no scope-shortcuts.** Every change ships finished: no `todo!`/`unimplemented!`, no "skeleton" impls, no half-ported subsystems. When porting parity from the original C++, port the full feature; if backing data is missing, add the data rather than declaring it out of scope.
- **IDs are newtypes**, never raw integers or string-keyed dispatch (use the `define_id!` macro). The `World` trait never returns a raw C pointer.
- Small, focused Rust crates are acceptable dependencies (`bitflags`, `arc-swap`, etc.) when they cut complexity.

## AHBot (`ahbot/`)

Separate auction-house bot subsystem, still C++, **out of scope** for the Rust migration. Config template: `ahbot/ahbot.conf.dist.in`.

## Configuration & Database

- Bot config templates: `playerbot/aiplayerbot.conf.dist.in[.tbc|.wotlk]`. After building, copy the matching `.dist` to the server dir and drop the `.dist` suffix.
- SQL patches in `sql/`: apply `characters/` to the characters DB and `world/<expansion>/` to the world DB — **only the folder matching your expansion**. The core's `InstallFullDB.sh` automates this when `PLAYERBOTS_DB="YES"`.
