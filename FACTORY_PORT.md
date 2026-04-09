# PlayerbotFactory C++ → Rust Port — Status & Remaining Work

Companion to `REWRITE.md`. `REWRITE.md` covers Phases 1–8 (BT engine, combat,
encounters, strategy deletion, type safety). **This document tracks the separate
effort to port `playerbot/PlayerbotFactory.cpp` to `playerbot-rs/src/factory/`.**

The factory is the code that runs once when a bot is created / re-rolled:
clear inventory, hand out gear, teach spells, pick a spec, grant reputations,
flag taxi nodes, etc. It does not run every tick, so porting is done
method-by-method rather than as one big sweep.

---

## Established pattern

Every port follows the same recipe — when continuing, keep the pattern intact
rather than inventing a new shape.

1. **FFI callbacks** (`cpp_wrapper/botffi.h` `BotCallbacks` vtable) — one per
   CMaNGOS operation the Rust policy needs. Prefer bundling multi-step C++
   state dances into a single callback (see `bot_pick_spec_no` for the
   canonical example) over exposing every primitive.
2. **C++ bridge** (`cpp_wrapper/BotBridge.h` + `BotBridge.cpp`) — declare
   `CB_XxxYyy`, implement it (resolve `BotHandle` → `Player*` via `FindBot`),
   wire it in `BotBridge::MakeCallbacks()`.
3. **Rust trait** (`playerbot-rs/src/ffi/interface.rs`) — add a default
   method on `BotInterface` (returns empty/zero), plus a `RealInterface`
   impl that calls `self.cbs.xxx_yyy.unwrap()(self.handle, ...)`.
4. **Policy module** (`playerbot-rs/src/factory/<name>.rs`) — pure Rust logic
   on `&dyn BotInterface`. No `unsafe`, no raw FFI. Includes unit tests that
   stub every trait method via a local `MockIface`.
5. **Dispatcher wiring** (`playerbot-rs/src/factory/mod.rs`) — add a variant
   to `MiscKind` (for u8-arg-free steps) or add a dedicated top-level
   `extern "C"` export in `src/lib.rs` (for steps with non-trivial args,
   like `playerbot_factory_init_talents(spec_no: u32)`).
6. **C++ stub collapse** — the old `PlayerbotFactory::InitXxx` body becomes
   a one-liner calling `ai->FactoryMiscViaRust(N)` or a new
   `ai->FactoryXxxViaRust(args)` method on `PlayerbotRust`.
7. **Verify** — `cargo test --lib` then
   `cmake --build /home/cg/Code/gitea/Karatefylla/mangos/classic/build --target playerbots -- -j8`.

See `factory/reputations.rs` for the simplest complete example, `factory/taxi_nodes.rs`
for a malloc/free list-returning callback, and `factory/talents.rs` for the
pattern with a bespoke extern "C" export (non-u8 args).

---

## Status snapshot

Paths below are relative to repo root (`/home/cg/Code/github/theCarlG/playerbots`).

### Ported (C++ side is a one-line stub into Rust)

| C++ method | Rust module | Dispatcher |
|---|---|---|
| `ClearSkills` | `factory/progression.rs::clear_trade_skills` | `ResetProgressionViaRust(0)` |
| `ClearSpells` | `factory/progression.rs::clear_spells` | `ResetProgressionViaRust(1)` |
| `ResetQuests` | `factory/progression.rs::reset_all_quests` | `ResetProgressionViaRust(2)` |
| `ClearInventory` | `factory/inventory.rs::clear` | `ClearInventoryViaRust(0)` |
| `ClearAllItems` | `factory/inventory.rs::clear` | `ClearInventoryViaRust(1)` |
| `CancelAuras` | `factory/misc.rs::cancel_auras` | `FactoryMiscViaRust(0)` |
| `InitInventorySkill` | `factory/misc.rs::init_skill_tool_kit` | `FactoryMiscViaRust(1)` |
| `InitMounts` | `factory/mounts.rs` | `FactoryMiscViaRust(2)` |
| `InitBags` | `factory/misc.rs::init_bags` | `FactoryMiscViaRust(3)` |
| `InitReputations` | `factory/reputations.rs` | `FactoryMiscViaRust(4)` |
| `InitAmmo` | `factory/ammo.rs` | `FactoryMiscViaRust(5)` |
| `InitInventoryTrade` | `factory/inventory_trade.rs` | `FactoryMiscViaRust(6)` |
| `InitSkills` | `factory/skills.rs` | `FactoryMiscViaRust(7)` |
| `InitSpecialSpells` | `factory/special_spells.rs` | `FactoryMiscViaRust(8)` |
| `InitTaxiNodes` | `factory/taxi_nodes.rs` | `FactoryMiscViaRust(9)` |
| `InitAvailableSpells` | `factory/available_spells.rs` | `FactoryMiscViaRust(10)` |
| `InitPotions` | `factory/consumables.rs::init_potions` | `InitConsumablesViaRust(0)` |
| `InitFood` | `factory/consumables.rs::init_food` | `InitConsumablesViaRust(1)` |
| `InitReagents` | `factory/consumables.rs::init_reagents` | `InitConsumablesViaRust(2)` |
| `InitTalents(specNo)` | `factory/talents.rs::init_talents` | `playerbot_factory_init_talents(spec_no)` |
| `InitTalentsTree(incremental)` | `factory/talents.rs::init_talents_tree` | `playerbot_factory_init_talents_tree(incremental)` |

`playerbot-rs/src/factory/` is ~4.6k lines of Rust replacing roughly the same
amount of C++. All modules have unit tests; the last full run was 193 tests
passing.

### Deleted as dead code (no port needed)

- `InitImmersive` (~85 lines) — computed a `percentMap` that was stored in
  `sRandomPlayerbotMgr` KV and never read back anywhere. No callers.
- `InitSpells` (6 lines) — just `for i in 0..15: InitAvailableSpells()`. No callers.
- `InitSecondEquipmentSet` (~120 lines) — no callers.
- Old taxi-node tables `overworldTaxiNodeLevelsA/H` and the 27-line populator
  in `Init()` — superseded by the Rust-side policy.

Run `grep -rn "\\bXxx\\b" playerbot/ cpp_wrapper/` before deleting any more
"dead" method — confirm zero external references, not just zero references to
its *own* source file.

### Still in C++ — the remaining work

Grouped by effort. Line counts are approximate.

#### Tier 1 — medium ports, clean shape (good next targets)

1. **`InitQuests(std::list<uint32>& questMap)`** — `PlayerbotFactory.cpp:3174`, ~22 lines.
   For each quest id: `GetQuestTemplate` → `SatisfyQuestClass` / `SatisfyQuestRace`
   level check → `SetQuestStatus(COMPLETE)` + `RewardQuest`.
   - **Shape complication:** caller passes a C++ `std::list<uint32>&` (populated in
     `Init()` from `sPlayerbotAIConfig.randomBotQuestIds`). Two options:
     (a) Port the list-building to Rust too and add a `config_random_bot_quest_ids`
         FFI callback (symmetric with `get_random_bot_spell_ids`).
     (b) Keep the list on the C++ side and add a top-level export
         `playerbot_factory_reward_quests(state, ids: *const u32, len: u32)`.
   - Prefer (a) — it's the same pattern as `InitSpecialSpells` and removes the
     only remaining `std::list<uint32>` that crosses the factory boundary.
   - **New FFI:** `bot_satisfy_quest_class(quest_id)`, `bot_satisfy_quest_race(quest_id)`,
     `bot_quest_min_level(quest_id)`, `bot_reward_quest_as_complete(quest_id)`,
     `config_random_bot_quest_ids`.

2. **`InitAllSkills`** — `PlayerbotFactory.cpp:2743`. Two-liner:
   `InitSkills(); InitTradeSkills();`. Trivially collapsible *once* `InitTradeSkills`
   is ported. Not worth porting standalone.

#### Tier 2 — large ports, bounded scope

3. **`InitTradeSkills`** — `PlayerbotFactory.cpp:2749`, ~190 lines. The biggest
   remaining single-method target. Split into two phases:
   - **Policy** (straightforward): class → {firstSkill, secondSkill} table, `urand`
     fallback, persistence via the already-planned `bot_{get,set}_random_mgr_value`
     FFI (see Tier 3 note below), `SetRandomSkill` calls for first aid / fishing /
     cooking / picked two.
   - **Trainer recipe loop** (harder): walks `sCreatureStorage` for every creature
     with `TrainerType == TRAINER_TYPE_TRADESKILLS`, enumerates
     `sObjectMgr.GetNpcTrainerTemplateSpells` / `GetNpcTrainerSpells`, filters by
     `GetTrainerSpellState == TRAINER_SPELL_GREEN`, reads `SpellEntry.Effect` +
     `EffectTriggerSpell` + `EffectMiscValue`, and learns either the trigger
     target or the learned spell directly.
   - **Recommended split:** new FFI `get_tradeskill_recipes_for_bot(handle) ->
     BotRecipe[]` that runs the entire trainer enumeration on the C++ side and
     returns a flat list of `{spell_id, learned_spell_id, is_skill_step,
     skill_id_if_apprentice_guard}` tuples. Rust then iterates the flat list and
     calls `bot_learn_spell` / `cast_spell_on_self` per entry. This avoids
     exposing the `TrainerSpell` struct across FFI.
   - **Dead-code sweep:** once `InitTradeSkills` no longer calls `SetRandomSkill`,
     the `#if 0` block in `PlayerbotFactory.cpp:3080-3248` (containing
     `InitSkills_removed` and `SetRandomSkill`) can be fully deleted.
   - Retires `InitTradeSkills`, `UpdateTradeSkills`, `SetRandomSkill`,
     `InitSkills_removed`, `InitAllSkills` in one pass.

4. **`InitInventoryEquip`** — `PlayerbotFactory.cpp:3258`, ~70 lines. Random quality
   roll → full `sItemStorage` iteration with `CanEquipArmor` / `CanEquipWeapon` /
   `ItemLevel` / `randomGearMaxLevel` filters → `StoreItem` up to `urand(0,5)` items.
   - **New FFI:** requires item prototype iteration. Easiest shape is
     `factory_pick_random_gear_ids(handle, desired_quality, max_count) -> u32[]`
     — does the whole filter loop in C++ and returns up to N item ids. Rust just
     calls `inventory_add_item` per id.
   - Alternative: enumerate item prototypes lazily via
     `item_prototype_next(handle, after_id) -> BotItemPrototype` — far more FFI
     chatter but unlocks `InitEquipment` and `InitSecondEquipmentSet` reuse.
     Not worth it given `InitSecondEquipmentSet` is already dead.

#### Tier 3 — large ports, heavy FFI

5. **`InitEquipment(incremental, syncWithMaster, progressive, partialUpgrade)`** —
   `PlayerbotFactory.cpp:1955`, several hundred lines. The main gear-selection
   loop. Needs `CanEquipItem` / `CanEquipWeapon` / `CanEquipArmor` helpers
   (currently in `PlayerbotFactory.cpp`), `AddItemStats` / `AddItemSpellStats`
   (scoring), `Shuffle`, plus per-slot item-prototype enumeration with stat
   scoring. The single largest remaining method; likely needs a dedicated
   `factory/equipment/` submodule with its own stat-scoring tables.
   - **Prerequisite:** decide whether to port the `ItemPrototype` stat fields
     into Rust (via a new `BotItemPrototype` plain-data struct) or keep scoring
     on the C++ side and return a pre-sorted candidate list.
   - **Recommendation:** score on the C++ side. Return
     `factory_score_gear_candidates(handle, slot, desired_quality) -> BotGearCandidate[]`
     where each candidate has `{item_id, score}`. Rust picks the best.

6. **`InitPet`** (537) + **`InitPetSpells`** (667) — hunter pet creation and
   level-gated pet spell tables. Large per-family data tables that would need
   to be translated to Rust. Low priority — pets work fine as-is and the tables
   rarely change.

7. **`InitGems`** — `PlayerbotFactory.cpp:3623`, WIP in C++. Leave until the
   C++ version stabilizes.

8. **Enchanting helpers** — `EnchantItem`, `AddGems`, `EnchantEquipment`,
   `ApplyEnchantTemplate`, `LoadEnchantContainer`. Only reachable from
   `InitEquipment` path; port as part of that effort or keep in C++.

#### Tier 4 — cross-subsystem (may never port)

9. **`InitGuild`** — `PlayerbotFactory.cpp:3328`. Touches guild manager,
   creation, membership. Needs a full guild-subsystem bridge.

10. **`InitArenaTeam`** — `PlayerbotFactory.cpp:3383`, TBC/WotLK only. Touches
    arena team manager. Same concern as guild.

#### Tier 5 — orchestration (unlikely to port)

These are the top-level factory flow functions. They're thin call-sequences;
porting them would mean calling Rust from Rust via a top-level
`playerbot_factory_randomize(state, ...)` export. Probably not worth it until
every sub-step is ported.

- `Init` (98)
- `Prepare` (125)
- `Randomize(incremental, syncWithMaster)` (183) — calls everything in sequence
- `Refresh` (349)
- `AddConsumables` (364)

Helpers consumed only by unported paths (retire with their callers):
`AddItemStats`, `AddItemSpellStats`, `Shuffle`, `CanEquipItem`, `CanEquipWeapon`,
`CanEquipArmor`, `SetRandomSkill`, `StoreItem`.

---

## Shared infrastructure gaps

These are FFI bridges that several Tier 1–2 ports need. Building them once
unlocks multiple methods.

### 1. `sRandomPlayerbotMgr` key-value persistence

Currently *only* `bot_pick_spec_no` bundles a hard-coded `"specNo"` get/set.
`InitTradeSkills` needs `"firstSkill"` / `"secondSkill"`. `InitImmersive` used
`"immersive_stat_<n>"` before deletion. Future ports will need more.

**Proposed FFI** (add to `BotCallbacks` in `botffi.h`):

```c
uint32_t (*bot_kv_get)(BotHandle bot, const char* key);
void     (*bot_kv_set)(BotHandle bot, const char* key, uint32_t value);
```

C++ side wraps `sRandomPlayerbotMgr.GetValue(bot->GetGUIDLow(), key)` and
`SetValue(bot, key, value)`. `bot_pick_spec_no` can then be expressed in Rust
instead of C++, but it's cheap to leave both in place.

**Rust side:** add `bot_kv_get(&self, key: &str) -> u32` and
`bot_kv_set(&self, key: &str, value: u32)` to `BotInterface`. `RealInterface`
allocates a `CString` on each call — fine for factory code (not hot path).

### 2. Config table accessors

Currently exposed ad hoc: `get_random_bot_spell_ids`. Next ports will want
similar accessors for `sPlayerbotAIConfig.randomBotQuestIds`,
`randomGearMaxLevel`, `randomGearMaxDiff`, `randomGearLoweringChance`,
`specProbability` (currently baked into `bot_pick_spec_no`), etc.

**Proposed approach:** add a `BotFactoryConfig` plain-data struct in `botffi.h`
with every scalar field the factory reads, plus dedicated `u32[]` callbacks
for the list fields. Fetched once via
`get_factory_config(BotHandle) -> BotFactoryConfig` at the start of each
factory step. Avoids sprinkling single-field getters across the vtable.

### 3. Item prototype iteration

Blocker for `InitInventoryEquip` and `InitEquipment`. Two possible shapes:

(a) **Pre-filtered lists** — C++ does the entire filter+score, returns a
    candidate array. Cleaner, smaller FFI, but every new gear policy is a
    new callback.

(b) **Cursor iteration** — `item_prototype_count()`,
    `item_prototype_at(idx) -> BotItemPrototype`. General-purpose, but `BotItemPrototype`
    is a wide struct (Class, SubClass, Quality, ItemLevel, InventoryType, stats,
    ...) and the iteration happens hundreds of times per factory call.

**Recommendation:** start with (a) for `InitInventoryEquip`. Revisit when
`InitEquipment` forces the decision.

---

## Verification checklist for every port

After any factory change:

```bash
# From repo root
cd playerbot-rs
cargo test --lib                                         # all features
cargo test --lib --no-default-features --features vanilla # classic only
cd ..

# From anywhere
cmake --build /home/cg/Code/gitea/Karatefylla/mangos/classic/build \
      --target playerbots -- -j8
```

LSP diagnostics on `BotBridge.cpp`, `PlayerbotRust.*`, and `PlayerbotFactory.cpp`
(unknown `ObjectGuid`, missing `botpch.h`, etc.) are **pre-existing noise from
standalone-file analysis** — ignore them. Only trust `cmake --build` output.

The build target `playerbots` alone is sufficient for most changes; the full
`mangosd` link is only needed when touching externally-visible symbols
(checked last time because the `SetRandomSkill` regression only surfaced at
the final `mangosd` link step).

---

## Recommended next step

**Sweep the `#if 0` dead-code block in `PlayerbotFactory.cpp:3080-3248`** only
if `SetRandomSkill` has been moved or `InitTradeSkills` has been ported.
Otherwise, pick the next port based on what's most valuable to the project:

- **If you want to retire the most C++ in one go:** `InitTradeSkills` (Tier 2).
  Retires 5 methods and a `#if 0` block. Plan on ~2 sittings: one for the
  policy + kv FFI, one for the trainer recipe enumeration.
- **If you want the smallest next increment:** `InitQuests` (Tier 1). Single
  policy function, ~4 new callbacks, follows the same shape as
  `InitSpecialSpells`.
- **If you want to unblock the biggest remaining method:** start on the item
  prototype iteration FFI (shared-infra item 3) so `InitInventoryEquip` and
  eventually `InitEquipment` become feasible.

Every port above assumes the existing recipe from the "Established pattern"
section. Don't invent new shapes — consistency across factory/ modules is
what makes this port reviewable.
